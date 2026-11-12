//! Materialises the server side of a modpack in an instance directory.

use std::collections::BTreeMap;
use std::path::Path;

use tokio::fs;

use crate::error::Result;
use crate::modpack::Modpack;

/// Records which pack files were written to the instance directory and with
/// what content, so later runs can tell their own files from the server's.
pub const STATE_FILE: &str = "relic-deployed.json";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DeployReport {
    pub written: usize,
    pub removed: usize,
    /// Files with an unknown side, which were therefore left out.
    pub unclassified: usize,
}

async fn read_state(dir: &Path) -> BTreeMap<String, String> {
    match fs::read(dir.join(STATE_FILE)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => BTreeMap::new(),
    }
}

/// The pack files now in `dir`, by path relative to it.
pub async fn deployed(dir: &Path) -> BTreeMap<String, String> {
    read_state(dir).await
}

/// Brings `dir` in line with the server-side files of the instance's pack.
///
/// * JARs are hard links into the blob store (copies where linking fails).
/// * Everything else is copied, because the server rewrites configuration
///   in place and must never write through a link into the store.
/// * A file the server has since edited is only replaced when the pack's
///   version of it changes.
/// * Files deployed earlier that have left the pack are deleted; files the
///   server created itself are never touched.
///
/// Call only while the server is stopped.
pub async fn deploy(modpack: &Modpack, instance_id: &str, dir: &Path) -> Result<DeployReport> {
    fs::create_dir_all(dir).await?;
    let previous = read_state(dir).await;
    let wanted = modpack.server_files(instance_id).await?;
    let mut report = DeployReport {
        unclassified: modpack.unclassified(instance_id).await?.len(),
        ..DeployReport::default()
    };

    let mut state = BTreeMap::new();
    for file in &wanted {
        // Paths were normalised on ingestion: relative, no `..`.
        let dest = dir.join(&file.path);
        let exists = fs::try_exists(&dest).await.unwrap_or(false);
        // The client-side `sync_once` flag is irrelevant here: the admin owns
        // both the pack and the server, so a new pack version always lands.
        let up_to_date = exists && previous.get(&file.path) == Some(&file.sha256);
        if !up_to_date {
            if file.path.to_ascii_lowercase().ends_with(".jar") {
                modpack.blobs().link_to(&file.sha256, &dest).await?;
            } else {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).await?;
                }
                // Remove first: `dest` may be a link left by an older layout.
                let _ = fs::remove_file(&dest).await;
                fs::copy(modpack.blobs().path(&file.sha256)?, &dest).await?;
            }
            report.written += 1;
        }
        state.insert(file.path.clone(), file.sha256.clone());
    }

    for path in previous.keys().filter(|p| !state.contains_key(*p)) {
        if fs::remove_file(dir.join(path)).await.is_ok() {
            report.removed += 1;
        }
    }

    let json = serde_json::to_vec_pretty(&state).expect("string map serializes");
    fs::write(dir.join(STATE_FILE), json).await?;
    Ok(report)
}

/// Sets `key=value` pairs in a Java `.properties` file, keeping every other
/// line as it is. Creates the file if needed.
pub async fn set_properties(path: &Path, pairs: &[(&str, String)]) -> Result<()> {
    let text = fs::read_to_string(path).await.unwrap_or_default();
    let mut pending: Vec<(&str, &String)> = pairs.iter().map(|(k, v)| (*k, v)).collect();
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let key = line.split('=').next().unwrap_or_default().trim();
        match pending.iter().position(|(k, _)| *k == key) {
            Some(i) if !line.trim_start().starts_with('#') => {
                let (k, v) = pending.remove(i);
                lines.push(format!("{k}={v}"));
            }
            _ => lines.push(line.to_owned()),
        }
    }
    for (k, v) in pending {
        lines.push(format!("{k}={v}"));
    }
    lines.push(String::new());
    fs::write(path, lines.join("\n")).await?;
    Ok(())
}

/// Reads one value from a `.properties` file.
pub async fn get_property(path: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(path).await.ok()?;
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().to_owned())
        })
}

#[cfg(test)]
mod tests {
    use relic_proto::{Loader, LoaderKind, Side};

    use super::*;
    use crate::blobs::BlobStore;
    use crate::instances::{self, NewInstance};
    use crate::modpack::FileHints;

    async fn setup() -> (tempfile::TempDir, Modpack) {
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
                    kind: LoaderKind::Fabric,
                    version: "0.16.9".into(),
                },
                java_major: 21,
                port: 25565,
                memory_mb: 2048,
            },
        )
        .await
        .unwrap();
        (tmp, Modpack::new(db, blobs))
    }

    async fn add(modpack: &Modpack, path: &str, body: &[u8], side: Side) {
        let hints = FileHints {
            side: Some(side),
            optional: None,
        };
        modpack
            .add_file("main", path, body, None, hints)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn deploys_server_files_and_cleans_up_only_its_own() {
        let (tmp, modpack) = setup().await;
        let dir = tmp.path().join("instance");
        add(&modpack, "config/a.toml", b"a = 1", Side::Both).await;
        add(&modpack, "config/client.toml", b"c = 1", Side::Client).await;
        add(&modpack, "config/server.toml", b"s = 1", Side::Server).await;

        let report = deploy(&modpack, "main", &dir).await.unwrap();
        assert_eq!(report.written, 2);
        assert!(dir.join("config/a.toml").is_file());
        assert!(dir.join("config/server.toml").is_file());
        assert!(!dir.join("config/client.toml").exists());

        // The server edits a config and creates a file of its own.
        std::fs::write(dir.join("config/a.toml"), b"a = 2").unwrap();
        std::fs::write(dir.join("config/generated.toml"), b"g").unwrap();
        let report = deploy(&modpack, "main", &dir).await.unwrap();
        assert_eq!(report, DeployReport::default());
        assert_eq!(std::fs::read(dir.join("config/a.toml")).unwrap(), b"a = 2");

        // A new pack version of the file wins over the local edit.
        add(&modpack, "config/a.toml", b"a = 3", Side::Both).await;
        modpack
            .remove_file("main", "config/server.toml")
            .await
            .unwrap();
        let report = deploy(&modpack, "main", &dir).await.unwrap();
        assert_eq!((report.written, report.removed), (1, 1));
        assert_eq!(std::fs::read(dir.join("config/a.toml")).unwrap(), b"a = 3");
        assert!(!dir.join("config/server.toml").exists());
        assert!(dir.join("config/generated.toml").is_file());

        // Copied configs must not alias the blob store.
        std::fs::write(dir.join("config/a.toml"), b"scribble").unwrap();
        let file = modpack.get_file("main", "config/a.toml").await.unwrap();
        let blob = modpack.blobs().path(&file.sha256).unwrap();
        assert_eq!(std::fs::read(blob).unwrap(), b"a = 3");
    }

    #[tokio::test]
    async fn properties_are_updated_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("server.properties");
        std::fs::write(
            &path,
            "#Minecraft server properties\nmotd=Hi\nserver-port=25565\n",
        )
        .unwrap();
        set_properties(
            &path,
            &[
                ("server-port", "25570".into()),
                ("online-mode", "true".into()),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "#Minecraft server properties\nmotd=Hi\nserver-port=25570\nonline-mode=true\n"
        );
        assert_eq!(get_property(&path, "motd").await.as_deref(), Some("Hi"));
        assert_eq!(get_property(&path, "missing").await, None);
    }
}
