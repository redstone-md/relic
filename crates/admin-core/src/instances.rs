//! Minecraft server instances as stored in the database. Process control
//! lives in the supervisor; this module is only the persistent definition.

use relic_proto::{LOCKABLE_DIRS, Loader, LoaderKind, ModPolicy, Scene};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};
use uuid::Uuid;

use crate::error::{Error, Result, now};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub description: String,
    pub minecraft_version: String,
    pub loader: Loader,
    pub java_major: u32,
    pub port: u16,
    pub memory_mb: u32,
    pub mod_policy: ModPolicy,
    pub auto_start: bool,
    pub auto_restart: bool,
    /// See [`relic_proto::InstanceInfo::banner_url`].
    pub banner_url: Option<String>,
    pub scene: Scene,
    /// Folders of the player's game where only the pack's files may live.
    pub locked_dirs: Vec<String>,
    pub build_id: Uuid,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewInstance {
    pub id: String,
    pub name: String,
    pub description: String,
    pub minecraft_version: String,
    pub loader: Loader,
    pub java_major: u32,
    pub port: u16,
    pub memory_mb: u32,
}

/// Instance ids end up in URLs and directory names: short lowercase slugs.
pub fn validate_id(id: &str) -> Result<()> {
    let ok = (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(Error::invalid(
            "instance id must be 1-32 characters of a-z, 0-9 and '-', not starting or ending with '-'",
        ))
    }
}

fn from_row(row: &SqliteRow) -> Result<Instance> {
    let corrupt = |what: &str| Error::Other(anyhow::anyhow!("instances row has invalid {what}"));
    let loader_kind: String = row.try_get("loader_kind")?;
    let mod_policy: String = row.try_get("mod_policy")?;
    let build_id: String = row.try_get("build_id")?;
    Ok(Instance {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        minecraft_version: row.try_get("minecraft_version")?,
        loader: Loader {
            kind: LoaderKind::parse(&loader_kind).ok_or_else(|| corrupt("loader_kind"))?,
            version: row.try_get("loader_version")?,
        },
        java_major: row.try_get::<u32, _>("java_major")?,
        port: row.try_get::<u16, _>("port")?,
        memory_mb: row.try_get::<u32, _>("memory_mb")?,
        mod_policy: ModPolicy::parse(&mod_policy).ok_or_else(|| corrupt("mod_policy"))?,
        auto_start: row.try_get("auto_start")?,
        auto_restart: row.try_get("auto_restart")?,
        banner_url: row.try_get("banner_url")?,
        scene: Scene::parse(&row.try_get::<String, _>("scene")?).unwrap_or_default(),
        locked_dirs: serde_json::from_str(&row.try_get::<String, _>("locked_dirs")?)
            .unwrap_or_default(),
        build_id: Uuid::parse_str(&build_id).map_err(|_| corrupt("build_id"))?,
        created_at: row.try_get("created_at")?,
    })
}

pub async fn create(db: &SqlitePool, new: NewInstance) -> Result<Instance> {
    validate_id(&new.id)?;
    if new.name.trim().is_empty() {
        return Err(Error::invalid("instance name must not be empty"));
    }
    if new.minecraft_version.trim().is_empty() {
        return Err(Error::invalid("minecraft version must not be empty"));
    }
    if new.port < 1024 {
        return Err(Error::invalid("port must be 1024 or higher"));
    }
    if new.memory_mb < 512 {
        return Err(Error::invalid("memory must be at least 512 MB"));
    }

    let result = sqlx::query(
        "INSERT INTO instances
            (id, name, description, minecraft_version, loader_kind, loader_version,
             java_major, port, memory_mb, build_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&new.id)
    .bind(new.name.trim())
    .bind(&new.description)
    .bind(new.minecraft_version.trim())
    .bind(new.loader.kind.as_str())
    .bind(&new.loader.version)
    .bind(new.java_major)
    .bind(new.port)
    .bind(new.memory_mb)
    .bind(Uuid::new_v4().to_string())
    .bind(now())
    .execute(db)
    .await;

    match result {
        Ok(_) => get(db, &new.id).await,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(Error::Conflict(
            "an instance with this id or port already exists".to_owned(),
        )),
        Err(e) => Err(e.into()),
    }
}

pub async fn get(db: &SqlitePool, id: &str) -> Result<Instance> {
    let row = sqlx::query("SELECT * FROM instances WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(Error::NotFound("instance"))?;
    from_row(&row)
}

pub async fn list(db: &SqlitePool) -> Result<Vec<Instance>> {
    sqlx::query("SELECT * FROM instances ORDER BY created_at, id")
        .fetch_all(db)
        .await?
        .iter()
        .map(from_row)
        .collect()
}

pub async fn set_mod_policy(db: &SqlitePool, id: &str, policy: ModPolicy) -> Result<()> {
    let done = sqlx::query("UPDATE instances SET mod_policy = ?, build_id = ? WHERE id = ?")
        .bind(policy.as_str())
        .bind(Uuid::new_v4().to_string())
        .bind(id)
        .execute(db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(Error::NotFound("instance"));
    }
    Ok(())
}

/// Fields an admin may change after creation; `None` leaves a field as is.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct InstanceUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub minecraft_version: Option<String>,
    pub loader: Option<Loader>,
    pub java_major: Option<u32>,
    pub port: Option<u16>,
    pub memory_mb: Option<u32>,
    pub mod_policy: Option<ModPolicy>,
    pub auto_start: Option<bool>,
    pub auto_restart: Option<bool>,
    /// An empty string takes the banner away.
    pub banner_url: Option<String>,
    pub scene: Option<Scene>,
    /// Any of [`LOCKABLE_DIRS`].
    pub locked_dirs: Option<Vec<String>>,
}

pub async fn update(db: &SqlitePool, id: &str, update: InstanceUpdate) -> Result<Instance> {
    let current = get(db, id).await?;
    let name = update.name.unwrap_or(current.name.clone());
    if name.trim().is_empty() {
        return Err(Error::invalid("instance name must not be empty"));
    }
    let port = update.port.unwrap_or(current.port);
    if port < 1024 {
        return Err(Error::invalid("port must be 1024 or higher"));
    }
    let memory_mb = update.memory_mb.unwrap_or(current.memory_mb);
    if memory_mb < 512 {
        return Err(Error::invalid("memory must be at least 512 MB"));
    }
    let minecraft_version = update
        .minecraft_version
        .unwrap_or(current.minecraft_version.clone());
    let loader = update.loader.unwrap_or(current.loader.clone());
    let mod_policy = update.mod_policy.unwrap_or(current.mod_policy);
    let mut locked_dirs = update.locked_dirs.unwrap_or(current.locked_dirs.clone());
    if locked_dirs
        .iter()
        .any(|dir| !LOCKABLE_DIRS.contains(&dir.as_str()))
    {
        return Err(Error::invalid(
            "only the resourcepacks and shaderpacks folders can be locked",
        ));
    }
    locked_dirs.sort();
    locked_dirs.dedup();
    // Anything a launcher acts on invalidates its cached manifest.
    let client_visible = minecraft_version != current.minecraft_version
        || loader != current.loader
        || mod_policy != current.mod_policy
        || locked_dirs != current.locked_dirs;
    let build_id = if client_visible {
        Uuid::new_v4()
    } else {
        current.build_id
    };

    let banner_url = match update.banner_url {
        Some(url) => Some(url.trim().to_owned()).filter(|url| !url.is_empty()),
        None => current.banner_url.clone(),
    };
    if banner_url.as_ref().is_some_and(|url| url.len() > 2048) {
        return Err(Error::invalid("the banner address is too long"));
    }
    if banner_url
        .as_ref()
        .is_some_and(|url| !(url.starts_with("https://") || url.starts_with("/api/v1/media/")))
    {
        return Err(Error::invalid(
            "the banner must be an uploaded file or an address starting with https://",
        ));
    }

    let result = sqlx::query(
        "UPDATE instances SET
            name = ?, description = ?, minecraft_version = ?, loader_kind = ?,
            loader_version = ?, java_major = ?, port = ?, memory_mb = ?, mod_policy = ?,
            auto_start = ?, auto_restart = ?, banner_url = ?, scene = ?, locked_dirs = ?,
            build_id = ?
         WHERE id = ?",
    )
    .bind(name.trim())
    .bind(update.description.unwrap_or(current.description))
    .bind(minecraft_version.trim())
    .bind(loader.kind.as_str())
    .bind(&loader.version)
    .bind(update.java_major.unwrap_or(current.java_major))
    .bind(port)
    .bind(memory_mb)
    .bind(mod_policy.as_str())
    .bind(update.auto_start.unwrap_or(current.auto_start))
    .bind(update.auto_restart.unwrap_or(current.auto_restart))
    .bind(banner_url)
    .bind(update.scene.unwrap_or(current.scene).as_str())
    .bind(serde_json::to_string(&locked_dirs).unwrap_or_else(|_| "[]".to_owned()))
    .bind(build_id.to_string())
    .bind(id)
    .execute(db)
    .await;
    match result {
        Ok(_) => get(db, id).await,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(Error::Conflict(
            "another instance already uses this port".to_owned(),
        )),
        Err(e) => Err(e.into()),
    }
}

/// Deletes the definition and its file index. Blobs and the server directory
/// are left for the caller to clean up.
pub async fn delete(db: &SqlitePool, id: &str) -> Result<()> {
    let done = sqlx::query("DELETE FROM instances WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(Error::NotFound("instance"));
    }
    Ok(())
}
