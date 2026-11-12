//! News shown in the launcher.

use relic_proto::NewsItem;
use serde::Deserialize;
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::error::{Error, Result, now};

#[derive(Debug, Clone, Deserialize)]
pub struct NewsDraft {
    pub title: String,
    /// Markdown.
    pub body: String,
    #[serde(default)]
    pub image_url: Option<String>,
}

impl NewsDraft {
    fn validated(mut self) -> Result<Self> {
        self.title = self.title.trim().to_owned();
        if self.title.is_empty() || self.title.chars().count() > 120 {
            return Err(Error::invalid("title must be 1-120 characters"));
        }
        if self.body.len() > 20_000 {
            return Err(Error::invalid("body is too long"));
        }
        self.image_url = self.image_url.filter(|u| !u.trim().is_empty());
        if self
            .image_url
            .as_deref()
            .is_some_and(|u| !(u.starts_with("https://") || u.starts_with("/api/v1/media/")))
        {
            return Err(Error::invalid(
                "image must be an uploaded file or an address starting with https://",
            ));
        }
        Ok(self)
    }
}

fn from_row(row: &SqliteRow) -> Result<NewsItem> {
    Ok(NewsItem {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        body: row.try_get("body")?,
        published_at: row.try_get("published_at")?,
        image_url: row.try_get("image_url")?,
    })
}

/// Newest first.
pub async fn list(db: &SqlitePool, limit: u32) -> Result<Vec<NewsItem>> {
    sqlx::query("SELECT * FROM news ORDER BY published_at DESC, id DESC LIMIT ?")
        .bind(limit)
        .fetch_all(db)
        .await?
        .iter()
        .map(from_row)
        .collect()
}

pub async fn get(db: &SqlitePool, id: i64) -> Result<NewsItem> {
    let row = sqlx::query("SELECT * FROM news WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(Error::NotFound("news item"))?;
    from_row(&row)
}

pub async fn create(db: &SqlitePool, draft: NewsDraft) -> Result<NewsItem> {
    let draft = draft.validated()?;
    let done =
        sqlx::query("INSERT INTO news (title, body, image_url, published_at) VALUES (?, ?, ?, ?)")
            .bind(&draft.title)
            .bind(&draft.body)
            .bind(&draft.image_url)
            .bind(now())
            .execute(db)
            .await?;
    get(db, done.last_insert_rowid()).await
}

pub async fn update(db: &SqlitePool, id: i64, draft: NewsDraft) -> Result<NewsItem> {
    let draft = draft.validated()?;
    let done = sqlx::query("UPDATE news SET title = ?, body = ?, image_url = ? WHERE id = ?")
        .bind(&draft.title)
        .bind(&draft.body)
        .bind(&draft.image_url)
        .bind(id)
        .execute(db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(Error::NotFound("news item"));
    }
    get(db, id).await
}

pub async fn delete(db: &SqlitePool, id: i64) -> Result<()> {
    let done = sqlx::query("DELETE FROM news WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(Error::NotFound("news item"));
    }
    Ok(())
}
