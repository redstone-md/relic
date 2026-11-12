-- Timestamps are unix seconds. Hashes are lowercase hex.

CREATE TABLE settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE users (
    id            TEXT PRIMARY KEY NOT NULL,          -- UUID, also the game profile id
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,                      -- argon2id PHC string
    role          TEXT NOT NULL DEFAULT 'player' CHECK (role IN ('player', 'admin')),
    status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('pending', 'active', 'banned')),
    skin_sha256   TEXT,
    skin_model    TEXT NOT NULL DEFAULT 'classic' CHECK (skin_model IN ('classic', 'slim')),
    cape_sha256   TEXT,
    created_at    INTEGER NOT NULL
) STRICT;

CREATE TABLE invites (
    id         INTEGER PRIMARY KEY,
    code_hash  TEXT NOT NULL UNIQUE,                  -- sha256 of the code; the code itself is never stored
    note       TEXT NOT NULL DEFAULT '',
    created_by TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    used_by    TEXT REFERENCES users (id) ON DELETE SET NULL,
    used_at    INTEGER
) STRICT;

CREATE TABLE sessions (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash   TEXT NOT NULL UNIQUE,                -- sha256 of the bearer token
    kind         TEXT NOT NULL CHECK (kind IN ('launcher', 'admin', 'game')),
    client_token TEXT,                                -- Yggdrasil clientToken, game sessions only
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL
) STRICT;
CREATE INDEX sessions_user ON sessions (user_id);

CREATE TABLE password_resets (
    code_hash  TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL,
    used_at    INTEGER
) STRICT;

CREATE TABLE instances (
    id                TEXT PRIMARY KEY NOT NULL,      -- slug, used in URLs and directory names
    name              TEXT NOT NULL,
    description       TEXT NOT NULL DEFAULT '',
    minecraft_version TEXT NOT NULL,
    loader_kind       TEXT NOT NULL CHECK (loader_kind IN ('vanilla', 'fabric', 'quilt', 'forge', 'neoforge')),
    loader_version    TEXT NOT NULL DEFAULT '',
    java_major        INTEGER NOT NULL,
    port              INTEGER NOT NULL UNIQUE,
    memory_mb         INTEGER NOT NULL DEFAULT 4096,
    mod_policy        TEXT NOT NULL DEFAULT 'optional' CHECK (mod_policy IN ('strict', 'optional', 'permissive')),
    auto_start        INTEGER NOT NULL DEFAULT 0,
    auto_restart      INTEGER NOT NULL DEFAULT 1,
    build_id          TEXT NOT NULL,                  -- regenerated on every change to the client file set
    created_at        INTEGER NOT NULL
) STRICT;

-- Content-addressed storage index. The bytes live in <data>/blobs/<aa>/<sha256>.
CREATE TABLE blobs (
    sha256     TEXT PRIMARY KEY NOT NULL,
    size       INTEGER NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

-- One row per file of an instance's modpack.
CREATE TABLE files (
    instance_id       TEXT NOT NULL REFERENCES instances (id) ON DELETE CASCADE,
    path              TEXT NOT NULL,                  -- instance-relative, forward slashes
    sha256            TEXT NOT NULL REFERENCES blobs (sha256),
    detected_side     TEXT NOT NULL CHECK (detected_side IN ('client', 'server', 'both', 'unknown')),
    detect_confidence TEXT NOT NULL DEFAULT 'none' CHECK (detect_confidence IN ('none', 'heuristic', 'declared')),
    detect_reason     TEXT NOT NULL DEFAULT '',
    sync_action       TEXT NOT NULL DEFAULT 'enforce' CHECK (sync_action IN ('enforce', 'sync_once')),
    optional          INTEGER NOT NULL DEFAULT 0,
    mod_id            TEXT,
    mod_name          TEXT,
    mod_version       TEXT,
    created_at        INTEGER NOT NULL,
    PRIMARY KEY (instance_id, path)
) STRICT;
CREATE INDEX files_sha256 ON files (sha256);

-- Manual side classification. Keyed by content so it follows a JAR across
-- instances, renames and re-uploads.
CREATE TABLE side_overrides (
    sha256     TEXT PRIMARY KEY NOT NULL,
    side       TEXT NOT NULL CHECK (side IN ('client', 'server', 'both')),
    updated_at INTEGER NOT NULL
) STRICT;
