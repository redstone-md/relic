//! Who is calling: bearer tokens for launchers, a cookie for the panel.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::header::{AUTHORIZATION, COOKIE, HOST, ORIGIN};
use axum::http::request::Parts;
use axum::http::{Extensions, HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;

use super::AppState;
use crate::error::{Error, Result};
use crate::users::{self, Role, SessionKind, User};

pub const SESSION_COOKIE: &str = "relic_session";

pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

pub fn cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(name, value)| (name == SESSION_COOKIE && !value.is_empty()).then_some(value))
}

fn forwarded<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)?
        .to_str()
        .ok()?
        .split(',')
        .next_back()
        .map(str::trim)
}

/// Whether the request reached the public endpoint over TLS.
pub fn is_https(headers: &HeaderMap) -> bool {
    forwarded(headers, "x-forwarded-proto") == Some("https")
}

/// `host[:port]` the client addressed.
pub fn host(headers: &HeaderMap) -> &str {
    headers
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost")
}

/// A `host[:port]` without the port; IPv6 literals keep their brackets.
pub fn bare_host(host: &str) -> &str {
    match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    }
}

/// Browsers attach `Origin` to cross-site requests and to every WebSocket
/// handshake. A request that carries one must come from our own pages.
pub fn check_origin(headers: &HeaderMap, required: bool) -> Result<()> {
    let Some(origin) = headers.get(ORIGIN).and_then(|v| v.to_str().ok()) else {
        return if required {
            Err(Error::Forbidden("missing Origin header".into()))
        } else {
            Ok(())
        };
    };
    let origin_host = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    if origin_host.eq_ignore_ascii_case(host(headers)) {
        Ok(())
    } else {
        Err(Error::Forbidden("cross-site request refused".into()))
    }
}

/// The address to hold login attempts against.
pub fn client_ip(app: &AppState, headers: &HeaderMap, extensions: &Extensions) -> String {
    if app.trust_proxy
        && let Some(ip) = forwarded(headers, "x-forwarded-for")
    {
        return ip.to_owned();
    }
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "unknown".to_owned(), |info| info.0.ip().to_string())
}

/// Resolves the caller to an admin. Cookie-authenticated requests that
/// change state must also prove they come from the panel itself.
pub async fn admin(app: &AppState, method: &Method, headers: &HeaderMap) -> Result<User> {
    let token = match bearer(headers) {
        Some(token) => token,
        None => {
            let token = cookie(headers).ok_or(Error::Unauthorized)?;
            let safe = matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);
            check_origin(headers, !safe)?;
            token
        }
    };
    let user = users::authenticate(&app.db, token, SessionKind::Admin).await?;
    if user.role != Role::Admin {
        return Err(Error::Forbidden("administrator access required".into()));
    }
    Ok(user)
}

/// Guards the `/admin` routes and hands the admin to the handlers.
pub async fn require_admin(
    State(app): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let user = admin(&app, request.method(), request.headers()).await?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

/// Any signed-in account: a launcher session or an admin session.
pub struct Member(pub User);

impl FromRequestParts<AppState> for Member {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, app: &AppState) -> Result<Self> {
        if let Some(token) = bearer(&parts.headers)
            && let Ok(user) = users::authenticate(&app.db, token, SessionKind::Launcher).await
        {
            return Ok(Member(user));
        }
        admin(app, &parts.method, &parts.headers).await.map(Member)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        pairs
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), HeaderValue::from_static(v)))
            .collect()
    }

    #[test]
    fn tokens_are_read_from_header_and_cookie() {
        let h = headers(&[
            ("authorization", "Bearer abc"),
            ("cookie", "a=1; relic_session=xyz; b=2"),
        ]);
        assert_eq!(bearer(&h), Some("abc"));
        assert_eq!(cookie(&h), Some("xyz"));
        let h = headers(&[
            ("authorization", "Basic abc"),
            ("cookie", "relic_session_old=1"),
        ]);
        assert_eq!(bearer(&h), None);
        assert_eq!(cookie(&h), None);
    }

    #[test]
    fn origin_must_match_the_host() {
        let same = headers(&[
            ("host", "mc.example.org"),
            ("origin", "https://mc.example.org"),
        ]);
        assert!(check_origin(&same, true).is_ok());
        let cross = headers(&[
            ("host", "mc.example.org"),
            ("origin", "https://evil.example"),
        ]);
        assert!(check_origin(&cross, false).is_err());
        let none = headers(&[("host", "mc.example.org")]);
        assert!(check_origin(&none, false).is_ok());
        assert!(check_origin(&none, true).is_err());
    }

    #[test]
    fn ports_are_stripped_from_hosts() {
        assert_eq!(bare_host("mc.example.org:8080"), "mc.example.org");
        assert_eq!(bare_host("mc.example.org"), "mc.example.org");
        assert_eq!(bare_host("[::1]:8080"), "[::1]");
        assert_eq!(bare_host("[::1]"), "[::1]");
    }
}
