//! `admin-core`: the server-side daemon of Relic.

pub mod api;
pub mod backups;
pub mod blobs;
pub mod db;
pub mod deploy;
pub mod error;
pub mod files;
pub mod instances;
pub mod media;
pub mod modpack;
pub mod news;
pub mod settings;
pub mod sources;
pub mod supervisor;
pub mod textures;
pub mod users;
pub mod yggdrasil;

pub use error::{Error, Result};
