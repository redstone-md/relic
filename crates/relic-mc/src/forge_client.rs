//! Client-side installation of Forge and NeoForge.
//!
//! Their installers only install a client through a GUI, so this module
//! does what that GUI does: unpacks the files bundled in the installer,
//! downloads the libraries `install_profile.json` lists and runs its
//! processors (the tools that patch the game JAR).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::fs;
use tokio::process::Command;

use crate::client::{self, fetch_all, jar_path, library_fetches, library_path, safe_id};
use crate::http::{Checksum, download, file_digest, get_text};
use crate::profile::{Library, Os, Profile, maven_path};
use crate::{Error, Result};

const PROCESSOR_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Deserialize)]
struct SidedValue {
    client: String,
}

#[derive(Debug, Deserialize)]
struct Processor {
    /// Absent means both sides.
    #[serde(default)]
    sides: Option<Vec<String>>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    /// File to expected SHA-1, both written as data references.
    #[serde(default)]
    outputs: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct InstallProfile {
    /// Id of the version this installer creates.
    version: String,
    /// Path of the version document inside the installer.
    json: String,
    #[serde(default)]
    data: HashMap<String, SidedValue>,
    #[serde(default)]
    processors: Vec<Processor>,
    #[serde(default)]
    libraries: Vec<Library>,
}

struct Unpacked {
    install: InstallProfile,
    version_json: Vec<u8>,
}

fn archive_error(e: zip::result::ZipError) -> Error {
    Error::Archive(e.to_string())
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<Vec<u8>> {
    let mut entry = archive
        .by_name(name.trim_start_matches('/'))
        .map_err(|e| Error::Archive(format!("{name}: {e}")))?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Reads the installer's two documents, copies its bundled Maven
/// repository into the libraries directory and the files its data table
/// refers to into `scratch`.
fn unpack(installer: &Path, libraries: &Path, scratch: &Path) -> Result<Unpacked> {
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(installer)?).map_err(archive_error)?;
    let raw = read_entry(&mut archive, "install_profile.json")?;
    let document: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|e| Error::Installer(e.to_string()))?;
    if document.get("install").is_some() || document.get("versionInfo").is_some() {
        return Err(Error::Unsupported(
            "this Forge build uses the old installer format; use 14.23.5.2851 or newer".into(),
        ));
    }
    let install: InstallProfile =
        serde_json::from_value(document).map_err(|e| Error::Installer(e.to_string()))?;
    let version_json = read_entry(&mut archive, &install.json)?;

    let wanted: Vec<String> = install
        .data
        .values()
        .filter(|value| value.client.starts_with('/'))
        .map(|value| value.client.trim_start_matches('/').to_owned())
        .collect();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(archive_error)?;
        if entry.is_dir() {
            continue;
        }
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let dest = match relative.strip_prefix("maven") {
            Ok(inside) => libraries.join(inside),
            Err(_) if wanted.iter().any(|name| name == entry.name()) => scratch.join(&relative),
            Err(_) => continue,
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut entry, &mut std::fs::File::create(&dest)?)?;
    }
    Ok(Unpacked {
        install,
        version_json,
    })
}

/// `Main-Class` of a JAR's manifest. Long manifest lines continue on the
/// next line after a single space.
fn main_class(jar: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?).map_err(archive_error)?;
    let manifest = String::from_utf8_lossy(&read_entry(&mut archive, "META-INF/MANIFEST.MF")?)
        .replace("\r\n", "\n")
        .replace("\n ", "");
    manifest
        .lines()
        .find_map(|line| line.strip_prefix("Main-Class:"))
        .map(|class| class.trim().to_owned())
        .ok_or_else(|| Error::Installer(format!("{} has no Main-Class", jar.display())))
}

/// Resolves what installer documents write in place of a value:
/// `[maven:coordinates]` is a file in the libraries directory, `'text'` is
/// literal, and `{KEY}` refers to the data table.
fn resolve(token: &str, root: &Path, data: &HashMap<String, String>) -> Result<String> {
    if let Some(coordinates) = token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        let path = library_path(root, &maven_path(coordinates)?);
        return Ok(path.to_string_lossy().into_owned());
    }
    if let Some(literal) = token.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')) {
        return Ok(literal.to_owned());
    }
    let mut out = String::with_capacity(token.len());
    let mut rest = token;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            return Err(Error::Installer(format!(
                "unterminated reference in {token:?}"
            )));
        };
        let key = &after[..end];
        let value = data
            .get(key)
            .ok_or_else(|| Error::Installer(format!("installer refers to unknown data {key:?}")))?;
        out.push_str(value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

async fn outputs_current(outputs: &[(String, String)]) -> bool {
    if outputs.is_empty() {
        return false;
    }
    for (path, sha1) in outputs {
        let like = Checksum::Sha1(sha1.clone());
        match file_digest(Path::new(path), &like).await {
            Ok(actual) if actual.eq_ignore_ascii_case(sha1) => {}
            _ => return false,
        }
    }
    true
}

async fn run_processor(
    java: &Path,
    root: &Path,
    processor: &Processor,
    data: &HashMap<String, String>,
) -> Result<()> {
    let mut outputs = Vec::new();
    for (path, sha1) in &processor.outputs {
        outputs.push((resolve(path, root, data)?, resolve(sha1, root, data)?));
    }
    if outputs_current(&outputs).await {
        return Ok(());
    }

    let jar = library_path(root, &maven_path(&processor.jar)?);
    let class = {
        let jar = jar.clone();
        tokio::task::spawn_blocking(move || main_class(&jar))
            .await
            .map_err(|e| Error::Installer(e.to_string()))??
    };
    let mut classpath = vec![jar];
    for coordinates in &processor.classpath {
        classpath.push(library_path(root, &maven_path(coordinates)?));
    }
    let classpath = std::env::join_paths(classpath)
        .map_err(|e| Error::Installer(format!("unusable library path: {e}")))?;
    let mut args = Vec::new();
    for arg in &processor.args {
        args.push(resolve(arg, root, data)?);
    }

    let mut command = Command::new(java);
    command
        .arg("-cp")
        .arg(classpath)
        .arg(&class)
        .args(&args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = tokio::time::timeout(PROCESSOR_TIMEOUT, command.output())
        .await
        .map_err(|_| Error::Installer(format!("{class} did not finish in time")))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let text = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        let tail: Vec<&str> = text.lines().rev().take(12).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(Error::Installer(format!(
            "{class} exited with {}: {}",
            output.status,
            tail.join("\n")
        )));
    }
    for (path, sha1) in &outputs {
        let actual = file_digest(Path::new(path), &Checksum::Sha1(sha1.clone())).await?;
        if !actual.eq_ignore_ascii_case(sha1) {
            return Err(Error::Installer(format!(
                "{class} produced an unexpected {path}"
            )));
        }
    }
    Ok(())
}

/// Installs the loader whose installer is at `url` and returns the id of
/// the version it created. The game JAR for `mc` must already be in place.
pub(crate) async fn install(
    client: &reqwest::Client,
    root: &Path,
    java: &Path,
    mc: &str,
    url: &str,
) -> Result<String> {
    let name = url.rsplit('/').next().unwrap_or("installer.jar");
    let installers = root.join("installers");
    let installer = installers.join(safe_id(name)?);
    // Both repositories publish a digest next to every file.
    let sha1 = get_text(client, &format!("{url}.sha1")).await?;
    let sha1 = sha1
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    download(client, url, &installer, &Checksum::Sha1(sha1)).await?;

    let scratch = installers.join(format!("{name}.data"));
    let result = install_from(client, root, java, mc, &installer, &scratch).await;
    let _ = fs::remove_dir_all(&scratch).await;
    result
}

async fn install_from(
    client: &reqwest::Client,
    root: &Path,
    java: &Path,
    mc: &str,
    installer: &Path,
    scratch: &Path,
) -> Result<String> {
    let unpacked = {
        let (installer, libraries, scratch) = (
            installer.to_owned(),
            root.join("libraries"),
            scratch.to_owned(),
        );
        tokio::task::spawn_blocking(move || unpack(&installer, &libraries, &scratch))
            .await
            .map_err(|e| Error::Installer(e.to_string()))??
    };
    let install = unpacked.install;
    let id = safe_id(&install.version)?.to_owned();
    let profile: Profile = serde_json::from_slice(&unpacked.version_json)
        .map_err(|e| Error::Installer(format!("version document: {e}")))?;

    let os = Os::current();
    let quiet = |_: client::Stage| {};
    let mut fetches = library_fetches(root, &install.libraries, os)?;
    // The loader's own libraries may be inputs of the processors.
    fetches.extend(library_fetches(root, &profile.libraries, os).unwrap_or_default());
    fetch_all(client, fetches, "Installing mod loader", &quiet).await?;

    let text = |path: PathBuf| path.to_string_lossy().into_owned();
    let mut data = HashMap::from([
        ("SIDE".to_owned(), "client".to_owned()),
        ("MINECRAFT_JAR".to_owned(), text(jar_path(root, mc))),
        ("MINECRAFT_VERSION".to_owned(), mc.to_owned()),
        ("ROOT".to_owned(), text(root.to_owned())),
        ("INSTALLER".to_owned(), text(installer.to_owned())),
        ("LIBRARY_DIR".to_owned(), text(root.join("libraries"))),
    ]);
    let builtin = HashMap::new();
    for (key, value) in &install.data {
        let value = match value.client.strip_prefix('/') {
            Some(inside) => {
                let mut path = scratch.to_owned();
                path.extend(inside.split('/'));
                text(path)
            }
            None => resolve(&value.client, root, &builtin)?,
        };
        data.insert(key.clone(), value);
    }

    for processor in &install.processors {
        let client_side = processor
            .sides
            .as_ref()
            .is_none_or(|sides| sides.iter().any(|side| side == "client"));
        if client_side {
            run_processor(java, root, processor, &data).await?;
        }
    }

    // Last, so that a version document on disk means a finished install.
    client::write_profile(root, &id, &unpacked.version_json).await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn data() -> HashMap<String, String> {
        HashMap::from([
            ("SIDE".to_owned(), "client".to_owned()),
            ("BINPATCH".to_owned(), "/tmp/client.lzma".to_owned()),
        ])
    }

    #[test]
    fn references_resolve_to_files_text_and_data() {
        let root = Path::new("root");
        let library = resolve("[net.minecraft:client:1.20.1:slim]", root, &data()).unwrap();
        assert_eq!(
            PathBuf::from(library),
            root.join("libraries")
                .join("net")
                .join("minecraft")
                .join("client")
                .join("1.20.1")
                .join("client-1.20.1-slim.jar")
        );
        assert_eq!(resolve("'abc123'", root, &data()).unwrap(), "abc123");
        assert_eq!(
            resolve("{BINPATCH}", root, &data()).unwrap(),
            "/tmp/client.lzma"
        );
        assert_eq!(
            resolve("--side={SIDE}", root, &data()).unwrap(),
            "--side=client"
        );
        assert_eq!(resolve("--task", root, &data()).unwrap(), "--task");
        assert!(resolve("{MISSING}", root, &data()).is_err());
        assert!(resolve("[../../evil:a:b]", root, &data()).is_err());
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn manifest_main_class_survives_line_wrapping() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("tool.jar");
        write_zip(
            &jar,
            &[(
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\r\nMain-Class: net.minecraftforge.installertools.Conso\r\n leTool\r\n",
            )],
        );
        assert_eq!(
            main_class(&jar).unwrap(),
            "net.minecraftforge.installertools.ConsoleTool"
        );
    }

    #[test]
    fn installer_contents_land_where_processors_expect_them() {
        let dir = tempfile::tempdir().unwrap();
        let installer = dir.path().join("installer.jar");
        write_zip(
            &installer,
            &[
                (
                    "install_profile.json",
                    br#"{"version":"1.20.1-forge-47.2.0","json":"/version.json",
                        "data":{"BINPATCH":{"client":"/data/client.lzma","server":"/data/server.lzma"},
                                "MC_SLIM":{"client":"[net.minecraft:client:1.20.1:slim]","server":"x"}},
                        "processors":[{"sides":["server"],"jar":"a:b:1","args":[]}],
                        "libraries":[]}"#,
                ),
                ("version.json", br#"{"id":"1.20.1-forge-47.2.0"}"#),
                ("maven/net/minecraftforge/forge/1/forge-1.jar", b"forge"),
                ("data/client.lzma", b"patch"),
                ("data/server.lzma", b"other"),
            ],
        );
        let (libraries, scratch) = (dir.path().join("libraries"), dir.path().join("scratch"));
        let unpacked = unpack(&installer, &libraries, &scratch).unwrap();
        assert_eq!(unpacked.install.version, "1.20.1-forge-47.2.0");
        assert_eq!(unpacked.install.processors.len(), 1);
        assert_eq!(
            std::fs::read(libraries.join("net/minecraftforge/forge/1/forge-1.jar")).unwrap(),
            b"forge"
        );
        assert_eq!(
            std::fs::read(scratch.join("data/client.lzma")).unwrap(),
            b"patch"
        );
        assert!(!scratch.join("data/server.lzma").exists());
    }

    #[test]
    fn the_old_installer_format_is_refused_clearly() {
        let dir = tempfile::tempdir().unwrap();
        let installer = dir.path().join("installer.jar");
        write_zip(
            &installer,
            &[(
                "install_profile.json",
                br#"{"install":{},"versionInfo":{}}"#,
            )],
        );
        let err = unpack(&installer, dir.path(), dir.path()).err().unwrap();
        assert!(matches!(err, Error::Unsupported(_)));
    }
}
