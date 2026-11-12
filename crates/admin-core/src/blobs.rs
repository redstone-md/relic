//! Content-addressed file storage: `<root>/<first two hex chars>/<sha256>`.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBlob {
    pub sha256: String,
    pub size: u64,
    /// The content was already present; nothing new was written.
    pub existed: bool,
}

#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
    tmp: PathBuf,
}

/// True for a lowercase 64-character hex string.
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl BlobStore {
    pub async fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let tmp = root.join("tmp");
        fs::create_dir_all(&tmp).await?;
        // Uploads interrupted by a previous shutdown leave partial files here.
        let mut entries = fs::read_dir(&tmp).await?;
        while let Some(entry) = entries.next_entry().await? {
            let _ = fs::remove_file(entry.path()).await;
        }
        Ok(Self { root, tmp })
    }

    /// On-disk location of a blob. Rejects anything that is not a hash, so a
    /// caller-supplied id can never escape the store.
    pub fn path(&self, sha256: &str) -> Result<PathBuf> {
        if !is_sha256(sha256) {
            return Err(Error::invalid(
                "blob id must be a lowercase sha256 hex digest",
            ));
        }
        Ok(self.root.join(&sha256[..2]).join(sha256))
    }

    pub async fn exists(&self, sha256: &str) -> Result<bool> {
        Ok(fs::try_exists(self.path(sha256)?).await?)
    }

    /// Streams `reader` into the store, hashing as it goes. `max_bytes`
    /// bounds the upload; exceeding it aborts and leaves nothing behind.
    pub async fn put<R: AsyncRead + Unpin>(
        &self,
        mut reader: R,
        max_bytes: Option<u64>,
    ) -> Result<StoredBlob> {
        let tmp_path = self.tmp.join(Uuid::new_v4().simple().to_string());
        let result = async {
            let mut file = fs::File::create(&tmp_path).await?;
            let mut hasher = Sha256::new();
            let mut size: u64 = 0;
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                size += n as u64;
                if max_bytes.is_some_and(|max| size > max) {
                    return Err(Error::invalid("file exceeds the upload size limit"));
                }
                hasher.update(&buf[..n]);
                file.write_all(&buf[..n]).await?;
            }
            file.sync_all().await?;
            Ok((hex::encode(hasher.finalize()), size))
        }
        .await;

        let (sha256, size) = match result {
            Ok(done) => done,
            Err(err) => {
                let _ = fs::remove_file(&tmp_path).await;
                return Err(err);
            }
        };

        let dest = self.path(&sha256)?;
        if fs::try_exists(&dest).await? {
            fs::remove_file(&tmp_path).await?;
            return Ok(StoredBlob {
                sha256,
                size,
                existed: true,
            });
        }
        fs::create_dir_all(dest.parent().expect("blob path has a parent")).await?;
        fs::rename(&tmp_path, &dest).await?;
        Ok(StoredBlob {
            sha256,
            size,
            existed: false,
        })
    }

    pub async fn put_bytes(&self, bytes: &[u8]) -> Result<StoredBlob> {
        self.put(bytes, None).await
    }

    pub async fn open_blob(&self, sha256: &str) -> Result<fs::File> {
        match fs::File::open(self.path(sha256)?).await {
            Ok(file) => Ok(file),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(Error::NotFound("blob")),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn remove(&self, sha256: &str) -> Result<()> {
        match fs::remove_file(self.path(sha256)?).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Materialises a blob at `dest`: a hard link when the filesystem allows
    /// it, otherwise a copy. Replaces whatever is at `dest`.
    pub async fn link_to(&self, sha256: &str, dest: &Path) -> Result<()> {
        let source = self.path(sha256)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).await?;
        }
        match fs::remove_file(dest).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
        if fs::hard_link(&source, dest).await.is_err() {
            fs::copy(&source, dest).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    #[tokio::test]
    async fn put_hashes_and_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).await.unwrap();

        let first = store.put_bytes(b"hello").await.unwrap();
        assert_eq!(first.sha256, HELLO_SHA256);
        assert_eq!(first.size, 5);
        assert!(!first.existed);

        let second = store.put_bytes(b"hello").await.unwrap();
        assert!(second.existed);
        assert_eq!(second.sha256, first.sha256);

        let on_disk = fs::read(store.path(HELLO_SHA256).unwrap()).await.unwrap();
        assert_eq!(on_disk, b"hello");
        assert_eq!(
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count(),
            0
        );
    }

    #[tokio::test]
    async fn oversized_upload_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).await.unwrap();

        let err = store.put(&b"hello"[..], Some(4)).await.unwrap_err();
        assert!(matches!(err, Error::Invalid(_)));
        assert!(!store.exists(HELLO_SHA256).await.unwrap());
        assert_eq!(
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count(),
            0
        );
    }

    #[tokio::test]
    async fn path_rejects_anything_that_is_not_a_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).await.unwrap();
        assert!(store.path("../../etc/passwd").is_err());
        assert!(store.path(&HELLO_SHA256.to_uppercase()).is_err());
        assert!(store.path(HELLO_SHA256).is_ok());
    }

    #[tokio::test]
    async fn link_to_replaces_the_destination() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path().join("blobs")).await.unwrap();
        let blob = store.put_bytes(b"hello").await.unwrap();

        let dest = dir.path().join("server/mods/a.jar");
        fs::create_dir_all(dest.parent().unwrap()).await.unwrap();
        fs::write(&dest, b"stale").await.unwrap();

        store.link_to(&blob.sha256, &dest).await.unwrap();
        assert_eq!(fs::read(&dest).await.unwrap(), b"hello");
    }
}
