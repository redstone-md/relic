//! Eclipse Temurin runtimes, installed into a private directory so nothing
//! depends on (or disturbs) a system-wide Java.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::fs;

use crate::http::{Checksum, download, get_json};
use crate::{Error, Result};

/// Name of the file recording which release a runtime directory holds.
const RELEASE_MARKER: &str = ".relic-release";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    /// Adoptium `os` value: `linux`, `windows` or `mac`.
    pub os: &'static str,
    /// Adoptium `architecture` value: `x64` or `aarch64`.
    pub arch: &'static str,
}

impl Platform {
    pub fn current() -> Result<Self> {
        let os = match std::env::consts::OS {
            "linux" => "linux",
            "windows" => "windows",
            "macos" => "mac",
            other => return Err(Error::Unsupported(format!("unsupported OS: {other}"))),
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "aarch64",
            other => {
                return Err(Error::Unsupported(format!(
                    "unsupported CPU architecture: {other}"
                )));
            }
        };
        Ok(Self { os, arch })
    }

    fn java_binary(self) -> &'static str {
        if self.os == "windows" {
            "java.exe"
        } else {
            "java"
        }
    }
}

/// Maps a required Java version to one Temurin actually ships as a
/// maintained JRE. Java 16 (Minecraft 1.17) is end-of-life; 17 runs it fine.
pub fn runtime_major(required: u32) -> u32 {
    match required {
        0..=8 => 8,
        9..=17 => 17,
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaPackage {
    pub release_name: String,
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct Asset {
    binary: Binary,
    release_name: String,
}

#[derive(Deserialize)]
struct Binary {
    package: Package,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    link: String,
    checksum: String,
    size: u64,
}

/// Looks up the latest Temurin JRE build of `major` for `platform`.
pub async fn resolve(
    client: &reqwest::Client,
    major: u32,
    platform: Platform,
) -> Result<JavaPackage> {
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot\
         ?architecture={}&image_type=jre&os={}&vendor=eclipse",
        platform.arch, platform.os
    );
    let assets: Vec<Asset> = match get_json(client, &url).await {
        Ok(assets) => assets,
        Err(Error::Status { status: 404, .. }) => Vec::new(),
        Err(err) => return Err(err),
    };
    let asset = assets.into_iter().next().ok_or_else(|| {
        Error::NotFound(format!(
            "Temurin has no Java {major} JRE for {} {}",
            platform.os, platform.arch
        ))
    })?;
    Ok(JavaPackage {
        release_name: asset.release_name,
        file_name: asset.binary.package.name,
        url: asset.binary.package.link,
        sha256: asset.binary.package.checksum,
        size: asset.binary.package.size,
    })
}

/// Directory holding the runtime for `major` under `root`.
pub fn runtime_dir(root: &Path, major: u32) -> PathBuf {
    root.join(format!("temurin-{major}"))
}

/// Finds `bin/java` inside an extracted runtime. Archives wrap everything in
/// one top-level directory, and macOS adds `Contents/Home` below it.
fn find_java(dir: &Path, platform: Platform) -> Option<PathBuf> {
    let binary = platform.java_binary();
    let direct = dir.join("bin").join(binary);
    if direct.is_file() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let top = entry.path();
        if !top.is_dir() {
            continue;
        }
        for candidate in [
            top.join("bin").join(binary),
            top.join("Contents").join("Home").join("bin").join(binary),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Path of the `java` executable of an already installed runtime, if any.
pub fn installed_java(root: &Path, major: u32) -> Option<PathBuf> {
    let dir = runtime_dir(root, major);
    if !dir.join(RELEASE_MARKER).is_file() {
        return None;
    }
    find_java(&dir, Platform::current().ok()?)
}

/// Makes sure a Temurin JRE able to run `required_major` exists under `root`
/// and returns the path of its `java` executable.
///
/// An existing runtime is reused as is; it is not upgraded to newer patch
/// releases, so a working installation never changes underneath a server.
pub async fn ensure_runtime(
    client: &reqwest::Client,
    root: &Path,
    required_major: u32,
) -> Result<PathBuf> {
    let major = runtime_major(required_major);
    if let Some(java) = installed_java(root, major) {
        return Ok(java);
    }
    let platform = Platform::current()?;
    let package = resolve(client, major, platform).await?;
    tracing::info!(release = %package.release_name, "installing Java runtime");

    fs::create_dir_all(root).await?;
    let archive = root.join(&package.file_name);
    download(
        client,
        &package.url,
        &archive,
        &Checksum::Sha256(package.sha256.clone()),
    )
    .await?;

    // Extract next to the final location, then swap it in, so an interrupted
    // extraction is never mistaken for a usable runtime.
    let dir = runtime_dir(root, major);
    let staging = root.join(format!("temurin-{major}.staging"));
    let _ = fs::remove_dir_all(&staging).await;
    let (archive_for_task, staging_for_task) = (archive.clone(), staging.clone());
    tokio::task::spawn_blocking(move || extract(&archive_for_task, &staging_for_task))
        .await
        .map_err(|e| Error::Archive(e.to_string()))??;
    fs::write(staging.join(RELEASE_MARKER), &package.release_name).await?;

    let _ = fs::remove_dir_all(&dir).await;
    fs::rename(&staging, &dir).await?;
    let _ = fs::remove_file(&archive).await;

    find_java(&dir, platform).ok_or_else(|| {
        Error::Archive(format!(
            "{} does not contain a java executable",
            package.file_name
        ))
    })
}

fn extract(archive: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let file = std::fs::File::open(archive)?;
    if name.ends_with(".zip") {
        // `extract` refuses entries that would land outside `dest`.
        zip::ZipArchive::new(file)
            .and_then(|mut zip| zip.extract(dest))
            .map_err(|e| Error::Archive(e.to_string()))
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        // `unpack` likewise skips paths escaping `dest` and keeps the
        // executable bits and symlinks a JRE relies on.
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        tar.set_preserve_permissions(true);
        tar.unpack(dest).map_err(|e| Error::Archive(e.to_string()))
    } else {
        Err(Error::Archive(format!("unknown archive format: {name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_major_collapses_to_maintained_releases() {
        assert_eq!(runtime_major(8), 8);
        assert_eq!(runtime_major(16), 17);
        assert_eq!(runtime_major(17), 17);
        assert_eq!(runtime_major(21), 21);
        assert_eq!(runtime_major(25), 25);
    }

    #[test]
    fn java_is_found_under_a_wrapping_directory() {
        let platform = Platform::current().unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(find_java(dir.path(), platform).is_none());

        let bin = dir.path().join("jdk-21.0.13+9-jre").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(platform.java_binary()), b"").unwrap();
        assert_eq!(
            find_java(dir.path(), platform).unwrap(),
            bin.join(platform.java_binary())
        );

        // Without the marker the directory is treated as not installed.
        assert!(installed_java(dir.path(), 21).is_none());
    }
}
