// Mirrors of the daemon's JSON. Source of truth: crates/relic-proto and
// crates/admin-core/src/api.

export type Side = "client" | "server" | "both" | "unknown";
export type LoaderKind = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";
export type ModPolicy = "strict" | "optional" | "permissive";
export type FileKind = "mod" | "config" | "resource_pack" | "shader_pack" | "other";
export type SyncAction = "enforce" | "sync_once";
export type RegistrationPolicy = "strict_invite" | "approval_required" | "closed";
export type InstanceStatus =
  | "stopped"
  | "installing"
  | "starting"
  | "running"
  | "stopping"
  | "crashed";
export type Provider = "modrinth" | "curseforge";

export interface Loader {
  type: LoaderKind;
  version: string;
}

export interface RuntimeInfo {
  status: InstanceStatus;
  players: string[];
  players_max: number;
  pid: number | null;
  started_at: number | null;
  last_error: string | null;
  crash_report: string | null;
}

export interface Instance {
  id: string;
  name: string;
  description: string;
  minecraft_version: string;
  loader: Loader;
  java_major: number;
  port: number;
  memory_mb: number;
  mod_policy: ModPolicy;
  /** Folders of the game where players keep nothing of their own. */
  locked_dirs: string[];
  auto_start: boolean;
  auto_restart: boolean;
  /** A picture behind the modpack in the launcher; replaces the scene. */
  banner_url?: string | null;
  scene: Scene;
  build_id: string;
  created_at: number;
  runtime: RuntimeInfo;
  eula_accepted: boolean;
  installed: boolean;
}

export interface InstanceUpdate {
  name?: string;
  description?: string;
  minecraft_version?: string;
  loader?: { type: LoaderKind; version?: string };
  java_major?: number;
  port?: number;
  memory_mb?: number;
  mod_policy?: ModPolicy;
  locked_dirs?: string[];
  auto_start?: boolean;
  auto_restart?: boolean;
  /** An empty string takes the banner away. */
  banner_url?: string;
  scene?: Scene;
}

/** The launcher's built-in animated backdrops. */
export type Scene = "mine" | "deep" | "forest" | "desert" | "snow";

/** One file or folder in an instance's directory. */
export interface FsEntry {
  name: string;
  dir: boolean;
  size: number;
  /** Unix seconds. */
  modified: number;
  /** Placed from the modpack, and put back by the next deploy. */
  pack: boolean;
}

export interface NewInstance {
  id: string;
  name: string;
  description: string;
  minecraft_version: string;
  loader: { type: LoaderKind; version?: string };
  port?: number;
  memory_mb?: number;
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
}

export interface FileRecord {
  instance_id: string;
  path: string;
  sha256: string;
  size: number;
  kind: FileKind;
  side: Side;
  side_source: "detected" | "override";
  detected_side: Side;
  detect_confidence: "none" | "heuristic" | "declared";
  detect_reason: string;
  sync_action: SyncAction;
  optional: boolean;
  mod_id: string | null;
  mod_name: string | null;
  mod_version: string | null;
  /** What the mod says about itself. */
  description: string;
  /** Hash of the mod's picture, served like a file of the pack. */
  icon: string | null;
  /** Paths of the files this one does not load without. */
  requires: string[];
  /** Mod ids it needs that no file of the pack provides. */
  missing: string[];
}

/** How much of the disk an instance takes, largest folder first. */
export interface FsUsage {
  total: number;
  /** Loose files of the top folder come under an empty name. */
  folders: { name: string; size: number }[];
}

/** A ready-made modpack in the catalogue. */
export interface PackHit {
  id: string;
  title: string;
  description: string;
  author: string;
  icon_url: string | null;
  cover_url: string | null;
  page_url: string;
  downloads: number;
  categories: string[];
}

export interface SearchHit {
  provider: Provider;
  id: string;
  title: string;
  description: string;
  author: string;
  icon_url: string | null;
  page_url: string;
  downloads: number;
  side: Side;
  distributable: boolean;
}

export interface ModVersion {
  id: string;
  name: string;
  filename: string;
  size: number;
  channel: string;
}

export interface Dependency {
  id: string;
  title: string;
}

export interface Installed {
  file: FileRecord;
  requires: Dependency[];
}

export interface MrpackReport {
  name: string;
  version: string;
  minecraft_version: string | null;
  loader: Loader | null;
  mismatch: boolean;
  added: number;
  failed: { path: string; error: string }[];
}

export interface BackupInfo {
  id: string;
  created_at: number;
  size: number;
}

export interface Palette {
  background: string;
  surface: string;
  border: string;
  accent: string;
  text_primary: string;
  text_secondary: string;
}

export interface Theme {
  palette: Palette;
  font_url?: string;
  logo_url?: string;
  background_url?: string;
}

export interface LinkItem {
  label: string;
  url: string;
  icon?: string;
}

export interface General {
  server_name: string;
  public_host: string;
  registration: RegistrationPolicy;
  theme: Theme;
  links: LinkItem[];
  backup_interval_hours: number;
  backup_keep: number;
}

export interface Settings extends General {
  curseforge_key_set: boolean;
}

export interface NewsItem {
  id: number;
  title: string;
  body: string;
  published_at: number;
  image_url?: string;
}

export interface NewsDraft {
  title: string;
  body: string;
  image_url?: string | null;
}

export interface User {
  id: string;
  username: string;
  role: "player" | "admin";
  status: "pending" | "active" | "banned";
  skin_sha256: string | null;
  skin_model: string;
  cape_sha256: string | null;
  created_at: number;
}

export interface Invite {
  id: number;
  note: string;
  created_by: string | null;
  created_at: number;
  expires_at: number | null;
  used_by: string | null;
  used_at: number | null;
}

export interface NewInvite {
  invite: Invite;
  /** Shown once; the server keeps only its hash. */
  code: string;
}

export interface ResetCode {
  code: string;
  expires_at: number;
}

export interface SystemInfo {
  cpu_percent: number;
  memory_total: number;
  memory_used: number;
  instances: { id: string; memory_bytes: number }[];
}

export interface ConsoleLine {
  seq: number;
  time: number;
  text: string;
  system: boolean;
}
