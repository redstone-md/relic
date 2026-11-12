//! Key-value settings stored in the database.

use relic_proto::{Link, RegistrationPolicy, Theme};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sqlx::SqlitePool;

use crate::error::{Error, Result};

pub const SERVER_NAME: &str = "server_name";
pub const PUBLIC_HOST: &str = "public_host";
pub const REGISTRATION: &str = "registration";
pub const THEME: &str = "theme";
pub const LINKS: &str = "links";
pub const CURSEFORGE_API_KEY: &str = "curseforge_api_key";
pub const BACKUP_INTERVAL_HOURS: &str = "backup_interval_hours";
pub const BACKUP_KEEP: &str = "backup_keep";
pub const SETUP_TOKEN_HASH: &str = "setup_token_hash";

pub async fn get(db: &SqlitePool, key: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(db)
            .await?,
    )
}

pub async fn set(db: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn remove(db: &SqlitePool, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(db)
        .await?;
    Ok(())
}

/// A JSON-encoded setting; a missing or unreadable value yields the default.
pub async fn get_json<T: DeserializeOwned + Default>(db: &SqlitePool, key: &str) -> Result<T> {
    Ok(get(db, key)
        .await?
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default())
}

pub async fn set_json<T: Serialize>(db: &SqlitePool, key: &str, value: &T) -> Result<()> {
    let json = serde_json::to_string(value).map_err(|e| Error::Other(e.into()))?;
    set(db, key, &json).await
}

/// The settings an admin edits as one form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct General {
    pub server_name: String,
    /// Host name players use for the game servers; empty to let the launcher
    /// reuse the host it reached the API on.
    pub public_host: String,
    pub registration: RegistrationPolicy,
    pub theme: Theme,
    pub links: Vec<Link>,
    /// 0 disables scheduled backups.
    pub backup_interval_hours: u32,
    pub backup_keep: u32,
}

pub const DEFAULT_BACKUP_KEEP: u32 = 10;

pub async fn general(db: &SqlitePool) -> Result<General> {
    let number =
        |v: Option<String>, default: u32| v.and_then(|v| v.parse().ok()).unwrap_or(default);
    Ok(General {
        server_name: get(db, SERVER_NAME)
            .await?
            .unwrap_or_else(|| "Relic".to_owned()),
        public_host: get(db, PUBLIC_HOST).await?.unwrap_or_default(),
        registration: get(db, REGISTRATION)
            .await?
            .and_then(|v| RegistrationPolicy::parse(&v))
            .unwrap_or_default(),
        theme: get_json(db, THEME).await?,
        links: get_json(db, LINKS).await?,
        backup_interval_hours: number(get(db, BACKUP_INTERVAL_HOURS).await?, 0),
        backup_keep: number(get(db, BACKUP_KEEP).await?, DEFAULT_BACKUP_KEEP),
    })
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_web_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://") || value.starts_with('/')
}

pub async fn set_general(db: &SqlitePool, general: &General) -> Result<()> {
    let name = general.server_name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(Error::invalid("server name must be 1-64 characters"));
    }
    let palette = &general.theme.palette;
    for color in [
        &palette.background,
        &palette.surface,
        &palette.border,
        &palette.accent,
        &palette.text_primary,
        &palette.text_secondary,
    ] {
        if !is_hex_color(color) {
            return Err(Error::invalid("palette colours must look like #rrggbb"));
        }
    }
    let theme = &general.theme;
    let urls = [&theme.font_url, &theme.logo_url, &theme.background_url];
    if urls.into_iter().flatten().any(|u| !is_web_url(u)) {
        return Err(Error::invalid(
            "theme URLs must be http(s) or site-relative",
        ));
    }
    if general.links.len() > 12 {
        return Err(Error::invalid("at most 12 links"));
    }
    for link in &general.links {
        // Launchers open these in a browser; never hand them other schemes.
        if !(link.url.starts_with("https://") || link.url.starts_with("http://")) {
            return Err(Error::invalid(
                "link URLs must start with http:// or https://",
            ));
        }
        if link.label.trim().is_empty() {
            return Err(Error::invalid("links need a label"));
        }
    }
    if general.backup_keep == 0 {
        return Err(Error::invalid("keep at least one backup"));
    }

    set(db, SERVER_NAME, name).await?;
    set(db, PUBLIC_HOST, general.public_host.trim()).await?;
    set(db, REGISTRATION, general.registration.as_str()).await?;
    set_json(db, THEME, &general.theme).await?;
    set_json(db, LINKS, &general.links).await?;
    set(
        db,
        BACKUP_INTERVAL_HOURS,
        &general.backup_interval_hours.to_string(),
    )
    .await?;
    set(db, BACKUP_KEEP, &general.backup_keep.to_string()).await?;
    Ok(())
}
