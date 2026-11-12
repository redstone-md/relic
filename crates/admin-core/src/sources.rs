//! Where mods come from besides manual upload: Modrinth, CurseForge and
//! `.mrpack` archives.

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

/// A ready-made modpack in the Modrinth catalogue.
#[derive(Debug, Clone, Serialize)]
pub struct PackHit {
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
}

#[derive(Deserialize)]
struct CfAuthor {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfLogo {
    thumbnail_url: Option<String>,
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

/// Reads the index and unpacks the override trees into `scratch`.
fn open_mrpack(archive: &Path, scratch: &Path) -> Result<(MrpackIndex, Vec<Override>)> {
    let bad = |e: zip::result::ZipError| Error::invalid(format!("not a valid .mrpack: {e}"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?).map_err(bad)?;
    if zip.len() > MRPACK_MAX_ENTRIES {
        return Err(Error::invalid("the .mrpack has too many entries"));
    }
    let index: MrpackIndex = {
        let entry = zip
            .by_name("modrinth.index.json")
            .map_err(|_| Error::invalid("the archive has no modrinth.index.json"))?;
        serde_json::from_reader(entry.take(16 * 1024 * 1024))
            .map_err(|e| Error::invalid(format!("unreadable modrinth.index.json: {e}")))?
    };
    if index.format_version != 1 || index.game != "minecraft" {
        return Err(Error::invalid("unsupported .mrpack format"));
    }

    let mut overrides = Vec::new();
    let mut unpacked = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(bad)?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        // Later trees win over `overrides/`; the client tree is not ours to run.
        let (side, rest) = if let Some(rest) = name.strip_prefix("overrides/") {
            (Side::Both, rest)
        } else if let Some(rest) = name.strip_prefix("server-overrides/") {
            (Side::Server, rest)
        } else if let Some(rest) = name.strip_prefix("client-overrides/") {
            (Side::Client, rest)
        } else {
            continue;
        };
        let Ok(path) = normalize_path(rest) else {
            continue;
        };
        unpacked += entry.size();
        if unpacked > MRPACK_MAX_UNPACKED {
            return Err(Error::invalid("the .mrpack unpacks to more than 2 GiB"));
        }
        let file = scratch.join(format!("o{i}"));
        let mut out = std::fs::File::create(&file)?;
        // `size()` is only a claim; cap what is actually written.
        let written = std::io::copy(&mut (&mut entry).take(MRPACK_MAX_UNPACKED), &mut out)?;
        if written > entry.size() {
            return Err(Error::invalid("the .mrpack lies about its entry sizes"));
        }
        overrides.push(Override { path, side, file });
    }
    Ok((index, overrides))
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
        instance: &Instance,
        query: &str,
        category: Option<&str>,
        sort: Option<&str>,
    ) -> Result<Vec<PackHit>> {
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

    /// Downloads the newest build of a catalogue modpack that fits the
    /// instance and adds what it describes to the instance's pack.
    pub async fn install_pack(&self, instance: &Instance, project: &str) -> Result<MrpackReport> {
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
        tokio::fs::create_dir_all(&self.tmp).await?;
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
            self.import_mrpack(&instance.id, &scratch).await
        }
        .await;
        let _ = tokio::fs::remove_file(&scratch).await;
        result
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
        tokio::fs::create_dir_all(&self.tmp).await?;
        let scratch = self.tmp.join(format!("dl-{}", Uuid::new_v4()));
        let result = async {
            http::download(&self.http, source, &scratch, &checksum)
                .await
                .map_err(upstream)?;
            let file = tokio::fs::File::open(&scratch).await?;
            self.modpack
                .add_file(instance_id, path, file, None, hints)
                .await
        }
        .await;
        let _ = tokio::fs::remove_file(&scratch).await;
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

    /// Adds everything a `.mrpack` describes to the instance's pack. Files
    /// that fail to download are reported, the rest still lands.
    pub async fn import_mrpack(&self, instance_id: &str, archive: &Path) -> Result<MrpackReport> {
        let instance = instances::get(self.modpack.db(), instance_id).await?;
        let scratch = self.tmp.join(format!("mrpack-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&scratch).await?;
        let result = self.import_mrpack_in(&instance, archive, &scratch).await;
        let _ = tokio::fs::remove_dir_all(&scratch).await;
        result
    }

    async fn import_mrpack_in(
        &self,
        instance: &Instance,
        archive: &Path,
        scratch: &Path,
    ) -> Result<MrpackReport> {
        let (index, overrides) = {
            let (archive, scratch) = (archive.to_owned(), scratch.to_owned());
            tokio::task::spawn_blocking(move || open_mrpack(&archive, &scratch))
                .await
                .map_err(|e| Error::Other(e.into()))??
        };
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

        // In archive order, so the side-specific trees replace `overrides/`.
        for item in overrides {
            let hints = FileHints {
                side: Some(item.side),
                optional: None,
            };
            let file = tokio::fs::File::open(&item.file).await?;
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
        report.failed.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(report)
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
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
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

        let report = sources.import_mrpack("main", &archive).await.unwrap();
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
        assert_eq!(
            std::fs::read_dir(tmp.path().join("tmp")).unwrap().count(),
            0
        );
    }
}
