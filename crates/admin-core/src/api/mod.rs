//! The HTTP surface: `/api/v1` for launchers and the admin panel, and the
//! embedded panel itself.

mod account;
mod admin;
mod assets;
mod auth;
mod public;
mod ws;
mod yggdrasil;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::Router;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use relic_mc::reqwest;
use relic_proto::ApiError;
use sqlx::SqlitePool;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::backups::Backups;
use crate::blobs::BlobStore;
use crate::error::{Error, Result, now};
use crate::modpack::Modpack;
use crate::settings;
use crate::sources::Sources;
use crate::supervisor::{Paths, Supervisor};
use crate::textures::{self, TextureKind};
use crate::users;
use crate::yggdrasil::{Joins, Keys};

pub use auth::SESSION_COOKIE;

/// Failed sign-ins allowed per account and per address in one window.
const USER_ATTEMPTS: u32 = 10;
const IP_ATTEMPTS: u32 = 30;

pub struct App {
    pub db: SqlitePool,
    pub modpack: Modpack,
    pub supervisor: Supervisor,
    pub backups: Backups,
    pub sources: Sources,
    pub http: reqwest::Client,
    /// Scratch space for uploads in flight; emptied on startup.
    pub tmp: PathBuf,
    /// Pictures and fonts uploaded for the launcher.
    pub media: PathBuf,
    /// Believe `X-Forwarded-For`; only behind a reverse proxy that sets it.
    pub trust_proxy: bool,
    limiter: Limiter,
    /// Serialises first-run setup so only one admin can come out of it.
    setup: tokio::sync::Mutex<()>,
    system: Mutex<sysinfo::System>,
    minecraft_versions: Mutex<Option<(Instant, Vec<String>)>>,
    /// The profile signing key, read (or made) on first use.
    keys: tokio::sync::OnceCell<Keys>,
    keys_path: PathBuf,
    joins: Joins,
    /// `scheme://host` players last reached the Yggdrasil API by.
    public_origin: Mutex<Option<String>>,
}

pub type AppState = Arc<App>;

impl App {
    /// Opens the data directory and wires the services together. Starts no
    /// background work.
    pub async fn open(data_dir: &Path, trust_proxy: bool) -> Result<AppState> {
        tokio::fs::create_dir_all(data_dir).await?;
        let db = crate::db::open(&data_dir.join("relic.db")).await?;
        let blobs = BlobStore::open(data_dir.join("blobs")).await?;
        let modpack = Modpack::new(db.clone(), blobs);
        let http = relic_mc::http::client();
        let supervisor = Supervisor::new(
            db.clone(),
            modpack.clone(),
            http.clone(),
            Paths {
                instances: data_dir.join("instances"),
                runtimes: data_dir.join("runtimes"),
            },
        );
        let tmp = data_dir.join("tmp");
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        tokio::fs::create_dir_all(&tmp).await?;
        let keys_dir = data_dir.join("keys");
        tokio::fs::create_dir_all(&keys_dir).await?;
        Ok(Arc::new(App {
            backups: Backups::new(db.clone(), supervisor.clone(), data_dir.join("backups")),
            sources: Sources::new(http.clone(), modpack.clone(), tmp.clone()),
            db,
            modpack,
            supervisor,
            http,
            tmp,
            media: data_dir.join("media"),
            trust_proxy,
            limiter: Limiter::default(),
            setup: tokio::sync::Mutex::new(()),
            system: Mutex::new(sysinfo::System::new()),
            minecraft_versions: Mutex::new(None),
            keys: tokio::sync::OnceCell::new(),
            keys_path: keys_dir.join("yggdrasil.pem"),
            joins: Joins::default(),
            public_origin: Mutex::new(None),
        }))
    }

    /// The key profiles are signed with. The first call on a fresh data
    /// directory generates it, which takes a few seconds.
    pub async fn keys(&self) -> Result<&Keys> {
        self.keys
            .get_or_try_init(|| Keys::load_or_create(&self.keys_path))
            .await
    }

    /// While no admin account exists, issues a fresh one-time setup token and
    /// returns it for the operator to read off the log.
    pub async fn issue_setup_token(&self) -> Result<Option<String>> {
        if users::admin_exists(&self.db).await? {
            settings::remove(&self.db, settings::SETUP_TOKEN_HASH).await?;
            return Ok(None);
        }
        let token = users::random_code();
        settings::set(
            &self.db,
            settings::SETUP_TOKEN_HASH,
            &users::sha256_hex(&token),
        )
        .await?;
        Ok(Some(token))
    }
}

/// Counts failures per key in fixed windows.
#[derive(Default)]
struct Limiter {
    failures: Mutex<HashMap<String, (u32, i64)>>,
}

const LIMIT_WINDOW: i64 = 10 * 60;

impl Limiter {
    fn check(&self, key: &str, max: u32) -> Result<()> {
        let failures = self.failures.lock().expect("limiter lock");
        match failures.get(key) {
            Some((count, since)) if now() - since < LIMIT_WINDOW && *count >= max => {
                Err(Error::RateLimited)
            }
            _ => Ok(()),
        }
    }

    fn fail(&self, key: &str) {
        let time = now();
        let mut failures = self.failures.lock().expect("limiter lock");
        if failures.len() > 10_000 {
            failures.retain(|_, (_, since)| time - *since < LIMIT_WINDOW);
        }
        let entry = failures.entry(key.to_owned()).or_insert((0, time));
        if time - entry.1 >= LIMIT_WINDOW {
            *entry = (0, time);
        }
        entry.0 += 1;
    }

    fn clear(&self, key: &str) {
        self.failures.lock().expect("limiter lock").remove(key);
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Error::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Error::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid"),
            Error::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Error::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Error::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            Error::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Error::Upstream(_) => (StatusCode::BAD_GATEWAY, "upstream"),
            Error::Db(_) | Error::Migrate(_) | Error::Io(_) | Error::Other(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            // Details stay in the log; they may name paths or queries.
            tracing::error!(error = %self, "request failed");
            "internal error".to_owned()
        } else {
            self.to_string()
        };
        let body = ApiError {
            code: code.to_owned(),
            message,
        };
        (status, axum::Json(body)).into_response()
    }
}

/// For failures that can only be our own fault.
fn internal(err: impl std::fmt::Display) -> Error {
    Error::Other(anyhow::anyhow!("{err}"))
}

/// Checks an uploaded skin or cape and puts it on the account. Returns the
/// texture's hash.
async fn store_texture(
    app: &AppState,
    user_id: Uuid,
    kind: TextureKind,
    upload: axum::body::Bytes,
    slim: bool,
) -> Result<String> {
    if upload.len() > textures::MAX_UPLOAD {
        return Err(Error::invalid("the image is too large"));
    }
    // Decoding is CPU work.
    let png = tokio::task::spawn_blocking(move || textures::sanitize(kind, &upload))
        .await
        .map_err(internal)??;
    app.modpack.set_texture(user_id, kind, &png, slim).await
}

/// Tells authlib-injector where the Yggdrasil API is, so players can type
/// just the server's address.
async fn api_location(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        HeaderName::from_static("x-authlib-injector-api-location"),
        HeaderValue::from_static("/api/yggdrasil/"),
    );
    response
}

async fn unknown_endpoint() -> Error {
    Error::NotFound("endpoint")
}

pub fn router(app: AppState) -> Router {
    let admin = Router::new()
        .route("/system", get(admin::system))
        .route("/events", get(ws::events))
        .route("/users", get(admin::users).post(admin::create_user))
        .route(
            "/users/{user_id}",
            patch(admin::update_user).delete(admin::delete_user),
        )
        .route("/users/{user_id}/reset-code", post(admin::reset_code))
        .route(
            "/invites",
            get(admin::list_invites).post(admin::create_invite),
        )
        .route("/invites/{invite_id}", delete(admin::delete_invite))
        .route(
            "/settings",
            get(admin::get_settings).put(admin::put_settings),
        )
        .route("/settings/curseforge-key", put(admin::put_curseforge_key))
        .route("/news", post(admin::create_news))
        .route(
            "/media",
            post(admin::upload_media).layer(axum::extract::DefaultBodyLimit::max(
                crate::media::MAX_UPLOAD,
            )),
        )
        .route(
            "/news/{news_id}",
            put(admin::update_news).delete(admin::delete_news),
        )
        .route("/meta/minecraft-versions", get(admin::minecraft_versions))
        .route("/meta/loader-versions", get(admin::loader_versions))
        .route("/sides/{sha256}", put(admin::set_side))
        .route(
            "/instances",
            get(admin::list_instances).post(admin::create_instance),
        )
        .route(
            "/instances/{id}",
            get(admin::get_instance)
                .patch(admin::update_instance)
                .delete(admin::delete_instance),
        )
        .route("/instances/{id}/start", post(admin::start))
        .route("/instances/{id}/stop", post(admin::stop))
        .route("/instances/{id}/restart", post(admin::restart))
        .route("/instances/{id}/command", post(admin::command))
        .route("/instances/{id}/eula", post(admin::accept_eula))
        .route("/instances/{id}/console", get(ws::console))
        .route("/instances/{id}/files", get(admin::list_files))
        .route(
            "/instances/{id}/files/{*path}",
            put(admin::upload_file)
                .patch(admin::patch_file)
                .delete(admin::delete_file),
        )
        .route(
            "/instances/{id}/fs",
            get(admin::fs_list).delete(admin::fs_remove),
        )
        .route(
            "/instances/{id}/fs/content",
            get(admin::fs_read).put(admin::fs_write),
        )
        .route("/instances/{id}/fs/usage", get(admin::fs_usage))
        .route("/instances/{id}/fs/archive", get(admin::fs_archive))
        .route("/instances/{id}/fs/unpack", post(admin::fs_unpack))
        .route("/instances/{id}/fs/mkdir", post(admin::fs_mkdir))
        .route("/instances/{id}/fs/rename", post(admin::fs_rename))
        .route("/instances/{id}/mrpack", post(admin::import_mrpack))
        .route("/instances/{id}/packs/search", get(admin::search_packs))
        .route("/instances/{id}/packs/install", post(admin::install_pack))
        .route("/instances/{id}/mods/search", get(admin::search_mods))
        .route("/instances/{id}/mods/versions", get(admin::mod_versions))
        .route("/instances/{id}/mods/install", post(admin::install_mod))
        .route(
            "/instances/{id}/backups",
            get(admin::list_backups).post(admin::create_backup),
        )
        .route(
            "/instances/{id}/backups/{backup_id}",
            delete(admin::delete_backup),
        )
        .route(
            "/instances/{id}/backups/{backup_id}/restore",
            post(admin::restore_backup),
        )
        .route(
            "/instances/{id}/backups/{backup_id}/download",
            get(admin::download_backup),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            app.clone(),
            auth::require_admin,
        ));

    let api = Router::new()
        .route("/handshake", get(public::handshake))
        .route("/news", get(public::news))
        .route("/media/{name}", get(public::media))
        .route("/setup", get(public::setup_status).post(public::setup))
        .route("/auth/login", post(public::login))
        .route("/auth/logout", post(public::logout))
        .route("/auth/me", get(public::me))
        .route("/auth/register", post(account::register))
        .route("/auth/reset", post(account::reset))
        .route("/account/password", post(account::change_password))
        .route("/account/game-session", post(account::game_session))
        .route(
            "/account/{kind}",
            put(account::put_texture)
                .delete(account::delete_texture)
                .layer(axum::extract::DefaultBodyLimit::max(textures::MAX_UPLOAD)),
        )
        .route("/instances/{id}/manifest", get(public::manifest))
        .route("/instances/{id}/blobs/{sha256}", get(public::blob))
        .nest("/admin", admin)
        .fallback(unknown_endpoint);

    Router::new()
        .nest("/api/v1", api)
        .merge(yggdrasil::router())
        .fallback(assets::serve)
        .layer(axum::middleware::from_fn(api_location))
        .layer(TraceLayer::new_for_http())
        .with_state(app)
}
