//! The admin panel, compiled into the binary.

use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist"]
struct Panel;

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; img-src 'self' https: data:; \
     style-src 'self' 'unsafe-inline'; connect-src 'self' ws: wss:; \
     frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    // Unknown paths are client-side routes of the single-page app.
    let (name, file) = match Panel::get(path) {
        Some(file) if !path.is_empty() => (path, file),
        _ => match Panel::get("index.html") {
            Some(file) => ("index.html", file),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    "The admin panel is not part of this build. Run `pnpm build` in web/ and rebuild.",
                )
                    .into_response();
            }
        },
    };
    let mime = mime_guess::from_path(name).first_or_octet_stream();
    // Vite fingerprints everything under assets/, so those never change.
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut response = file.data.into_owned().into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        headers.insert(CONTENT_TYPE, value);
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(cache));
    for (key, value) in [
        ("content-security-policy", CONTENT_SECURITY_POLICY),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
    ] {
        headers.insert(
            HeaderName::from_static(key),
            HeaderValue::from_static(value),
        );
    }
    response
}
