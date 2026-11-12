//! World backups: `tar` + `zstd` archives of an instance's world directories.

use std::collections::HashSet;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use relic_proto::InstanceStatus;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::deploy;
use crate::error::{Error, Result, now};
use crate::instances;
use crate::settings;
use crate::supervisor::Supervisor;

const EXTENSION: &str = ".tar.zst";
const SAVE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupInfo {
    /// Unix seconds of creation, as a string; also the file stem.
    pub id: String,
    pub created_at: i64,
    pub size: u64,
}

#[derive(Clone)]
pub struct Backups {
    db: SqlitePool,
    supervisor: Supervisor,
    root: PathBuf,
    /// Instances with a backup or restore in progress.
    busy: Arc<Mutex<HashSet<String>>>,
}

struct Busy {
    set: Arc<Mutex<HashSet<String>>>,
    id: String,
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.set.lock().expect("busy lock").remove(&self.id);
    }
}

fn valid_backup_id(id: &str) -> bool {
    (1..=12).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

/// The directories that hold the world: `level-name` and, on Bukkit-style
/// servers, its `_nether` and `_the_end` siblings.
async fn world_dirs(dir: &Path) -> Result<Vec<String>> {
    let level = deploy::get_property(&dir.join("server.properties"), "level-name")
        .await
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "world".to_owned());
    let mut components = Path::new(&level).components();
    let simple = matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    );
    if !simple {
        return Err(Error::invalid(
            "level-name must be a plain directory name to be backed up",
        ));
    }
    let mut found = Vec::new();
    for name in [
        level.clone(),
        format!("{level}_nether"),
        format!("{level}_the_end"),
    ] {
        if tokio::fs::metadata(dir.join(&name))
            .await
            .is_ok_and(|m| m.is_dir())
        {
            found.push(name);
        }
    }
    Ok(found)
}

/// Adds `source` to the archive under `name`, recursively. Files that vanish
/// or cannot be read mid-walk are skipped: the server may still be running.
fn append_dir(
    builder: &mut tar::Builder<impl std::io::Write>,
    source: &Path,
    name: &Path,
) -> std::io::Result<()> {
    builder.append_dir(name, source)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let inner = name.join(entry.file_name());
        if kind.is_dir() {
            append_dir(builder, &path, &inner)?;
        } else if kind.is_file() && entry.file_name() != "session.lock" {
            // The server holds `session.lock` locked; it is recreated anyway.
            match File::open(&path) {
                Ok(mut file) => builder.append_file(&inner, &mut file)?,
                Err(err) => tracing::warn!(path = %path.display(), %err, "skipped in backup"),
            }
        }
    }
    Ok(())
}

fn write_archive(dir: &Path, worlds: &[String], dest: &Path) -> std::io::Result<()> {
    let encoder = zstd::Encoder::new(File::create(dest)?, 3)?;
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);
    for world in worlds {
        append_dir(&mut builder, &dir.join(world), Path::new(world))?;
    }
    builder.into_inner()?.finish()?.sync_all()
}

impl Backups {
    pub fn new(db: SqlitePool, supervisor: Supervisor, root: PathBuf) -> Self {
        Self {
            db,
            supervisor,
            root,
            busy: Arc::default(),
        }
    }

    fn claim(&self, instance_id: &str) -> Result<Busy> {
        if !self
            .busy
            .lock()
            .expect("busy lock")
            .insert(instance_id.to_owned())
        {
            return Err(Error::Conflict(
                "a backup or restore is already running for this instance".into(),
            ));
        }
        Ok(Busy {
            set: self.busy.clone(),
            id: instance_id.to_owned(),
        })
    }

    /// The archive of a backup, for downloads.
    pub fn path(&self, instance_id: &str, backup_id: &str) -> Result<PathBuf> {
        instances::validate_id(instance_id)?;
        if !valid_backup_id(backup_id) {
            return Err(Error::NotFound("backup"));
        }
        Ok(self
            .root
            .join(instance_id)
            .join(format!("{backup_id}{EXTENSION}")))
    }

    /// Newest first.
    pub async fn list(&self, instance_id: &str) -> Result<Vec<BackupInfo>> {
        instances::validate_id(instance_id)?;
        let mut backups = Vec::new();
        let mut entries = match tokio::fs::read_dir(self.root.join(instance_id)).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(backups),
            Err(err) => return Err(err.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(EXTENSION)) else {
                continue;
            };
            if !valid_backup_id(id) {
                continue;
            }
            backups.push(BackupInfo {
                id: id.to_owned(),
                created_at: id.parse().unwrap_or_default(),
                size: entry.metadata().await?.len(),
            });
        }
        backups.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(backups)
    }

    /// Asks a running server to write everything to disk and to stop saving
    /// until `save-on`. Returns whether saving was switched off.
    async fn pause_saving(&self, instance_id: &str) -> bool {
        if self.supervisor.info(instance_id).status != InstanceStatus::Running {
            return false;
        }
        let (_, mut lines) = self.supervisor.console(instance_id, u64::MAX);
        if self
            .supervisor
            .command(instance_id, "save-off")
            .await
            .is_err()
            || self
                .supervisor
                .command(instance_id, "save-all flush")
                .await
                .is_err()
        {
            return true;
        }
        // "Saved the game" (1.13+) or "Saved the world" (1.12). Chat lines
        // carry the sender between `]: ` and the text, so they cannot match.
        let saved = async {
            loop {
                match lines.recv().await {
                    Ok(line) if !line.system && line.text.contains("]: Saved the ") => break,
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        };
        if tokio::time::timeout(SAVE_TIMEOUT, saved).await.is_err() {
            tracing::warn!(
                instance = instance_id,
                "no save confirmation, backing up anyway"
            );
        }
        true
    }

    pub async fn create(&self, instance_id: &str) -> Result<BackupInfo> {
        instances::get(&self.db, instance_id).await?;
        let _busy = self.claim(instance_id)?;
        let status = self.supervisor.info(instance_id).status;
        if matches!(
            status,
            InstanceStatus::Installing | InstanceStatus::Starting | InstanceStatus::Stopping
        ) {
            return Err(Error::Conflict(
                "wait until the instance has finished starting or stopping".into(),
            ));
        }
        let dir = self.supervisor.instance_dir(instance_id);
        let worlds = world_dirs(&dir).await?;
        if worlds.is_empty() {
            return Err(Error::Conflict("this instance has no world yet".into()));
        }

        let folder = self.root.join(instance_id);
        tokio::fs::create_dir_all(&folder).await?;
        let created_at = now();
        let dest = self.path(instance_id, &created_at.to_string())?;
        let part = folder.join(format!("{created_at}.part"));

        let paused = self.pause_saving(instance_id).await;
        let written = {
            let (part, dir) = (part.clone(), dir.clone());
            tokio::task::spawn_blocking(move || write_archive(&dir, &worlds, &part)).await
        };
        if paused {
            let _ = self.supervisor.command(instance_id, "save-on").await;
        }
        let written = written
            .map_err(|e| Error::Other(e.into()))
            .and_then(|r| Ok(r?));
        if let Err(err) = written {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(err);
        }
        let _ = tokio::fs::remove_file(&dest).await;
        tokio::fs::rename(&part, &dest).await?;
        Ok(BackupInfo {
            id: created_at.to_string(),
            created_at,
            size: tokio::fs::metadata(&dest).await?.len(),
        })
    }

    pub async fn delete(&self, instance_id: &str, backup_id: &str) -> Result<()> {
        match tokio::fs::remove_file(self.path(instance_id, backup_id)?).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(Error::NotFound("backup"))
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Replaces the instance's world with the one in the backup. The instance
    /// must be stopped. Each world directory is swapped in only after the
    /// whole archive has been unpacked.
    pub async fn restore(&self, instance_id: &str, backup_id: &str) -> Result<()> {
        instances::get(&self.db, instance_id).await?;
        let archive = self.path(instance_id, backup_id)?;
        if !tokio::fs::try_exists(&archive).await? {
            return Err(Error::NotFound("backup"));
        }
        let _busy = self.claim(instance_id)?;
        let status = self.supervisor.info(instance_id).status;
        if !matches!(status, InstanceStatus::Stopped | InstanceStatus::Crashed) {
            return Err(Error::Conflict(
                "stop the instance before restoring a backup".into(),
            ));
        }
        let dir = self.supervisor.instance_dir(instance_id);
        tokio::task::spawn_blocking(move || restore_archive(&archive, &dir))
            .await
            .map_err(|e| Error::Other(e.into()))?
    }

    /// Deletes all but the newest `keep` backups.
    pub async fn prune(&self, instance_id: &str, keep: usize) -> Result<usize> {
        let backups = self.list(instance_id).await?;
        let mut removed = 0;
        for old in backups.iter().skip(keep.max(1)) {
            if self.delete(instance_id, &old.id).await.is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Removes every backup of a deleted instance.
    pub async fn forget(&self, instance_id: &str) -> Result<()> {
        instances::validate_id(instance_id)?;
        match tokio::fs::remove_dir_all(self.root.join(instance_id)).await {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err.into()),
            _ => Ok(()),
        }
    }

    /// One pass of the schedule: backs up running instances whose newest
    /// backup is older than the configured interval, then applies retention.
    pub async fn run_schedule(&self) -> Result<()> {
        let general = settings::general(&self.db).await?;
        if general.backup_interval_hours == 0 {
            return Ok(());
        }
        let interval = i64::from(general.backup_interval_hours) * 3600;
        for instance in instances::list(&self.db).await? {
            if self.supervisor.info(&instance.id).status != InstanceStatus::Running {
                continue;
            }
            let newest = self
                .list(&instance.id)
                .await?
                .first()
                .map_or(0, |b| b.created_at);
            if now() - newest < interval {
                continue;
            }
            match self.create(&instance.id).await {
                Ok(backup) => {
                    tracing::info!(instance = %instance.id, size = backup.size, "scheduled backup");
                    self.prune(&instance.id, general.backup_keep as usize)
                        .await?;
                }
                Err(err) => {
                    tracing::warn!(instance = %instance.id, %err, "scheduled backup failed")
                }
            }
        }
        Ok(())
    }

    /// Runs the schedule until the task is dropped.
    pub fn spawn_scheduler(&self) -> tokio::task::JoinHandle<()> {
        let backups = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(300));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                if let Err(err) = backups.run_schedule().await {
                    tracing::warn!(%err, "backup schedule failed");
                }
            }
        })
    }
}

fn restore_archive(archive: &Path, dir: &Path) -> Result<()> {
    let staging = dir.join(".relic-restore");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let result = (|| -> Result<()> {
        // `unpack` refuses entries that would land outside `staging`.
        tar::Archive::new(zstd::Decoder::new(File::open(archive)?)?).unpack(&staging)?;
        for entry in std::fs::read_dir(&staging)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let live = dir.join(entry.file_name());
            let old = dir.join(format!(
                ".relic-old-{}",
                entry.file_name().to_string_lossy()
            ));
            let _ = std::fs::remove_dir_all(&old);
            let had_live = match std::fs::rename(&live, &old) {
                Ok(()) => true,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
                Err(err) => return Err(err.into()),
            };
            if let Err(err) = std::fs::rename(entry.path(), &live) {
                if had_live {
                    let _ = std::fs::rename(&old, &live);
                }
                return Err(err.into());
            }
            let _ = std::fs::remove_dir_all(&old);
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

#[cfg(test)]
mod tests {
    use relic_proto::{Loader, LoaderKind};

    use super::*;
    use crate::blobs::BlobStore;
    use crate::instances::NewInstance;
    use crate::modpack::Modpack;
    use crate::supervisor::Paths;

    #[tokio::test]
    async fn backup_restore_and_retention() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::db::open_in_memory().await.unwrap();
        let blobs = BlobStore::open(tmp.path().join("blobs")).await.unwrap();
        instances::create(
            &db,
            NewInstance {
                id: "main".into(),
                name: "Main".into(),
                description: String::new(),
                minecraft_version: "1.21.1".into(),
                loader: Loader {
                    kind: LoaderKind::Vanilla,
                    version: String::new(),
                },
                java_major: 21,
                port: 25565,
                memory_mb: 1024,
            },
        )
        .await
        .unwrap();
        let supervisor = Supervisor::new(
            db.clone(),
            Modpack::new(db.clone(), blobs),
            relic_mc::http::client(),
            Paths {
                instances: tmp.path().join("instances"),
                runtimes: tmp.path().join("runtimes"),
            },
        );
        let backups = Backups::new(db, supervisor.clone(), tmp.path().join("backups"));
        let dir = supervisor.instance_dir("main");

        // No world yet.
        assert!(matches!(
            backups.create("main").await,
            Err(Error::Conflict(_))
        ));

        std::fs::create_dir_all(dir.join("world/region")).unwrap();
        std::fs::write(dir.join("world/level.dat"), b"v1").unwrap();
        std::fs::write(dir.join("world/region/r.0.0.mca"), b"chunks").unwrap();
        std::fs::write(dir.join("world/session.lock"), b"lock").unwrap();
        std::fs::write(dir.join("server.properties"), "level-name=world\n").unwrap();

        let backup = backups.create("main").await.unwrap();
        assert_eq!(backups.list("main").await.unwrap(), vec![backup.clone()]);

        std::fs::write(dir.join("world/level.dat"), b"v2").unwrap();
        std::fs::write(dir.join("world/new-file"), b"x").unwrap();
        backups.restore("main", &backup.id).await.unwrap();
        assert_eq!(std::fs::read(dir.join("world/level.dat")).unwrap(), b"v1");
        assert_eq!(
            std::fs::read(dir.join("world/region/r.0.0.mca")).unwrap(),
            b"chunks"
        );
        assert!(!dir.join("world/new-file").exists());
        assert!(!dir.join("world/session.lock").exists());
        assert!(!dir.join(".relic-restore").exists());
        assert!(dir.join("server.properties").is_file());

        // Retention keeps the newest archives.
        let folder = tmp.path().join("backups/main");
        for id in ["100", "200", "300"] {
            std::fs::copy(
                backups.path("main", &backup.id).unwrap(),
                folder.join(format!("{id}.tar.zst")),
            )
            .unwrap();
        }
        assert_eq!(backups.prune("main", 2).await.unwrap(), 2);
        let left: Vec<String> = backups
            .list("main")
            .await
            .unwrap()
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(left, vec![backup.id.clone(), "300".to_owned()]);

        assert!(backups.path("main", "../x").is_err());
        assert!(matches!(
            backups.restore("main", "999").await,
            Err(Error::NotFound(_))
        ));
    }
}
