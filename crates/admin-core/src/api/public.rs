//! Endpoints the launcher talks to, plus sign-in for the panel.

use axum::Json;
use axum::extract::{Path, Request, State};
use axum::http::header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH, SET_COOKIE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{Extensions, HeaderMap, HeaderValue, Method};
use axum::response::{IntoResponse, Response};
use relic_proto::{API_VERSION, Handshake, InstanceInfo, Manifest, NewsItem};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use super::auth::{self, Member, SESSION_COOKIE};
use super::{AppState, IP_ATTEMPTS, USER_ATTEMPTS, internal};
use crate::blobs::is_sha256;
use crate::error::{Error, Result};
use crate::users::{self, Role, SessionKind, Status, User};
use crate::{instances, media, news, settings};

fn session_cookie(token: &str, max_age: i64, headers: &HeaderMap) -> Result<HeaderValue> {
    let secure = if auth::is_https(headers) {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{secure}"
    ))
    .map_err(internal)
}

pub async fn handshake(State(app): State<AppState>, headers: HeaderMap) -> Result<Json<Handshake>> {
    let general = settings::general(&app.db).await?;
    let request_host = auth::host(&headers);
    let bare_host = auth::bare_host(request_host);
    let game_host = if general.public_host.is_empty() {
        bare_host
    } else {
        general.public_host.as_str()
    };
    let scheme = if auth::is_https(&headers) {
        "https"
    } else {
        "http"
    };

    let instances = instances::list(&app.db)
        .await?
        .into_iter()
        .map(|instance| {
            let runtime = app.supervisor.info(&instance.id);
            InstanceInfo {
                address: format!("{game_host}:{}", instance.port),
                status: runtime.status,
                players_online: runtime.players.len() as u32,
                players_max: runtime.players_max,
                id: instance.id,
                name: instance.name,
                description: instance.description,
                minecraft_version: instance.minecraft_version,
                loader: instance.loader,
                build_id: instance.build_id,
                banner_url: instance.banner_url,
                scene: instance.scene,
            }
        })
        .collect();

    Ok(Json(Handshake {
        api_version: API_VERSION,
        server_name: general.server_name,
        registration: general.registration,
        yggdrasil_url: format!("{scheme}://{request_host}/api/yggdrasil"),
        theme: general.theme,
        links: general.links,
        instances,
    }))
}

/// A picture or font the admin uploaded. Public: a launcher draws the
/// server's logo before anyone has signed in.
pub async fn media(
    State(app): State<AppState>,
    Path(name): Path<String>,
    request: Request,
) -> Result<Response> {
    let path = media::path(&app.media, &name).await?;
    let etag = format!("\"{}\"", name.split('.').next().unwrap_or_default());
    let mut response = if request
        .headers()
        .get(IF_NONE_MATCH)
        .is_some_and(|seen| seen.as_bytes() == etag.as_bytes())
    {
        axum::http::StatusCode::NOT_MODIFIED.into_response()
    } else {
        let mime: mime_guess::Mime = media::mime(&name).parse().map_err(internal)?;
        ServeFile::new_with_mime(path, &mime)
            .oneshot(request)
            .await
            .map_err(internal)?
            .map(axum::body::Body::new)
    };
    let headers = response.headers_mut();
    headers.insert(ETAG, HeaderValue::from_str(&etag).map_err(internal)?);
    // The hash is the name, so the content can never change.
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

pub async fn news(State(app): State<AppState>) -> Result<Json<Vec<NewsItem>>> {
    Ok(Json(news::list(&app.db, 50).await?))
}

#[derive(Serialize)]
pub struct SetupStatus {
    needed: bool,
}

pub async fn setup_status(State(app): State<AppState>) -> Result<Json<SetupStatus>> {
    Ok(Json(SetupStatus {
        needed: !users::admin_exists(&app.db).await?,
    }))
}

#[derive(Deserialize)]
pub struct SetupRequest {
    token: String,
    username: String,
    password: String,
}

/// Creates the first admin account in exchange for the token from the log.
pub async fn setup(
    State(app): State<AppState>,
    headers: HeaderMap,
    extensions: Extensions,
    Json(body): Json<SetupRequest>,
) -> Result<Response> {
    auth::check_origin(&headers, false)?;
    let ip_key = format!("setup:{}", auth::client_ip(&app, &headers, &extensions));
    app.limiter.check(&ip_key, USER_ATTEMPTS)?;

    let _guard = app.setup.lock().await;
    if users::admin_exists(&app.db).await? {
        return Err(Error::Conflict("setup has already been completed".into()));
    }
    let expected = settings::get(&app.db, settings::SETUP_TOKEN_HASH).await?;
    let given = users::sha256_hex(body.token.trim().to_ascii_uppercase().as_str());
    if expected.as_deref() != Some(given.as_str()) {
        app.limiter.fail(&ip_key);
        return Err(Error::Forbidden("wrong setup token".into()));
    }

    let user = users::create(
        &app.db,
        &body.username,
        &body.password,
        Role::Admin,
        Status::Active,
    )
    .await?;
    settings::remove(&app.db, settings::SETUP_TOKEN_HASH).await?;
    tracing::info!(username = %user.username, "first administrator created");

    let token = users::create_session(&app.db, user.id, SessionKind::Admin).await?;
    let cookie = session_cookie(&token, SessionKind::Admin.ttl(), &headers)?;
    Ok(([(SET_COOKIE, cookie)], Json(user)).into_response())
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
    /// Sign in to the panel (cookie session) rather than the launcher.
    #[serde(default)]
    admin: bool,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub(super) user: User,
    /// Launcher sessions only; the panel gets a cookie instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) token: Option<String>,
}

pub async fn login(
    State(app): State<AppState>,
    headers: HeaderMap,
    extensions: Extensions,
    Json(body): Json<LoginRequest>,
) -> Result<Response> {
    auth::check_origin(&headers, false)?;
    let user_key = format!("user:{}", body.username.to_ascii_lowercase());
    let ip_key = format!("ip:{}", auth::client_ip(&app, &headers, &extensions));
    app.limiter.check(&user_key, USER_ATTEMPTS)?;
    app.limiter.check(&ip_key, IP_ATTEMPTS)?;

    let user = match users::verify_login(&app.db, &body.username, &body.password).await {
        Ok(user) => user,
        Err(e) => {
            if matches!(e, Error::Unauthorized) {
                app.limiter.fail(&user_key);
                app.limiter.fail(&ip_key);
            }
            return Err(e);
        }
    };
    app.limiter.clear(&user_key);

    if !body.admin {
        let token = users::create_session(&app.db, user.id, SessionKind::Launcher).await?;
        return Ok(Json(LoginResponse {
            user,
            token: Some(token),
        })
        .into_response());
    }
    if user.role != Role::Admin {
        return Err(Error::Forbidden("administrator access required".into()));
    }
    let token = users::create_session(&app.db, user.id, SessionKind::Admin).await?;
    let cookie = session_cookie(&token, SessionKind::Admin.ttl(), &headers)?;
    Ok((
        [(SET_COOKIE, cookie)],
        Json(LoginResponse { user, token: None }),
    )
        .into_response())
}

pub async fn logout(State(app): State<AppState>, headers: HeaderMap) -> Result<Response> {
    if let Some(token) = auth::bearer(&headers) {
        users::revoke_session(&app.db, token).await?;
    }
    if let Some(token) = auth::cookie(&headers) {
        auth::check_origin(&headers, true)?;
        users::revoke_session(&app.db, token).await?;
    }
    let cookie = session_cookie("", 0, &headers)?;
    Ok((axum::http::StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response())
}

pub async fn me(Member(user): Member) -> Json<User> {
    Json(user)
}

pub async fn manifest(
    State(app): State<AppState>,
    Member(_): Member,
    Path(id): Path<String>,
) -> Result<Json<Manifest>> {
    Ok(Json(app.modpack.manifest(&id).await?))
}

/// Serves one file of the pack by hash, with `Range` support for resuming.
pub async fn blob(
    State(app): State<AppState>,
    Member(user): Member,
    Path((id, sha256)): Path<(String, String)>,
    request: Request,
) -> Result<Response> {
    if !is_sha256(&sha256) {
        return Err(Error::NotFound("file"));
    }
    // Players get what their client needs; server-only files stay private.
    let mut allowed = app.modpack.is_client_blob(&id, &sha256).await?;
    if !allowed && user.role == Role::Admin {
        allowed = app
            .modpack
            .list_files(&id)
            .await?
            .iter()
            .any(|f| f.sha256 == sha256);
    }
    if !allowed {
        return Err(Error::NotFound("file"));
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return Err(Error::NotFound("endpoint"));
    }

    let path = app.modpack.blobs().path(&sha256)?;
    // Mod pictures are shown as they are; everything else is a download.
    let mime = if app.modpack.is_icon(&id, &sha256).await? {
        mime_guess::mime::IMAGE_PNG
    } else {
        mime_guess::mime::APPLICATION_OCTET_STREAM
    };
    let mut response = ServeFile::new_with_mime(path, &mime)
        .oneshot(request)
        .await
        .map_err(internal)?
        .map(axum::body::Body::new);
    // The hash is the name, so the content can never change.
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    Ok(response)
}
