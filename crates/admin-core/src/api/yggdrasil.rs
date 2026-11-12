//! The Yggdrasil API, as authlib-injector specifies it. Game clients and
//! game servers talk to this instead of Mojang's services.

use axum::extract::{Multipart, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{Extensions, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{AppState, IP_ATTEMPTS, USER_ATTEMPTS, auth, store_texture};
use crate::blobs::is_sha256;
use crate::error::Error;
use crate::textures::TextureKind;
use crate::users::{self, User};
use crate::yggdrasil::{brief_profile, full_profile, unsigned_id};
use crate::{settings, textures};

/// Where the API lives. Clients are pointed here by the launcher and by the
/// `X-Authlib-Injector-API-Location` header.
pub const ROOT: &str = "/api/yggdrasil";

/// An error in the shape the game expects.
pub struct Failure {
    status: StatusCode,
    error: &'static str,
    message: String,
}

type Ygg<T> = Result<T, Failure>;

const FORBIDDEN: &str = "ForbiddenOperationException";
const ILLEGAL_ARGUMENT: &str = "IllegalArgumentException";

impl Failure {
    fn new(status: StatusCode, error: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            error,
            message: message.into(),
        }
    }

    fn invalid_token() -> Self {
        Self::new(StatusCode::FORBIDDEN, FORBIDDEN, "Invalid token.")
    }

    fn invalid_credentials() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            FORBIDDEN,
            "Invalid credentials. Invalid username or password.",
        )
    }

    fn illegal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ILLEGAL_ARGUMENT, message)
    }
}

impl From<Error> for Failure {
    fn from(err: Error) -> Self {
        match err {
            Error::Invalid(message) | Error::Conflict(message) => Self::illegal(message),
            Error::Forbidden(message) => Self::new(StatusCode::FORBIDDEN, FORBIDDEN, message),
            Error::Unauthorized => Self::new(
                StatusCode::UNAUTHORIZED,
                FORBIDDEN,
                "Authentication required.",
            ),
            Error::RateLimited => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                FORBIDDEN,
                "Too many attempts, try again later.",
            ),
            Error::NotFound(what) => Self::new(
                StatusCode::NOT_FOUND,
                "NotFoundException",
                format!("{what} not found"),
            ),
            err => {
                tracing::error!(error = %err, "yggdrasil request failed");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "internal error",
                )
            }
        }
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let body = json!({ "error": self.error, "errorMessage": self.message });
        (self.status, Json(body)).into_response()
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(
        auth::bare_host(host),
        "localhost" | "127.0.0.1" | "[::1]" | "::1"
    )
}

/// `scheme://host[:port]` the caller addressed us by.
fn request_origin(headers: &HeaderMap) -> String {
    let scheme = if auth::is_https(headers) {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://{}", auth::host(headers))
}

/// Game servers reach the API over loopback, but the texture URLs they pass
/// on must work for players. So the address signed-in players come in by is
/// remembered and used whenever the caller's own is not a public one.
fn remember_origin(app: &AppState, headers: &HeaderMap) -> String {
    let origin = request_origin(headers);
    if !is_loopback(auth::host(headers)) {
        *app.public_origin.lock().expect("origin lock") = Some(origin.clone());
    }
    origin
}

fn public_origin(app: &AppState, headers: &HeaderMap) -> String {
    if is_loopback(auth::host(headers))
        && let Some(origin) = app.public_origin.lock().expect("origin lock").clone()
    {
        return origin;
    }
    request_origin(headers)
}

fn textures_url(origin: &str) -> String {
    format!("{origin}{ROOT}/textures")
}

async fn meta(State(app): State<AppState>, headers: HeaderMap) -> Ygg<Json<Value>> {
    let general = settings::general(&app.db).await?;
    let keys = app.keys().await?;

    // Hosts clients may load textures from.
    let mut domains = vec![auth::bare_host(auth::host(&headers)).to_owned()];
    let remembered = app.public_origin.lock().expect("origin lock").clone();
    let remembered_host = remembered
        .as_deref()
        .and_then(|origin| origin.split_once("://"))
        .map(|(_, host)| auth::bare_host(host).to_owned());
    for host in [Some(general.public_host), remembered_host]
        .into_iter()
        .flatten()
    {
        if !host.is_empty() && !domains.contains(&host) {
            domains.push(host);
        }
    }

    Ok(Json(json!({
        "meta": {
            "serverName": general.server_name,
            "implementationName": "relic",
            "implementationVersion": env!("CARGO_PKG_VERSION"),
            "feature.non_email_login": true,
        },
        "skinDomains": domains,
        "signaturePublickey": keys.public_pem(),
    })))
}

/// Checks a name and password under the same limits as the launcher login.
async fn check_credentials(
    app: &AppState,
    headers: &HeaderMap,
    extensions: &Extensions,
    username: &str,
    password: &str,
) -> Ygg<User> {
    let user_key = format!("user:{}", username.to_ascii_lowercase());
    let ip_key = format!("ip:{}", auth::client_ip(app, headers, extensions));
    app.limiter.check(&user_key, USER_ATTEMPTS)?;
    app.limiter.check(&ip_key, IP_ATTEMPTS)?;
    match users::verify_login(&app.db, username, password).await {
        Ok(user) => {
            app.limiter.clear(&user_key);
            Ok(user)
        }
        Err(Error::Unauthorized) => {
            app.limiter.fail(&user_key);
            app.limiter.fail(&ip_key);
            Err(Failure::invalid_credentials())
        }
        Err(e) => Err(e.into()),
    }
}

fn user_object(user: &User) -> Value {
    json!({ "id": unsigned_id(user.id), "properties": [] })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Authenticate {
    username: String,
    password: String,
    client_token: Option<String>,
    #[serde(default)]
    request_user: bool,
}

fn check_client_token(token: &str) -> Ygg<()> {
    if token.is_empty() || token.len() > 128 {
        return Err(Failure::illegal("clientToken must be 1-128 characters"));
    }
    Ok(())
}

async fn authenticate(
    State(app): State<AppState>,
    headers: HeaderMap,
    extensions: Extensions,
    Json(body): Json<Authenticate>,
) -> Ygg<Json<Value>> {
    let client_token = match body.client_token {
        Some(token) => {
            check_client_token(&token)?;
            token
        }
        None => Uuid::new_v4().simple().to_string(),
    };
    let user =
        check_credentials(&app, &headers, &extensions, &body.username, &body.password).await?;
    remember_origin(&app, &headers);
    let access_token = users::create_game_session(&app.db, user.id, &client_token).await?;

    // One account is one character.
    let profile = brief_profile(&user);
    let mut response = json!({
        "accessToken": access_token,
        "clientToken": client_token,
        "availableProfiles": [profile],
        "selectedProfile": profile,
    });
    if body.request_user {
        response["user"] = user_object(&user);
    }
    Ok(Json(response))
}

/// The session behind an access token; a client token, when given, must be
/// the one the session was opened with.
async fn session(
    app: &AppState,
    access_token: &str,
    client_token: Option<&str>,
) -> Ygg<(User, String)> {
    let (user, bound) = users::game_session(&app.db, access_token)
        .await?
        .ok_or_else(Failure::invalid_token)?;
    if client_token.is_some_and(|given| given != bound) {
        return Err(Failure::invalid_token());
    }
    Ok((user, bound))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Refresh {
    access_token: String,
    client_token: Option<String>,
    #[serde(default)]
    request_user: bool,
    selected_profile: Option<Value>,
}

async fn refresh(State(app): State<AppState>, Json(body): Json<Refresh>) -> Ygg<Json<Value>> {
    let (user, client_token) =
        session(&app, &body.access_token, body.client_token.as_deref()).await?;
    if body.selected_profile.is_some() {
        // Tokens are issued with the account's only profile already chosen.
        return Err(Failure::illegal(
            "Access token already has a profile assigned.",
        ));
    }
    users::revoke_session(&app.db, &body.access_token).await?;
    let access_token = users::create_game_session(&app.db, user.id, &client_token).await?;
    let mut response = json!({
        "accessToken": access_token,
        "clientToken": client_token,
        "selectedProfile": brief_profile(&user),
    });
    if body.request_user {
        response["user"] = user_object(&user);
    }
    Ok(Json(response))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenPair {
    access_token: String,
    client_token: Option<String>,
}

async fn validate(State(app): State<AppState>, Json(body): Json<TokenPair>) -> Ygg<StatusCode> {
    session(&app, &body.access_token, body.client_token.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Always succeeds, whether or not the token was any good.
async fn invalidate(State(app): State<AppState>, Json(body): Json<TokenPair>) -> Ygg<StatusCode> {
    if users::game_session(&app.db, &body.access_token)
        .await?
        .is_some()
    {
        users::revoke_session(&app.db, &body.access_token).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Signout {
    username: String,
    password: String,
}

async fn signout(
    State(app): State<AppState>,
    headers: HeaderMap,
    extensions: Extensions,
    Json(body): Json<Signout>,
) -> Ygg<StatusCode> {
    let user =
        check_credentials(&app, &headers, &extensions, &body.username, &body.password).await?;
    users::revoke_game_sessions(&app.db, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Join {
    access_token: String,
    selected_profile: String,
    server_id: String,
}

/// A client announces the server it is about to enter.
async fn join(
    State(app): State<AppState>,
    headers: HeaderMap,
    extensions: Extensions,
    Json(body): Json<Join>,
) -> Ygg<StatusCode> {
    let (user, _) = session(&app, &body.access_token, None).await?;
    let profile = Uuid::parse_str(&body.selected_profile).ok();
    if profile != Some(user.id) {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            FORBIDDEN,
            "Invalid profile.",
        ));
    }
    if body.server_id.is_empty() || body.server_id.len() > 128 {
        return Err(Failure::illegal("serverId must be 1-128 characters"));
    }
    let origin = remember_origin(&app, &headers);
    let ip = auth::client_ip(&app, &headers, &extensions);
    app.joins.announce(&body.server_id, user.id, ip, origin);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HasJoined {
    username: String,
    server_id: String,
    ip: Option<String>,
}

/// A game server asks whether the player connecting to it is who they say.
async fn has_joined(State(app): State<AppState>, Query(query): Query<HasJoined>) -> Ygg<Response> {
    let Some((user_id, origin)) = app.joins.confirm(&query.server_id, query.ip.as_deref()) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let user = match users::get(&app.db, user_id).await {
        Ok(user) => user,
        Err(Error::NotFound(_)) => return Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => return Err(e.into()),
    };
    if user.status != users::Status::Active || !user.username.eq_ignore_ascii_case(&query.username)
    {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    let keys = app.keys().await?;
    let profile = full_profile(&user, &textures_url(&origin), Some(keys))?;
    Ok(Json(profile).into_response())
}

#[derive(Deserialize)]
struct ProfileQuery {
    unsigned: Option<bool>,
}

async fn profile(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<ProfileQuery>,
) -> Ygg<Response> {
    let Ok(id) = Uuid::parse_str(&id) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let user = match users::get(&app.db, id).await {
        Ok(user) => user,
        Err(Error::NotFound(_)) => return Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => return Err(e.into()),
    };
    let keys = if query.unsigned == Some(false) {
        Some(app.keys().await?)
    } else {
        None
    };
    let url = textures_url(&public_origin(&app, &headers));
    Ok(Json(full_profile(&user, &url, keys)?).into_response())
}

/// Most names one lookup may ask about.
const MAX_NAMES: usize = 100;

async fn profiles(
    State(app): State<AppState>,
    Json(names): Json<Vec<String>>,
) -> Ygg<Json<Vec<Value>>> {
    if names.len() > MAX_NAMES {
        return Err(Failure::illegal("too many names in one request"));
    }
    let mut found: Vec<User> = Vec::new();
    for name in &names {
        if let Some(user) = users::find_by_name(&app.db, name).await?
            && !found.iter().any(|known| known.id == user.id)
        {
            found.push(user);
        }
    }
    Ok(Json(found.iter().map(brief_profile).collect()))
}

/// The account a texture request acts on: the bearer of a game token, and
/// only for their own profile.
async fn texture_owner(app: &AppState, headers: &HeaderMap, profile: &str) -> Ygg<User> {
    let unauthorized = || {
        Failure::new(
            StatusCode::UNAUTHORIZED,
            FORBIDDEN,
            "Authentication required.",
        )
    };
    let token = auth::bearer(headers).ok_or_else(unauthorized)?;
    let (user, _) = users::game_session(&app.db, token)
        .await?
        .ok_or_else(unauthorized)?;
    if Uuid::parse_str(profile).ok() != Some(user.id) {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            FORBIDDEN,
            "This profile belongs to another account.",
        ));
    }
    Ok(user)
}

fn texture_kind(kind: &str) -> Ygg<TextureKind> {
    TextureKind::parse(kind).ok_or_else(|| Failure::illegal("texture type must be skin or cape"))
}

async fn put_texture(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path((profile, kind)): Path<(String, String)>,
    mut form: Multipart,
) -> Ygg<StatusCode> {
    let kind = texture_kind(&kind)?;
    let user = texture_owner(&app, &headers, &profile).await?;

    let malformed = |_| Failure::illegal("malformed upload");
    let mut slim = false;
    let mut file = None;
    while let Some(field) = form.next_field().await.map_err(malformed)? {
        match field.name() {
            Some("model") => slim = field.text().await.map_err(malformed)? == "slim",
            Some("file") => file = Some(field.bytes().await.map_err(malformed)?),
            _ => {}
        }
    }
    let file = file.ok_or_else(|| Failure::illegal("the upload has no file"))?;
    store_texture(&app, user.id, kind, file, slim).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_texture(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path((profile, kind)): Path<(String, String)>,
) -> Ygg<StatusCode> {
    let kind = texture_kind(&kind)?;
    let user = texture_owner(&app, &headers, &profile).await?;
    users::clear_texture(&app.db, user.id, kind).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Serves a skin or cape by hash. Other blobs of the store stay private.
async fn texture(State(app): State<AppState>, Path(sha256): Path<String>) -> Ygg<Response> {
    if !is_sha256(&sha256) || !users::is_texture(&app.db, &sha256).await? {
        return Err(Error::NotFound("texture").into());
    }
    let bytes = match tokio::fs::read(app.modpack.blobs().path(&sha256)?).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotFound("texture").into());
        }
        Err(e) => return Err(Error::from(e).into()),
    };
    Ok((
        [
            (CONTENT_TYPE, "image/png"),
            // The hash is the name, so the content can never change.
            (CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response())
}

async fn unknown_endpoint() -> Failure {
    Error::NotFound("endpoint").into()
}

pub fn router() -> Router<AppState> {
    let at = |path: &str| format!("{ROOT}{path}");
    let session = "/sessionserver/session/minecraft";
    Router::new()
        // authlib-injector asks for the root with a trailing slash.
        .route(ROOT, get(meta))
        .route(&at("/"), get(meta))
        .route(&at("/authserver/authenticate"), post(authenticate))
        .route(&at("/authserver/refresh"), post(refresh))
        .route(&at("/authserver/validate"), post(validate))
        .route(&at("/authserver/invalidate"), post(invalidate))
        .route(&at("/authserver/signout"), post(signout))
        .route(&at(&format!("{session}/join")), post(join))
        .route(&at(&format!("{session}/hasJoined")), get(has_joined))
        .route(&at(&format!("{session}/profile/{{id}}")), get(profile))
        .route(&at("/api/profiles/minecraft"), post(profiles))
        .route(
            &at("/api/user/profile/{id}/{kind}"),
            put(put_texture)
                .delete(delete_texture)
                .layer(axum::extract::DefaultBodyLimit::max(
                    textures::MAX_UPLOAD + 64 * 1024,
                )),
        )
        .route(&at("/textures/{sha256}"), get(texture))
        .route(&at("/{*rest}"), any(unknown_endpoint))
}
