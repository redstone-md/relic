//! Wire types shared by `admin-core` and `launcher-client`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Version of the launcher-facing HTTP API (`/api/v1/...`).
pub const API_VERSION: u32 = 1;

/// DNS SRV service label used for discovery: `_mcadmin._tcp.<host>`.
pub const SRV_SERVICE: &str = "_mcadmin._tcp";

// ---------------------------------------------------------------------------
// Modpack
// ---------------------------------------------------------------------------

/// Which side of the game a file belongs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Client,
    Server,
    Both,
    /// Could not be determined from metadata; an admin has to classify it.
    Unknown,
}

impl Side {
    pub fn on_client(self) -> bool {
        matches!(self, Side::Client | Side::Both)
    }

    pub fn on_server(self) -> bool {
        matches!(self, Side::Server | Side::Both)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Side::Client => "client",
            Side::Server => "server",
            Side::Both => "both",
            Side::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "client" => Side::Client,
            "server" => Side::Server,
            "both" => Side::Both,
            "unknown" => Side::Unknown,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl LoaderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LoaderKind::Vanilla => "vanilla",
            LoaderKind::Fabric => "fabric",
            LoaderKind::Quilt => "quilt",
            LoaderKind::Forge => "forge",
            LoaderKind::NeoForge => "neoforge",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "vanilla" => LoaderKind::Vanilla,
            "fabric" => LoaderKind::Fabric,
            "quilt" => LoaderKind::Quilt,
            "forge" => LoaderKind::Forge,
            "neoforge" => LoaderKind::NeoForge,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loader {
    #[serde(rename = "type")]
    pub kind: LoaderKind,
    /// Loader version; empty for vanilla.
    #[serde(default)]
    pub version: String,
}

/// What the launcher does with JARs in `mods/` that the manifest does not list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModPolicy {
    /// Anything not in the manifest is removed before launch.
    Strict,
    /// Only manifest files are allowed; files marked `optional` are opt-in.
    #[default]
    Optional,
    /// Extra JARs supplied by the player are left alone.
    Permissive,
}

impl ModPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            ModPolicy::Strict => "strict",
            ModPolicy::Optional => "optional",
            ModPolicy::Permissive => "permissive",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "strict" => ModPolicy::Strict,
            "optional" => ModPolicy::Optional,
            "permissive" => ModPolicy::Permissive,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Mod,
    Config,
    ResourcePack,
    ShaderPack,
    Other,
}

impl FileKind {
    /// Classifies a file by its instance-relative path (forward slashes).
    pub fn from_path(path: &str) -> Self {
        match path.split('/').next().unwrap_or_default() {
            "mods" => FileKind::Mod,
            "config" | "defaultconfigs" => FileKind::Config,
            "resourcepacks" => FileKind::ResourcePack,
            "shaderpacks" => FileKind::ShaderPack,
            _ => FileKind::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::Mod => "mod",
            FileKind::Config => "config",
            FileKind::ResourcePack => "resource_pack",
            FileKind::ShaderPack => "shader_pack",
            FileKind::Other => "other",
        }
    }
}

/// How strictly the launcher keeps a file in line with the server copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    /// Always matches the server; local edits are overwritten.
    Enforce,
    /// The player's once it lands (typical for configs). A newer version
    /// from the server replaces it only while the player has not changed it.
    SyncOnce,
}

impl SyncAction {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncAction::Enforce => "enforce",
            SyncAction::SyncOnce => "sync_once",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "enforce" => SyncAction::Enforce,
            "sync_once" => SyncAction::SyncOnce,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFile {
    /// Instance-relative path with forward slashes, e.g. `mods/sodium.jar`.
    pub path: String,
    /// Lowercase hex SHA-256 (64 chars); also the blob id for download.
    pub sha256: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub kind: FileKind,
    pub action: SyncAction,
    /// Opt-in for the player. Only meaningful under [`ModPolicy::Optional`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// What a player would call it, e.g. `Xaero's Minimap`. Empty when the
    /// file does not say.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// SHA-256 of a PNG to show next to the name; fetched like a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Paths of optional files this one does not work without. Switching
    /// this file on brings them along.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaRuntime {
    pub major_version: u32,
    pub vendor: String,
}

/// Everything a launcher needs to reproduce the client side of one instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Changes whenever the file set or game/loader version changes.
    pub build_id: Uuid,
    pub instance_id: String,
    pub minecraft_version: String,
    pub loader: Loader,
    pub java_runtime: JavaRuntime,
    pub policy: ModPolicy,
    pub files: Vec<ManifestFile>,
    /// Glob patterns of what belongs to the player (`options.txt`,
    /// `saves/*`, ...). A file of the pack on such a path is written once
    /// and never overwritten or removed afterwards.
    pub protected_paths: Vec<String>,
    /// Folders in which only the pack's files may stay; anything else the
    /// launcher removes before the game starts. `mods` follows `policy`
    /// and is not listed here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locked_dirs: Vec<String>,
}

/// Folders an admin may lock besides `mods`.
pub const LOCKABLE_DIRS: &[&str] = &["resourcepacks", "shaderpacks"];

/// Files of a pack that belong to the player from the first launch on.
pub fn player_owned(path: &str) -> bool {
    matches!(
        path.to_ascii_lowercase().as_str(),
        "options.txt" | "optionsof.txt" | "optionsshaders.txt" | "servers.dat"
    )
}

/// Paths that are always protected, regardless of server configuration.
pub const DEFAULT_PROTECTED_PATHS: &[&str] = &[
    "options.txt",
    "optionsof.txt",
    "optionsshaders.txt",
    "servers.dat",
    "screenshots/*",
    "saves/*",
    "logs/*",
    "crash-reports/*",
];

// ---------------------------------------------------------------------------
// Handshake, branding
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub background: String,
    pub surface: String,
    pub border: String,
    pub accent: String,
    pub text_primary: String,
    pub text_secondary: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            background: "#0d0d0f".into(),
            surface: "#24262d".into(),
            border: "#3a3d47".into(),
            accent: "#f2b441".into(),
            text_primary: "#e8e1cf".into(),
            text_secondary: "#99948a".into(),
        }
    }
}

/// Server-provided branding applied to the launcher's fixed layout.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Theme {
    #[serde(default)]
    pub palette: Palette,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub label: String,
    pub url: String,
    /// Icon name from the launcher's built-in set (`discord`, `globe`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationPolicy {
    /// Registration needs a single-use invite code.
    #[default]
    StrictInvite,
    /// Anyone may register; an admin has to approve before first login.
    ApprovalRequired,
    /// Accounts are created by admins only.
    Closed,
}

impl RegistrationPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            RegistrationPolicy::StrictInvite => "strict_invite",
            RegistrationPolicy::ApprovalRequired => "approval_required",
            RegistrationPolicy::Closed => "closed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "strict_invite" => RegistrationPolicy::StrictInvite,
            "approval_required" => RegistrationPolicy::ApprovalRequired,
            "closed" => RegistrationPolicy::Closed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Stopped,
    Installing,
    Starting,
    Running,
    Stopping,
    Crashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceInfo {
    /// A picture to show behind the modpack, still or animated (PNG, JPEG,
    /// GIF, WebP). Takes the place of the built-in scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub scene: Scene,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub minecraft_version: String,
    pub loader: Loader,
    /// Game address players connect to, `host:port`.
    pub address: String,
    pub status: InstanceStatus,
    pub players_online: u32,
    pub players_max: u32,
    /// Current manifest build; lets the launcher skip an unchanged manifest.
    pub build_id: Uuid,
}

/// Which of the launcher's built-in animated scenes stands behind a modpack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scene {
    /// A stone mine shaft with gold and diamonds.
    #[default]
    Mine,
    /// The same shaft far down, in deepslate.
    Deep,
    /// A tunnel through trees.
    Forest,
    /// Sand and gravel.
    Desert,
    /// Snow and stone.
    Snow,
}

impl Scene {
    pub fn as_str(self) -> &'static str {
        match self {
            Scene::Mine => "mine",
            Scene::Deep => "deep",
            Scene::Forest => "forest",
            Scene::Desert => "desert",
            Scene::Snow => "snow",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "mine" => Scene::Mine,
            "deep" => Scene::Deep,
            "forest" => Scene::Forest,
            "desert" => Scene::Desert,
            "snow" => Scene::Snow,
            _ => return None,
        })
    }
}

/// Response of `GET /api/v1/handshake`: the first thing a launcher fetches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handshake {
    pub api_version: u32,
    pub server_name: String,
    pub registration: RegistrationPolicy,
    /// Root of the authlib-injector compatible API.
    pub yggdrasil_url: String,
    pub theme: Theme,
    pub links: Vec<Link>,
    pub instances: Vec<InstanceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: i64,
    pub title: String,
    /// Markdown.
    pub body: String,
    /// Unix seconds.
    pub published_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// JSON body of every non-2xx response from the `/api/v1` surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// Stable machine-readable code, e.g. `invalid_credentials`.
    pub code: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_with_prd_field_names() {
        let json = r#"{
            "build_id": "c8f90240-a178-4ee4-904d-e967dfad54ba",
            "instance_id": "main",
            "minecraft_version": "1.21.1",
            "loader": { "type": "fabric", "version": "0.16.9" },
            "java_runtime": { "major_version": 21, "vendor": "temurin" },
            "policy": "optional",
            "files": [{
                "path": "mods/sodium.jar",
                "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "size": 1420834,
                "type": "mod",
                "action": "enforce"
            }],
            "protected_paths": ["options.txt"]
        }"#;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.loader.kind, LoaderKind::Fabric);
        assert_eq!(manifest.files[0].kind, FileKind::Mod);
        assert!(!manifest.files[0].optional);

        let back: Manifest =
            serde_json::from_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
        assert_eq!(back, manifest);
    }

    #[test]
    fn file_kind_follows_top_level_directory() {
        assert_eq!(FileKind::from_path("mods/a.jar"), FileKind::Mod);
        assert_eq!(FileKind::from_path("config/a/b.toml"), FileKind::Config);
        assert_eq!(
            FileKind::from_path("shaderpacks/x.zip"),
            FileKind::ShaderPack
        );
        assert_eq!(FileKind::from_path("kubejs/startup.js"), FileKind::Other);
    }
}
