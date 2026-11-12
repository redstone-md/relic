//! The file set of an instance: ingestion, side classification and the two
//! views derived from it — the client manifest and the server deployment.

use std::collections::BTreeMap;
use std::sync::Arc;

use relic_jar::Confidence;
use relic_proto::{
    DEFAULT_PROTECTED_PATHS, FileKind, JavaRuntime, Manifest, ManifestFile, Side, SyncAction,
};
use sqlx::{Row, SqliteConnection, SqlitePool, sqlite::SqliteRow};
use tokio::io::AsyncRead;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::blobs::{BlobStore, is_sha256};
use crate::error::{Error, Result, now};
use crate::instances;
use crate::textures::TextureKind;

/// Where the effective side of a file came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideSource {
    Detected,
    Override,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileRecord {
    pub instance_id: String,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub kind: FileKind,
    /// Override if present, otherwise what detection found.
    pub side: Side,
    pub side_source: SideSource,
    pub detected_side: Side,
    pub detect_confidence: Confidence,
    pub detect_reason: String,
    pub sync_action: SyncAction,
    pub optional: bool,
    pub mod_id: Option<String>,
    pub mod_name: Option<String>,
    pub mod_version: Option<String>,
    /// What the mod says about itself.
    pub description: String,
    /// Hash of the mod's picture, served like a file of the pack.
    pub icon: Option<String>,
    /// Mod ids this file does not load without.
    pub depends: Vec<String>,
    /// Every other mod id this file answers to or carries inside.
    #[serde(skip)]
    pub provides: Vec<String>,
    /// Paths of the files that satisfy `depends`.
    pub requires: Vec<String>,
    /// Dependencies no file of the pack satisfies.
    pub missing: Vec<String>,
}

/// Caller-supplied facts that take precedence over detection, e.g. the
/// per-file `env` of an imported `.mrpack`.
#[derive(Debug, Clone, Default)]
pub struct FileHints {
    pub side: Option<Side>,
    pub optional: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Modpack {
    db: SqlitePool,
    blobs: BlobStore,
    /// Ingestion holds this shared, garbage collection exclusively, so a blob
    /// cannot be swept between being stored and being referenced.
    gc_lock: Arc<RwLock<()>>,
}

/// Validates an instance-relative path and returns it with forward slashes.
/// Rejects anything that could address a location outside the instance.
pub fn normalize_path(raw: &str) -> Result<String> {
    let unified = raw.replace('\\', "/");
    let mut segments = Vec::new();
    for segment in unified.split('/') {
        let bad = segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.ends_with([' ', '.'])
            || segment
                .chars()
                .any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'));
        if bad {
            return Err(Error::invalid(format!("invalid file path: {raw:?}")));
        }
        segments.push(segment);
    }
    let path = segments.join("/");
    if path.len() > 512 {
        return Err(Error::invalid("file path is too long"));
    }
    Ok(path)
}

fn confidence_str(c: Confidence) -> &'static str {
    match c {
        Confidence::None => "none",
        Confidence::Heuristic => "heuristic",
        Confidence::Declared => "declared",
    }
}

fn parse_confidence(s: &str) -> Confidence {
    match s {
        "declared" => Confidence::Declared,
        "heuristic" => Confidence::Heuristic,
        _ => Confidence::None,
    }
}

const SELECT_FILES: &str = "
    SELECT f.instance_id, f.path, f.sha256, b.size, f.detected_side, f.detect_confidence,
           f.detect_reason, f.sync_action, f.optional, f.mod_id, f.mod_name, f.mod_version,
           f.mod_description, f.icon_sha256, f.mod_depends, f.mod_provides,
           o.side AS override_side
    FROM files f
    JOIN blobs b ON b.sha256 = f.sha256
    LEFT JOIN side_overrides o ON o.sha256 = f.sha256";

fn record_from_row(row: &SqliteRow) -> Result<FileRecord> {
    let corrupt = |what: &str| Error::Other(anyhow::anyhow!("files row has invalid {what}"));
    let path: String = row.try_get("path")?;
    let detected: String = row.try_get("detected_side")?;
    let detected_side = Side::parse(&detected).ok_or_else(|| corrupt("detected_side"))?;
    let override_side: Option<String> = row.try_get("override_side")?;
    let override_side = override_side.as_deref().and_then(Side::parse);
    let sync_action: String = row.try_get("sync_action")?;
    let confidence: String = row.try_get("detect_confidence")?;
    Ok(FileRecord {
        instance_id: row.try_get("instance_id")?,
        kind: FileKind::from_path(&path),
        path,
        sha256: row.try_get("sha256")?,
        size: row.try_get::<i64, _>("size")? as u64,
        side: override_side.unwrap_or(detected_side),
        side_source: if override_side.is_some() {
            SideSource::Override
        } else {
            SideSource::Detected
        },
        detected_side,
        detect_confidence: parse_confidence(&confidence),
        detect_reason: row.try_get("detect_reason")?,
        sync_action: SyncAction::parse(&sync_action).ok_or_else(|| corrupt("sync_action"))?,
        optional: row.try_get("optional")?,
        mod_id: row.try_get("mod_id")?,
        mod_name: row.try_get("mod_name")?,
        mod_version: row.try_get("mod_version")?,
        description: row.try_get("mod_description")?,
        icon: row.try_get("icon_sha256")?,
        depends: id_list(row.try_get("mod_depends")?),
        provides: id_list(row.try_get("mod_provides")?),
        requires: Vec::new(),
        missing: Vec::new(),
    })
}

fn id_list(json: Option<String>) -> Vec<String> {
    json.and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// Ids that name the same mod.
fn alternatives(id: &str) -> &'static [&'static str] {
    match id {
        "fabric" => &["fabric-api"],
        "fabric-api" => &["fabric"],
        _ => &[],
    }
}

/// Fills in which files of the pack each file needs, and what it needs that
/// the pack does not have.
fn link(records: &mut [FileRecord]) {
    let mut owners: BTreeMap<String, String> = BTreeMap::new();
    for record in records.iter() {
        for id in record.mod_id.iter().chain(&record.provides) {
            owners
                .entry(id.to_ascii_lowercase())
                .or_insert_with(|| record.path.clone());
        }
    }
    for record in records.iter_mut() {
        for id in &record.depends {
            let lower = id.to_ascii_lowercase();
            let owner = std::iter::once(lower.as_str())
                .chain(alternatives(&lower).iter().copied())
                .find_map(|candidate| owners.get(candidate));
            match owner {
                Some(path) if *path == record.path => {}
                Some(path) => {
                    if !record.requires.contains(path) {
                        record.requires.push(path.clone());
                    }
                }
                None => record.missing.push(id.clone()),
            }
        }
    }
}

async fn bump_build(conn: &mut SqliteConnection, instance_id: &str) -> Result<()> {
    sqlx::query("UPDATE instances SET build_id = ? WHERE id = ?")
        .bind(Uuid::new_v4().to_string())
        .bind(instance_id)
        .execute(conn)
        .await?;
    Ok(())
}

struct Detection {
    side: Side,
    confidence: Confidence,
    reason: String,
    mod_id: Option<String>,
    mod_name: Option<String>,
    mod_version: Option<String>,
    description: String,
    icon: Option<Vec<u8>>,
    depends: Vec<String>,
    provides: Vec<String>,
}

impl Detection {
    fn plain(side: Side, confidence: Confidence, reason: String) -> Self {
        Self {
            side,
            confidence,
            reason,
            mod_id: None,
            mod_name: None,
            mod_version: None,
            description: String::new(),
            icon: None,
            depends: Vec::new(),
            provides: Vec::new(),
        }
    }
}

fn json(ids: &[String]) -> String {
    serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_owned())
}

impl Modpack {
    pub fn new(db: SqlitePool, blobs: BlobStore) -> Self {
        Self {
            db,
            blobs,
            gc_lock: Arc::default(),
        }
    }

    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }

    pub fn db(&self) -> &SqlitePool {
        &self.db
    }

    /// Works out the side of a freshly stored file. JARs under `mods/` are
    /// inspected; everything else is classified by where it lives.
    async fn detect(&self, path: &str, sha256: &str) -> Result<Detection> {
        let kind = FileKind::from_path(path);
        let is_jar = path.to_ascii_lowercase().ends_with(".jar");
        if kind == FileKind::Mod && is_jar {
            let blob_path = self.blobs.path(sha256)?;
            let info = tokio::task::spawn_blocking(move || relic_jar::inspect_path(blob_path))
                .await
                .map_err(|e| Error::Other(e.into()))?;
            return Ok(match info {
                Ok(mut info) => {
                    let depends = info.depends();
                    let own = info.mods.first().map(|m| m.id.clone());
                    let provides = info
                        .ids()
                        .into_iter()
                        .filter(|id| Some(id) != own.as_ref())
                        .collect();
                    let icon = info.icon.take();
                    let first = info.mods.into_iter().next();
                    Detection {
                        side: info.side,
                        confidence: info.confidence,
                        reason: info.reason,
                        mod_id: first.as_ref().map(|m| m.id.clone()),
                        mod_name: first.as_ref().map(|m| m.name.clone()),
                        description: first
                            .as_ref()
                            .map(|m| m.description.clone())
                            .unwrap_or_default(),
                        mod_version: first.map(|m| m.version),
                        icon,
                        depends,
                        provides,
                    }
                }
                Err(err) => Detection::plain(Side::Unknown, Confidence::None, err.to_string()),
            });
        }

        let (side, reason) = match kind {
            FileKind::ResourcePack | FileKind::ShaderPack => {
                (Side::Client, "resource and shader packs are client-only")
            }
            _ => (Side::Both, "non-mod files are deployed to both sides"),
        };
        Ok(Detection::plain(
            side,
            Confidence::Declared,
            reason.to_owned(),
        ))
    }

    /// Keeps a mod's picture next to the pack's files. Returns its hash.
    async fn store_icon(&self, icon: Option<&[u8]>) -> Result<Option<String>> {
        let Some(png) = icon else {
            return Ok(None);
        };
        let blob = self.blobs.put_bytes(png).await?;
        sqlx::query("INSERT OR IGNORE INTO blobs (sha256, size, created_at) VALUES (?, ?, ?)")
            .bind(&blob.sha256)
            .bind(blob.size as i64)
            .bind(now())
            .execute(&self.db)
            .await?;
        Ok(Some(blob.sha256))
    }

    /// Reads names, pictures and dependencies of mods stored before the
    /// daemon knew to look for them. Returns how many files it went through.
    pub async fn rescan(&self) -> Result<u64> {
        let _ingesting = self.gc_lock.read().await;
        let rows = sqlx::query(
            "SELECT instance_id, path, sha256 FROM files
             WHERE mod_provides IS NULL AND lower(path) LIKE 'mods/%.jar'",
        )
        .fetch_all(&self.db)
        .await?;
        let mut touched = std::collections::BTreeSet::new();
        for row in &rows {
            let instance_id: String = row.try_get("instance_id")?;
            let path: String = row.try_get("path")?;
            let sha256: String = row.try_get("sha256")?;
            let Ok(detection) = self.detect(&path, &sha256).await else {
                continue;
            };
            let icon = self.store_icon(detection.icon.as_deref()).await?;
            sqlx::query(
                "UPDATE files SET mod_description = ?, icon_sha256 = ?, mod_depends = ?,
                    mod_provides = ?, mod_name = COALESCE(?, mod_name)
                 WHERE instance_id = ? AND path = ?",
            )
            .bind(&detection.description)
            .bind(icon)
            .bind(json(&detection.depends))
            .bind(json(&detection.provides))
            .bind(&detection.mod_name)
            .bind(&instance_id)
            .bind(&path)
            .execute(&self.db)
            .await?;
            touched.insert(instance_id);
        }
        let mut conn = self.db.acquire().await?;
        for instance_id in &touched {
            bump_build(&mut conn, instance_id).await?;
        }
        Ok(rows.len() as u64)
    }

    /// Stores `reader` as `path` of the instance, replacing an existing file
    /// at that path. Admin-chosen settings of a replaced file are kept.
    pub async fn add_file<R: AsyncRead + Unpin>(
        &self,
        instance_id: &str,
        path: &str,
        reader: R,
        max_bytes: Option<u64>,
        hints: FileHints,
    ) -> Result<FileRecord> {
        let path = normalize_path(path)?;
        instances::get(&self.db, instance_id).await?;

        let _ingesting = self.gc_lock.read().await;
        let blob = self.blobs.put(reader, max_bytes).await?;
        let mut detection = self.detect(&path, &blob.sha256).await?;
        if let Some(side) = hints.side.filter(|s| *s != Side::Unknown) {
            detection.side = side;
            detection.confidence = Confidence::Declared;
            detection.reason = "stated by the import source".to_owned();
        }
        // Settings are the player's once delivered; the rest stays the pack's.
        let default_action = match FileKind::from_path(&path) {
            FileKind::Config => SyncAction::SyncOnce,
            _ if relic_proto::player_owned(&path) => SyncAction::SyncOnce,
            _ => SyncAction::Enforce,
        };
        let icon = self.store_icon(detection.icon.as_deref()).await?;

        let mut tx = self.db.begin().await?;
        sqlx::query("INSERT OR IGNORE INTO blobs (sha256, size, created_at) VALUES (?, ?, ?)")
            .bind(&blob.sha256)
            .bind(blob.size as i64)
            .bind(now())
            .execute(&mut *tx)
            .await?;
        // On replace, `sync_action` and `optional` survive unless a hint says otherwise.
        sqlx::query(
            "INSERT INTO files
                (instance_id, path, sha256, detected_side, detect_confidence, detect_reason,
                 sync_action, optional, mod_id, mod_name, mod_version, created_at,
                 mod_description, icon_sha256, mod_depends, mod_provides)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (instance_id, path) DO UPDATE SET
                sha256 = excluded.sha256,
                detected_side = excluded.detected_side,
                detect_confidence = excluded.detect_confidence,
                detect_reason = excluded.detect_reason,
                optional = CASE WHEN ? THEN excluded.optional ELSE files.optional END,
                mod_id = excluded.mod_id,
                mod_name = excluded.mod_name,
                mod_version = excluded.mod_version,
                mod_description = excluded.mod_description,
                icon_sha256 = excluded.icon_sha256,
                mod_depends = excluded.mod_depends,
                mod_provides = excluded.mod_provides",
        )
        .bind(instance_id)
        .bind(&path)
        .bind(&blob.sha256)
        .bind(detection.side.as_str())
        .bind(confidence_str(detection.confidence))
        .bind(&detection.reason)
        .bind(default_action.as_str())
        .bind(hints.optional.unwrap_or(false))
        .bind(&detection.mod_id)
        .bind(&detection.mod_name)
        .bind(&detection.mod_version)
        .bind(now())
        .bind(&detection.description)
        .bind(icon)
        .bind(json(&detection.depends))
        .bind(json(&detection.provides))
        .bind(hints.optional.is_some())
        .execute(&mut *tx)
        .await?;
        bump_build(&mut tx, instance_id).await?;
        tx.commit().await?;

        self.get_file(instance_id, &path).await
    }

    pub async fn get_file(&self, instance_id: &str, path: &str) -> Result<FileRecord> {
        // Through the whole list: what a file needs depends on its neighbours.
        self.list_files(instance_id)
            .await?
            .into_iter()
            .find(|file| file.path == path)
            .ok_or(Error::NotFound("file"))
    }

    pub async fn list_files(&self, instance_id: &str) -> Result<Vec<FileRecord>> {
        let mut records = sqlx::query(&format!(
            "{SELECT_FILES} WHERE f.instance_id = ? ORDER BY f.path"
        ))
        .bind(instance_id)
        .fetch_all(&self.db)
        .await?
        .iter()
        .map(record_from_row)
        .collect::<Result<Vec<_>>>()?;
        link(&mut records);
        Ok(records)
    }

    pub async fn remove_file(&self, instance_id: &str, path: &str) -> Result<()> {
        let mut tx = self.db.begin().await?;
        let done = sqlx::query("DELETE FROM files WHERE instance_id = ? AND path = ?")
            .bind(instance_id)
            .bind(path)
            .execute(&mut *tx)
            .await?;
        if done.rows_affected() == 0 {
            return Err(Error::NotFound("file"));
        }
        bump_build(&mut tx, instance_id).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_file_options(
        &self,
        instance_id: &str,
        path: &str,
        sync_action: Option<SyncAction>,
        optional: Option<bool>,
    ) -> Result<FileRecord> {
        let mut tx = self.db.begin().await?;
        let done = sqlx::query(
            "UPDATE files SET
                sync_action = COALESCE(?, sync_action),
                optional = COALESCE(?, optional)
             WHERE instance_id = ? AND path = ?",
        )
        .bind(sync_action.map(SyncAction::as_str))
        .bind(optional)
        .bind(instance_id)
        .bind(path)
        .execute(&mut *tx)
        .await?;
        if done.rows_affected() == 0 {
            return Err(Error::NotFound("file"));
        }
        bump_build(&mut tx, instance_id).await?;
        tx.commit().await?;
        self.get_file(instance_id, path).await
    }

    /// Sets (`Some`) or clears (`None`) the manual side of a piece of
    /// content. Applies to every instance that carries the same bytes.
    pub async fn set_side_override(&self, sha256: &str, side: Option<Side>) -> Result<()> {
        if !is_sha256(sha256) {
            return Err(Error::invalid("sha256 must be a lowercase hex digest"));
        }
        if side == Some(Side::Unknown) {
            return Err(Error::invalid("an override must be client, server or both"));
        }
        let mut tx = self.db.begin().await?;
        match side {
            Some(side) => {
                sqlx::query(
                    "INSERT INTO side_overrides (sha256, side, updated_at) VALUES (?, ?, ?)
                     ON CONFLICT (sha256) DO UPDATE SET
                        side = excluded.side, updated_at = excluded.updated_at",
                )
                .bind(sha256)
                .bind(side.as_str())
                .bind(now())
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM side_overrides WHERE sha256 = ?")
                    .bind(sha256)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let affected: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT instance_id FROM files WHERE sha256 = ?")
                .bind(sha256)
                .fetch_all(&mut *tx)
                .await?;
        for instance_id in &affected {
            bump_build(&mut tx, instance_id).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The client view: every file whose effective side includes the client.
    /// Files with an undetermined side are left out until an admin decides.
    pub async fn manifest(&self, instance_id: &str) -> Result<Manifest> {
        let instance = instances::get(&self.db, instance_id).await?;
        let records: Vec<FileRecord> = self
            .list_files(instance_id)
            .await?
            .into_iter()
            .filter(|f| f.side.on_client())
            .collect();
        // Required files are always there; only optional ones need naming.
        let optional: std::collections::BTreeSet<String> = records
            .iter()
            .filter(|f| f.optional)
            .map(|f| f.path.clone())
            .collect();
        let files = records
            .into_iter()
            .map(|f| ManifestFile {
                requires: f
                    .requires
                    .into_iter()
                    .filter(|path| optional.contains(path))
                    .collect(),
                name: f.mod_name.unwrap_or_default(),
                description: f.description,
                icon: f.icon,
                path: f.path,
                sha256: f.sha256,
                size: f.size,
                kind: f.kind,
                action: f.sync_action,
                optional: f.optional,
            })
            .collect();
        Ok(Manifest {
            build_id: instance.build_id,
            instance_id: instance.id,
            minecraft_version: instance.minecraft_version,
            loader: instance.loader,
            java_runtime: JavaRuntime {
                major_version: instance.java_major,
                vendor: "temurin".to_owned(),
            },
            policy: instance.mod_policy,
            files,
            protected_paths: DEFAULT_PROTECTED_PATHS
                .iter()
                .map(|p| (*p).to_owned())
                .collect(),
            locked_dirs: instance.locked_dirs,
        })
    }

    /// The server view: files to materialise in the instance directory.
    pub async fn server_files(&self, instance_id: &str) -> Result<Vec<FileRecord>> {
        Ok(self
            .list_files(instance_id)
            .await?
            .into_iter()
            .filter(|f| f.side.on_server())
            .collect())
    }

    /// Files nobody has classified yet; they are deployed nowhere.
    pub async fn unclassified(&self, instance_id: &str) -> Result<Vec<FileRecord>> {
        Ok(self
            .list_files(instance_id)
            .await?
            .into_iter()
            .filter(|f| f.side == Side::Unknown)
            .collect())
    }

    /// Whether any instance's client manifest serves this blob. Used to gate
    /// downloads so the store cannot be probed for arbitrary content.
    pub async fn is_client_blob(&self, instance_id: &str, sha256: &str) -> Result<bool> {
        let rows = sqlx::query(&format!(
            "{SELECT_FILES} WHERE f.instance_id = ? AND f.sha256 = ?"
        ))
        .bind(instance_id)
        .bind(sha256)
        .fetch_all(&self.db)
        .await?;
        for row in &rows {
            if record_from_row(row)?.side.on_client() {
                return Ok(true);
            }
        }
        self.is_icon(instance_id, sha256).await
    }

    /// Whether the blob is the picture of one of the instance's mods.
    pub async fn is_icon(&self, instance_id: &str, sha256: &str) -> Result<bool> {
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM files WHERE instance_id = ? AND icon_sha256 = ? LIMIT 1",
        )
        .bind(instance_id)
        .bind(sha256)
        .fetch_optional(&self.db)
        .await?;
        Ok(found.is_some())
    }

    /// Stores a sanitised texture and points the account's skin or cape at
    /// it. Returns the texture's hash.
    pub async fn set_texture(
        &self,
        user_id: Uuid,
        kind: TextureKind,
        png: &[u8],
        slim: bool,
    ) -> Result<String> {
        // Held until the account references the blob, so a sweep cannot
        // take it away in between.
        let _ingesting = self.gc_lock.read().await;
        let blob = self.blobs.put_bytes(png).await?;
        let mut tx = self.db.begin().await?;
        sqlx::query("INSERT OR IGNORE INTO blobs (sha256, size, created_at) VALUES (?, ?, ?)")
            .bind(&blob.sha256)
            .bind(blob.size as i64)
            .bind(now())
            .execute(&mut *tx)
            .await?;
        let updated = match kind {
            TextureKind::Skin => {
                sqlx::query("UPDATE users SET skin_sha256 = ?, skin_model = ? WHERE id = ?")
                    .bind(&blob.sha256)
                    .bind(if slim { "slim" } else { "classic" })
            }
            TextureKind::Cape => {
                sqlx::query("UPDATE users SET cape_sha256 = ? WHERE id = ?").bind(&blob.sha256)
            }
        }
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(Error::NotFound("user"));
        }
        tx.commit().await?;
        Ok(blob.sha256)
    }

    /// Deletes blobs no file row references any more. Returns how many.
    pub async fn collect_garbage(&self) -> Result<u64> {
        let _sweeping = self.gc_lock.write().await;
        let orphans: Vec<String> = sqlx::query_scalar(
            "SELECT sha256 FROM blobs
             WHERE sha256 NOT IN (SELECT sha256 FROM files)
               AND sha256 NOT IN (SELECT icon_sha256 FROM files WHERE icon_sha256 IS NOT NULL)
               AND sha256 NOT IN (SELECT skin_sha256 FROM users WHERE skin_sha256 IS NOT NULL)
               AND sha256 NOT IN (SELECT cape_sha256 FROM users WHERE cape_sha256 IS NOT NULL)",
        )
        .fetch_all(&self.db)
        .await?;
        let mut removed = 0;
        for sha256 in orphans {
            // Row first: a crash in between leaves a stray file, never a row
            // pointing at missing bytes.
            sqlx::query("DELETE FROM blobs WHERE sha256 = ?")
                .bind(&sha256)
                .execute(&self.db)
                .await?;
            self.blobs.remove(&sha256).await?;
            removed += 1;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests;
