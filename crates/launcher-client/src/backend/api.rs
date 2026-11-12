//! Client for the launcher-facing `/api/v1` surface of `admin-core`.

use std::time::Duration;

use relic_proto::{API_VERSION, ApiError, Handshake, Manifest, NewsItem};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::lang::{Msg, t, tf};

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// The server answered with an error of its own.
    #[error("{message}")]
    Api {
        status: u16,
        code: String,
        message: String,
    },
    #[error("{}", tf(Msg::NetworkFailed, &[.0]))]
    Network(String),
    #[error("{}", tf(Msg::ProtocolFailed, &[.0]))]
    Protocol(String),
    #[error("{}", t(Msg::LauncherOutdated))]
    Outdated,
}

impl Error {
    /// The session is gone; the player has to sign in again.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Error::Api { status: 401, .. })
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub status: String,
    #[serde(default)]
    pub skin_sha256: Option<String>,
    #[serde(default)]
    pub skin_model: String,
    #[serde(default)]
    pub cape_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Login {
    pub user: Account,
    /// Absent when the account still waits for approval.
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameProfile {
    pub id: String,
    pub name: String,
}

/// What the game is started with instead of a password.
#[derive(Debug, Clone, Deserialize)]
pub struct GameSession {
    pub access_token: String,
    pub profile: GameProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Texture {
    Skin,
}

impl Texture {
    fn as_str(self) -> &'static str {
        match self {
            Texture::Skin => "skin",
        }
    }
}

/// One server, optionally with a signed-in session. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    origin: String,
    token: Option<String>,
}

impl Client {
    pub fn new(origin: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("relic-launcher/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .build()
            .expect("static client configuration is valid");
        Self {
            http,
            origin: origin.into(),
            token: None,
        }
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.origin)
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let request = self.http.request(method, self.url(path));
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    pub async fn handshake(&self) -> Result<Handshake> {
        let handshake: Handshake = json(self.request(Method::GET, "/handshake")).await?;
        if handshake.api_version > API_VERSION {
            return Err(Error::Outdated);
        }
        Ok(handshake)
    }

    pub async fn news(&self) -> Result<Vec<NewsItem>> {
        json(self.request(Method::GET, "/news")).await
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<Login> {
        let body = json!({ "username": username, "password": password });
        json(self.request(Method::POST, "/auth/login").json(&body)).await
    }

    pub async fn register(
        &self,
        username: &str,
        password: &str,
        invite: Option<&str>,
    ) -> Result<Login> {
        #[derive(Serialize)]
        struct Body<'a> {
            username: &'a str,
            password: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            invite: Option<&'a str>,
        }
        let body = Body {
            username,
            password,
            invite,
        };
        json(self.request(Method::POST, "/auth/register").json(&body)).await
    }

    pub async fn reset_password(&self, username: &str, code: &str, password: &str) -> Result<()> {
        let body = json!({ "username": username, "code": code, "password": password });
        empty(self.request(Method::POST, "/auth/reset").json(&body)).await
    }

    pub async fn me(&self) -> Result<Account> {
        json(self.request(Method::GET, "/auth/me")).await
    }

    pub async fn logout(&self) -> Result<()> {
        empty(self.request(Method::POST, "/auth/logout")).await
    }

    pub async fn change_password(&self, current: &str, new: &str) -> Result<()> {
        let body = json!({ "current": current, "new": new });
        empty(self.request(Method::POST, "/account/password").json(&body)).await
    }

    pub async fn game_session(&self) -> Result<GameSession> {
        json(self.request(Method::POST, "/account/game-session")).await
    }

    pub async fn put_texture(&self, kind: Texture, png: Vec<u8>, slim: bool) -> Result<Account> {
        let model = if slim { "slim" } else { "classic" };
        let path = format!("/account/{}?model={model}", kind.as_str());
        let request = self
            .request(Method::PUT, &path)
            .header("content-type", "image/png")
            .body(png);
        json(request).await
    }

    pub async fn delete_texture(&self, kind: Texture) -> Result<Account> {
        let path = format!("/account/{}", kind.as_str());
        json(self.request(Method::DELETE, &path)).await
    }

    pub async fn manifest(&self, instance: &str) -> Result<Manifest> {
        json(self.request(Method::GET, &format!("/instances/{instance}/manifest"))).await
    }

    /// A mod's picture by its hash, when the server has it and it is of a
    /// size a picture can be.
    pub async fn icon(&self, instance: &str, sha256: &str) -> Option<Vec<u8>> {
        const MAX_ICON: usize = 1024 * 1024;
        let path = format!("/instances/{instance}/blobs/{sha256}");
        let response = self.request(Method::GET, &path).send().await.ok()?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|len| len > MAX_ICON as u64)
        {
            return None;
        }
        let bytes = response.bytes().await.ok()?;
        (bytes.len() <= MAX_ICON).then(|| bytes.to_vec())
    }

    /// Where one file of an instance's pack is downloaded from.
    pub fn blob_url(&self, instance: &str, sha256: &str) -> String {
        self.url(&format!("/instances/{instance}/blobs/{sha256}"))
    }
}

fn network(err: reqwest::Error) -> Error {
    let reason = if err.is_timeout() {
        t(Msg::TimedOut).to_owned()
    } else if err.is_connect() {
        t(Msg::NothingAnswered).to_owned()
    } else {
        err.without_url().to_string()
    };
    Error::Network(reason)
}

async fn send(request: RequestBuilder) -> Result<Response> {
    let response = request.send().await.map_err(network)?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.bytes().await.unwrap_or_default();
    Err(error_from(status, &body))
}

fn error_from(status: StatusCode, body: &[u8]) -> Error {
    match serde_json::from_slice::<ApiError>(body) {
        Ok(api) => Error::Api {
            status: status.as_u16(),
            code: api.code,
            message: api.message,
        },
        // Not one of ours: a proxy's error page, or a different site entirely.
        Err(_) => Error::Protocol(format!("HTTP {}", status.as_u16())),
    }
}

async fn json<T: DeserializeOwned>(request: RequestBuilder) -> Result<T> {
    let body = send(request).await?.bytes().await.map_err(network)?;
    serde_json::from_slice(&body).map_err(|e| Error::Protocol(e.to_string()))
}

async fn empty(request: RequestBuilder) -> Result<()> {
    send(request).await.map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_errors_keep_their_message() {
        let err = error_from(
            StatusCode::UNAUTHORIZED,
            br#"{"code":"unauthorized","message":"wrong username or password"}"#,
        );
        assert_eq!(err.to_string(), "wrong username or password");
        assert!(err.is_unauthorized());
    }

    #[test]
    fn foreign_error_pages_are_not_shown_verbatim() {
        let err = error_from(StatusCode::BAD_GATEWAY, b"<html>nginx</html>");
        assert!(matches!(err, Error::Protocol(ref m) if m == "HTTP 502"));
    }

    #[test]
    fn pending_account_has_no_token() {
        let login: Login = serde_json::from_str(
            r#"{"user":{"id":"c8f90240-a178-4ee4-904d-e967dfad54ba","username":"Steve",
                "role":"player","status":"pending","skin_sha256":null,"skin_model":"classic",
                "cape_sha256":null,"created_at":1}}"#,
        )
        .unwrap();
        assert!(login.token.is_none());
        assert_eq!(login.user.status, "pending");
    }

    #[test]
    fn urls_are_rooted_at_the_api() {
        let client = Client::new("http://127.0.0.1:8097");
        assert_eq!(
            client.url("/handshake"),
            "http://127.0.0.1:8097/api/v1/handshake"
        );
        assert_eq!(
            client.blob_url("main", "abc"),
            "http://127.0.0.1:8097/api/v1/instances/main/blobs/abc"
        );
    }
}
