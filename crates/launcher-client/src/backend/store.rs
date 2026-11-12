//! What the launcher remembers between runs. Session tokens are not here:
//! they live in the system credential store (see [`super::secrets`]).

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MIN_MEMORY_MB: u32 = 1024;
pub const MAX_MEMORY_MB: u32 = 32768;
const DEFAULT_MEMORY_MB: u32 = 4096;

/// Where the launcher keeps everything. `RELIC_LAUNCHER_DIR` moves it, which
/// keeps a development copy apart from the real one.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("RELIC_LAUNCHER_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Relic")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    /// As the player typed it.
    pub address: String,
    /// The origin it resolved to.
    pub origin: String,
    /// Last account signed in here, to prefill the form.
    #[serde(default)]
    pub username: String,
    /// Instance selected last.
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: Option<Server>,
    /// Heap given to the game.
    #[serde(default = "default_memory")]
    pub memory_mb: u32,
    /// Optional files the player switched on, by instance id.
    #[serde(default)]
    pub optional: BTreeMap<String, BTreeSet<String>>,
    /// The language the player chose, as its code. Absent while the
    /// launcher follows the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Where the game goes: modpacks, the files they share, Java. Absent
    /// while it lives next to the launcher's own files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_dir: Option<PathBuf>,
}

fn default_memory() -> u32 {
    DEFAULT_MEMORY_MB
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: None,
            memory_mb: DEFAULT_MEMORY_MB,
            optional: BTreeMap::new(),
            language: None,
            game_dir: None,
        }
    }
}

impl Config {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("launcher.json")
    }

    /// Reads the saved configuration. A missing or unreadable file gives the
    /// defaults: losing preferences beats refusing to start.
    pub fn load(dir: &Path) -> Self {
        let mut config: Config = std::fs::read(Self::path(dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        config.memory_mb = config.memory_mb.clamp(MIN_MEMORY_MB, MAX_MEMORY_MB);
        config
    }

    /// Writes through a temporary file so a crash never leaves half a config.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = Self::path(dir);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }

    pub fn set_optional(&mut self, instance: &str, path: &str, enabled: bool) {
        let set = self.optional.entry(instance.to_owned()).or_default();
        if enabled {
            set.insert(path.to_owned());
        } else {
            set.remove(path);
        }
        if set.is_empty() {
            self.optional.remove(instance);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_garbage() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(dir.path()), Config::default());

        let mut config = Config {
            server: Some(Server {
                address: "play.example.com".into(),
                origin: "https://play.example.com".into(),
                username: "Steve".into(),
                instance: Some("main".into()),
            }),
            memory_mb: 6144,
            ..Config::default()
        };
        config.set_optional("main", "mods/minimap.jar", true);
        config.save(dir.path()).unwrap();
        assert_eq!(Config::load(dir.path()), config);

        std::fs::write(Config::path(dir.path()), b"{ not json").unwrap();
        assert_eq!(Config::load(dir.path()), Config::default());
    }

    #[test]
    fn memory_is_clamped_on_load() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(Config::path(dir.path()), br#"{"memory_mb": 12}"#).unwrap();
        assert_eq!(Config::load(dir.path()).memory_mb, MIN_MEMORY_MB);
    }

    #[test]
    fn optional_files_toggle() {
        let mut config = Config::default();
        config.set_optional("main", "mods/a.jar", true);
        assert!(config.optional["main"].contains("mods/a.jar"));
        assert!(!config.optional.contains_key("other"));
        config.set_optional("main", "mods/a.jar", false);
        assert!(config.optional.is_empty());
    }
}
