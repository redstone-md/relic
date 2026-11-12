//! Version lists of the supported mod loaders.

use std::collections::HashMap;

use relic_proto::LoaderKind;
use serde::{Deserialize, Serialize};

use crate::http::get_json;
use crate::mojang::version_key;
use crate::{Error, Result};

const FORGE_VERSIONS_URL: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/maven-metadata.json";
const FORGE_PROMOS_URL: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE_VERSIONS_URL: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderVersion {
    pub version: String,
    /// Whether the loader's authors mark this build as suitable for
    /// production (Forge: the recommended build).
    pub stable: bool,
}

#[derive(Deserialize)]
struct MetaEntry {
    loader: MetaLoader,
}

#[derive(Deserialize)]
struct MetaLoader {
    version: String,
    stable: Option<bool>,
}

#[derive(Deserialize)]
struct ForgePromos {
    promos: HashMap<String, String>,
}

#[derive(Deserialize)]
struct NeoForgeVersions {
    versions: Vec<String>,
}

/// Loader builds available for `minecraft_version`, newest first. An empty
/// list means the loader does not support that game version.
pub async fn versions(
    client: &reqwest::Client,
    kind: LoaderKind,
    minecraft_version: &str,
) -> Result<Vec<LoaderVersion>> {
    match kind {
        LoaderKind::Vanilla => Ok(Vec::new()),
        LoaderKind::Fabric => {
            let url = format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}");
            meta_versions(client, &url).await
        }
        LoaderKind::Quilt => {
            let url = format!("https://meta.quiltmc.org/v3/versions/loader/{minecraft_version}");
            meta_versions(client, &url).await
        }
        LoaderKind::Forge => {
            let all: HashMap<String, Vec<String>> = get_json(client, FORGE_VERSIONS_URL).await?;
            let promos: ForgePromos = get_json(client, FORGE_PROMOS_URL).await?;
            let recommended = promos
                .promos
                .get(&format!("{minecraft_version}-recommended"));
            let prefix = format!("{minecraft_version}-");
            let mut list: Vec<LoaderVersion> = all
                .get(minecraft_version)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|full| full.strip_prefix(&prefix))
                .map(|version| LoaderVersion {
                    version: version.to_owned(),
                    stable: recommended.is_some_and(|r| r == version),
                })
                .collect();
            list.sort_by_key(|v| std::cmp::Reverse(version_key(&v.version)));
            Ok(list)
        }
        LoaderKind::NeoForge => {
            let Some(prefix) = neoforge_prefix(minecraft_version) else {
                return Ok(Vec::new());
            };
            let all: NeoForgeVersions = get_json(client, NEOFORGE_VERSIONS_URL).await?;
            let mut list: Vec<LoaderVersion> = all
                .versions
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .map(|version| LoaderVersion {
                    stable: !version.contains('-'),
                    version,
                })
                .collect();
            list.sort_by_key(|v| std::cmp::Reverse(version_key(&v.version)));
            Ok(list)
        }
    }
}

/// Fabric and Quilt share the meta-server response shape. Quilt has no
/// `stable` flag; its pre-releases carry a `-beta` style suffix instead.
async fn meta_versions(client: &reqwest::Client, url: &str) -> Result<Vec<LoaderVersion>> {
    let entries: Vec<MetaEntry> = match get_json(client, url).await {
        Ok(entries) => entries,
        // Both meta servers answer 400/404 for game versions they don't know.
        Err(Error::Status {
            status: 400 | 404, ..
        }) => Vec::new(),
        Err(err) => return Err(err),
    };
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersion {
            stable: e
                .loader
                .stable
                .unwrap_or_else(|| !e.loader.version.contains('-')),
            version: e.loader.version,
        })
        .collect())
}

/// The version to preselect: the newest stable build, else the newest one.
pub fn pick_default(versions: &[LoaderVersion]) -> Option<&LoaderVersion> {
    versions.iter().find(|v| v.stable).or(versions.first())
}

/// NeoForge encodes the game version in its own: `1.21.1` → `21.1.x`,
/// `1.21` → `21.0.x`, and for year-based ids `26.3` → `26.3.0.x`,
/// `26.1.2` → `26.1.2.x`. Returns the prefix including the trailing dot.
///
/// NeoForge for 1.20.1 was published under a different artifact with Forge's
/// own numbering and is not supported.
pub fn neoforge_prefix(minecraft_version: &str) -> Option<String> {
    let parts: Vec<&str> = minecraft_version.split('.').collect();
    if parts.iter().any(|p| p.parse::<u32>().is_err()) {
        return None;
    }
    match parts.as_slice() {
        ["1", minor] => Some(format!("{minor}.0.")),
        ["1", minor, patch] => {
            if version_key(minecraft_version) < version_key("1.20.2") {
                return None;
            }
            Some(format!("{minor}.{patch}."))
        }
        [year, drop] if *year != "1" => Some(format!("{year}.{drop}.0.")),
        [year, drop, hotfix] => Some(format!("{year}.{drop}.{hotfix}.")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_prefix_follows_both_numbering_schemes() {
        assert_eq!(neoforge_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(neoforge_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(neoforge_prefix("1.20.2").as_deref(), Some("20.2."));
        assert_eq!(neoforge_prefix("26.3").as_deref(), Some("26.3.0."));
        assert_eq!(neoforge_prefix("26.1.2").as_deref(), Some("26.1.2."));
        assert_eq!(neoforge_prefix("1.20.1"), None);
        assert_eq!(neoforge_prefix("1.12.2"), None);
        assert_eq!(neoforge_prefix("25w14craftmine"), None);
    }

    #[test]
    fn default_prefers_stable_over_newest() {
        let v = |version: &str, stable| LoaderVersion {
            version: version.to_owned(),
            stable,
        };
        let list = [v("0.20.0-beta.9", false), v("0.19.2", true)];
        assert_eq!(pick_default(&list).unwrap().version, "0.19.2");
        let list = [v("66.1.5", false), v("66.1.4", false)];
        assert_eq!(pick_default(&list).unwrap().version, "66.1.5");
        assert!(pick_default(&[]).is_none());
    }
}
