//! Game client installation and launch command construction.
//!
//! Everything lives under one root laid out like the official launcher's
//! directory (`versions/`, `libraries/`, `assets/`), shared by all
//! instances; only the game directory is per instance.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use relic_proto::{Loader, LoaderKind};
use serde::Deserialize;
use tokio::fs;

use crate::http::{Checksum, download, get_text};
use crate::mojang::{self, version_key};
use crate::profile::{Library, Os, Profile, select, substitute};
use crate::{Error, Result, forge_client, java};

const RESOURCES_URL: &str = "https://resources.download.minecraft.net";
const PARALLEL_DOWNLOADS: usize = 16;

/// What `prepare` is busy with. `total` is zero while the amount of work
/// is not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage {
    pub label: &'static str,
    pub done: u64,
    pub total: u64,
}

pub type Progress<'a> = &'a (dyn Fn(Stage) + Send + Sync);

fn stage(progress: Progress<'_>, label: &'static str) {
    progress(Stage {
        label,
        done: 0,
        total: 0,
    });
}

/// Version ids come from the network and become directory names.
pub(crate) fn safe_id(id: &str) -> Result<&str> {
    let bad = id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\', ':', '\0']);
    if bad {
        return Err(Error::Unsupported(format!("unusable version id {id:?}")));
    }
    Ok(id)
}

pub(crate) fn profile_path(root: &Path, id: &str) -> PathBuf {
    root.join("versions").join(id).join(format!("{id}.json"))
}

pub(crate) fn jar_path(root: &Path, id: &str) -> PathBuf {
    root.join("versions").join(id).join(format!("{id}.jar"))
}

/// Absolute path of a file given relative to the libraries directory.
pub(crate) fn library_path(root: &Path, relative: &str) -> PathBuf {
    let mut path = root.join("libraries");
    path.extend(relative.split('/'));
    path
}

async fn read_profile(root: &Path, id: &str) -> Result<Option<Profile>> {
    match fs::read(profile_path(root, id)).await {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(crate) async fn write_profile(root: &Path, id: &str, json: &[u8]) -> Result<()> {
    let path = profile_path(root, safe_id(id)?);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, json).await?;
    Ok(())
}

fn parse_profile(url: &str, text: &str) -> Result<Profile> {
    serde_json::from_str(text).map_err(|e| Error::Decode {
        url: url.to_owned(),
        message: e.to_string(),
    })
}

/// The game's own version document, from disk when it is there so that a
/// prepared game starts without Mojang's servers.
async fn vanilla_profile(client: &reqwest::Client, root: &Path, mc: &str) -> Result<Profile> {
    safe_id(mc)?;
    if let Some(profile) = read_profile(root, mc).await? {
        return Ok(profile);
    }
    let manifest = mojang::version_manifest(client).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == mc)
        .ok_or_else(|| Error::NotFound(format!("unknown Minecraft version {mc}")))?;
    let text = get_text(client, &entry.url).await?;
    let profile = parse_profile(&entry.url, &text)?;
    write_profile(root, mc, text.as_bytes()).await?;
    Ok(profile)
}

fn loader_marker(root: &Path, mc: &str, loader: &Loader) -> PathBuf {
    let name = format!("{:?}-{mc}-{}.id", loader.kind, loader.version).to_lowercase();
    root.join("versions").join(".relic").join(name)
}

/// Makes sure the loader's version document (and, for Forge and NeoForge,
/// the files its installer produces) is in place. Returns the version id
/// to launch.
async fn ensure_loader(
    client: &reqwest::Client,
    root: &Path,
    java: &Path,
    mc: &str,
    loader: &Loader,
    progress: Progress<'_>,
) -> Result<String> {
    if loader.kind == LoaderKind::Vanilla {
        return Ok(mc.to_owned());
    }
    if loader.version.is_empty() {
        return Err(Error::Unsupported("loader version is required".into()));
    }
    let marker = loader_marker(root, mc, loader);
    if let Ok(id) = fs::read_to_string(&marker).await
        && safe_id(id.trim()).is_ok()
        && read_profile(root, id.trim()).await?.is_some()
    {
        return Ok(id.trim().to_owned());
    }

    stage(progress, "Installing mod loader");
    let version = loader.version.as_str();
    let id = match loader.kind {
        LoaderKind::Vanilla => unreachable!("handled above"),
        LoaderKind::Fabric | LoaderKind::Quilt => {
            let url = if loader.kind == LoaderKind::Fabric {
                format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{version}/profile/json")
            } else {
                format!("https://meta.quiltmc.org/v3/versions/loader/{mc}/{version}/profile/json")
            };
            let text = get_text(client, &url).await?;
            let profile = parse_profile(&url, &text)?;
            write_profile(root, &profile.id, text.as_bytes()).await?;
            profile.id
        }
        LoaderKind::Forge => {
            let full = format!("{mc}-{version}");
            let url = format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"
            );
            forge_client::install(client, root, java, mc, &url).await?
        }
        LoaderKind::NeoForge => {
            let url = format!(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"
            );
            forge_client::install(client, root, java, mc, &url).await?
        }
    };
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(&marker, &id).await?;
    Ok(id)
}

/// A file to have on disk.
pub(crate) struct Fetch {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl Fetch {
    /// Whether the file is already there. Sizes are compared rather than
    /// digests: hashing every library and asset on each start would take
    /// longer than starting the game.
    async fn present(&self) -> bool {
        match fs::metadata(&self.dest).await {
            Ok(meta) => match (self.size, &self.sha1) {
                (Some(size), _) => meta.len() == size,
                // `download` compares digests itself.
                (None, Some(_)) => false,
                (None, None) => true,
            },
            Err(_) => false,
        }
    }
}

pub(crate) async fn fetch_all(
    client: &reqwest::Client,
    items: Vec<Fetch>,
    label: &'static str,
    progress: Progress<'_>,
) -> Result<()> {
    let mut missing = Vec::new();
    for item in items {
        if !item.present().await {
            missing.push(item);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    let total = missing.len() as u64;
    let done = AtomicU64::new(0);
    progress(Stage {
        label,
        done: 0,
        total,
    });
    // Boxed and collected up front: the compiler cannot otherwise prove a
    // stream of borrowing futures is `Send` once a caller spawns it.
    let done = &done;
    let jobs: Vec<BoxFuture<'_, Result<()>>> = missing
        .iter()
        .map(|item| -> BoxFuture<'_, Result<()>> {
            Box::pin(async move {
                let checksum = match &item.sha1 {
                    Some(sha1) => Checksum::Sha1(sha1.clone()),
                    None => Checksum::None,
                };
                download(client, &item.url, &item.dest, &checksum).await?;
                progress(Stage {
                    label,
                    done: done.fetch_add(1, Ordering::Relaxed) + 1,
                    total,
                });
                Ok(())
            })
        })
        .collect();
    let mut downloads = futures_util::stream::iter(jobs).buffer_unordered(PARALLEL_DOWNLOADS);
    while let Some(result) = downloads.next().await {
        result?;
    }
    Ok(())
}

/// Unpacks a pre-1.19 bundle of native libraries.
fn extract_natives(jar: &Path, into: &Path, exclude: &[String]) -> Result<()> {
    let archive_err = |e: zip::result::ZipError| Error::Archive(format!("{}: {e}", jar.display()));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?).map_err(archive_err)?;
    std::fs::create_dir_all(into)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(archive_err)?;
        if entry.is_dir()
            || exclude
                .iter()
                .any(|prefix| entry.name().starts_with(prefix))
        {
            continue;
        }
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let dest = into.join(relative);
        if std::fs::metadata(&dest).is_ok_and(|meta| meta.len() == entry.size()) {
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut entry, &mut std::fs::File::create(&dest)?)?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

async fn ensure_assets(
    client: &reqwest::Client,
    root: &Path,
    profile: &Profile,
    progress: Progress<'_>,
) -> Result<()> {
    let Some(index) = &profile.asset_index else {
        return Ok(());
    };
    safe_id(&index.id)?;
    let assets = root.join("assets");
    let index_path = assets.join("indexes").join(format!("{}.json", index.id));
    download(
        client,
        &index.url,
        &index_path,
        &Checksum::Sha1(index.sha1.clone()),
    )
    .await?;
    let parsed: AssetIndex =
        serde_json::from_slice(&fs::read(&index_path).await?).map_err(|e| Error::Decode {
            url: index.url.clone(),
            message: e.to_string(),
        })?;

    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for object in parsed.objects.into_values() {
        let hash = object.hash;
        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Decode {
                url: index.url.clone(),
                message: format!("bad asset hash {hash:?}"),
            });
        }
        if !seen.insert(hash.clone()) {
            continue;
        }
        let prefix = &hash[..2];
        items.push(Fetch {
            url: format!("{RESOURCES_URL}/{prefix}/{hash}"),
            dest: assets.join("objects").join(prefix).join(&hash),
            sha1: Some(hash),
            size: Some(object.size),
        });
    }
    fetch_all(client, items, "Downloading assets", progress).await
}

pub(crate) fn library_fetches(root: &Path, libraries: &[Library], os: Os) -> Result<Vec<Fetch>> {
    let mut items = Vec::new();
    for library in libraries.iter().filter(|library| library.applies(os)) {
        for file in [library.artifact()?, library.native(os)?]
            .into_iter()
            .flatten()
        {
            let dest = library_path(root, &file.path);
            match file.url {
                Some(url) => items.push(Fetch {
                    url,
                    dest,
                    sha1: file.sha1,
                    size: file.size,
                }),
                // Produced by an installer; nothing to do if it is there.
                None if dest.exists() => {}
                None => {
                    return Err(Error::NotFound(format!(
                        "library {} is missing and cannot be downloaded",
                        library.name
                    )));
                }
            }
        }
    }
    Ok(items)
}

pub struct PrepareRequest<'a> {
    /// Shared game files: `versions/`, `libraries/`, `assets/`.
    pub root: &'a Path,
    /// Directory holding the shared Java runtimes.
    pub runtimes: &'a Path,
    pub minecraft_version: &'a str,
    pub loader: &'a Loader,
}

/// A client ready to start.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub root: PathBuf,
    pub minecraft_version: String,
    /// The merged version document.
    pub profile: Profile,
    pub java: PathBuf,
    pub java_major: u32,
    pub classpath: Vec<PathBuf>,
    pub natives: PathBuf,
    /// JVM argument selecting Mojang's logging configuration.
    pub logging: Option<String>,
}

/// Downloads and installs whatever is missing for this game and loader
/// version. Cheap when nothing is.
pub async fn prepare(
    client: &reqwest::Client,
    req: &PrepareRequest<'_>,
    progress: Progress<'_>,
) -> Result<Prepared> {
    let (root, mc) = (req.root, req.minecraft_version);
    if !mojang::is_supported(mc) {
        return Err(Error::Unsupported(format!(
            "Minecraft {mc} is older than {}",
            mojang::MIN_SUPPORTED
        )));
    }
    let os = Os::current();

    stage(progress, "Checking game files");
    let vanilla = vanilla_profile(client, root, mc).await?;
    let java_major = vanilla.java_major();
    if java::installed_java(req.runtimes, java_major).is_none() {
        stage(progress, "Downloading Java");
    }
    let java = java::ensure_runtime(client, req.runtimes, java_major).await?;

    let jar = vanilla
        .downloads
        .as_ref()
        .and_then(|d| d.client.clone())
        .ok_or_else(|| Error::Unsupported(format!("Minecraft {mc} has no client download")))?;
    let vanilla_jar = jar_path(root, mc);
    fetch_all(
        client,
        vec![Fetch {
            url: jar.url,
            dest: vanilla_jar.clone(),
            sha1: Some(jar.sha1),
            size: Some(jar.size),
        }],
        "Downloading the game",
        progress,
    )
    .await?;

    let id = ensure_loader(client, root, &java, mc, req.loader, progress).await?;
    let profile = if id == mc {
        vanilla
    } else {
        let loader = read_profile(root, &id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("version {id} is not installed")))?;
        match loader.inherits_from.as_deref() {
            Some(parent) if parent != mc => {
                return Err(Error::Unsupported(format!(
                    "{id} is built for Minecraft {parent}, not {mc}"
                )));
            }
            _ => loader.over(vanilla),
        }
    };

    // Loaders expect the game JAR under their own version's name.
    let game_jar = jar_path(root, &id);
    if id != mc {
        let same = fs::metadata(&game_jar)
            .await
            .is_ok_and(|meta| meta.len() == jar.size);
        if !same {
            let _ = fs::remove_file(&game_jar).await;
            if fs::hard_link(&vanilla_jar, &game_jar).await.is_err() {
                fs::copy(&vanilla_jar, &game_jar).await?;
            }
        }
    }

    let fetches = library_fetches(root, &profile.libraries, os)?;
    fetch_all(client, fetches, "Downloading libraries", progress).await?;

    let natives = root.join("versions").join(&id).join("natives");
    fs::create_dir_all(&natives).await?;
    let mut classpath = Vec::new();
    let mut seen = HashSet::new();
    for library in profile.libraries.iter().filter(|l| l.applies(os)) {
        if let Some(file) = library.artifact()?
            && seen.insert(file.path.clone())
        {
            classpath.push(library_path(root, &file.path));
        }
        if let Some(file) = library.native(os)? {
            let bundle = library_path(root, &file.path);
            let into = natives.clone();
            let exclude = library
                .extract
                .as_ref()
                .map(|e| e.exclude.clone())
                .unwrap_or_default();
            tokio::task::spawn_blocking(move || extract_natives(&bundle, &into, &exclude))
                .await
                .map_err(|e| Error::Archive(e.to_string()))??;
        }
    }
    classpath.push(game_jar);

    ensure_assets(client, root, &profile, progress).await?;

    let mut logging = None;
    if let Some(config) = profile.logging.as_ref().and_then(|l| l.client.as_ref()) {
        let path = root
            .join("assets")
            .join("log_configs")
            .join(safe_id(&config.file.id)?);
        download(
            client,
            &config.file.url,
            &path,
            &Checksum::Sha1(config.file.sha1.clone()),
        )
        .await?;
        let values = HashMap::from([("path", path.to_string_lossy().into_owned())]);
        logging = Some(substitute(&config.argument, &values));
    }

    Ok(Prepared {
        root: root.to_owned(),
        minecraft_version: mc.to_owned(),
        profile,
        java,
        java_major,
        classpath,
        natives,
        logging,
    })
}

pub struct LaunchOptions<'a> {
    pub game_dir: &'a Path,
    pub memory_mb: u32,
    pub player_name: &'a str,
    /// Without dashes.
    pub uuid: &'a str,
    pub access_token: &'a str,
    /// Extra JVM arguments placed before the game's own (the
    /// authlib-injector agent).
    pub jvm_args: &'a [String],
    /// Server to join once the game is up.
    pub join: Option<(&'a str, u16)>,
}

/// Arguments for the Java executable, in order.
pub fn launch_arguments(prepared: &Prepared, options: &LaunchOptions<'_>) -> Result<Vec<String>> {
    let os = Os::current();
    let profile = &prepared.profile;
    let main_class = profile
        .main_class
        .clone()
        .ok_or_else(|| Error::Unsupported(format!("version {} has no main class", profile.id)))?;
    let text = |path: &Path| path.to_string_lossy().into_owned();
    let classpath = prepared
        .classpath
        .iter()
        .map(|path| text(path))
        .collect::<Vec<_>>()
        .join(os.classpath_separator());
    let assets = prepared.root.join("assets");

    let values: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", options.player_name.to_owned()),
        ("version_name", profile.id.clone()),
        ("game_directory", text(options.game_dir)),
        ("assets_root", text(&assets)),
        ("game_assets", text(&assets.join("virtual").join("legacy"))),
        (
            "assets_index_name",
            profile
                .asset_index
                .as_ref()
                .map(|index| index.id.clone())
                .unwrap_or_default(),
        ),
        ("auth_uuid", options.uuid.to_owned()),
        ("auth_access_token", options.access_token.to_owned()),
        ("auth_session", options.access_token.to_owned()),
        ("clientid", String::new()),
        ("auth_xuid", String::new()),
        ("user_type", "mojang".to_owned()),
        ("user_properties", "{}".to_owned()),
        (
            "version_type",
            profile.kind.clone().unwrap_or_else(|| "release".into()),
        ),
        ("natives_directory", text(&prepared.natives)),
        ("launcher_name", "relic".to_owned()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_owned()),
        ("classpath", classpath),
        ("classpath_separator", os.classpath_separator().to_owned()),
        ("library_directory", text(&prepared.root.join("libraries"))),
    ]);

    let mut args = vec![
        format!("-Xmx{}M", options.memory_mb.max(512)),
        // Log4j 2.10 – 2.14 (Minecraft 1.17 – 1.18) honours this switch;
        // older ones are covered by Mojang's logging configuration.
        "-Dlog4j2.formatMsgNoLookups=true".to_owned(),
    ];
    args.extend(options.jvm_args.iter().cloned());
    args.extend(prepared.logging.clone());

    let jvm = profile
        .arguments
        .as_ref()
        .map(|a| select(&a.jvm, os))
        .filter(|jvm| !jvm.is_empty())
        .unwrap_or_else(|| {
            // Versions before 1.13 leave these to the launcher.
            let mut legacy = Vec::new();
            if os.name == "osx" {
                legacy.push("-XstartOnFirstThread".to_owned());
            }
            legacy.extend(
                [
                    "-Djava.library.path=${natives_directory}",
                    "-cp",
                    "${classpath}",
                ]
                .map(str::to_owned),
            );
            legacy
        });
    args.extend(jvm.iter().map(|arg| substitute(arg, &values)));
    args.push(main_class);

    let game = match &profile.arguments {
        Some(arguments) if !arguments.game.is_empty() => select(&arguments.game, os),
        _ => profile
            .minecraft_arguments
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
    };
    args.extend(game.iter().map(|arg| substitute(arg, &values)));

    if let Some((host, port)) = options.join {
        if version_key(&prepared.minecraft_version) >= vec![1, 20] {
            args.extend([
                "--quickPlayMultiplayer".to_owned(),
                format!("{host}:{port}"),
            ]);
        } else {
            args.extend([
                "--server".to_owned(),
                host.to_owned(),
                "--port".to_owned(),
                port.to_string(),
            ]);
        }
    }
    Ok(args)
}

/// Writes `args` as a Java argument file (`java @file`). Keeps the command
/// line under the operating system's limit and the access token out of the
/// process list. Only Java 9 and newer read these.
pub fn argument_file(args: &[String]) -> String {
    let mut out = String::new();
    for arg in args {
        out.push('"');
        for c in arg.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                c => out.push(c),
            }
        }
        out.push_str("\"\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared(profile: &str, mc: &str) -> Prepared {
        Prepared {
            root: PathBuf::from("root"),
            minecraft_version: mc.to_owned(),
            profile: serde_json::from_str(profile).unwrap(),
            java: PathBuf::from("java"),
            java_major: 21,
            classpath: vec![PathBuf::from("a.jar"), PathBuf::from("b.jar")],
            natives: PathBuf::from("natives"),
            logging: Some("-Dlog4j.configurationFile=log.xml".into()),
        }
    }

    fn options<'a>(jvm_args: &'a [String], join: Option<(&'a str, u16)>) -> LaunchOptions<'a> {
        LaunchOptions {
            game_dir: Path::new("game"),
            memory_mb: 4096,
            player_name: "Steve",
            uuid: "00000000000000000000000000000001",
            access_token: "token",
            jvm_args,
            join,
        }
    }

    #[test]
    fn modern_versions_launch_from_their_argument_lists() {
        let prepared = prepared(
            r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","type":"release",
                "assetIndex":{"id":"17","sha1":"aa","url":"https://example.invalid/17.json"},
                "arguments":{
                    "jvm":["-Djava.library.path=${natives_directory}","-cp","${classpath}"],
                    "game":["--username","${auth_player_name}","--uuid","${auth_uuid}",
                        "--assetIndex","${assets_index_name}",
                        {"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"}]}}"#,
            "1.21.1",
        );
        let agent = ["-javaagent:agent.jar=http://127.0.0.1:1".to_owned()];
        let args =
            launch_arguments(&prepared, &options(&agent, Some(("mc.example", 25565)))).unwrap();
        let separator = Os::current().classpath_separator();
        assert_eq!(
            args,
            [
                "-Xmx4096M",
                "-Dlog4j2.formatMsgNoLookups=true",
                "-javaagent:agent.jar=http://127.0.0.1:1",
                "-Dlog4j.configurationFile=log.xml",
                "-Djava.library.path=natives",
                "-cp",
                &format!("a.jar{separator}b.jar"),
                "net.minecraft.client.main.Main",
                "--username",
                "Steve",
                "--uuid",
                "00000000000000000000000000000001",
                "--assetIndex",
                "17",
                "--quickPlayMultiplayer",
                "mc.example:25565",
            ]
        );
    }

    #[test]
    fn old_versions_launch_from_one_argument_string() {
        let prepared = prepared(
            r#"{"id":"1.12.2","mainClass":"net.minecraft.launchwrapper.Launch",
                "minecraftArguments":"--username ${auth_player_name} --accessToken ${auth_access_token} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker"}"#,
            "1.12.2",
        );
        let args = launch_arguments(&prepared, &options(&[], Some(("mc.example", 25570)))).unwrap();
        let main = args
            .iter()
            .position(|arg| arg == "net.minecraft.launchwrapper.Launch")
            .unwrap();
        assert_eq!(args[main - 2], "-cp");
        assert!(args[..main].contains(&"-Djava.library.path=natives".to_owned()));
        assert_eq!(
            args[main + 1..],
            [
                "--username",
                "Steve",
                "--accessToken",
                "token",
                "--tweakClass",
                "net.minecraftforge.fml.common.launcher.FMLTweaker",
                "--server",
                "mc.example",
                "--port",
                "25570",
            ]
        );
    }

    #[test]
    fn argument_files_quote_every_argument() {
        let args = [r"C:\game dir\a.jar".to_owned(), r#"say "hi""#.to_owned()];
        assert_eq!(
            argument_file(&args),
            "\"C:\\\\game dir\\\\a.jar\"\n\"say \\\"hi\\\"\"\n"
        );
    }

    #[test]
    fn version_ids_cannot_leave_the_versions_directory() {
        assert!(safe_id("1.21.1").is_ok());
        assert!(safe_id("fabric-loader-0.16.5-1.21.1").is_ok());
        assert!(safe_id("..").is_err());
        assert!(safe_id("a/b").is_err());
        assert!(safe_id(r"a\b").is_err());
        assert!(safe_id("").is_err());
    }

    #[test]
    fn natives_are_unpacked_without_excluded_entries() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("natives.jar");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("lwjgl.dll", options).unwrap();
        writer.write_all(b"native").unwrap();
        writer.start_file("META-INF/MANIFEST.MF", options).unwrap();
        writer.write_all(b"manifest").unwrap();
        writer.finish().unwrap();

        let into = dir.path().join("out");
        extract_natives(&jar, &into, &["META-INF/".to_owned()]).unwrap();
        assert_eq!(std::fs::read(into.join("lwjgl.dll")).unwrap(), b"native");
        assert!(!into.join("META-INF").exists());
    }
}
