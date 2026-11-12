//! Minecraft plumbing shared by `admin-core` and `launcher-client`:
//! Mojang version metadata, verified downloads, Temurin runtimes and
//! mod loader installation.

pub mod authlib;
pub mod client;
mod forge_client;
pub mod http;
pub mod java;
pub mod loaders;
pub mod mojang;
pub mod profile;
pub mod server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request to {url} failed: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{url} returned HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("checksum mismatch for {url}: expected {expected}, got {actual}")]
    Checksum {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("unexpected response from {url}: {message}")]
    Decode { url: String, message: String },
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("installer failed: {0}")]
    Installer(String),
    #[error("archive error: {0}")]
    Archive(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub use reqwest;
