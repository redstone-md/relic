//! The file manager: the directory of one instance, as the admin sees it.
//!
//! Every path the panel sends is relative to the instance directory and is
//! checked twice: by shape (no `..`, no drive or root) and, against links
//! planted inside the directory, by where it really leads.

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use tokio::fs;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::deploy;
use crate::error::{Error, Result};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    /// Bytes; zero for a directory.
    pub size: u64,
    /// Unix seconds.
    pub modified: i64,
    /// Whether the file was put here from the modpack, and will be put back
    /// in the pack's version when that changes.
    pub pack: bool,
}

/// `a/b\c` to its parts, refusing anything that is not a plain name.
fn parts(relative: &str) -> Result<Vec<&str>> {
    let mut out = Vec::new();
    for part in relative.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        let plain = matches!(
            Path::new(part).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        );
        if !plain || part.contains([':', '\0']) || part.ends_with([' ', '.']) {
            return Err(Error::invalid(format!("“{part}” is not a usable name")));
        }
        out.push(part);
    }
    Ok(out)
}

/// The real path of `relative` inside `root`, which must exist.
///
/// The deepest part of the path that exists is resolved through any links
/// and must still be inside `root`; what does not exist yet cannot be a link.
async fn resolve(root: &Path, relative: &str) -> Result<PathBuf> {
    let names = parts(relative)?;
    let root = fs::canonicalize(root).await?;
    let mut real = root.clone();
    let mut rest = names.iter();
    for name in rest.by_ref() {
        let next = real.join(name);
        match fs::canonicalize(&next).await {
            Ok(found) => real = found,
            Err(_) => {
                real = next;
                break;
            }
        }
        if !real.starts_with(&root) {
            return Err(Error::Forbidden(
                "the path leads outside the instance".into(),
            ));
        }
    }
    for name in rest {
        real.push(name);
    }
    Ok(real)
}

fn is_root(root: &Path, path: &Path) -> bool {
    std::fs::canonicalize(root).is_ok_and(|root| root == path)
}

pub async fn list(root: &Path, relative: &str) -> Result<Vec<Entry>> {
    fs::create_dir_all(root).await?;
    let dir = resolve(root, relative).await?;
    let mut reader = match fs::read_dir(&dir).await {
        Ok(reader) => reader,
        Err(_) => return Err(Error::NotFound("folder")),
    };
    let deployed = deploy::deployed(root).await;
    let prefix = parts(relative)?.join("/");
    let mut entries = Vec::new();
    while let Some(entry) = reader.next_entry().await? {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // What the daemon keeps for itself, and uploads still in flight.
        if name == deploy::STATE_FILE || name.starts_with(TEMP_PREFIX) {
            continue;
        }
        // Follows links, so a linked folder opens as a folder.
        let Ok(meta) = fs::metadata(entry.path()).await else {
            continue;
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |span| span.as_secs() as i64);
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        entries.push(Entry {
            dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified,
            pack: deployed.contains_key(&path),
            name,
        });
    }
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// The path of an existing file, for sending it.
pub async fn file(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = resolve(root, relative).await?;
    match fs::metadata(&path).await {
        Ok(meta) if meta.is_file() => Ok(path),
        _ => Err(Error::NotFound("file")),
    }
}

const TEMP_PREFIX: &str = ".relic-upload-";

/// Writes `content` to a file, replacing what was there.
///
/// The bytes land in a temporary file that then takes the name. A JAR from
/// the modpack is a hard link into the blob store, and writing through it
/// would change the pack for every player; replacing the name cannot.
pub async fn write(
    root: &Path,
    relative: &str,
    mut content: impl AsyncRead + Unpin,
    limit: u64,
) -> Result<()> {
    fs::create_dir_all(root).await?;
    let path = resolve(root, relative).await?;
    if is_root(root, &path) {
        return Err(Error::invalid("a file needs a name"));
    }
    if fs::metadata(&path).await.is_ok_and(|meta| meta.is_dir()) {
        return Err(Error::Conflict("a folder has this name".into()));
    }
    let parent = path.parent().expect("inside the root");
    fs::create_dir_all(parent).await?;
    let temp = parent.join(format!("{TEMP_PREFIX}{}", Uuid::new_v4().simple()));

    let outcome = async {
        let mut file = fs::File::create(&temp).await?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let read = content.read(&mut buf).await?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > limit {
                return Err(Error::invalid("the file is too large"));
            }
            file.write_all(&buf[..read]).await?;
        }
        file.flush().await?;
        drop(file);
        // Windows does not rename over an existing file that is open
        // elsewhere, nor over a read-only one; removing first covers both
        // the common cases and costs nothing on other systems.
        let _ = fs::remove_file(&path).await;
        fs::rename(&temp, &path).await?;
        Ok(())
    }
    .await;
    if outcome.is_err() {
        let _ = fs::remove_file(&temp).await;
    }
    outcome
}

pub async fn create_dir(root: &Path, relative: &str) -> Result<()> {
    fs::create_dir_all(root).await?;
    let path = resolve(root, relative).await?;
    if fs::try_exists(&path).await? {
        return Err(Error::Conflict("this name is taken".into()));
    }
    fs::create_dir_all(&path).await?;
    Ok(())
}

/// Renames or moves a file or folder; never over something that exists.
pub async fn rename(root: &Path, from: &str, to: &str) -> Result<()> {
    let source = resolve(root, from).await?;
    let target = resolve(root, to).await?;
    if is_root(root, &source) || is_root(root, &target) {
        return Err(Error::invalid(
            "the instance folder itself stays where it is",
        ));
    }
    if !fs::try_exists(&source).await? {
        return Err(Error::NotFound("file"));
    }
    if target.starts_with(&source) && target != source {
        return Err(Error::invalid("a folder cannot be moved into itself"));
    }
    // A change of letter case alone is the same file on Windows and macOS.
    let same = fs::canonicalize(&target)
        .await
        .is_ok_and(|real| real == source);
    if !same && fs::try_exists(&target).await? {
        return Err(Error::Conflict("this name is taken".into()));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::rename(&source, &target).await?;
    Ok(())
}

/// Deletes a file, or a folder with everything in it.
pub async fn remove(root: &Path, relative: &str) -> Result<()> {
    let path = resolve(root, relative).await?;
    if is_root(root, &path) {
        return Err(Error::invalid(
            "the instance folder itself cannot be deleted",
        ));
    }
    let meta = match fs::symlink_metadata(&path).await {
        Ok(meta) => meta,
        Err(_) => return Err(Error::NotFound("file")),
    };
    if meta.is_dir() {
        fs::remove_dir_all(&path).await?;
    } else {
        fs::remove_file(&path).await?;
    }
    Ok(())
}

/// How much of the disk an instance takes, and where it went.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Usage {
    pub total: u64,
    /// What sits directly in the instance folder, largest first. Loose
    /// files are counted together under an empty name.
    pub folders: Vec<Share>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Share {
    pub name: String,
    pub size: u64,
}

/// The bytes under `path`. Links are counted as links, not followed.
fn weigh(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return if meta.is_file() { meta.len() } else { 0 };
    }
    let Ok(reader) = std::fs::read_dir(path) else {
        return 0;
    };
    reader.flatten().map(|entry| weigh(&entry.path())).sum()
}

pub async fn usage(root: &Path) -> Result<Usage> {
    fs::create_dir_all(root).await?;
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut folders = Vec::new();
        let mut loose = 0u64;
        for entry in std::fs::read_dir(&root)?.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let path = entry.path();
            let is_dir = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir());
            let size = weigh(&path);
            if is_dir {
                folders.push(Share { name, size });
            } else {
                loose += size;
            }
        }
        if loose > 0 {
            folders.push(Share {
                name: String::new(),
                size: loose,
            });
        }
        folders.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        Ok(Usage {
            total: folders.iter().map(|share| share.size).sum(),
            folders,
        })
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e)))?
}

/// Packs a folder of the instance into a zip at `dest`, the folder itself
/// being the top entry. Links are left out.
pub async fn archive(root: &Path, relative: &str, dest: &Path) -> Result<String> {
    let dir = resolve(root, relative).await?;
    if is_root(root, &dir) || !fs::metadata(&dir).await.is_ok_and(|meta| meta.is_dir()) {
        return Err(Error::NotFound("folder"));
    }
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("folder")
        .to_owned();
    let (dest, top) = (dest.to_owned(), name.clone());
    tokio::task::spawn_blocking(move || -> Result<()> {
        use zip::write::SimpleFileOptions;
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&dest)?);
        let mut pending = vec![(dir, top)];
        while let Some((dir, prefix)) = pending.pop() {
            writer
                .add_directory(format!("{prefix}/"), SimpleFileOptions::default())
                .map_err(zip_error)?;
            for entry in std::fs::read_dir(&dir)?.flatten() {
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                let path = entry.path();
                let Ok(meta) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                let inner = format!("{prefix}/{name}");
                if meta.is_dir() {
                    pending.push((path, inner));
                } else if meta.is_file() {
                    // A running server keeps `session.lock` to itself.
                    let Ok(mut file) = std::fs::File::open(&path) else {
                        continue;
                    };
                    let options =
                        SimpleFileOptions::default().large_file(meta.len() >= u32::MAX as u64);
                    writer.start_file(inner, options).map_err(zip_error)?;
                    std::io::copy(&mut file, &mut writer)?;
                }
            }
        }
        writer.finish().map_err(zip_error)?;
        Ok(())
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e)))??;
    Ok(name)
}

fn zip_error(err: zip::result::ZipError) -> Error {
    Error::invalid(format!("the archive could not be handled: {err}"))
}

const UNPACK_MAX_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const UNPACK_MAX_ENTRIES: usize = 200_000;

/// Unpacks a zip into a new folder of the instance.
///
/// The folder must not exist yet. When everything in the archive sits in
/// one top folder, as a zipped world does, that folder is not repeated.
/// Returns how many files were written.
pub async fn unpack(root: &Path, relative: &str, archive: &Path) -> Result<usize> {
    fs::create_dir_all(root).await?;
    let target = resolve(root, relative).await?;
    if is_root(root, &target) {
        return Err(Error::invalid("the folder needs a name"));
    }
    if fs::try_exists(&target).await? {
        return Err(Error::Conflict("this name is taken".into()));
    }
    let archive = archive.to_owned();
    let made = target.clone();
    let outcome = tokio::task::spawn_blocking(move || -> Result<usize> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive)?)
            .map_err(|_| Error::invalid("this is not a zip archive"))?;
        if zip.len() > UNPACK_MAX_ENTRIES {
            return Err(Error::invalid("the archive holds too many files"));
        }
        // Names as the archive gives them, already made safe to join.
        let mut names = Vec::with_capacity(zip.len());
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(zip_error)?;
            let name = entry
                .enclosed_name()
                .ok_or_else(|| Error::invalid("the archive holds a path that leaves it"))?;
            names.push((name, entry.is_dir()));
        }
        let top = names
            .first()
            .and_then(|(name, _)| name.components().next())
            .map(|part| PathBuf::from(part.as_os_str()));
        let wrapped = top.as_ref().is_some_and(|top| {
            names
                .iter()
                .all(|(name, dir)| name.starts_with(top) && (*dir || name != top))
        });
        let mut written = 0usize;
        let mut bytes = 0u64;
        std::fs::create_dir_all(&target)?;
        for (i, (name, dir)) in names.iter().enumerate() {
            let inner = match (&top, wrapped) {
                (Some(top), true) => name.strip_prefix(top).unwrap_or(name),
                _ => name,
            };
            if inner.as_os_str().is_empty() {
                continue;
            }
            let dest = target.join(inner);
            if *dir {
                std::fs::create_dir_all(&dest)?;
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let entry = zip.by_index(i).map_err(zip_error)?;
            let room = UNPACK_MAX_BYTES - bytes;
            let mut out = std::fs::File::create(&dest)?;
            // The size in the header is only a claim; count what comes out.
            let copied = std::io::copy(&mut std::io::Read::take(entry, room + 1), &mut out)?;
            if copied > room {
                return Err(Error::invalid("the archive unpacks to more than 16 GiB"));
            }
            bytes += copied;
            written += 1;
        }
        Ok(written)
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e)))?;
    if outcome.is_err() {
        let _ = fs::remove_dir_all(&made).await;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zipped(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        use std::io::Write;
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut writer = zip::ZipWriter::new(file.reopen().unwrap());
        for (name, bytes) in entries {
            if name.ends_with('/') {
                writer
                    .add_directory(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
            } else {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(bytes).unwrap();
            }
        }
        writer.finish().unwrap();
        file
    }

    #[tokio::test]
    async fn weighs_the_instance_by_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "world/region/r.mca", &[0u8; 30][..], 100)
            .await
            .unwrap();
        write(root, "mods/a.jar", &[0u8; 10][..], 100)
            .await
            .unwrap();
        write(root, "server.properties", &[0u8; 5][..], 100)
            .await
            .unwrap();
        let usage = usage(root).await.unwrap();
        assert_eq!(usage.total, 45);
        let seen: Vec<(&str, u64)> = usage
            .folders
            .iter()
            .map(|share| (share.name.as_str(), share.size))
            .collect();
        assert_eq!(seen, [("world", 30), ("mods", 10), ("", 5)]);
    }

    #[tokio::test]
    async fn a_folder_survives_being_zipped_and_unpacked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("instance");
        write(&root, "world/level.dat", &b"level"[..], 100)
            .await
            .unwrap();
        write(&root, "world/region/r.0.0.mca", &b"chunks"[..], 100)
            .await
            .unwrap();
        let packed = dir.path().join("world.zip");
        assert_eq!(archive(&root, "world", &packed).await.unwrap(), "world");
        assert!(archive(&root, "", &packed).await.is_err());

        // The one top folder of the archive is not repeated.
        assert_eq!(unpack(&root, "copy", &packed).await.unwrap(), 2);
        let level = file(&root, "copy/level.dat").await.unwrap();
        assert_eq!(std::fs::read(level).unwrap(), b"level");
        assert!(file(&root, "copy/region/r.0.0.mca").await.is_ok());
        assert!(matches!(
            unpack(&root, "copy", &packed).await,
            Err(Error::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn an_archive_without_a_top_folder_unpacks_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let packed = zipped(&[
            ("level.dat", b"l"),
            ("region/", b""),
            ("region/r.mca", b"r"),
        ]);
        assert_eq!(unpack(root, "world", packed.path()).await.unwrap(), 2);
        assert!(file(root, "world/level.dat").await.is_ok());
        assert!(file(root, "world/region/r.mca").await.is_ok());
    }

    #[tokio::test]
    async fn an_archive_that_climbs_out_is_refused_and_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("instance");
        let packed = zipped(&[("ok.txt", b"1"), ("../escaped.txt", b"2")]);
        assert!(unpack(&root, "world", packed.path()).await.is_err());
        assert!(!dir.path().join("escaped.txt").exists());
        assert!(!root.join("world").exists());

        let not_zip = dir.path().join("x.zip");
        std::fs::write(&not_zip, b"nope").unwrap();
        assert!(unpack(&root, "world", &not_zip).await.is_err());
    }

    async fn names(root: &Path, relative: &str) -> Vec<String> {
        list(root, relative)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    }

    #[tokio::test]
    async fn writes_lists_moves_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "config/a.txt", &b"one"[..], 100).await.unwrap();
        write(root, "server.properties", &b"x=1"[..], 100)
            .await
            .unwrap();
        create_dir(root, "world").await.unwrap();

        // Folders first, then files, both by name.
        assert_eq!(
            names(root, "").await,
            ["config", "world", "server.properties"]
        );
        assert_eq!(names(root, "config").await, ["a.txt"]);

        write(root, "config/a.txt", &b"two"[..], 100).await.unwrap();
        let path = file(root, "config/a.txt").await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"two");
        assert_eq!(names(root, "config").await, ["a.txt"], "no temp files left");

        rename(root, "config/a.txt", "world/b.txt").await.unwrap();
        assert_eq!(names(root, "world").await, ["b.txt"]);
        assert!(matches!(
            rename(root, "world/b.txt", "server.properties").await,
            Err(Error::Conflict(_))
        ));
        assert!(rename(root, "world", "world/inner").await.is_err());

        remove(root, "world").await.unwrap();
        assert_eq!(names(root, "").await, ["config", "server.properties"]);
        assert!(remove(root, "").await.is_err());
    }

    #[tokio::test]
    async fn refuses_paths_that_leave_the_instance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("instance");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(dir.path().join("secret.txt"), b"s").unwrap();

        for path in ["../secret.txt", "a/../../secret.txt", "C:/Windows", "..\\x"] {
            assert!(file(&root, path).await.is_err(), "{path}");
            assert!(write(&root, path, &b""[..], 10).await.is_err(), "{path}");
            assert!(remove(&root, path).await.is_err(), "{path}");
        }
        assert!(dir.path().join("secret.txt").exists());
    }

    #[tokio::test]
    async fn a_too_large_upload_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(write(root, "big.bin", &[0u8; 64][..], 10).await.is_err());
        assert!(names(root, "").await.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn does_not_follow_links_out_of_the_instance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("instance");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        assert!(list(&root, "link").await.is_err());
        assert!(write(&root, "link/x.txt", &b"x"[..], 10).await.is_err());
        assert!(!outside.join("x.txt").exists());
    }
}
