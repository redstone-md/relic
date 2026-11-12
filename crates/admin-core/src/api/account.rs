//! What players do to their own account: registering, passwords, skins and
//! the game sessions the launcher starts the game with.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{Extensions, HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::auth::{self, Member};
use super::public::LoginResponse;
use super::{AppState, IP_ATTEMPTS, USER_ATTEMPTS, store_texture};
use crate::error::{Error, Result};
use crate::settings;
use crate::textures::TextureKind;
use crate::users::{self, SessionKind, Status, User};
use crate::yggdrasil::brief_profile;

/// Registrations one address may attempt per window, successful or not.
const REGISTRATIONS: u32 = 10;

#[derive(Deserialize)]
pub struct Register {
    username: String,
    password: String,
    invite: Option<String>,
}

/// Creates an account under the server's registration policy. An account
/// that still awaits approval comes back without a token.
pub async fn register(
    State(app): State<AppState>,
    headers: HeaderMap,
    extensions: Extensions,
    Json(body): Json<Register>,
) -> Result<(StatusCode, Json<LoginResponse>)> {
    auth::check_origin(&headers, false)?;
    // Every attempt counts: this is what keeps invite codes from being
    // guessed and the user table from being flooded.
    let key = format!("register:{}", auth::client_ip(&app, &headers, &extensions));
    app.limiter.check(&key, REGISTRATIONS)?;
    app.limiter.fail(&key);

    let policy = settings::general(&app.db).await?.registration;
    let user = users::register(
        &app.db,
        &body.username,
        &body.password,
        policy,
        body.invite.as_deref(),
    )
    .await?;
    let token = if user.status == Status::Active {
        Some(users::create_session(&app.db, user.id, SessionKind::Launcher).await?)
    } else {
        None
    };
    Ok((StatusCode::CREATED, Json(LoginResponse { user, token })))
}

#[derive(Deserialize)]
pub struct Reset {
    username: String,
    code: String,
    password: String,
}

/// Sets a new password in exchange for a code an admin issued.
pub async fn reset(
    State(app): State<AppState>,
    headers: HeaderMap,
    extensions: Extensions,
    Json(body): Json<Reset>,
) -> Result<StatusCode> {
    auth::check_origin(&headers, false)?;
    let user_key = format!("user:{}", body.username.to_ascii_lowercase());
    let ip_key = format!("ip:{}", auth::client_ip(&app, &headers, &extensions));
    app.limiter.check(&user_key, USER_ATTEMPTS)?;
    app.limiter.check(&ip_key, IP_ATTEMPTS)?;
    match users::reset_password(&app.db, &body.username, &body.code, &body.password).await {
        Ok(_) => {
            app.limiter.clear(&user_key);
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            if matches!(e, Error::Forbidden(_)) {
                app.limiter.fail(&user_key);
                app.limiter.fail(&ip_key);
            }
            Err(e)
        }
    }
}

#[derive(Deserialize)]
pub struct ChangePassword {
    current: String,
    new: String,
}

/// Changes the caller's password and signs them out everywhere else.
pub async fn change_password(
    State(app): State<AppState>,
    Member(user): Member,
    headers: HeaderMap,
    Json(body): Json<ChangePassword>,
) -> Result<StatusCode> {
    let key = format!("user:{}", user.username.to_ascii_lowercase());
    app.limiter.check(&key, USER_ATTEMPTS)?;
    let keep = auth::bearer(&headers).or_else(|| auth::cookie(&headers));
    match users::change_password(&app.db, user.id, &body.current, &body.new, keep).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            if matches!(e, Error::Forbidden(_) | Error::Unauthorized) {
                app.limiter.fail(&key);
            }
            Err(e)
        }
    }
}

#[derive(Serialize)]
pub struct GameSession {
    access_token: String,
    client_token: String,
    profile: Value,
}

/// Opens the Yggdrasil session the launcher hands to the game, so the
/// password never has to be typed (or kept) a second time.
pub async fn game_session(
    State(app): State<AppState>,
    Member(user): Member,
) -> Result<Json<GameSession>> {
    let client_token = Uuid::new_v4().simple().to_string();
    let access_token = users::create_game_session(&app.db, user.id, &client_token).await?;
    Ok(Json(GameSession {
        access_token,
        client_token,
        profile: brief_profile(&user),
    }))
}

fn texture_kind(kind: &str) -> Result<TextureKind> {
    TextureKind::parse(kind).ok_or(Error::NotFound("endpoint"))
}

#[derive(Deserialize)]
pub struct TextureOptions {
    model: Option<String>,
}

/// Puts a skin or cape on the caller's account. The body is the PNG itself.
pub async fn put_texture(
    State(app): State<AppState>,
    Member(user): Member,
    Path(kind): Path<String>,
    Query(options): Query<TextureOptions>,
    body: Bytes,
) -> Result<Json<User>> {
    let kind = texture_kind(&kind)?;
    let slim = match options.model.as_deref() {
        None | Some("classic") => false,
        Some("slim") => true,
        Some(_) => return Err(Error::invalid("model must be classic or slim")),
    };
    store_texture(&app, user.id, kind, body, slim).await?;
    Ok(Json(users::get(&app.db, user.id).await?))
}

pub async fn delete_texture(
    State(app): State<AppState>,
    Member(user): Member,
    Path(kind): Path<String>,
) -> Result<Json<User>> {
    let kind = texture_kind(&kind)?;
    users::clear_texture(&app.db, user.id, kind).await?;
    Ok(Json(users::get(&app.db, user.id).await?))
}
