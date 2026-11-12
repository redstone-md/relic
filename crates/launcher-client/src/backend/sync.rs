//! Brings a game directory in line with the server's manifest: fetches
//! what is missing or changed, removes what no longer belongs, and leaves
//! the player's own files alone.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use relic_mc::reqwest::{StatusCode, header};
use relic_proto::{LOCKABLE_DIRS, Manifest, ManifestFile, ModPolicy, SyncAction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::api::Client;
use crate::lang::{Msg, tf};

const PARALLEL_DOWNLOADS: usize = 4;
const ATTEMPTS: u32 = 3;
/// The launcher's own corner of a game directory.
pub const PRIVATE_DIR: &str = ".relic";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", tf(Msg::PathUnusable, &[.0]))]
    Path(String),
    #[error("{}", tf(Msg::DownloadFailed, &[path, reason]))]
    Download { path: String, reason: String },
    #[error("{}", tf(Msg::Corrupt, &[.0]))]
    Corrupt(String),
    #[error("{}", tf(Msg::FilesFailed, &[.0]))]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Manifest paths become paths on the player's disk; anything that could
/// point outside the game directory is refused.
fn checked(path: &str) -> Result<&str> {
    let bad = path.is_empty()
        || path.starts_with('/')
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path.split('/').next() == Some(PRIVATE_DIR);
    if bad {
        return Err(Error::Path(path.to_owned()));
    }
    Ok(path)
}

fn local(game_dir: &Path, path: &str) -> PathBuf {
    let mut out = game_dir.to_owned();
    out.extend(path.split('/'));
    out
}

/// How a path is compared: Windows and (by default) macOS do not tell
/// `Mods/A.jar` from `mods/a.jar`.
fn key(path: &str) -> String {
    if cfg!(any(windows, target_os = "macos")) {
        path.to_lowercase()
    } else {
        path.to_owned()
    }
}

/// Glob match where `*` spans any characters, directory separators
/// included (so `saves/*` covers everything below), and `?` is one.
fn glob(pattern: &str, text: &str) -> bool {
    let (pattern, text): (Vec<char>, Vec<char>) =
        (pattern.chars().collect(), text.chars().collect());
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

fn protected(manifest: &Manifest, path: &str) -> bool {
    let path = key(path);
    manifest
        .protected_paths
        .iter()
        .any(|pattern| glob(&key(pattern), &path))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Known {
    sha256: String,
    size: u64,
    /// Modification time when the content was last verified.
    mtime_ms: u64,
}

/// What the launcher put into a game directory, so that it neither hashes
/// unchanged files again nor deletes files it did not bring.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    files: BTreeMap<String, Known>,
    /// Hash of what was delivered for each file the player may edit. A newer
    /// version from the server replaces such a file only while it still has
    /// this content.
    #[serde(default)]
    once: BTreeMap<String, String>,
}

impl State {
    fn path(game_dir: &Path) -> PathBuf {
        game_dir.join(PRIVATE_DIR).join("sync.json")
    }

    pub fn load(game_dir: &Path) -> Self {
        std::fs::read(Self::path(game_dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, game_dir: &Path) -> io::Result<()> {
        let path = Self::path(game_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as u64)
}

fn sha256_of(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// What has to change on disk.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plan {
    pub download: Vec<ManifestFile>,
    /// Game-directory-relative paths, forward slashes.
    pub delete: Vec<String>,
    /// Downloads that become the player's own once they land.
    pub once: BTreeSet<String>,
}

impl Plan {
    pub fn bytes(&self) -> u64 {
        self.download.iter().map(|file| file.size).sum()
    }
}

/// One thing found in a folder the server controls.
struct Found {
    path: String,
    /// A symbolic link or junction: it could point at files kept elsewhere,
    /// so it is never followed and never belongs.
    link: bool,
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<Found>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}/{name}");
        match entry.file_type() {
            Ok(kind) if kind.is_symlink() => out.push(Found { path, link: true }),
            Ok(kind) if kind.is_dir() => walk(&entry.path(), &path, out),
            Ok(_) => out.push(Found { path, link: false }),
            Err(_) => {}
        }
    }
}

/// The optional files that end up installed: the ones the player picked and
/// everything those (and the required files) cannot load without.
pub fn resolve(manifest: &Manifest, enabled: &BTreeSet<String>) -> BTreeSet<String> {
    let by_path: BTreeMap<&str, &ManifestFile> = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut on = BTreeSet::new();
    let mut queue: Vec<&ManifestFile> = Vec::new();
    for file in &manifest.files {
        if !file.optional || (enabled.contains(&file.path) && on.insert(file.path.clone())) {
            queue.push(file);
        }
    }
    while let Some(file) = queue.pop() {
        for needed in &file.requires {
            if let Some(target) = by_path.get(needed.as_str())
                && target.optional
                && on.insert(target.path.clone())
            {
                queue.push(target);
            }
        }
    }
    on
}

/// Compares the game directory with the manifest. Reads and hashes files;
/// run it off the UI thread. `state` loses entries for files that are gone
/// and gains verified ones.
pub fn plan(
    manifest: &Manifest,
    enabled: &BTreeSet<String>,
    game_dir: &Path,
    state: &mut State,
) -> Result<Plan> {
    let mut plan = Plan::default();
    let mut wanted = BTreeSet::new();
    let mut enforced = BTreeMap::new();
    let mut once = BTreeMap::new();
    let enabled = resolve(manifest, enabled);

    for file in &manifest.files {
        let path = checked(&file.path)?;
        if file.optional && !enabled.contains(path) {
            continue;
        }
        if !wanted.insert(key(path)) {
            continue;
        }
        let dest = local(game_dir, path);
        let meta = std::fs::metadata(&dest).ok().filter(|meta| meta.is_file());
        // The player's settings arrive once, whatever the pack says.
        let action = if protected(manifest, path) {
            SyncAction::SyncOnce
        } else {
            file.action
        };
        match (action, meta) {
            (SyncAction::SyncOnce, None) => {
                plan.once.insert(file.path.clone());
                plan.download.push(file.clone());
            }
            (SyncAction::SyncOnce, Some(_)) => match state.once.get(path) {
                // Nothing new from the server.
                Some(base) if base.eq_ignore_ascii_case(&file.sha256) => {
                    once.insert(path.to_owned(), base.clone());
                }
                Some(base) => {
                    let current = sha256_of(&dest)?;
                    if current.eq_ignore_ascii_case(&file.sha256) {
                        once.insert(path.to_owned(), file.sha256.clone());
                    } else if current.eq_ignore_ascii_case(base) {
                        // The player never touched it: take the new version.
                        plan.once.insert(file.path.clone());
                        plan.download.push(file.clone());
                    } else {
                        once.insert(path.to_owned(), base.clone());
                    }
                }
                // Not brought by the launcher, so the player's from the start.
                None => {}
            },
            (SyncAction::Enforce, None) => plan.download.push(file.clone()),
            (SyncAction::Enforce, Some(meta)) => {
                let seen = Known {
                    sha256: file.sha256.clone(),
                    size: meta.len(),
                    mtime_ms: mtime_ms(&meta),
                };
                let current = meta.len() == file.size
                    && (state.files.get(path) == Some(&seen)
                        || sha256_of(&dest)?.eq_ignore_ascii_case(&file.sha256));
                if current {
                    enforced.insert(path.to_owned(), seen);
                } else {
                    plan.download.push(file.clone());
                }
            }
        }
    }

    // Files the launcher brought earlier and the server dropped since.
    let mut doomed = BTreeSet::new();
    for path in state.files.keys() {
        if !wanted.contains(&key(path)) && !protected(manifest, path) {
            doomed.insert(path.clone());
        }
    }
    // Unless the server lets players add their own, mods it did not list
    // do not belong either.
    if manifest.policy != ModPolicy::Permissive {
        let mut found = Vec::new();
        walk(&game_dir.join("mods"), "mods", &mut found);
        for entry in found {
            let lower = entry.path.to_lowercase();
            let loadable = entry.link || lower.ends_with(".jar") || lower.ends_with(".zip");
            if loadable && !wanted.contains(&key(&entry.path)) {
                doomed.insert(entry.path);
            }
        }
    }
    // Folders the server keeps to itself hold only what it listed. The
    // names are checked here: a server cannot name the saves folder.
    for dir in &manifest.locked_dirs {
        if !LOCKABLE_DIRS.contains(&dir.as_str()) {
            continue;
        }
        let mut found = Vec::new();
        walk(&game_dir.join(dir), dir, &mut found);
        for entry in found {
            if !wanted.contains(&key(&entry.path)) {
                doomed.insert(entry.path);
            }
        }
    }
    for path in doomed {
        // A link counts even when what it points at is gone.
        if checked(&path).is_ok() && std::fs::symlink_metadata(local(game_dir, &path)).is_ok() {
            plan.delete.push(path);
        }
    }

    state.files = enforced;
    state.once = once;
    Ok(plan)
}

/// Bytes fetched so far and in total.
pub type Progress<'a> = &'a (dyn Fn(u64, u64) + Send + Sync);

fn failed(path: &str, reason: impl ToString) -> Error {
    Error::Download {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

/// Downloads one file to `part`, continuing a partial download when the
/// server agrees to.
async fn fetch(
    client: &Client,
    instance: &str,
    file: &ManifestFile,
    part: &Path,
    count: &(dyn Fn(i64) + Send + Sync),
) -> Result<()> {
    let have = match tokio::fs::metadata(part).await {
        Ok(meta) if meta.len() < file.size => meta.len(),
        _ => 0,
    };
    let mut request = client.http().get(client.blob_url(instance, &file.sha256));
    if let Some(token) = client.token() {
        request = request.bearer_auth(token);
    }
    if have > 0 {
        request = request.header(header::RANGE, format!("bytes={have}-"));
    }
    let mut response = request
        .send()
        .await
        .map_err(|e| failed(&file.path, e.without_url()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(failed(&file.path, tf(Msg::ServerAnswered, &[&status])));
    }
    let resumed = have > 0 && status == StatusCode::PARTIAL_CONTENT;
    let mut out = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(part)
        .await?;
    let mut written = 0i64;
    if resumed {
        written = have as i64;
        count(written);
    }
    let result = async {
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| failed(&file.path, e.without_url()))?
        {
            out.write_all(&chunk).await?;
            written += chunk.len() as i64;
            count(chunk.len() as i64);
        }
        out.flush().await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        // The next attempt starts from what is on disk.
        count(-written);
    }
    result
}

async fn install(
    client: &Client,
    instance: &str,
    file: &ManifestFile,
    game_dir: &Path,
    count: &(dyn Fn(i64) + Send + Sync),
) -> Result<Known> {
    let parts = game_dir.join(PRIVATE_DIR).join("parts");
    tokio::fs::create_dir_all(&parts).await?;
    let part = parts.join(format!("{}.part", file.sha256));
    let dest = local(game_dir, &file.path);

    let mut tries = 0;
    loop {
        tries += 1;
        let attempt = async {
            fetch(client, instance, file, &part, count).await?;
            let hashed = {
                let part = part.clone();
                tokio::task::spawn_blocking(move || sha256_of(&part))
                    .await
                    .map_err(io::Error::other)??
            };
            if !hashed.eq_ignore_ascii_case(&file.sha256) {
                tokio::fs::remove_file(&part).await.ok();
                count(-(file.size as i64));
                return Err(Error::Corrupt(file.path.clone()));
            }
            Ok(())
        }
        .await;
        match attempt {
            Ok(()) => break,
            Err(Error::Download { .. } | Error::Corrupt(_)) if tries < ATTEMPTS => {
                tokio::time::sleep(Duration::from_millis(500 * u64::from(tries))).await;
            }
            Err(err) => return Err(err),
        }
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // `rename` does not replace an existing file on Windows.
    tokio::fs::remove_file(&dest).await.ok();
    tokio::fs::rename(&part, &dest).await?;
    let meta = tokio::fs::metadata(&dest).await?;
    Ok(Known {
        sha256: file.sha256.clone(),
        size: meta.len(),
        mtime_ms: mtime_ms(&meta),
    })
}

/// Carries out a plan. `state` is saved even when a download fails, so the
/// next run only does what is left.
pub async fn apply(
    client: &Client,
    instance: &str,
    game_dir: &Path,
    plan: &Plan,
    state: &mut State,
    progress: Progress<'_>,
) -> Result<()> {
    use futures_util::StreamExt;
    use futures_util::future::BoxFuture;

    for path in &plan.delete {
        let target = local(game_dir, path);
        match tokio::fs::remove_file(&target).await {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            // A link to a folder goes the way folders do; only the link does.
            Err(e) => {
                let linked = tokio::fs::symlink_metadata(&target)
                    .await
                    .is_ok_and(|meta| !meta.is_file());
                if !linked || tokio::fs::remove_dir(&target).await.is_err() {
                    return Err(e.into());
                }
            }
        }
    }

    let total = plan.bytes();
    let done = AtomicU64::new(0);
    let count = |delta: i64| {
        let now = if delta >= 0 {
            done.fetch_add(delta as u64, Ordering::Relaxed) + delta as u64
        } else {
            done.fetch_sub(delta.unsigned_abs(), Ordering::Relaxed) - delta.unsigned_abs()
        };
        progress(now.min(total), total);
    };
    // Boxed and collected up front: the compiler cannot otherwise prove a
    // stream of borrowing futures is `Send`.
    type Job<'a> = BoxFuture<'a, Result<(&'a ManifestFile, Known)>>;
    let count: &(dyn Fn(i64) + Send + Sync) = &count;
    let jobs: Vec<Job<'_>> = plan
        .download
        .iter()
        .map(|file| -> Job<'_> {
            Box::pin(async move {
                let known = install(client, instance, file, game_dir, count).await?;
                Ok((file, known))
            })
        })
        .collect();
    let mut downloads = futures_util::stream::iter(jobs).buffer_unordered(PARALLEL_DOWNLOADS);

    let mut outcome = Ok(());
    while let Some(result) = downloads.next().await {
        match result {
            Ok((file, known)) => {
                if plan.once.contains(&file.path) {
                    state.once.insert(file.path.clone(), known.sha256);
                } else {
                    state.files.insert(file.path.clone(), known);
                }
            }
            Err(err) => {
                outcome = Err(err);
                break;
            }
        }
    }
    drop(downloads);
    state.save(game_dir)?;
    outcome
}

#[cfg(test)]
mod tests {
    use relic_proto::{FileKind, JavaRuntime, Loader, LoaderKind};

    use super::*;

    fn sha(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn file(path: &str, content: &[u8], action: SyncAction, optional: bool) -> ManifestFile {
        ManifestFile {
            path: path.into(),
            sha256: sha(content),
            size: content.len() as u64,
            kind: FileKind::Mod,
            action,
            optional,
            name: String::new(),
            description: String::new(),
            icon: None,
            requires: Vec::new(),
        }
    }

    fn manifest(policy: ModPolicy, files: Vec<ManifestFile>) -> Manifest {
        Manifest {
            build_id: uuid::Uuid::nil(),
            instance_id: "main".into(),
            minecraft_version: "1.21.1".into(),
            loader: Loader {
                kind: LoaderKind::Fabric,
                version: "0.16.5".into(),
            },
            java_runtime: JavaRuntime {
                major_version: 21,
                vendor: "temurin".into(),
            },
            policy,
            files,
            protected_paths: vec!["options.txt".into(), "saves/*".into()],
            locked_dirs: Vec::new(),
        }
    }

    fn put(dir: &Path, path: &str, content: &[u8]) {
        let dest = local(dir, path);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(dest, content).unwrap();
    }

    fn paths(files: &[ManifestFile]) -> Vec<&str> {
        files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn globs_cover_directories() {
        assert!(glob("options.txt", "options.txt"));
        assert!(!glob("options.txt", "config/options.txt"));
        assert!(glob("saves/*", "saves/world/level.dat"));
        assert!(!glob("saves/*", "saves"));
        assert!(glob("config/*.toml", "config/a.toml"));
        assert!(glob("*.dat", "servers.dat"));
        assert!(glob("a?c", "abc"));
        assert!(!glob("a?c", "ac"));
    }

    #[test]
    fn hostile_paths_are_refused() {
        for path in [
            "../evil.jar",
            "mods/../../evil.jar",
            "/etc/passwd",
            "C:/Windows/x",
            "mods\\a.jar",
            "mods//a.jar",
            ".relic/sync.json",
            "",
        ] {
            assert!(checked(path).is_err(), "{path}");
        }
        assert!(checked("mods/a.jar").is_ok());
        let bad = manifest(
            ModPolicy::Strict,
            vec![file("../evil.jar", b"x", SyncAction::Enforce, false)],
        );
        let dir = tempfile::tempdir().unwrap();
        assert!(plan(&bad, &BTreeSet::new(), dir.path(), &mut State::default()).is_err());
    }

    #[test]
    fn only_missing_and_changed_files_are_fetched() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "mods/same.jar", b"same");
        put(dir.path(), "mods/changed.jar", b"old!");
        put(dir.path(), "config/mine.toml", b"edited by the player");
        put(dir.path(), "options.txt", b"player options");
        let manifest = manifest(
            ModPolicy::Strict,
            vec![
                file("mods/same.jar", b"same", SyncAction::Enforce, false),
                file("mods/changed.jar", b"new!", SyncAction::Enforce, false),
                file("mods/missing.jar", b"missing", SyncAction::Enforce, false),
                file("config/mine.toml", b"default", SyncAction::SyncOnce, false),
                file("config/fresh.toml", b"default", SyncAction::SyncOnce, false),
                file("options.txt", b"server options", SyncAction::Enforce, false),
                file("mods/extra.jar", b"extra", SyncAction::Enforce, true),
            ],
        );
        let mut state = State::default();
        let plan = plan(&manifest, &BTreeSet::new(), dir.path(), &mut state).unwrap();
        assert_eq!(
            paths(&plan.download),
            ["mods/changed.jar", "mods/missing.jar", "config/fresh.toml"]
        );
        assert!(plan.delete.is_empty());
        assert_eq!(plan.bytes(), 4 + 7 + 7);
        // The verified file is remembered, so the next run need not hash it.
        assert!(state.files.contains_key("mods/same.jar"));

        let enabled = BTreeSet::from(["mods/extra.jar".to_owned()]);
        let plan = super::plan(&manifest, &enabled, dir.path(), &mut state).unwrap();
        assert!(paths(&plan.download).contains(&"mods/extra.jar"));
    }

    #[test]
    fn strict_packs_drop_foreign_mods_and_permissive_ones_keep_them() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "mods/pack.jar", b"pack");
        put(dir.path(), "mods/own.jar", b"brought by the player");
        put(dir.path(), "mods/notes.txt", b"not a mod");
        put(dir.path(), "mods/retired.jar", b"retired");
        put(dir.path(), "config/retired.toml", b"retired");
        put(dir.path(), "saves/world/level.dat", b"world");
        let files = vec![file("mods/pack.jar", b"pack", SyncAction::Enforce, false)];
        let known = |content: &[u8]| Known {
            sha256: sha(content),
            size: content.len() as u64,
            mtime_ms: 0,
        };
        let earlier = State {
            files: BTreeMap::from([
                ("mods/retired.jar".to_owned(), known(b"retired")),
                ("config/retired.toml".to_owned(), known(b"retired")),
                ("saves/world/level.dat".to_owned(), known(b"world")),
            ]),
            once: BTreeMap::new(),
        };

        let mut state = earlier.clone();
        let strict = manifest(ModPolicy::Strict, files.clone());
        let plan = super::plan(&strict, &BTreeSet::new(), dir.path(), &mut state).unwrap();
        assert_eq!(
            plan.delete,
            ["config/retired.toml", "mods/own.jar", "mods/retired.jar"]
        );
        assert!(plan.download.is_empty());

        let mut state = earlier;
        let permissive = manifest(ModPolicy::Permissive, files);
        let plan = super::plan(&permissive, &BTreeSet::new(), dir.path(), &mut state).unwrap();
        assert_eq!(plan.delete, ["config/retired.toml", "mods/retired.jar"]);
    }

    #[test]
    fn state_survives_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let state = State {
            files: BTreeMap::from([(
                "mods/a.jar".to_owned(),
                Known {
                    sha256: sha(b"a"),
                    size: 1,
                    mtime_ms: 5,
                },
            )]),
            once: BTreeMap::from([("options.txt".to_owned(), sha(b"o"))]),
        };
        state.save(dir.path()).unwrap();
        assert_eq!(State::load(dir.path()), state);
    }

    #[test]
    fn player_settings_arrive_once_and_stay_the_players() {
        let dir = tempfile::tempdir().unwrap();
        let pack = manifest(
            ModPolicy::Strict,
            vec![file("options.txt", b"server", SyncAction::Enforce, false)],
        );
        let mut state = State::default();
        let plan = super::plan(&pack, &BTreeSet::new(), dir.path(), &mut state).unwrap();
        assert_eq!(paths(&plan.download), ["options.txt"]);
        assert!(plan.once.contains("options.txt"));

        // Delivered, then changed by the game: never fetched or removed again.
        put(dir.path(), "options.txt", b"player");
        state.once.insert("options.txt".into(), sha(b"server"));
        let newer = manifest(
            ModPolicy::Strict,
            vec![file(
                "options.txt",
                b"server v2",
                SyncAction::Enforce,
                false,
            )],
        );
        let plan = super::plan(&newer, &BTreeSet::new(), dir.path(), &mut state).unwrap();
        assert!(plan.download.is_empty() && plan.delete.is_empty());
        let gone = manifest(ModPolicy::Strict, Vec::new());
        let plan = super::plan(&gone, &BTreeSet::new(), dir.path(), &mut state).unwrap();
        assert!(plan.delete.is_empty());
    }

    #[test]
    fn an_untouched_config_follows_the_server_and_an_edited_one_does_not() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "config/untouched.toml", b"v1");
        put(dir.path(), "config/edited.toml", b"mine");
        let mut state = State {
            files: BTreeMap::new(),
            once: BTreeMap::from([
                ("config/untouched.toml".to_owned(), sha(b"v1")),
                ("config/edited.toml".to_owned(), sha(b"v1")),
            ]),
        };
        let pack = manifest(
            ModPolicy::Strict,
            vec![
                file("config/untouched.toml", b"v2", SyncAction::SyncOnce, false),
                file("config/edited.toml", b"v2", SyncAction::SyncOnce, false),
            ],
        );
        let plan = super::plan(&pack, &BTreeSet::new(), dir.path(), &mut state).unwrap();
        assert_eq!(paths(&plan.download), ["config/untouched.toml"]);
        // The edited one is still compared against what was delivered.
        assert_eq!(state.once["config/edited.toml"], sha(b"v1"));
    }

    #[test]
    fn locked_folders_hold_only_what_the_server_listed() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "resourcepacks/pack.zip", b"pack");
        put(dir.path(), "resourcepacks/xray.zip", b"xray");
        put(dir.path(), "resourcepacks/unzipped/pack.mcmeta", b"{}");
        put(dir.path(), "shaderpacks/own.zip", b"shader");
        put(dir.path(), "saves/world/level.dat", b"world");
        let mut pack = manifest(
            ModPolicy::Permissive,
            vec![file(
                "resourcepacks/pack.zip",
                b"pack",
                SyncAction::Enforce,
                false,
            )],
        );
        let plan = super::plan(&pack, &BTreeSet::new(), dir.path(), &mut State::default()).unwrap();
        assert!(plan.delete.is_empty());

        // A folder outside the short list is ignored, whatever the server says.
        pack.locked_dirs = vec!["resourcepacks".into(), "saves".into()];
        let plan = super::plan(&pack, &BTreeSet::new(), dir.path(), &mut State::default()).unwrap();
        assert_eq!(
            plan.delete,
            [
                "resourcepacks/unzipped/pack.mcmeta",
                "resourcepacks/xray.zip"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn links_in_the_mods_folder_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        put(outside.path(), "cheat.jar", b"cheat");
        std::fs::create_dir_all(dir.path().join("mods")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("mods/extra")).unwrap();
        let pack = manifest(ModPolicy::Strict, Vec::new());
        let plan = super::plan(&pack, &BTreeSet::new(), dir.path(), &mut State::default()).unwrap();
        assert_eq!(plan.delete, ["mods/extra"]);
    }

    #[test]
    fn what_a_mod_needs_comes_with_it() {
        let mut addon = file("mods/addon.jar", b"addon", SyncAction::Enforce, true);
        addon.requires = vec!["mods/lib.jar".into()];
        let mut lib = file("mods/lib.jar", b"lib", SyncAction::Enforce, true);
        lib.requires = vec!["mods/core.jar".into(), "mods/addon.jar".into()];
        let core = file("mods/core.jar", b"core", SyncAction::Enforce, true);
        let other = file("mods/other.jar", b"other", SyncAction::Enforce, true);
        let pack = manifest(ModPolicy::Strict, vec![addon, lib, core, other]);

        assert!(resolve(&pack, &BTreeSet::new()).is_empty());
        let picked = BTreeSet::from(["mods/addon.jar".to_owned()]);
        assert_eq!(
            resolve(&pack, &picked).into_iter().collect::<Vec<_>>(),
            ["mods/addon.jar", "mods/core.jar", "mods/lib.jar"]
        );
        let dir = tempfile::tempdir().unwrap();
        let plan = super::plan(&pack, &picked, dir.path(), &mut State::default()).unwrap();
        assert_eq!(
            paths(&plan.download),
            ["mods/addon.jar", "mods/lib.jar", "mods/core.jar"]
        );
    }
}
