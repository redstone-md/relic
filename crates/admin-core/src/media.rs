//! Pictures and fonts the admin uploads for the launcher to show: the logo,
//! the wallpaper, news images, modpack banners.
//!
//! A file is named after its SHA-256 and its kind, which is read from the
//! bytes themselves and never from what the browser claims.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::fs;

use crate::error::{Error, Result};

/// The launcher refuses anything larger, so storing it would be pointless.
pub const MAX_UPLOAD: usize = 16 * 1024 * 1024;

/// The extension for what `bytes` are, if it is something a launcher can use.
fn kind(bytes: &[u8]) -> Option<&'static str> {
    Some(match bytes {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [0xff, 0xd8, 0xff, ..] => "jpg",
        [b'G', b'I', b'F', b'8', ..] => "gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "webp",
        [0, 1, 0, 0, ..] | [b't', b'r', b'u', b'e', ..] => "ttf",
        [b'O', b'T', b'T', b'O', ..] => "otf",
        _ => return None,
    })
}

pub fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("png") => "image/png",
        Some("jpg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => "application/octet-stream",
    }
}

/// Whether `name` is one this module could have given a file.
fn is_name(name: &str) -> bool {
    let Some((hash, ext)) = name.split_once('.') else {
        return false;
    };
    crate::blobs::is_sha256(hash) && matches!(ext, "png" | "jpg" | "gif" | "webp" | "ttf" | "otf")
}

/// Stores `bytes` and returns the name to ask for them by.
pub async fn put(dir: &Path, bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_UPLOAD {
        return Err(Error::invalid("the file is larger than 16 MB"));
    }
    let Some(ext) = kind(bytes) else {
        return Err(Error::invalid(
            "the file is not a PNG, JPEG, GIF or WebP image, nor a TTF or OTF font",
        ));
    };
    let name = format!("{}.{ext}", hex::encode(Sha256::digest(bytes)));
    fs::create_dir_all(dir).await?;
    let path = dir.join(&name);
    if !fs::try_exists(&path).await? {
        let temp = dir.join(format!("{name}.{}", uuid::Uuid::new_v4().simple()));
        fs::write(&temp, bytes).await?;
        if let Err(err) = fs::rename(&temp, &path).await {
            let _ = fs::remove_file(&temp).await;
            // Someone uploading the same file at the same moment got there first.
            if !fs::try_exists(&path).await? {
                return Err(err.into());
            }
        }
    }
    Ok(name)
}

/// Where the file called `name` is, if there is one.
pub async fn path(dir: &Path, name: &str) -> Result<PathBuf> {
    if !is_name(name) {
        return Err(Error::NotFound("file"));
    }
    let path = dir.join(name);
    if fs::try_exists(&path).await? {
        Ok(path)
    } else {
        Err(Error::NotFound("file"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keeps_images_by_content_and_refuses_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let png = [0x89, b'P', b'N', b'G', 1, 2, 3];
        let name = put(dir.path(), &png).await.unwrap();
        assert!(name.ends_with(".png"));
        assert_eq!(put(dir.path(), &png).await.unwrap(), name);
        assert_eq!(
            std::fs::read(path(dir.path(), &name).await.unwrap()).unwrap(),
            png
        );
        assert_eq!(mime(&name), "image/png");

        assert!(put(dir.path(), b"<html>").await.is_err());
        assert!(path(dir.path(), "../relic.db").await.is_err());
        assert!(path(dir.path(), &name.replace("png", "gif")).await.is_err());
    }
}
