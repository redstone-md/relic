//! Everything behind the admin session.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::Body;
use axum::extract::{Extension, Path, Query, Request, State};
use axum::http::header::CONTENT_DISPOSITION;
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use futures_util::TryStreamExt;
use relic_mc::loaders::{self, LoaderVersion};
use relic_mc::{java, mojang, server};
use relic_proto::{InstanceStatus, Loader, LoaderKind, NewsItem, Side, SyncAction};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio_util::io::StreamReader;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

use super::{AppState, internal};
use crate::backups::BackupInfo;
use crate::error::{Error, Result};
use crate::files;
use crate::instances::{self, Instance, InstanceUpdate, NewInstance};
use crate::modpack::{FileHints, FileRecord};
use crate::news::{self, NewsDraft};
use crate::settings::{self, General};
use crate::sources::{Installed, ModVersion, MrpackReport, PackHit, Provider, SearchHit};
use crate::supervisor::RuntimeInfo;
use crate::users::{self, Invite, Role, Status, User};

const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_MRPACK: u64 = 1024 * 1024 * 1024;
const VERSIONS_TTL: Duration = Duration::from_secs(10 * 60);
const DEFAULT_PORT: u16 = 25565;
const DEFAULT_MEMORY_MB: u32 = 4096;

fn upstream(err: relic_mc::Error) -> Error {
    match err {
        relic_mc::Error::NotFound(message) | relic_mc::Error::Unsupported(message) => {
            Error::Invalid(message)
        }
        other => Error::Upstream(other.to_string()),
    }
}

fn body_reader(body: Body) -> impl tokio::io::AsyncRead + Unpin {
    StreamReader::new(body.into_data_stream().map_err(std::io::Error::other))
}

// ---------------------------------------------------------------------------
// Instances
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct InstanceView {
    #[serde(flatten)]
    instance: Instance,
    runtime: RuntimeInfo,
    eula_accepted: bool,
    /// Whether the server software is on disk already.
    installed: bool,
}

async fn view(app: &AppState, instance: Instance) -> InstanceView {
    let dir = app.supervisor.instance_dir(&instance.id);
    InstanceView {
        runtime: app.supervisor.info(&instance.id),
        eula_accepted: server::eula_accepted(&dir).await,
        installed: matches!(server::installed(&dir).await, Ok(Some(_))),
        instance,
    }
}

fn halted(app: &AppState, id: &str) -> bool {
    matches!(
        app.supervisor.info(id).status,
        InstanceStatus::Stopped | InstanceStatus::Crashed
    )
}

pub async fn list_instances(State(app): State<AppState>) -> Result<Json<Vec<InstanceView>>> {
    let mut views = Vec::new();
    for instance in instances::list(&app.db).await? {
        views.push(view(&app, instance).await);
    }
    Ok(Json(views))
}

pub async fn get_instance(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<InstanceView>> {
    let instance = instances::get(&app.db, &id).await?;
    Ok(Json(view(&app, instance).await))
}

/// Java to run `minecraft_version` with, as a Temurin major we can fetch.
async fn java_for(app: &AppState, minecraft_version: &str) -> Result<u32> {
    if !mojang::is_supported(minecraft_version) {
        return Err(Error::invalid(format!(
            "Minecraft versions older than {} are not supported",
            mojang::MIN_SUPPORTED
        )));
    }
    let version = mojang::version_json(&app.http, minecraft_version)
        .await
        .map_err(upstream)?;
    Ok(java::runtime_major(version.java_major()))
}

/// Fills in the default loader build, or checks that the requested one exists.
async fn resolve_loader(app: &AppState, minecraft_version: &str, loader: Loader) -> Result<Loader> {
    if loader.kind == LoaderKind::Vanilla {
        return Ok(Loader {
            kind: LoaderKind::Vanilla,
            version: String::new(),
        });
    }
    let available = loaders::versions(&app.http, loader.kind, minecraft_version)
        .await
        .map_err(upstream)?;
    let version = if loader.version.trim().is_empty() {
        loaders::pick_default(&available).map(|v| v.version.clone())
    } else {
        available
            .iter()
            .find(|v| v.version == loader.version.trim())
            .map(|v| v.version.clone())
    };
    let version = version.ok_or_else(|| {
        Error::invalid(format!(
            "no matching {} build for Minecraft {minecraft_version}",
            loader.kind.as_str()
        ))
    })?;
    Ok(Loader {
        kind: loader.kind,
        version,
    })
}

#[derive(Deserialize)]
pub struct CreateInstance {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    minecraft_version: String,
    loader: Loader,
    port: Option<u16>,
    memory_mb: Option<u32>,
}

pub async fn create_instance(
    State(app): State<AppState>,
    Json(body): Json<CreateInstance>,
) -> Result<(StatusCode, Json<InstanceView>)> {
    instances::validate_id(&body.id)?;
    let minecraft_version = body.minecraft_version.trim().to_owned();
    let java_major = java_for(&app, &minecraft_version).await?;
    let loader = resolve_loader(&app, &minecraft_version, body.loader).await?;
    let port = match body.port {
        Some(port) => port,
        // The next one after the highest in use.
        None => instances::list(&app.db)
            .await?
            .iter()
            .map(|i| i.port.saturating_add(1))
            .max()
            .unwrap_or(DEFAULT_PORT),
    };
    let instance = instances::create(
        &app.db,
        NewInstance {
            id: body.id,
            name: body.name,
            description: body.description,
            minecraft_version,
            loader,
            java_major,
            port,
            memory_mb: body.memory_mb.unwrap_or(DEFAULT_MEMORY_MB),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(view(&app, instance).await)))
}

pub async fn update_instance(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(mut update): Json<InstanceUpdate>,
) -> Result<Json<InstanceView>> {
    let current = instances::get(&app.db, &id).await?;
    let minecraft_version = update
        .minecraft_version
        .as_deref()
        .map_or(current.minecraft_version.as_str(), str::trim)
        .to_owned();
    let version_changed = minecraft_version != current.minecraft_version;
    let loader_changed = update.loader.as_ref().is_some_and(|l| *l != current.loader);

    if version_changed || loader_changed {
        if !halted(&app, &id) {
            return Err(Error::Conflict(
                "stop the instance before changing its game version or loader".into(),
            ));
        }
        if version_changed && update.java_major.is_none() {
            update.java_major = Some(java_for(&app, &minecraft_version).await?);
        }
        let loader = update.loader.take().unwrap_or(Loader {
            // Loader builds are per game version; pick a fresh one.
            kind: current.loader.kind,
            version: String::new(),
        });
        update.loader = Some(resolve_loader(&app, &minecraft_version, loader).await?);
        update.minecraft_version = Some(minecraft_version);
    }
    if let Some(major) = update.java_major {
        update.java_major = Some(java::runtime_major(major));
    }

    let instance = instances::update(&app.db, &id, update).await?;
    Ok(Json(view(&app, instance).await))
}

#[derive(Deserialize)]
pub struct DeleteInstance {
    /// Also delete the server directory (worlds included) and backups.
    #[serde(default)]
    purge: bool,
}

pub async fn delete_instance(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DeleteInstance>,
) -> Result<StatusCode> {
    instances::get(&app.db, &id).await?;
    app.supervisor.stop(&id).await?;
    if !halted(&app, &id) {
        return Err(Error::Conflict(
            "the instance is still shutting down".into(),
        ));
    }
    instances::delete(&app.db, &id).await?;
    let dir = app.supervisor.instance_dir(&id);
    app.supervisor.forget(&id);
    if query.purge {
        match tokio::fs::remove_dir_all(&dir).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        app.backups.forget(&id).await?;
    }
    app.modpack.collect_garbage().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn start(State(app): State<AppState>, Path(id): Path<String>) -> Result<StatusCode> {
    instances::get(&app.db, &id).await?;
    if !server::eula_accepted(&app.supervisor.instance_dir(&id)).await {
        return Err(Error::Conflict(
            "the Minecraft EULA has not been accepted for this instance".into(),
        ));
    }
    app.supervisor.start(&id).await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn stop(State(app): State<AppState>, Path(id): Path<String>) -> Result<StatusCode> {
    instances::get(&app.db, &id).await?;
    // Stopping can take a minute; the panel follows it over the event stream.
    tokio::spawn(async move {
        if let Err(e) = app.supervisor.stop(&id).await {
            tracing::warn!(instance = %id, error = %e, "stop failed");
        }
    });
    Ok(StatusCode::ACCEPTED)
}

pub async fn restart(State(app): State<AppState>, Path(id): Path<String>) -> Result<StatusCode> {
    instances::get(&app.db, &id).await?;
    tokio::spawn(async move {
        if let Err(e) = app.supervisor.restart(&id).await {
            tracing::warn!(instance = %id, error = %e, "restart failed");
        }
    });
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
pub struct Command {
    line: String,
}

pub async fn command(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Command>,
) -> Result<StatusCode> {
    instances::get(&app.db, &id).await?;
    app.supervisor.command(&id, &body.line).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Eula {
    accept: bool,
}

/// Records that the admin agreed to Mojang's EULA for this instance. Never
/// done implicitly.
pub async fn accept_eula(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Eula>,
) -> Result<StatusCode> {
    instances::get(&app.db, &id).await?;
    if !body.accept {
        return Err(Error::invalid("the EULA can only be accepted explicitly"));
    }
    let dir = app.supervisor.instance_dir(&id);
    tokio::fs::create_dir_all(&dir).await?;
    server::accept_eula(&dir).await.map_err(upstream)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Pack files
// ---------------------------------------------------------------------------

pub async fn list_files(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<FileRecord>>> {
    instances::get(&app.db, &id).await?;
    Ok(Json(app.modpack.list_files(&id).await?))
}

#[derive(Deserialize)]
pub struct UploadOptions {
    side: Option<Side>,
    optional: Option<bool>,
}

/// The request body is the file itself, streamed straight into the store.
pub async fn upload_file(
    State(app): State<AppState>,
    Path((id, path)): Path<(String, String)>,
    Query(options): Query<UploadOptions>,
    body: Body,
) -> Result<Json<FileRecord>> {
    let hints = FileHints {
        side: options.side,
        optional: options.optional,
    };
    let record = app
        .modpack
        .add_file(&id, &path, body_reader(body), Some(MAX_FILE), hints)
        .await?;
    Ok(Json(record))
}

#[derive(Deserialize)]
pub struct FileOptions {
    sync_action: Option<SyncAction>,
    optional: Option<bool>,
}

pub async fn patch_file(
    State(app): State<AppState>,
    Path((id, path)): Path<(String, String)>,
    Json(body): Json<FileOptions>,
) -> Result<Json<FileRecord>> {
    let record = app
        .modpack
        .set_file_options(&id, &path, body.sync_action, body.optional)
        .await?;
    Ok(Json(record))
}

pub async fn delete_file(
    State(app): State<AppState>,
    Path((id, path)): Path<(String, String)>,
) -> Result<StatusCode> {
    app.modpack.remove_file(&id, &path).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SideOverride {
    /// `null` returns to what detection found.
    side: Option<Side>,
}

pub async fn set_side(
    State(app): State<AppState>,
    Path(sha256): Path<String>,
    Json(body): Json<SideOverride>,
) -> Result<StatusCode> {
    app.modpack.set_side_override(&sha256, body.side).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes the scratch file when the import ends, however it ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn save_body(body: Body, dest: PathBuf, max: u64) -> Result<()> {
    let mut stream = body.into_data_stream();
    let mut file = tokio::fs::File::create(dest).await?;
    let mut written = 0u64;
    while let Some(chunk) = stream
        .try_next()
        .await
        .map_err(|e| Error::invalid(e.to_string()))?
    {
        written += chunk.len() as u64;
        if written > max {
            return Err(Error::invalid("upload is too large"));
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

pub async fn import_mrpack(
    State(app): State<AppState>,
    Path(id): Path<String>,
    body: Body,
) -> Result<Json<MrpackReport>> {
    instances::get(&app.db, &id).await?;
    let scratch = Scratch(app.tmp.join(format!("{}.mrpack", uuid::Uuid::new_v4())));
    save_body(body, scratch.0.clone(), MAX_MRPACK).await?;
    Ok(Json(app.sources.import_mrpack(&id, &scratch.0).await?))
}

// ---------------------------------------------------------------------------
// Mod catalogues
// ---------------------------------------------------------------------------

async fn curseforge_key(app: &AppState) -> Result<Option<String>> {
    settings::get(&app.db, settings::CURSEFORGE_API_KEY).await
}

#[derive(Deserialize)]
pub struct SearchQuery {
    provider: Provider,
    #[serde(default)]
    q: String,
}

pub async fn search_mods(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchHit>>> {
    let instance = instances::get(&app.db, &id).await?;
    let key = curseforge_key(&app).await?;
    let hits = app
        .sources
        .search(query.provider, &instance, &query.q, key.as_deref())
        .await?;
    Ok(Json(hits))
}

#[derive(Deserialize)]
pub struct PacksQuery {
    #[serde(default)]
    q: String,
    category: Option<String>,
    sort: Option<String>,
}

pub async fn search_packs(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<PacksQuery>,
) -> Result<Json<Vec<PackHit>>> {
    let instance = instances::get(&app.db, &id).await?;
    let hits = app
        .sources
        .packs(
            &instance,
            &query.q,
            query.category.as_deref(),
            query.sort.as_deref(),
        )
        .await?;
    Ok(Json(hits))
}

#[derive(Deserialize)]
pub struct InstallPack {
    project: String,
}

pub async fn install_pack(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<InstallPack>,
) -> Result<Json<MrpackReport>> {
    let instance = instances::get(&app.db, &id).await?;
    Ok(Json(
        app.sources.install_pack(&instance, &body.project).await?,
    ))
}

#[derive(Deserialize)]
pub struct VersionsQuery {
    provider: Provider,
    project: String,
}

pub async fn mod_versions(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<VersionsQuery>,
) -> Result<Json<Vec<ModVersion>>> {
    let instance = instances::get(&app.db, &id).await?;
    let key = curseforge_key(&app).await?;
    let versions = app
        .sources
        .versions(query.provider, &instance, &query.project, key.as_deref())
        .await?;
    Ok(Json(versions))
}

#[derive(Deserialize)]
pub struct InstallMod {
    provider: Provider,
    project: String,
    /// Omitted: the newest build that fits the instance.
    version: Option<String>,
}

pub async fn install_mod(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<InstallMod>,
) -> Result<Json<Installed>> {
    let instance = instances::get(&app.db, &id).await?;
    let key = curseforge_key(&app).await?;
    let installed = app
        .sources
        .install(
            body.provider,
            &instance,
            &body.project,
            body.version.as_deref(),
            key.as_deref(),
        )
        .await?;
    Ok(Json(installed))
}

pub async fn minecraft_versions(State(app): State<AppState>) -> Result<Json<Vec<String>>> {
    let cached = app
        .minecraft_versions
        .lock()
        .expect("versions lock")
        .as_ref()
        .filter(|(at, _)| at.elapsed() < VERSIONS_TTL)
        .map(|(_, versions)| versions.clone());
    if let Some(versions) = cached {
        return Ok(Json(versions));
    }
    let versions: Vec<String> = mojang::version_manifest(&app.http)
        .await
        .map_err(upstream)?
        .versions
        .into_iter()
        .filter(|v| v.kind == "release" && mojang::is_supported(&v.id))
        .map(|v| v.id)
        .collect();
    *app.minecraft_versions.lock().expect("versions lock") =
        Some((Instant::now(), versions.clone()));
    Ok(Json(versions))
}

#[derive(Deserialize)]
pub struct LoaderQuery {
    kind: LoaderKind,
    mc: String,
}

pub async fn loader_versions(
    State(app): State<AppState>,
    Query(query): Query<LoaderQuery>,
) -> Result<Json<Vec<LoaderVersion>>> {
    let versions = loaders::versions(&app.http, query.kind, &query.mc)
        .await
        .map_err(upstream)?;
    Ok(Json(versions))
}

// ---------------------------------------------------------------------------
// Backups
// ---------------------------------------------------------------------------

pub async fn list_backups(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<BackupInfo>>> {
    instances::get(&app.db, &id).await?;
    Ok(Json(app.backups.list(&id).await?))
}

pub async fn create_backup(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<BackupInfo>)> {
    // Detached from the request: a dropped connection must not leave the
    // world with saving switched off.
    let backups = app.backups.clone();
    let info = tokio::spawn(async move { backups.create(&id).await })
        .await
        .map_err(internal)??;
    Ok((StatusCode::CREATED, Json(info)))
}

pub async fn delete_backup(
    State(app): State<AppState>,
    Path((id, backup_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    app.backups.delete(&id, &backup_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn restore_backup(
    State(app): State<AppState>,
    Path((id, backup_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    let backups = app.backups.clone();
    tokio::spawn(async move { backups.restore(&id, &backup_id).await })
        .await
        .map_err(internal)??;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn download_backup(
    State(app): State<AppState>,
    Path((id, backup_id)): Path<(String, String)>,
    request: Request,
) -> Result<Response> {
    let path = app.backups.path(&id, &backup_id)?;
    if !tokio::fs::try_exists(&path).await? {
        return Err(Error::NotFound("backup"));
    }
    let mime = mime_guess::mime::APPLICATION_OCTET_STREAM;
    let mut response = ServeFile::new_with_mime(path, &mime)
        .oneshot(request)
        .await
        .map_err(internal)?
        .map(Body::new);
    let disposition = format!("attachment; filename=\"{id}-{backup_id}.tar.zst\"");
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(internal)?,
    );
    Ok(response)
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Uploaded {
    /// Relative to the server, as the launcher expects its own files.
    url: String,
}

pub async fn upload_media(
    State(app): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Json<Uploaded>> {
    let name = crate::media::put(&app.media, &body).await?;
    Ok(Json(Uploaded {
        url: format!("/api/v1/media/{name}"),
    }))
}

// ---------------------------------------------------------------------------
// File manager
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FsPath {
    #[serde(default)]
    path: String,
}

#[derive(Deserialize)]
pub struct FsRename {
    from: String,
    to: String,
}

async fn fs_root(app: &AppState, id: &str) -> Result<PathBuf> {
    instances::get(&app.db, id).await?;
    Ok(app.supervisor.instance_dir(id))
}

pub async fn fs_list(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FsPath>,
) -> Result<Json<Vec<files::Entry>>> {
    let root = fs_root(&app, &id).await?;
    Ok(Json(files::list(&root, &query.path).await?))
}

pub async fn fs_read(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FsPath>,
    request: Request,
) -> Result<Response> {
    let root = fs_root(&app, &id).await?;
    let path = files::file(&root, &query.path).await?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_owned();
    // Always as a download: nothing from the server's folder is to be
    // rendered by the browser on the panel's own origin.
    let mime = mime_guess::mime::APPLICATION_OCTET_STREAM;
    let mut response = ServeFile::new_with_mime(path, &mime)
        .oneshot(request)
        .await
        .map_err(internal)?
        .map(Body::new);
    response
        .headers_mut()
        .insert(CONTENT_DISPOSITION, attachment(&name)?);
    Ok(response)
}

pub async fn fs_usage(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<files::Usage>> {
    let root = fs_root(&app, &id).await?;
    Ok(Json(files::usage(&root).await?))
}

fn attachment(name: &str) -> Result<HeaderValue> {
    let encoded: String = name
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    HeaderValue::from_str(&format!("attachment; filename*=UTF-8''{encoded}")).map_err(internal)
}

/// A folder as one zip, built in the scratch space and removed once sent.
pub async fn fs_archive(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FsPath>,
) -> Result<Response> {
    let root = fs_root(&app, &id).await?;
    let scratch = Scratch(app.tmp.join(format!("{}.zip", Uuid::new_v4())));
    let name = files::archive(&root, &query.path, &scratch.0).await?;
    let file = tokio::fs::File::open(&scratch.0).await?;
    let length = file.metadata().await?.len();
    // The stream owns the scratch file's guard: the zip goes when the
    // download ends or is dropped half way.
    let stream = tokio_util::io::ReaderStream::new(file).map_ok(move |chunk| {
        let _held = &scratch;
        chunk
    });
    Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "application/zip")
        .header(axum::http::header::CONTENT_LENGTH, length)
        .header(CONTENT_DISPOSITION, attachment(&format!("{name}.zip"))?)
        .body(Body::from_stream(stream))
        .map_err(internal)
}

#[derive(Serialize)]
pub struct Unpacked {
    files: usize,
}

/// A zip uploaded and unpacked into a new folder: how a world comes in.
pub async fn fs_unpack(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FsPath>,
    body: Body,
) -> Result<Json<Unpacked>> {
    let root = fs_root(&app, &id).await?;
    let scratch = Scratch(app.tmp.join(format!("{}.zip", Uuid::new_v4())));
    save_body(body, scratch.0.clone(), MAX_FILE).await?;
    let files = files::unpack(&root, &query.path, &scratch.0).await?;
    Ok(Json(Unpacked { files }))
}

pub async fn fs_write(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FsPath>,
    body: Body,
) -> Result<StatusCode> {
    let root = fs_root(&app, &id).await?;
    files::write(&root, &query.path, body_reader(body), MAX_FILE).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn fs_mkdir(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<FsPath>,
) -> Result<StatusCode> {
    let root = fs_root(&app, &id).await?;
    files::create_dir(&root, &body.path).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn fs_rename(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<FsRename>,
) -> Result<StatusCode> {
    let root = fs_root(&app, &id).await?;
    files::rename(&root, &body.from, &body.to).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn fs_remove(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FsPath>,
) -> Result<StatusCode> {
    let root = fs_root(&app, &id).await?;
    files::remove(&root, &query.path).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Settings, news, users
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SettingsView {
    #[serde(flatten)]
    general: General,
    /// The key itself never leaves the server.
    curseforge_key_set: bool,
}

async fn settings_view(app: &AppState) -> Result<Json<SettingsView>> {
    Ok(Json(SettingsView {
        general: settings::general(&app.db).await?,
        curseforge_key_set: curseforge_key(app).await?.is_some(),
    }))
}

pub async fn get_settings(State(app): State<AppState>) -> Result<Json<SettingsView>> {
    settings_view(&app).await
}

pub async fn put_settings(
    State(app): State<AppState>,
    Json(general): Json<General>,
) -> Result<Json<SettingsView>> {
    settings::set_general(&app.db, &general).await?;
    settings_view(&app).await
}

#[derive(Deserialize)]
pub struct CurseforgeKey {
    /// Empty or `null` removes the stored key.
    key: Option<String>,
}

pub async fn put_curseforge_key(
    State(app): State<AppState>,
    Json(body): Json<CurseforgeKey>,
) -> Result<StatusCode> {
    match body.key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        Some(key) if key.len() > 256 || key.contains(char::is_whitespace) => {
            return Err(Error::invalid("that does not look like an API key"));
        }
        Some(key) => settings::set(&app.db, settings::CURSEFORGE_API_KEY, key).await?,
        None => settings::remove(&app.db, settings::CURSEFORGE_API_KEY).await?,
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_news(
    State(app): State<AppState>,
    Json(draft): Json<NewsDraft>,
) -> Result<(StatusCode, Json<NewsItem>)> {
    Ok((
        StatusCode::CREATED,
        Json(news::create(&app.db, draft).await?),
    ))
}

pub async fn update_news(
    State(app): State<AppState>,
    Path(news_id): Path<i64>,
    Json(draft): Json<NewsDraft>,
) -> Result<Json<NewsItem>> {
    Ok(Json(news::update(&app.db, news_id, draft).await?))
}

pub async fn delete_news(
    State(app): State<AppState>,
    Path(news_id): Path<i64>,
) -> Result<StatusCode> {
    news::delete(&app.db, news_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Accounts and invites
// ---------------------------------------------------------------------------

pub async fn users(State(app): State<AppState>) -> Result<Json<Vec<User>>> {
    Ok(Json(users::list(&app.db).await?))
}

#[derive(Deserialize)]
pub struct CreateUser {
    username: String,
    password: String,
    role: Option<Role>,
}

pub async fn create_user(
    State(app): State<AppState>,
    Json(body): Json<CreateUser>,
) -> Result<(StatusCode, Json<User>)> {
    let role = body.role.unwrap_or(Role::Player);
    let user = users::create(
        &app.db,
        &body.username,
        &body.password,
        role,
        Status::Active,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

fn user_id(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|_| Error::NotFound("user"))
}

/// Admins act on other accounts here; their own goes through `/account`,
/// so nobody locks themselves out by a slip.
fn other_account(admin: &User, id: &str) -> Result<Uuid> {
    let id = user_id(id)?;
    if id == admin.id {
        return Err(Error::Conflict(
            "you cannot do this to your own account".into(),
        ));
    }
    Ok(id)
}

/// Throws a player out of every running server. Their sessions are gone
/// already, but the game only checks those when joining.
async fn kick(app: &AppState, username: &str) {
    let Ok(all) = instances::list(&app.db).await else {
        return;
    };
    for instance in all {
        if app.supervisor.info(&instance.id).status == InstanceStatus::Running {
            let _ = app
                .supervisor
                .command(&instance.id, &format!("kick {username}"))
                .await;
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateUser {
    status: Option<Status>,
    role: Option<Role>,
}

pub async fn update_user(
    State(app): State<AppState>,
    Extension(admin): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<UpdateUser>,
) -> Result<Json<User>> {
    let id = other_account(&admin, &id)?;
    if body.status == Some(Status::Pending) {
        return Err(Error::invalid("an account cannot be made pending again"));
    }
    if let Some(role) = body.role {
        users::set_role(&app.db, id, role).await?;
    }
    if let Some(status) = body.status {
        users::set_status(&app.db, id, status).await?;
    }
    let user = users::get(&app.db, id).await?;
    if body.status == Some(Status::Banned) {
        kick(&app, &user.username).await;
    }
    Ok(Json(user))
}

pub async fn delete_user(
    State(app): State<AppState>,
    Extension(admin): Extension<User>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let id = other_account(&admin, &id)?;
    let user = users::get(&app.db, id).await?;
    users::delete(&app.db, id).await?;
    kick(&app, &user.username).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct ResetCode {
    code: String,
    expires_at: i64,
}

/// Issues a one-time code the player trades for a new password.
pub async fn reset_code(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ResetCode>> {
    let (code, expires_at) = users::create_reset_code(&app.db, user_id(&id)?).await?;
    Ok(Json(ResetCode { code, expires_at }))
}

pub async fn list_invites(State(app): State<AppState>) -> Result<Json<Vec<Invite>>> {
    Ok(Json(users::list_invites(&app.db).await?))
}

#[derive(Deserialize)]
pub struct CreateInvite {
    #[serde(default)]
    note: String,
    expires_in_hours: Option<u32>,
}

#[derive(Serialize)]
pub struct NewInvite {
    invite: Invite,
    /// Shown once; only its hash is kept.
    code: String,
}

pub async fn create_invite(
    State(app): State<AppState>,
    Extension(admin): Extension<User>,
    Json(body): Json<CreateInvite>,
) -> Result<(StatusCode, Json<NewInvite>)> {
    let (invite, code) =
        users::create_invite(&app.db, admin.id, &body.note, body.expires_in_hours).await?;
    Ok((StatusCode::CREATED, Json(NewInvite { invite, code })))
}

pub async fn delete_invite(
    State(app): State<AppState>,
    Path(invite_id): Path<i64>,
) -> Result<StatusCode> {
    users::delete_invite(&app.db, invite_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Host
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct InstanceUsage {
    id: String,
    memory_bytes: u64,
}

#[derive(Serialize)]
pub struct SystemView {
    /// 0-100 across all cores, measured since the previous request.
    cpu_percent: f32,
    memory_total: u64,
    memory_used: u64,
    instances: Vec<InstanceUsage>,
}

pub async fn system(State(app): State<AppState>) -> Result<Json<SystemView>> {
    let running: Vec<(String, sysinfo::Pid)> = instances::list(&app.db)
        .await?
        .into_iter()
        .filter_map(|i| {
            let pid = app.supervisor.info(&i.id).pid?;
            Some((i.id, sysinfo::Pid::from_u32(pid)))
        })
        .collect();
    let pids: Vec<sysinfo::Pid> = running.iter().map(|(_, pid)| *pid).collect();

    let mut system = app.system.lock().expect("system lock");
    system.refresh_memory();
    system.refresh_cpu_usage();
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&pids), true);
    Ok(Json(SystemView {
        cpu_percent: system.global_cpu_usage(),
        memory_total: system.total_memory(),
        memory_used: system.used_memory(),
        instances: running
            .into_iter()
            .map(|(id, pid)| InstanceUsage {
                id,
                memory_bytes: system.process(pid).map_or(0, sysinfo::Process::memory),
            })
            .collect(),
    }))
}
