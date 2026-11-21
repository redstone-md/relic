use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::Result;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Where the database file is, for error messages.
static LOCATION: OnceLock<PathBuf> = OnceLock::new();

/// SQLite reports a file it cannot reach without naming it or saying what is
/// wrong with it. This adds both to errors about the file itself: cannot
/// open, read-only, disk full, i/o failure, corruption.
pub fn explain(err: &sqlx::Error) -> String {
    let Some(path) = LOCATION.get() else {
        return String::new();
    };
    let file_level = match err {
        sqlx::Error::Io(_) => true,
        sqlx::Error::Database(db) => {
            let code = db.code().and_then(|code| code.parse::<u32>().ok());
            // Extended codes keep the primary code in the low byte.
            matches!(
                code.map(|code| code & 0xff),
                Some(8 | 10 | 11 | 13 | 14 | 26)
            )
        }
        _ => false,
    };
    if !file_level {
        return String::new();
    }
    let state = |path: &Path| match std::fs::metadata(path) {
        Ok(meta) if meta.permissions().readonly() => "exists, read-only".to_owned(),
        Ok(_) => "exists".to_owned(),
        Err(err) => format!("cannot be read: {err}"),
    };
    let folder = path.parent().unwrap_or(Path::new("."));
    format!(
        " [database file {}: {}; its folder {}: {}]",
        path.display(),
        state(path),
        folder.display(),
        state(folder),
    )
}

/// Opens (creating if needed) the database file and applies migrations.
pub async fn open(path: &Path) -> Result<SqlitePool> {
    let _ = LOCATION.set(path.to_path_buf());
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// Private in-memory database for tests. A single connection, because every
/// connection to `:memory:` would otherwise get its own empty database.
pub async fn open_in_memory() -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
