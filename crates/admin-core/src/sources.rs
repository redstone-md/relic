//! Where mods come from besides manual upload: Modrinth, CurseForge and
//! modpack archives of either catalogue.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use relic_mc::http::{self, Checksum};
use relic_mc::reqwest::{self, Url};
use relic_proto::{Loader, LoaderKind, Side};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::instances::{self, Instance};
use crate::modpack::{FileHints, FileRecord, Modpack, normalize_path};

const MODRINTH: &str = "https://api.modrinth.com/v2";
const CURSEFORGE: &str = "https://api.curseforge.com/v1";
const CURSEFORGE_MINECRAFT: &str = "432";
const CURSEFORGE_MODS_CLASS: &str = "6";
const CURSEFORGE_PACKS_CLASS: &str = "4471";
/// CurseForge answers at most this many items per request.
const CURSEFORGE_BATCH: usize = 50;
/// Hosts a `.mrpack` may download from, per the format specification.
const MRPACK_HOSTS: [&str; 4] = [
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];
const MRPACK_MAX_ENTRIES: usize = 20_000;
const MRPACK_MAX_UNPACKED: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Modrinth,
    Curseforge,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub provider: Provider,
    pub id: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub page_url: String,
    pub downloads: u64,
    /// What the catalogue says about where the mod runs.
    pub side: Side,
    /// CurseForge authors may forbid downloads by third-party tools; such
    /// mods have to be fetched in a browser and uploaded by hand.
    pub distributable: bool,
}

/// A ready-made modpack in a catalogue.
#[derive(Debug, Clone, Serialize)]
pub struct PackHit {
    pub provider: Provider,
    pub id: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: Option<String>,
    /// A wide picture the authors chose to show the pack with.
    pub cover_url: Option<String>,
    pub page_url: String,
    pub downloads: u64,
    pub categories: Vec<String>,
}

/// What the catalogue can be narrowed to; anything else is ignored.
pub const PACK_CATEGORIES: [&str; 10] = [
    "adventure",
    "challenging",
    "combat",
    "kitchen-sink",
    "lightweight",
    "magic",
    "multiplayer",
    "optimization",
    "quests",
    "technology",
];
const PACK_SORTS: [&str; 4] = ["relevance", "downloads", "follows", "updated"];

#[derive(Debug, Clone, Serialize)]
pub struct ModVersion {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub size: u64,
    /// `release`, `beta` or `alpha`.
    pub channel: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub file: FileRecord,
    /// Required dependencies; the caller decides whether to install them.
    pub requires: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MrpackFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MrpackReport {
    pub name: String,
    pub version: String,
    pub minecraft_version: Option<String>,
    pub loader: Option<Loader>,
    /// The pack targets another game version or loader than the instance.
    pub mismatch: bool,
    pub added: usize,
    pub failed: Vec<MrpackFailure>,
}

#[derive(Clone)]
pub struct Sources {
    http: reqwest::Client,
    modpack: Modpack,
    tmp: PathBuf,
}

fn upstream(err: impl std::fmt::Display) -> Error {
    Error::Upstream(err.to_string())
}

fn url(base: &str, path: &str, params: &[(&str, &str)]) -> Result<Url> {
    Url::parse_with_params(&format!("{base}{path}"), params).map_err(upstream)
}

/// Catalogue ids are interpolated into URL paths.
fn check_id(id: &str) -> Result<()> {
    let ok = (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(Error::invalid("malformed catalogue id"))
    }
}

/// The loaders whose mods run on an instance, most specific first.
fn compatible_loaders(instance: &Instance) -> Result<Vec<LoaderKind>> {
    Ok(match instance.loader.kind {
        LoaderKind::Vanilla => {
            return Err(Error::invalid("a vanilla instance cannot load mods"));
        }
        LoaderKind::Quilt => vec![LoaderKind::Quilt, LoaderKind::Fabric],
        // NeoForge 1.20.1 is still binary compatible with Forge.
        LoaderKind::NeoForge if instance.minecraft_version == "1.20.1" => {
            vec![LoaderKind::NeoForge, LoaderKind::Forge]
        }
        kind => vec![kind],
    })
}

fn modrinth_side(client: &str, server: &str) -> Side {
    match (client, server) {
        ("unsupported", "unsupported") | ("unknown", _) | (_, "unknown") | ("", _) | (_, "") => {
            Side::Unknown
        }
        ("unsupported", _) => Side::Server,
        (_, "unsupported") => Side::Client,
        _ => Side::Both,
    }
}

/// Mod files land in `mods/` under the name the catalogue gives them.
fn mod_path(filename: &str) -> Result<String> {
    let name = filename.rsplit(['/', '\\']).next().unwrap_or_default();
    if !name.to_ascii_lowercase().ends_with(".jar") || name.len() > 200 {
        return Err(Error::Upstream(format!(
            "unexpected mod file name: {filename}"
        )));
    }
    normalize_path(&format!("mods/{name}"))
}

/// Names of the jars inside an archive, lower-cased, wherever they sit.
fn jar_names(archive: &Path) -> Result<HashSet<String>> {
    let zip = zip::ZipArchive::new(fs_err::File::open(archive)?)
        .map_err(|e| Error::invalid(format!("not a valid server pack: {e}")))?;
    Ok(zip
        .file_names()
        .map(|name| name.replace('\\', "/").to_ascii_lowercase())
        .filter(|name| name.ends_with(".jar"))
        .filter_map(|name| name.rsplit('/').next().map(str::to_owned))
        .collect())
}

/// A mod the pack's own server build leaves out runs on the client only.
/// The server build is trusted only when it holds most of the pack's mods;
/// one made from other files would mark everything as client-only.
fn client_only_by_server_pack(
    server_jars: Option<&HashSet<String>>,
    mod_jars: &[String],
) -> HashSet<String> {
    let Some(server_jars) = server_jars else {
        return HashSet::new();
    };
    let (kept, dropped): (Vec<&String>, Vec<&String>) = mod_jars
        .iter()
        .partition(|name| server_jars.contains(*name));
    if kept.len() < dropped.len() {
        return HashSet::new();
    }
    dropped.into_iter().cloned().collect()
}

fn curseforge_host(url: &str) -> bool {
    https_host(url).is_some_and(|h| h == "forgecdn.net" || h.ends_with(".forgecdn.net"))
}

fn curseforge_checksum(file: &CfFile) -> Checksum {
    file.hashes
        .iter()
        .find(|h| h.algo == 1)
        .map_or(Checksum::None, |h| Checksum::Sha1(h.value.clone()))
}

/// CurseForge tags a file `Client` or `Server` when it runs on one side only.
fn curseforge_side(file: &CfFile) -> Option<Side> {
    let has = |tag: &str| file.game_versions.iter().any(|v| v == tag);
    match (has("Client"), has("Server")) {
        (true, false) => Some(Side::Client),
        (false, true) => Some(Side::Server),
        _ => None,
    }
}

fn https_host(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    (url.scheme() == "https").then(|| url.host_str().map(str::to_owned))?
}

// ---- Modrinth wire types ----

#[derive(Deserialize)]
struct MrSearch {
    hits: Vec<MrHit>,
}

#[derive(Deserialize)]
struct MrHit {
    project_id: String,
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    icon_url: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    client_side: String,
    #[serde(default)]
    server_side: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    featured_gallery: Option<String>,
    #[serde(default)]
    gallery: Vec<String>,
}

#[derive(Deserialize)]
struct MrVersion {
    id: String,
    project_id: String,
    name: String,
    #[serde(default)]
    version_type: String,
    files: Vec<MrFile>,
    #[serde(default)]
    dependencies: Vec<MrDependency>,
}

#[derive(Deserialize)]
struct MrFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    size: u64,
    hashes: MrHashes,
}

#[derive(Deserialize)]
struct MrHashes {
    sha1: String,
}

#[derive(Deserialize)]
struct MrDependency {
    project_id: Option<String>,
    dependency_type: String,
}

#[derive(Deserialize)]
struct MrProject {
    id: String,
    title: String,
    #[serde(default)]
    client_side: String,
    #[serde(default)]
    server_side: String,
}

impl MrVersion {
    fn primary(&self) -> Option<&MrFile> {
        self.files.iter().find(|f| f.primary).or(self.files.first())
    }
}

// ---- CurseForge wire types ----

#[derive(Deserialize)]
struct CfEnvelope<T> {
    data: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfMod {
    id: u64,
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    download_count: f64,
    #[serde(default)]
    authors: Vec<CfAuthor>,
    logo: Option<CfLogo>,
    links: Option<CfLinks>,
    allow_mod_distribution: Option<bool>,
    #[serde(default)]
    class_id: Option<u64>,
    #[serde(default)]
    screenshots: Vec<CfScreenshot>,
}

#[derive(Deserialize)]
struct CfAuthor {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfLogo {
    thumbnail_url: Option<String>,
    url: Option<String>,
}

#[derive(Deserialize)]
struct CfScreenshot {
    url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfLinks {
    website_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFile {
    id: u64,
    #[serde(default)]
    display_name: String,
    file_name: String,
    #[serde(default)]
    file_length: u64,
    #[serde(default)]
    release_type: u8,
    download_url: Option<String>,
    /// For a modpack file: the build of the same pack meant for servers.
    #[serde(default)]
    server_pack_file_id: Option<u64>,
    #[serde(default)]
    hashes: Vec<CfHash>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    dependencies: Vec<CfDependency>,
}

#[derive(Deserialize)]
struct CfHash {
    value: String,
    algo: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfDependency {
    mod_id: u64,
    relation_type: u8,
}

fn curseforge_loader(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Forge => "1",
        LoaderKind::Fabric => "4",
        LoaderKind::Quilt => "5",
        LoaderKind::NeoForge => "6",
        LoaderKind::Vanilla => "0",
    }
}

// ---- .mrpack wire types ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MrpackIndex {
    format_version: u32,
    game: String,
    #[serde(default)]
    version_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    files: Vec<MrpackFile>,
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct MrpackFile {
    path: String,
    hashes: MrHashes,
    env: Option<MrpackEnv>,
    #[serde(default)]
    downloads: Vec<String>,
}

#[derive(Deserialize)]
struct MrpackEnv {
    #[serde(default)]
    client: String,
    #[serde(default)]
    server: String,
}

struct Override {
    path: String,
    side: Side,
    file: PathBuf,
}

// ---- CurseForge modpack wire types ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfManifest {
    #[serde(default)]
    manifest_type: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    minecraft: CfManifestGame,
    #[serde(default)]
    files: Vec<CfManifestFile>,
    #[serde(default)]
    overrides: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfManifestGame {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<CfManifestLoader>,
}

#[derive(Deserialize)]
struct CfManifestLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct CfManifestFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "yes")]
    required: bool,
}

fn yes() -> bool {
    true
}

impl CfManifest {
    /// `forge-14.23.5.2860`, `fabric-0.16.9`, `neoforge-21.1.90`.
    fn loader(&self) -> Option<Loader> {
        let loaders = &self.minecraft.mod_loaders;
        let chosen = loaders.iter().find(|l| l.primary).or(loaders.first())?;
        let (name, version) = chosen.id.split_once('-')?;
        let kind = match name {
            "forge" => LoaderKind::Forge,
            "neoforge" => LoaderKind::NeoForge,
            "fabric" => LoaderKind::Fabric,
            "quilt" => LoaderKind::Quilt,
            _ => return None,
        };
        Some(Loader {
            kind,
            version: version.to_owned(),
        })
    }
}

/// Where a CurseForge project's files belong, by its class, and whether the
/// server has any use for them.
fn curseforge_folder(class: Option<u64>, file_name: &str) -> Option<(&'static str, Option<Side>)> {
    match class {
        Some(6) => Some(("mods", None)),
        Some(12) => Some(("resourcepacks", Some(Side::Client))),
        Some(6552) => Some(("shaderpacks", Some(Side::Client))),
        Some(_) => None,
        None => file_name
            .to_ascii_lowercase()
            .ends_with(".jar")
            .then_some(("mods", None)),
    }
}

/// A modpack archive of either catalogue.
enum PackArchive {
    Modrinth(MrpackIndex),
    Curseforge(CfManifest),
}

/// Reads the pack's description and unpacks its override trees into
/// `scratch`.
fn open_pack(archive: &Path, scratch: &Path) -> Result<(PackArchive, Vec<Override>)> {
    let bad =
        |e: zip::result::ZipError| Error::invalid(format!("not a valid modpack archive: {e}"));
    let mut zip = zip::ZipArchive::new(fs_err::File::open(archive)?).map_err(bad)?;
    if zip.len() > MRPACK_MAX_ENTRIES {
        return Err(Error::invalid("the modpack archive has too many entries"));
    }
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    let has = |name: &str| names.iter().any(|n| n == name);
    let pack = if has("modrinth.index.json") {
        let entry = zip.by_name("modrinth.index.json").map_err(bad)?;
        let index: MrpackIndex = serde_json::from_reader(entry.take(16 * 1024 * 1024))
            .map_err(|e| Error::invalid(format!("unreadable modrinth.index.json: {e}")))?;
        if index.format_version != 1 || index.game != "minecraft" {
            return Err(Error::invalid("unsupported .mrpack format"));
        }
        PackArchive::Modrinth(index)
    } else if has("manifest.json") {
        let entry = zip.by_name("manifest.json").map_err(bad)?;
        let manifest: CfManifest = serde_json::from_reader(entry.take(16 * 1024 * 1024))
            .map_err(|e| Error::invalid(format!("unreadable manifest.json: {e}")))?;
        if manifest.manifest_type != "minecraftModpack" {
            return Err(Error::invalid("manifest.json does not describe a modpack"));
        }
        PackArchive::Curseforge(manifest)
    } else {
        return Err(Error::invalid(
            "the archive has neither modrinth.index.json nor manifest.json; \
             it is not a Modrinth or CurseForge modpack",
        ));
    };

    // Later trees win over `overrides/`; the client tree is not ours to run.
    let trees: Vec<(String, Side)> = match &pack {
        PackArchive::Modrinth(_) => vec![
            ("overrides/".to_owned(), Side::Both),
            ("server-overrides/".to_owned(), Side::Server),
            ("client-overrides/".to_owned(), Side::Client),
        ],
        PackArchive::Curseforge(manifest) => {
            let folder = manifest.overrides.trim_matches('/');
            if folder.is_empty() {
                Vec::new()
            } else {
                vec![(format!("{folder}/"), Side::Both)]
            }
        }
    };

    let mut overrides = Vec::new();
    let mut unpacked = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(bad)?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let Some((side, rest)) = trees.iter().find_map(|(prefix, side)| {
            name.strip_prefix(prefix.as_str()).map(|rest| (*side, rest))
        }) else {
            continue;
        };
        let Ok(path) = normalize_path(rest) else {
            continue;
        };
        unpacked += entry.size();
        if unpacked > MRPACK_MAX_UNPACKED {
            return Err(Error::invalid("the modpack unpacks to more than 2 GiB"));
        }
        let file = scratch.join(format!("o{i}"));
        let mut out = fs_err::File::create(&file)?;
        // `size()` is only a claim; cap what is actually written.
        let written = std::io::copy(&mut (&mut entry).take(MRPACK_MAX_UNPACKED), &mut out)?;
        if written > entry.size() {
            return Err(Error::invalid(
                "the modpack archive lies about its entry sizes",
            ));
        }
        overrides.push(Override { path, side, file });
    }
    Ok((pack, overrides))
}

impl MrpackIndex {
    fn loader(&self) -> Option<Loader> {
        [
            ("fabric-loader", LoaderKind::Fabric),
            ("quilt-loader", LoaderKind::Quilt),
            ("neoforge", LoaderKind::NeoForge),
            ("forge", LoaderKind::Forge),
        ]
        .into_iter()
        .find_map(|(key, kind)| {
            self.dependencies.get(key).map(|version| Loader {
                kind,
                version: version.clone(),
            })
        })
    }
}

impl Sources {
    pub fn new(http: reqwest::Client, modpack: Modpack, tmp: PathBuf) -> Self {
        Self { http, modpack, tmp }
    }

    async fn fetch<T: DeserializeOwned>(&self, url: Url, api_key: Option<&str>) -> Result<T> {
        let mut request = self.http.get(url);
        if let Some(key) = api_key {
            request = request.header("x-api-key", key);
        }
        // `without_url`: errors are shown in the panel; keep them short.
        let response = request
            .send()
            .await
            .map_err(|e| upstream(e.without_url()))?;
        match response.status().as_u16() {
            200..=299 => {}
            401 | 403 if api_key.is_some() => {
                return Err(Error::Upstream("CurseForge rejected the API key".into()));
            }
            404 => return Err(Error::NotFound("catalogue entry")),
            status => return Err(Error::Upstream(format!("catalogue answered HTTP {status}"))),
        }
        response.json().await.map_err(|e| upstream(e.without_url()))
    }

    /// CurseForge looks up several mods or files with a POST.
    async fn curseforge_lookup<T: DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
        key: &str,
    ) -> Result<T> {
        let response = self
            .http
            .post(format!("{CURSEFORGE}{path}"))
            .header("x-api-key", key)
            .json(&body)
            .send()
            .await
            .map_err(|e| upstream(e.without_url()))?;
        match response.status().as_u16() {
            200..=299 => {}
            401 | 403 => return Err(Error::Upstream("CurseForge rejected the API key".into())),
            status => {
                return Err(Error::Upstream(format!(
                    "CurseForge answered HTTP {status}"
                )));
            }
        }
        let found: CfEnvelope<T> = response
            .json()
            .await
            .map_err(|e| upstream(e.without_url()))?;
        Ok(found.data)
    }

    fn curseforge_key(key: Option<&str>) -> Result<&str> {
        key.filter(|k| !k.trim().is_empty())
            .ok_or_else(|| Error::invalid("set a CurseForge API key in the settings first"))
    }

    pub async fn search(
        &self,
        provider: Provider,
        instance: &Instance,
        query: &str,
        curseforge_key: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        let loaders = compatible_loaders(instance)?;
        match provider {
            Provider::Modrinth => {
                let categories: Vec<String> = loaders
                    .iter()
                    .map(|l| format!("categories:{}", l.as_str()))
                    .collect();
                let facets = serde_json::json!([
                    ["project_type:mod"],
                    [format!("versions:{}", instance.minecraft_version)],
                    categories,
                ])
                .to_string();
                let url = url(
                    MODRINTH,
                    "/search",
                    &[("query", query), ("limit", "30"), ("facets", &facets)],
                )?;
                let found: MrSearch = self.fetch(url, None).await?;
                Ok(found
                    .hits
                    .into_iter()
                    .map(|hit| SearchHit {
                        provider,
                        page_url: format!("https://modrinth.com/mod/{}", hit.slug),
                        side: modrinth_side(&hit.client_side, &hit.server_side),
                        id: hit.project_id,
                        title: hit.title,
                        description: hit.description,
                        author: hit.author,
                        icon_url: hit.icon_url.filter(|u| !u.is_empty()),
                        downloads: hit.downloads,
                        distributable: true,
                    })
                    .collect())
            }
            Provider::Curseforge => {
                let key = Self::curseforge_key(curseforge_key)?;
                let url = url(
                    CURSEFORGE,
                    "/mods/search",
                    &[
                        ("gameId", CURSEFORGE_MINECRAFT),
                        ("classId", CURSEFORGE_MODS_CLASS),
                        ("searchFilter", query),
                        ("gameVersion", &instance.minecraft_version),
                        ("modLoaderType", curseforge_loader(loaders[0])),
                        ("sortField", "2"),
                        ("sortOrder", "desc"),
                        ("pageSize", "30"),
                    ],
                )?;
                let found: CfEnvelope<Vec<CfMod>> = self.fetch(url, Some(key)).await?;
                Ok(found
                    .data
                    .into_iter()
                    .map(|item| SearchHit {
                        provider,
                        id: item.id.to_string(),
                        title: item.name,
                        description: item.summary,
                        author: item
                            .authors
                            .into_iter()
                            .next()
                            .map(|a| a.name)
                            .unwrap_or_default(),
                        icon_url: item.logo.and_then(|l| l.thumbnail_url),
                        page_url: item
                            .links
                            .and_then(|l| l.website_url)
                            .filter(|u| u.starts_with("https://"))
                            .unwrap_or_else(|| "https://www.curseforge.com/minecraft".into()),
                        downloads: item.download_count.max(0.0) as u64,
                        side: Side::Unknown,
                        distributable: item.allow_mod_distribution.unwrap_or(true),
                    })
                    .collect())
            }
        }
    }

    /// Modpacks made for the instance's game version and loader.
    pub async fn packs(
        &self,
        provider: Provider,
        instance: &Instance,
        query: &str,
        category: Option<&str>,
        sort: Option<&str>,
        curseforge_key: Option<&str>,
    ) -> Result<Vec<PackHit>> {
        if provider == Provider::Curseforge {
            let key = Self::curseforge_key(curseforge_key)?;
            return self.curseforge_packs(instance, query, sort, key).await;
        }
        let loaders: Vec<String> = compatible_loaders(instance)?
            .iter()
            .map(|l| format!("categories:{}", l.as_str()))
            .collect();
        let mut facets = vec![
            serde_json::json!(["project_type:modpack"]),
            serde_json::json!([format!("versions:{}", instance.minecraft_version)]),
            serde_json::json!(loaders),
        ];
        if let Some(category) = category.filter(|c| PACK_CATEGORIES.contains(c)) {
            facets.push(serde_json::json!([format!("categories:{category}")]));
        }
        let facets = serde_json::Value::Array(facets).to_string();
        let sort = sort
            .filter(|s| PACK_SORTS.contains(s))
            .unwrap_or(if query.trim().is_empty() {
                "downloads"
            } else {
                "relevance"
            });
        let url = url(
            MODRINTH,
            "/search",
            &[
                ("query", query),
                ("limit", "30"),
                ("index", sort),
                ("facets", &facets),
            ],
        )?;
        let found: MrSearch = self.fetch(url, None).await?;
        let picture = |url: String| https_host(&url).is_some().then_some(url);
        Ok(found
            .hits
            .into_iter()
            .map(|hit| PackHit {
                provider: Provider::Modrinth,
                page_url: format!("https://modrinth.com/modpack/{}", hit.slug),
                id: hit.project_id,
                title: hit.title,
                description: hit.description,
                author: hit.author,
                icon_url: hit.icon_url.and_then(picture),
                cover_url: hit
                    .featured_gallery
                    .or_else(|| hit.gallery.into_iter().next())
                    .and_then(picture),
                downloads: hit.downloads,
                categories: hit
                    .categories
                    .into_iter()
                    .filter(|c| PACK_CATEGORIES.contains(&c.as_str()))
                    .collect(),
            })
            .collect())
    }

    /// CurseForge modpacks for the instance's game version. The loader is
    /// not part of the query: older packs are not tagged with one, and the
    /// import says so when a pack was made for another loader.
    async fn curseforge_packs(
        &self,
        instance: &Instance,
        query: &str,
        sort: Option<&str>,
        key: &str,
    ) -> Result<Vec<PackHit>> {
        compatible_loaders(instance)?;
        let sort = match sort {
            Some("updated") => "3",
            Some("downloads") => "6",
            // Popularity; with a search term CurseForge still ranks matches first.
            _ => "2",
        };
        let url = url(
            CURSEFORGE,
            "/mods/search",
            &[
                ("gameId", CURSEFORGE_MINECRAFT),
                ("classId", CURSEFORGE_PACKS_CLASS),
                ("searchFilter", query),
                ("gameVersion", &instance.minecraft_version),
                ("sortField", sort),
                ("sortOrder", "desc"),
                ("pageSize", "30"),
            ],
        )?;
        let found: CfEnvelope<Vec<CfMod>> = self.fetch(url, Some(key)).await?;
        let picture = |url: String| https_host(&url).is_some().then_some(url);
        Ok(found
            .data
            .into_iter()
            .map(|item| PackHit {
                provider: Provider::Curseforge,
                id: item.id.to_string(),
                title: item.name,
                description: item.summary,
                author: item
                    .authors
                    .into_iter()
                    .next()
                    .map(|a| a.name)
                    .unwrap_or_default(),
                cover_url: item
                    .screenshots
                    .into_iter()
                    .find_map(|s| s.url)
                    .or_else(|| item.logo.as_ref().and_then(|l| l.url.clone()))
                    .and_then(picture),
                icon_url: item.logo.and_then(|l| l.thumbnail_url).and_then(picture),
                page_url: item
                    .links
                    .and_then(|l| l.website_url)
                    .filter(|u| u.starts_with("https://"))
                    .unwrap_or_else(|| "https://www.curseforge.com/minecraft/modpacks".into()),
                downloads: item.download_count.max(0.0) as u64,
                categories: Vec::new(),
            })
            .collect())
    }

    /// Downloads the newest build of a catalogue modpack that fits the
    /// instance and adds what it describes to the instance's pack.
    pub async fn install_pack(
        &self,
        provider: Provider,
        instance: &Instance,
        project: &str,
        curseforge_key: Option<&str>,
    ) -> Result<MrpackReport> {
        if provider == Provider::Curseforge {
            let key = Self::curseforge_key(curseforge_key)?;
            return self.install_curseforge_pack(instance, project, key).await;
        }
        let versions = self.modrinth_versions(instance, project).await?;
        let chosen = versions
            .first()
            .ok_or(Error::NotFound("a build of this modpack for the instance"))?;
        let file = chosen
            .files
            .iter()
            .filter(|f| f.filename.to_ascii_lowercase().ends_with(".mrpack"))
            .find(|f| f.primary)
            .or_else(|| {
                chosen
                    .files
                    .iter()
                    .find(|f| f.filename.to_ascii_lowercase().ends_with(".mrpack"))
            })
            .ok_or(Error::NotFound("modpack file"))?;
        if https_host(&file.url).as_deref() != Some("cdn.modrinth.com") {
            return Err(Error::Upstream("unexpected download host".into()));
        }
        fs_err::tokio::create_dir_all(&self.tmp).await?;
        let scratch = self.tmp.join(format!("pack-{}.mrpack", Uuid::new_v4()));
        let result = async {
            http::download(
                &self.http,
                &file.url,
                &scratch,
                &Checksum::Sha1(file.hashes.sha1.clone()),
            )
            .await
            .map_err(upstream)?;
            self.import_pack(&instance.id, &scratch, curseforge_key)
                .await
        }
        .await;
        let _ = fs_err::tokio::remove_file(&scratch).await;
        result
    }

    async fn install_curseforge_pack(
        &self,
        instance: &Instance,
        project: &str,
        key: &str,
    ) -> Result<MrpackReport> {
        check_id(project)?;
        let url = url(
            CURSEFORGE,
            &format!("/mods/{project}/files"),
            &[
                ("gameVersion", instance.minecraft_version.as_str()),
                ("pageSize", "30"),
            ],
        )?;
        let files: CfEnvelope<Vec<CfFile>> = self.fetch(url, Some(key)).await?;
        // Server packs are listed next to the pack itself and carry no manifest.
        let chosen = files
            .data
            .iter()
            .find(|f| {
                let name = f.file_name.to_ascii_lowercase();
                name.ends_with(".zip") && !name.contains("server")
            })
            .ok_or(Error::NotFound("a build of this modpack for the instance"))?;
        let Some(source) = chosen.download_url.as_deref() else {
            return Err(Error::Forbidden(
                "the author does not allow third-party downloads of this modpack; \
                 download its zip from CurseForge in a browser and import the file"
                    .into(),
            ));
        };
        if !curseforge_host(source) {
            return Err(Error::Upstream("unexpected download host".into()));
        }
        fs_err::tokio::create_dir_all(&self.tmp).await?;
        let scratch = self.tmp.join(format!("pack-{}.zip", Uuid::new_v4()));
        let result = async {
            http::download(&self.http, source, &scratch, &curseforge_checksum(chosen))
                .await
                .map_err(upstream)?;
            let server_jars = match chosen.server_pack_file_id {
                Some(file) => self.server_pack_jars(project, file, key).await,
                None => None,
            };
            if server_jars.is_none() {
                tracing::info!(
                    instance = %instance.id,
                    pack = %chosen.file_name,
                    "no server pack to compare with; sides come from each mod's own metadata"
                );
            }
            self.import_pack_with(&instance.id, &scratch, Some(key), server_jars.as_ref())
                .await
        }
        .await;
        let _ = fs_err::tokio::remove_file(&scratch).await;
        result
    }

    /// The jars in the server build of a CurseForge modpack. A pack without
    /// a usable server build is not an error: the import falls back to what
    /// each mod says about itself.
    async fn server_pack_jars(
        &self,
        project: &str,
        file: u64,
        key: &str,
    ) -> Option<HashSet<String>> {
        let scratch = self.tmp.join(format!("server-pack-{}.zip", Uuid::new_v4()));
        let result: Result<HashSet<String>> = async {
            let url = url(CURSEFORGE, &format!("/mods/{project}/files/{file}"), &[])?;
            let found: CfEnvelope<CfFile> = self.fetch(url, Some(key)).await?;
            let source = found.data.download_url.as_deref().ok_or_else(|| {
                Error::Forbidden("the author does not allow third-party downloads".into())
            })?;
            if !curseforge_host(source) {
                return Err(Error::Upstream("unexpected download host".into()));
            }
            http::download(
                &self.http,
                source,
                &scratch,
                &curseforge_checksum(&found.data),
            )
            .await
            .map_err(upstream)?;
            let archive = scratch.clone();
            tokio::task::spawn_blocking(move || jar_names(&archive))
                .await
                .map_err(|e| Error::Other(e.into()))?
        }
        .await;
        let _ = fs_err::tokio::remove_file(&scratch).await;
        match result {
            Ok(jars) => Some(jars),
            Err(err) => {
                tracing::warn!(project, file, error = %err, "could not read the server pack");
                None
            }
        }
    }

    async fn modrinth_versions(
        &self,
        instance: &Instance,
        project: &str,
    ) -> Result<Vec<MrVersion>> {
        check_id(project)?;
        let loaders: Vec<&str> = compatible_loaders(instance)?
            .iter()
            .map(|l| l.as_str())
            .collect();
        let url = url(
            MODRINTH,
            &format!("/project/{project}/version"),
            &[
                ("loaders", &serde_json::json!(loaders).to_string()),
                (
                    "game_versions",
                    &serde_json::json!([instance.minecraft_version]).to_string(),
                ),
            ],
        )?;
        self.fetch(url, None).await
    }

    async fn curseforge_files(
        &self,
        instance: &Instance,
        project: &str,
        key: &str,
    ) -> Result<Vec<CfFile>> {
        check_id(project)?;
        let mut files = Vec::new();
        for loader in compatible_loaders(instance)? {
            let url = url(
                CURSEFORGE,
                &format!("/mods/{project}/files"),
                &[
                    ("gameVersion", instance.minecraft_version.as_str()),
                    ("modLoaderType", curseforge_loader(loader)),
                    ("pageSize", "30"),
                ],
            )?;
            let page: CfEnvelope<Vec<CfFile>> = self.fetch(url, Some(key)).await?;
            files.extend(page.data);
            if !files.is_empty() {
                break;
            }
        }
        Ok(files)
    }

    /// Builds of a mod that fit the instance, newest first.
    pub async fn versions(
        &self,
        provider: Provider,
        instance: &Instance,
        project: &str,
        curseforge_key: Option<&str>,
    ) -> Result<Vec<ModVersion>> {
        match provider {
            Provider::Modrinth => Ok(self
                .modrinth_versions(instance, project)
                .await?
                .iter()
                .filter_map(|v| {
                    let file = v.primary()?;
                    Some(ModVersion {
                        id: v.id.clone(),
                        name: v.name.clone(),
                        filename: file.filename.clone(),
                        size: file.size,
                        channel: v.version_type.clone(),
                    })
                })
                .collect()),
            Provider::Curseforge => {
                let key = Self::curseforge_key(curseforge_key)?;
                Ok(self
                    .curseforge_files(instance, project, key)
                    .await?
                    .into_iter()
                    .map(|f| ModVersion {
                        id: f.id.to_string(),
                        name: f.display_name,
                        filename: f.file_name,
                        size: f.file_length,
                        channel: match f.release_type {
                            2 => "beta",
                            3 => "alpha",
                            _ => "release",
                        }
                        .to_owned(),
                    })
                    .collect())
            }
        }
    }

    /// Downloads to a scratch file, verifies it and adds it to the pack.
    async fn ingest(
        &self,
        instance_id: &str,
        path: &str,
        source: &str,
        checksum: Checksum,
        hints: FileHints,
    ) -> Result<FileRecord> {
        fs_err::tokio::create_dir_all(&self.tmp).await?;
        let scratch = self.tmp.join(format!("dl-{}", Uuid::new_v4()));
        let result = async {
            http::download(&self.http, source, &scratch, &checksum)
                .await
                .map_err(upstream)?;
            let file = fs_err::tokio::File::open(&scratch).await?;
            self.modpack
                .add_file(instance_id, path, file, None, hints)
                .await
        }
        .await;
        let _ = fs_err::tokio::remove_file(&scratch).await;
        result
    }

    /// Installs one build of a mod; `version` of `None` picks the newest.
    pub async fn install(
        &self,
        provider: Provider,
        instance: &Instance,
        project: &str,
        version: Option<&str>,
        curseforge_key: Option<&str>,
    ) -> Result<Installed> {
        match provider {
            Provider::Modrinth => {
                let versions = self.modrinth_versions(instance, project).await?;
                let chosen = match version {
                    Some(id) => versions.iter().find(|v| v.id == id),
                    None => versions.first(),
                }
                .ok_or(Error::NotFound("a build of this mod for the instance"))?;
                let file = chosen.primary().ok_or(Error::NotFound("mod file"))?;
                if https_host(&file.url).as_deref() != Some("cdn.modrinth.com") {
                    return Err(Error::Upstream("unexpected download host".into()));
                }
                check_id(&chosen.project_id)?;
                let project: MrProject = self
                    .fetch(
                        url(MODRINTH, &format!("/project/{}", chosen.project_id), &[])?,
                        None,
                    )
                    .await?;
                let hints = FileHints {
                    side: Some(modrinth_side(&project.client_side, &project.server_side)),
                    optional: None,
                };
                let record = self
                    .ingest(
                        &instance.id,
                        &mod_path(&file.filename)?,
                        &file.url,
                        Checksum::Sha1(file.hashes.sha1.clone()),
                        hints,
                    )
                    .await?;

                let required: Vec<&str> = chosen
                    .dependencies
                    .iter()
                    .filter(|d| d.dependency_type == "required")
                    .filter_map(|d| d.project_id.as_deref())
                    .filter(|id| check_id(id).is_ok())
                    .collect();
                let requires = if required.is_empty() {
                    Vec::new()
                } else {
                    let ids = serde_json::json!(required).to_string();
                    let projects: Vec<MrProject> = self
                        .fetch(url(MODRINTH, "/projects", &[("ids", &ids)])?, None)
                        .await
                        .unwrap_or_default();
                    projects
                        .into_iter()
                        .map(|p| Dependency {
                            id: p.id,
                            title: p.title,
                        })
                        .collect()
                };
                Ok(Installed {
                    file: record,
                    requires,
                })
            }
            Provider::Curseforge => {
                let key = Self::curseforge_key(curseforge_key)?;
                let files = self.curseforge_files(instance, project, key).await?;
                let chosen = match version {
                    Some(id) => files.iter().find(|f| f.id.to_string() == id),
                    None => files.first(),
                }
                .ok_or(Error::NotFound("a build of this mod for the instance"))?;
                let Some(source) = chosen.download_url.as_deref() else {
                    return Err(Error::Forbidden(
                        "the author does not allow third-party downloads of this mod; \
                         download it from CurseForge in a browser and upload the file"
                            .into(),
                    ));
                };
                let trusted = https_host(source)
                    .is_some_and(|h| h == "forgecdn.net" || h.ends_with(".forgecdn.net"));
                if !trusted {
                    return Err(Error::Upstream("unexpected download host".into()));
                }
                let checksum = chosen
                    .hashes
                    .iter()
                    .find(|h| h.algo == 1)
                    .map_or(Checksum::None, |h| Checksum::Sha1(h.value.clone()));
                let has = |tag: &str| chosen.game_versions.iter().any(|v| v == tag);
                let side = match (has("Client"), has("Server")) {
                    (true, false) => Some(Side::Client),
                    (false, true) => Some(Side::Server),
                    _ => None,
                };
                let hints = FileHints {
                    side,
                    optional: None,
                };
                let record = self
                    .ingest(
                        &instance.id,
                        &mod_path(&chosen.file_name)?,
                        source,
                        checksum,
                        hints,
                    )
                    .await?;

                let mut requires = Vec::new();
                for dependency in chosen.dependencies.iter().filter(|d| d.relation_type == 3) {
                    let url = url(CURSEFORGE, &format!("/mods/{}", dependency.mod_id), &[])?;
                    if let Ok(found) = self.fetch::<CfEnvelope<CfMod>>(url, Some(key)).await {
                        requires.push(Dependency {
                            id: found.data.id.to_string(),
                            title: found.data.name,
                        });
                    }
                }
                Ok(Installed {
                    file: record,
                    requires,
                })
            }
        }
    }

    /// Adds everything a modpack archive describes to the instance's pack:
    /// a Modrinth `.mrpack` or a CurseForge zip. Files that fail to download
    /// are reported, the rest still lands.
    pub async fn import_pack(
        &self,
        instance_id: &str,
        archive: &Path,
        curseforge_key: Option<&str>,
    ) -> Result<MrpackReport> {
        self.import_pack_with(instance_id, archive, curseforge_key, None)
            .await
    }

    /// `server_jars` are the jars of the pack's server build, when it has
    /// one: the mods missing from it are given to players only.
    async fn import_pack_with(
        &self,
        instance_id: &str,
        archive: &Path,
        curseforge_key: Option<&str>,
        server_jars: Option<&HashSet<String>>,
    ) -> Result<MrpackReport> {
        let instance = instances::get(self.modpack.db(), instance_id).await?;
        let scratch = self.tmp.join(format!("pack-{}", Uuid::new_v4()));
        fs_err::tokio::create_dir_all(&scratch).await?;
        let result = async {
            let (pack, overrides) = {
                let (archive, scratch) = (archive.to_owned(), scratch.clone());
                tokio::task::spawn_blocking(move || open_pack(&archive, &scratch))
                    .await
                    .map_err(|e| Error::Other(e.into()))??
            };
            let mut report = match pack {
                PackArchive::Modrinth(index) => self.import_mrpack_files(&instance, index).await?,
                PackArchive::Curseforge(manifest) => {
                    let key = Self::curseforge_key(curseforge_key)?;
                    self.import_curseforge_files(&instance, manifest, key, server_jars)
                        .await?
                }
            };
            self.import_overrides(&instance, overrides, &mut report)
                .await?;
            report.failed.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(report)
        }
        .await;
        let _ = fs_err::tokio::remove_dir_all(&scratch).await;
        result
    }

    /// Resolves the files a CurseForge manifest lists and downloads those
    /// the authors let third-party tools fetch.
    async fn import_curseforge_files(
        &self,
        instance: &Instance,
        manifest: CfManifest,
        key: &str,
        server_jars: Option<&HashSet<String>>,
    ) -> Result<MrpackReport> {
        let loader = manifest.loader();
        let mismatch = manifest.minecraft.version != instance.minecraft_version
            || loader.as_ref().map(|l| l.kind) != Some(instance.loader.kind);
        let mut report = MrpackReport {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            minecraft_version: Some(manifest.minecraft.version.clone()),
            loader,
            mismatch,
            added: 0,
            failed: Vec::new(),
        };

        let mut files: Vec<CfFile> = Vec::new();
        for batch in manifest.files.chunks(CURSEFORGE_BATCH) {
            let ids: Vec<u64> = batch.iter().map(|f| f.file_id).collect();
            let found: Vec<CfFile> = self
                .curseforge_lookup("/mods/files", serde_json::json!({ "fileIds": ids }), key)
                .await?;
            files.extend(found);
        }
        let mut projects: HashMap<u64, CfMod> = HashMap::new();
        let mut project_ids: Vec<u64> = manifest.files.iter().map(|f| f.project_id).collect();
        project_ids.sort_unstable();
        project_ids.dedup();
        for batch in project_ids.chunks(CURSEFORGE_BATCH) {
            let found: Vec<CfMod> = self
                .curseforge_lookup("/mods", serde_json::json!({ "modIds": batch }), key)
                .await?;
            projects.extend(found.into_iter().map(|item| (item.id, item)));
        }

        struct Job {
            path: String,
            source: String,
            checksum: Checksum,
            hints: FileHints,
        }
        let mod_jars: Vec<String> = files
            .iter()
            .map(|f| f.file_name.to_ascii_lowercase())
            .filter(|name| name.ends_with(".jar"))
            .collect();
        let client_only = client_only_by_server_pack(server_jars, &mod_jars);
        if !client_only.is_empty() {
            tracing::info!(
                instance = %instance.id,
                mods = client_only.len(),
                "mods missing from the server pack go to players only"
            );
        }

        let mut jobs = Vec::new();
        for listed in &manifest.files {
            let project = projects.get(&listed.project_id);
            let title = project.map_or_else(
                || format!("project {}", listed.project_id),
                |p| p.name.clone(),
            );
            let Some(file) = files.iter().find(|f| f.id == listed.file_id) else {
                report.failed.push(MrpackFailure {
                    path: title,
                    error: format!(
                        "file {} is no longer on CurseForge; pick another build of this mod",
                        listed.file_id
                    ),
                });
                continue;
            };
            let name = file
                .file_name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default();
            let Some((folder, side)) = curseforge_folder(project.and_then(|p| p.class_id), name)
            else {
                report.failed.push(MrpackFailure {
                    path: name.to_owned(),
                    error: format!("{title} is neither a mod, a resource pack nor a shader pack"),
                });
                continue;
            };
            let path = match normalize_path(&format!("{folder}/{name}")) {
                Ok(path) => path,
                Err(err) => {
                    report.failed.push(MrpackFailure {
                        path: name.to_owned(),
                        error: err.to_string(),
                    });
                    continue;
                }
            };
            let Some(source) = file.download_url.clone() else {
                let page = project
                    .and_then(|p| p.links.as_ref())
                    .and_then(|l| l.website_url.as_deref())
                    .filter(|u| u.starts_with("https://"))
                    .map(|u| format!("{}/files/{}", u.trim_end_matches('/'), file.id))
                    .unwrap_or_else(|| "CurseForge".to_owned());
                report.failed.push(MrpackFailure {
                    path,
                    error: format!(
                        "the author of {title} does not allow third-party downloads; \
                         download the file from {page} in a browser and upload it"
                    ),
                });
                continue;
            };
            if !curseforge_host(&source) {
                report.failed.push(MrpackFailure {
                    path,
                    error: "unexpected download host".to_owned(),
                });
                continue;
            }
            jobs.push(Job {
                path,
                source,
                checksum: curseforge_checksum(file),
                hints: FileHints {
                    side: side
                        .or_else(|| {
                            client_only
                                .contains(&name.to_ascii_lowercase())
                                .then_some(Side::Client)
                        })
                        .or_else(|| curseforge_side(file)),
                    optional: (!listed.required).then_some(true),
                },
            });
        }

        let downloads = futures_util::stream::iter(jobs.into_iter().map(|job| {
            let (this, instance_id) = (self.clone(), instance.id.clone());
            async move {
                let outcome = this
                    .ingest(
                        &instance_id,
                        &job.path,
                        &job.source,
                        job.checksum,
                        job.hints,
                    )
                    .await;
                (job.path, outcome)
            }
        }))
        .buffer_unordered(6)
        .collect::<Vec<_>>()
        .await;
        for (path, outcome) in downloads {
            match outcome {
                Ok(_) => report.added += 1,
                Err(err) => report.failed.push(MrpackFailure {
                    path,
                    error: err.to_string(),
                }),
            }
        }
        Ok(report)
    }

    async fn import_mrpack_files(
        &self,
        instance: &Instance,
        index: MrpackIndex,
    ) -> Result<MrpackReport> {
        let minecraft_version = index.dependencies.get("minecraft").cloned();
        let loader = index.loader();
        let mismatch = minecraft_version.as_deref() != Some(instance.minecraft_version.as_str())
            || loader.as_ref().map(|l| l.kind) != Some(instance.loader.kind);
        let mut report = MrpackReport {
            name: index.name.clone(),
            version: index.version_id.clone(),
            minecraft_version,
            loader,
            mismatch,
            added: 0,
            failed: Vec::new(),
        };

        // Each download owns what it needs: futures borrowing from here make
        // the whole import fail the `Send` check in a request handler.
        let downloads = futures_util::stream::iter(index.files.into_iter().map(|file| {
            let (this, instance_id) = (self.clone(), instance.id.clone());
            async move {
                let outcome = async {
                    let path = normalize_path(&file.path)?;
                    let source = file
                        .downloads
                        .iter()
                        .find(|u| https_host(u).is_some_and(|h| MRPACK_HOSTS.contains(&h.as_str())))
                        .ok_or_else(|| Error::invalid("no download from an allowed host"))?;
                    let (client, server) =
                        file.env.as_ref().map_or(("required", "required"), |e| {
                            (e.client.as_str(), e.server.as_str())
                        });
                    let hints = FileHints {
                        side: Some(modrinth_side(client, server)),
                        optional: Some(client == "optional" && server != "required"),
                    };
                    let checksum = Checksum::Sha1(file.hashes.sha1.clone());
                    this.ingest(&instance_id, &path, source, checksum, hints)
                        .await
                }
                .await;
                (file.path.clone(), outcome)
            }
        }))
        .buffer_unordered(6)
        .collect::<Vec<_>>()
        .await;
        for (path, outcome) in downloads {
            match outcome {
                Ok(_) => report.added += 1,
                Err(err) => report.failed.push(MrpackFailure {
                    path,
                    error: err.to_string(),
                }),
            }
        }

        Ok(report)
    }

    /// Adds the files the archive carries itself: configs, scripts, worlds.
    async fn import_overrides(
        &self,
        instance: &Instance,
        overrides: Vec<Override>,
        report: &mut MrpackReport,
    ) -> Result<()> {
        // In archive order, so the side-specific trees replace `overrides/`.
        for item in overrides {
            let hints = FileHints {
                side: Some(item.side),
                optional: None,
            };
            let file = fs_err::tokio::File::open(&item.file).await?;
            match self
                .modpack
                .add_file(&instance.id, &item.path, file, None, hints)
                .await
            {
                Ok(_) => report.added += 1,
                Err(err) => report.failed.push(MrpackFailure {
                    path: item.path,
                    error: err.to_string(),
                }),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::blobs::BlobStore;
    use crate::instances::NewInstance;

    #[test]
    fn sides_from_catalogue_metadata() {
        assert_eq!(modrinth_side("required", "unsupported"), Side::Client);
        assert_eq!(modrinth_side("unsupported", "required"), Side::Server);
        assert_eq!(modrinth_side("optional", "required"), Side::Both);
        assert_eq!(modrinth_side("unknown", "required"), Side::Unknown);
        assert_eq!(modrinth_side("", ""), Side::Unknown);
    }

    #[test]
    fn mod_file_names_cannot_escape_the_mods_folder() {
        assert_eq!(
            mod_path("sodium+mc1.21.jar").unwrap(),
            "mods/sodium+mc1.21.jar"
        );
        assert_eq!(mod_path("../../evil.jar").unwrap(), "mods/evil.jar");
        assert!(mod_path("readme.txt").is_err());
        assert!(check_id("AANobbMI").is_ok());
        assert!(check_id("a/../b").is_err());
        assert_eq!(
            https_host("https://cdn.modrinth.com/x").as_deref(),
            Some("cdn.modrinth.com")
        );
        assert_eq!(https_host("http://cdn.modrinth.com/x"), None);
    }

    #[tokio::test]
    async fn mrpack_overrides_are_imported_and_bad_downloads_reported() {
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
        let modpack = Modpack::new(db, blobs);
        let sources = Sources::new(http::client(), modpack.clone(), tmp.path().join("tmp"));

        let archive = tmp.path().join("pack.mrpack");
        let mut zip = zip::ZipWriter::new(fs_err::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("modrinth.index.json", options).unwrap();
        zip.write_all(
            br#"{"formatVersion":1,"game":"minecraft","versionId":"1.0","name":"Test",
                "files":[{"path":"mods/a.jar","hashes":{"sha1":"00","sha512":"00"},
                          "downloads":["https://evil.example/a.jar"],"fileSize":1}],
                "dependencies":{"minecraft":"1.21.1","fabric-loader":"0.16.9"}}"#,
        )
        .unwrap();
        for (name, body) in [
            ("overrides/config/shared.toml", "a"),
            ("server-overrides/config/server.toml", "b"),
            ("client-overrides/options.txt", "c"),
            ("overrides/../../escape.txt", "d"),
            ("stray.txt", "e"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        let report = sources.import_pack("main", &archive, None).await.unwrap();
        assert_eq!(report.name, "Test");
        assert!(!report.mismatch);
        assert_eq!(report.added, 3);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].path, "mods/a.jar");

        let files = modpack.list_files("main").await.unwrap();
        let sides: Vec<(&str, Side)> = files.iter().map(|f| (f.path.as_str(), f.side)).collect();
        assert_eq!(
            sides,
            vec![
                ("config/server.toml", Side::Server),
                ("config/shared.toml", Side::Both),
                ("options.txt", Side::Client),
            ]
        );
        // Scratch space is cleaned up.
        assert_eq!(fs_err::read_dir(tmp.path().join("tmp")).unwrap().count(), 0);
    }

    #[test]
    fn curseforge_archive_is_recognised() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("pack.zip");
        let mut zip = zip::ZipWriter::new(fs_err::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(
            br#"{"minecraft":{"version":"1.12.2",
                  "modLoaders":[{"id":"forge-14.23.5.2860","primary":true}]},
                "manifestType":"minecraftModpack","manifestVersion":1,
                "name":"Reloaded","version":"1.9.0","author":"someone",
                "files":[{"projectID":238222,"fileID":2803400,"required":true},
                         {"projectID":32274,"fileID":2916002,"required":false},
                         {"projectID":1,"fileID":2}],
                "overrides":"overrides"}"#,
        )
        .unwrap();
        for (name, body) in [
            ("overrides/config/forge.cfg", "a"),
            ("overrides/scripts/recipes.zs", "b"),
            ("overrides/../escape.txt", "c"),
            ("modlist.html", "d"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        let scratch = tmp.path().join("scratch");
        fs_err::create_dir_all(&scratch).unwrap();
        let (pack, overrides) = open_pack(&archive, &scratch).unwrap();
        let PackArchive::Curseforge(manifest) = pack else {
            panic!("expected a CurseForge pack");
        };
        assert_eq!(manifest.name, "Reloaded");
        assert_eq!(manifest.minecraft.version, "1.12.2");
        let loader = manifest.loader().unwrap();
        assert_eq!(loader.kind, LoaderKind::Forge);
        assert_eq!(loader.version, "14.23.5.2860");
        let required: Vec<bool> = manifest.files.iter().map(|f| f.required).collect();
        assert_eq!(required, vec![true, false, true]);
        let mut paths: Vec<&str> = overrides.iter().map(|o| o.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, vec!["config/forge.cfg", "scripts/recipes.zs"]);

        assert_eq!(curseforge_folder(Some(6), "a.jar"), Some(("mods", None)));
        assert_eq!(
            curseforge_folder(Some(12), "a.zip"),
            Some(("resourcepacks", Some(Side::Client)))
        );
        assert_eq!(curseforge_folder(Some(17), "world.zip"), None);
        assert_eq!(curseforge_folder(None, "a.jar"), Some(("mods", None)));
    }

    #[test]
    fn an_ordinary_zip_is_not_a_modpack() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("pack.zip");
        let mut zip = zip::ZipWriter::new(fs_err::File::create(&archive).unwrap());
        zip.start_file("readme.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();
        let err = open_pack(&archive, tmp.path()).err().unwrap();
        assert!(err.to_string().contains("manifest.json"), "{err}");
    }

    #[test]
    fn mods_missing_from_the_server_pack_are_client_only() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("server.zip");
        let mut zip = zip::ZipWriter::new(fs_err::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for name in [
            "mods/Tech-1.0.jar",
            "mods/1.12.2/Core.jar",
            "forge-universal.jar",
            "start.sh",
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
        let server = jar_names(&archive).unwrap();
        assert!(server.contains("tech-1.0.jar") && server.contains("core.jar"));
        assert!(!server.contains("start.sh"));

        let mods: Vec<String> = ["tech-1.0.jar", "core.jar", "minimap.jar"]
            .map(String::from)
            .to_vec();
        let client = client_only_by_server_pack(Some(&server), &mods);
        assert_eq!(client, HashSet::from(["minimap.jar".to_owned()]));

        // A server build made of other files says nothing about sides.
        let other = HashSet::from(["unrelated.jar".to_owned()]);
        assert!(client_only_by_server_pack(Some(&other), &mods).is_empty());
        assert!(client_only_by_server_pack(None, &mods).is_empty());
    }
}
