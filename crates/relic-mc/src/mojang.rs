//! Mojang's version metadata (`piston-meta`).

use serde::{Deserialize, Serialize};

use crate::http::get_json;
use crate::{Error, Result};

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    /// `release`, `snapshot`, `old_beta` or `old_alpha`.
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    pub sha1: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Download {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Downloads {
    pub client: Download,
    /// Absent for versions that never had a dedicated server.
    pub server: Option<Download>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaVersion {
    pub component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

/// The per-version document. Only what the server side needs is typed; the
/// client launch data (libraries, arguments, assets) stays in `raw`.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionJson {
    pub id: String,
    pub downloads: Downloads,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(flatten)]
    pub raw: serde_json::Map<String, serde_json::Value>,
}

impl VersionJson {
    /// Java major version to run this game version with. Versions published
    /// before Mojang recorded the requirement ran on Java 8.
    pub fn java_major(&self) -> u32 {
        self.java_version.as_ref().map_or(8, |j| j.major_version)
    }
}

pub async fn version_manifest(client: &reqwest::Client) -> Result<VersionManifest> {
    get_json(client, VERSION_MANIFEST_URL).await
}

pub async fn version_json(
    client: &reqwest::Client,
    minecraft_version: &str,
) -> Result<VersionJson> {
    let manifest = version_manifest(client).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == minecraft_version)
        .ok_or_else(|| Error::NotFound(format!("unknown Minecraft version {minecraft_version}")))?;
    get_json(client, &entry.url).await
}

/// Orders release ids so that both schemes compare correctly:
/// `1.12.2 < 1.21.1 < 26.1 < 26.3`. Non-numeric parts sort as zero.
pub fn version_key(id: &str) -> Vec<u32> {
    id.split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
        .collect()
}

/// Oldest game version Relic installs.
pub const MIN_SUPPORTED: &str = "1.12.2";

pub fn is_supported(id: &str) -> bool {
    version_key(id) >= version_key(MIN_SUPPORTED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering_spans_both_numbering_schemes() {
        let mut ids = vec![
            "26.1", "1.21.1", "1.12.2", "1.21", "26.3", "1.7.10", "1.20.6",
        ];
        ids.sort_by_key(|id| version_key(id));
        assert_eq!(
            ids,
            [
                "1.7.10", "1.12.2", "1.20.6", "1.21", "1.21.1", "26.1", "26.3"
            ]
        );
    }

    #[test]
    fn support_floor_is_1_12_2() {
        assert!(!is_supported("1.7.10"));
        assert!(!is_supported("1.12.1"));
        assert!(is_supported("1.12.2"));
        assert!(is_supported("1.21.1"));
        assert!(is_supported("26.3"));
    }

    #[test]
    fn version_json_keeps_untyped_fields_and_defaults_java_to_8() {
        let doc: VersionJson = serde_json::from_str(
            r#"{
                "id": "1.12.2",
                "mainClass": "net.minecraft.client.main.Main",
                "minecraftArguments": "--username ${auth_player_name}",
                "downloads": { "client": { "sha1": "aa", "size": 1, "url": "https://example.invalid/c" } }
            }"#,
        )
        .unwrap();
        assert_eq!(doc.java_major(), 8);
        assert!(doc.downloads.server.is_none());
        assert!(doc.raw.contains_key("minecraftArguments"));
    }
}
