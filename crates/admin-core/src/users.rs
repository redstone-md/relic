//! Accounts and bearer sessions.

use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash};
use relic_proto::RegistrationPolicy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};
use uuid::Uuid;

use crate::error::{Error, Result, now};
use crate::textures::TextureKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Player,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Player => "player",
            Role::Admin => "admin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Registered, waiting for an admin's approval.
    Pending,
    Active,
    Banned,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Active => "active",
            Status::Banned => "banned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Launcher,
    Admin,
    Game,
}

impl SessionKind {
    fn as_str(self) -> &'static str {
        match self {
            SessionKind::Launcher => "launcher",
            SessionKind::Admin => "admin",
            SessionKind::Game => "game",
        }
    }

    /// Lifetime in seconds. Idle admin sessions end sooner.
    pub fn ttl(self) -> i64 {
        match self {
            SessionKind::Admin => 7 * 24 * 3600,
            SessionKind::Launcher | SessionKind::Game => 30 * 24 * 3600,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub role: Role,
    pub status: Status,
    pub skin_sha256: Option<String>,
    pub skin_model: String,
    pub cape_sha256: Option<String>,
    pub created_at: i64,
}

/// Minecraft's own rules for player names.
pub fn validate_username(name: &str) -> Result<()> {
    let ok = (3..=16).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(Error::invalid(
            "username must be 3-16 characters: letters, digits and underscore",
        ))
    }
}

pub fn validate_password(password: &str) -> Result<()> {
    if (8..=128).contains(&password.chars().count()) {
        Ok(())
    } else {
        Err(Error::invalid("password must be 8-128 characters"))
    }
}

pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// A random secret with 256 bits of entropy, as lowercase hex.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    hex::encode(bytes)
}

/// A short code for humans to pass around (invites, password resets):
/// 80 bits, without look-alike characters.
pub fn random_code() -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 16];
    rand::fill(&mut bytes);
    let chars: Vec<char> = bytes
        .iter()
        .map(|b| ALPHABET[usize::from(b & 31)] as char)
        .collect();
    chars
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// Argon2id is deliberately slow; keep it off the async worker threads.
async fn hash_password(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| Error::Other(anyhow::anyhow!("password hashing failed: {e}")))
    })
    .await
    .map_err(|e| Error::Other(e.into()))?
}

async fn password_matches(password: String, phc: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&phc).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
}

fn from_row(row: &SqliteRow) -> Result<User> {
    let corrupt = |what: &str| Error::Other(anyhow::anyhow!("users row has invalid {what}"));
    let id: String = row.try_get("id")?;
    let role: String = row.try_get("role")?;
    let status: String = row.try_get("status")?;
    Ok(User {
        id: Uuid::parse_str(&id).map_err(|_| corrupt("id"))?,
        username: row.try_get("username")?,
        role: match role.as_str() {
            "player" => Role::Player,
            "admin" => Role::Admin,
            _ => return Err(corrupt("role")),
        },
        status: match status.as_str() {
            "pending" => Status::Pending,
            "active" => Status::Active,
            "banned" => Status::Banned,
            _ => return Err(corrupt("status")),
        },
        skin_sha256: row.try_get("skin_sha256")?,
        skin_model: row.try_get("skin_model")?,
        cape_sha256: row.try_get("cape_sha256")?,
        created_at: row.try_get("created_at")?,
    })
}

fn taken(err: sqlx::Error) -> Error {
    match err {
        sqlx::Error::Database(e) if e.is_unique_violation() => {
            Error::Conflict("this username is taken".into())
        }
        e => e.into(),
    }
}

async fn insert<'e>(
    db: impl sqlx::SqliteExecutor<'e>,
    id: Uuid,
    username: &str,
    password_hash: &str,
    role: Role,
    status: Status,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(username)
    .bind(password_hash)
    .bind(role.as_str())
    .bind(status.as_str())
    .bind(now())
    .execute(db)
    .await
    .map_err(taken)?;
    Ok(())
}

pub async fn create(
    db: &SqlitePool,
    username: &str,
    password: &str,
    role: Role,
    status: Status,
) -> Result<User> {
    validate_username(username)?;
    validate_password(password)?;
    let hash = hash_password(password.to_owned()).await?;
    let id = Uuid::new_v4();
    insert(db, id, username, &hash, role, status).await?;
    get(db, id).await
}

/// A code as its holder typed it: case and stray spaces do not matter.
fn code_hash(code: &str) -> String {
    sha256_hex(&code.trim().to_ascii_uppercase())
}

/// Self-service registration under the server's policy. An invite is used
/// up in the same transaction that creates the account.
pub async fn register(
    db: &SqlitePool,
    username: &str,
    password: &str,
    policy: RegistrationPolicy,
    invite: Option<&str>,
) -> Result<User> {
    let invite = invite.map(str::trim).filter(|code| !code.is_empty());
    let status = match (policy, invite) {
        (RegistrationPolicy::Closed, _) => {
            return Err(Error::Forbidden("registration is closed".into()));
        }
        (RegistrationPolicy::StrictInvite, None) => {
            return Err(Error::Forbidden("an invite code is required".into()));
        }
        (RegistrationPolicy::ApprovalRequired, None) => Status::Pending,
        (_, Some(_)) => Status::Active,
    };
    validate_username(username)?;
    validate_password(password)?;
    let hash = hash_password(password.to_owned()).await?;
    let id = Uuid::new_v4();

    let mut tx = db.begin().await?;
    insert(&mut *tx, id, username, &hash, Role::Player, status).await?;
    if let Some(code) = invite {
        let time = now();
        let used = sqlx::query(
            "UPDATE invites SET used_by = ?, used_at = ?
             WHERE code_hash = ? AND used_at IS NULL AND (expires_at IS NULL OR expires_at > ?)",
        )
        .bind(id.to_string())
        .bind(time)
        .bind(code_hash(code))
        .bind(time)
        .execute(&mut *tx)
        .await?;
        if used.rows_affected() != 1 {
            // Dropping the transaction takes the account back out.
            return Err(Error::Forbidden(
                "this invite code is invalid, expired or already used".into(),
            ));
        }
    }
    tx.commit().await?;
    get(db, id).await
}

pub async fn get(db: &SqlitePool, id: Uuid) -> Result<User> {
    let row = sqlx::query("SELECT * FROM users WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?
        .ok_or(Error::NotFound("user"))?;
    from_row(&row)
}

pub async fn list(db: &SqlitePool) -> Result<Vec<User>> {
    sqlx::query("SELECT * FROM users ORDER BY created_at, username")
        .fetch_all(db)
        .await?
        .iter()
        .map(from_row)
        .collect()
}

pub async fn find_by_name(db: &SqlitePool, username: &str) -> Result<Option<User>> {
    sqlx::query("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(db)
        .await?
        .as_ref()
        .map(from_row)
        .transpose()
}

async fn count_admins(db: &SqlitePool) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin' AND status = 'active'")
            .fetch_one(db)
            .await?,
    )
}

/// Whether `user` is the only administrator able to sign in.
async fn is_last_admin(db: &SqlitePool, user: &User) -> Result<bool> {
    Ok(user.role == Role::Admin && user.status == Status::Active && count_admins(db).await? <= 1)
}

/// Changes an account's standing. Anything but `Active` signs it out everywhere.
pub async fn set_status(db: &SqlitePool, id: Uuid, status: Status) -> Result<()> {
    let user = get(db, id).await?;
    if status != Status::Active && is_last_admin(db, &user).await? {
        return Err(Error::Conflict(
            "the last administrator cannot be locked out".into(),
        ));
    }
    sqlx::query("UPDATE users SET status = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(id.to_string())
        .execute(db)
        .await?;
    if status != Status::Active {
        revoke_sessions(db, id, None).await?;
    }
    Ok(())
}

pub async fn set_role(db: &SqlitePool, id: Uuid, role: Role) -> Result<()> {
    let user = get(db, id).await?;
    if role != Role::Admin && is_last_admin(db, &user).await? {
        return Err(Error::Conflict(
            "the last administrator cannot be demoted".into(),
        ));
    }
    sqlx::query("UPDATE users SET role = ? WHERE id = ?")
        .bind(role.as_str())
        .bind(id.to_string())
        .execute(db)
        .await?;
    if role != Role::Admin {
        // Panel sessions of a former admin are worthless; drop them.
        sqlx::query("DELETE FROM sessions WHERE user_id = ? AND kind = 'admin'")
            .bind(id.to_string())
            .execute(db)
            .await?;
    }
    Ok(())
}

pub async fn delete(db: &SqlitePool, id: Uuid) -> Result<()> {
    let user = get(db, id).await?;
    if is_last_admin(db, &user).await? {
        return Err(Error::Conflict(
            "the last administrator cannot be deleted".into(),
        ));
    }
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id.to_string())
        .execute(db)
        .await?;
    Ok(())
}

/// Takes the account's skin or cape off. The image itself goes with the
/// next sweep of unreferenced blobs.
pub async fn clear_texture(db: &SqlitePool, id: Uuid, kind: TextureKind) -> Result<()> {
    sqlx::query(match kind {
        TextureKind::Skin => {
            "UPDATE users SET skin_sha256 = NULL, skin_model = 'classic' WHERE id = ?"
        }
        TextureKind::Cape => "UPDATE users SET cape_sha256 = NULL WHERE id = ?",
    })
    .bind(id.to_string())
    .execute(db)
    .await?;
    Ok(())
}

/// Whether some account wears the blob as its skin or cape. Only those
/// blobs are public.
pub async fn is_texture(db: &SqlitePool, sha256: &str) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE skin_sha256 = ? OR cape_sha256 = ?")
            .bind(sha256)
            .bind(sha256)
            .fetch_one(db)
            .await?;
    Ok(count > 0)
}

/// Sets a new password and signs the account out everywhere, except for the
/// session whose token is `keep`.
async fn replace_password(
    db: &SqlitePool,
    id: Uuid,
    password: &str,
    keep: Option<&str>,
) -> Result<()> {
    validate_password(password)?;
    let hash = hash_password(password.to_owned()).await?;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(id.to_string())
        .execute(db)
        .await?;
    revoke_sessions(db, id, keep).await
}

/// Changes the password of a signed-in account that knows the current one.
pub async fn change_password(
    db: &SqlitePool,
    id: Uuid,
    current: &str,
    new: &str,
    keep: Option<&str>,
) -> Result<()> {
    validate_password(new)?;
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?
        .ok_or(Error::NotFound("user"))?;
    if !password_matches(current.to_owned(), hash).await {
        return Err(Error::Forbidden("the current password is wrong".into()));
    }
    replace_password(db, id, new, keep).await
}

/// How long a password reset code works, in seconds.
const RESET_TTL: i64 = 24 * 3600;

/// Issues a one-time password reset code for an admin to hand to the
/// player. Replaces any earlier code. Returns the code and its expiry.
pub async fn create_reset_code(db: &SqlitePool, id: Uuid) -> Result<(String, i64)> {
    get(db, id).await?;
    let code = random_code();
    let expires_at = now() + RESET_TTL;
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM password_resets WHERE user_id = ?")
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO password_resets (code_hash, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(code_hash(&code))
        .bind(id.to_string())
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((code, expires_at))
}

/// Trades a reset code for a new password. Every failure looks the same.
pub async fn reset_password(
    db: &SqlitePool,
    username: &str,
    code: &str,
    password: &str,
) -> Result<User> {
    validate_password(password)?;
    let refused = || Error::Forbidden("this reset code is invalid or expired".into());
    let user = find_by_name(db, username).await?.ok_or_else(refused)?;
    let time = now();
    let used = sqlx::query(
        "UPDATE password_resets SET used_at = ?
         WHERE code_hash = ? AND user_id = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(time)
    .bind(code_hash(code))
    .bind(user.id.to_string())
    .bind(time)
    .execute(db)
    .await?;
    if used.rows_affected() != 1 {
        return Err(refused());
    }
    replace_password(db, user.id, password, None).await?;
    Ok(user)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Invite {
    pub id: i64,
    pub note: String,
    pub created_by: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub used_by: Option<String>,
    pub used_at: Option<i64>,
}

const INVITE_COLUMNS: &str = "SELECT i.id, i.note, i.created_at, i.expires_at, i.used_at,
        c.username AS created_by, u.username AS used_by
     FROM invites i
     LEFT JOIN users c ON c.id = i.created_by
     LEFT JOIN users u ON u.id = i.used_by";

fn invite_from_row(row: &SqliteRow) -> Result<Invite> {
    Ok(Invite {
        id: row.try_get("id")?,
        note: row.try_get("note")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        used_by: row.try_get("used_by")?,
        used_at: row.try_get("used_at")?,
    })
}

/// Creates a single-use invite. The code is returned once and never stored.
pub async fn create_invite(
    db: &SqlitePool,
    created_by: Uuid,
    note: &str,
    expires_in_hours: Option<u32>,
) -> Result<(Invite, String)> {
    let note = note.trim();
    if note.chars().count() > 200 {
        return Err(Error::invalid("the note must be at most 200 characters"));
    }
    let code = random_code();
    let time = now();
    let id = sqlx::query(
        "INSERT INTO invites (code_hash, note, created_by, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(code_hash(&code))
    .bind(note)
    .bind(created_by.to_string())
    .bind(time)
    .bind(expires_in_hours.map(|hours| time + i64::from(hours) * 3600))
    .execute(db)
    .await?
    .last_insert_rowid();
    let row = sqlx::query(&format!("{INVITE_COLUMNS} WHERE i.id = ?"))
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok((invite_from_row(&row)?, code))
}

pub async fn list_invites(db: &SqlitePool) -> Result<Vec<Invite>> {
    sqlx::query(&format!(
        "{INVITE_COLUMNS} ORDER BY i.created_at DESC, i.id DESC"
    ))
    .fetch_all(db)
    .await?
    .iter()
    .map(invite_from_row)
    .collect()
}

pub async fn delete_invite(db: &SqlitePool, id: i64) -> Result<()> {
    let deleted = sqlx::query("DELETE FROM invites WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(Error::NotFound("invite"));
    }
    Ok(())
}

pub async fn admin_exists(db: &SqlitePool) -> Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
        .fetch_one(db)
        .await?;
    Ok(count > 0)
}

/// Checks a username and password. Unknown users cost as much time as known
/// ones, and every failure looks the same to the caller.
pub async fn verify_login(db: &SqlitePool, username: &str, password: &str) -> Result<User> {
    // A real hash of a throwaway secret, so the miss path does the same work.
    static DUMMY: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    let row = sqlx::query("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(db)
        .await?;
    let Some(row) = row else {
        let dummy = DUMMY
            .get_or_try_init(|| hash_password(random_token()))
            .await?;
        password_matches(password.to_owned(), dummy.clone()).await;
        return Err(Error::Unauthorized);
    };
    let hash: String = row.try_get("password_hash")?;
    if !password_matches(password.to_owned(), hash).await {
        return Err(Error::Unauthorized);
    }
    let user = from_row(&row)?;
    match user.status {
        Status::Active => Ok(user),
        Status::Pending => Err(Error::Forbidden(
            "this account is waiting for an admin's approval".into(),
        )),
        Status::Banned => Err(Error::Forbidden("this account is banned".into())),
    }
}

/// Opens a session and returns its bearer token. Only a hash is stored.
pub async fn create_session(db: &SqlitePool, user_id: Uuid, kind: SessionKind) -> Result<String> {
    let token = random_token();
    let time = now();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, kind, created_at, expires_at, last_used_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id.to_string())
    .bind(sha256_hex(&token))
    .bind(kind.as_str())
    .bind(time)
    .bind(time + kind.ttl())
    .bind(time)
    .execute(db)
    .await?;
    Ok(token)
}

/// Resolves a bearer token of the given kind to its active user.
pub async fn authenticate(db: &SqlitePool, token: &str, kind: SessionKind) -> Result<User> {
    let time = now();
    let hash = sha256_hex(token);
    let row = sqlx::query(
        "SELECT u.* FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.kind = ? AND s.expires_at > ?",
    )
    .bind(&hash)
    .bind(kind.as_str())
    .bind(time)
    .fetch_optional(db)
    .await?
    .ok_or(Error::Unauthorized)?;
    let user = from_row(&row)?;
    if user.status != Status::Active {
        return Err(Error::Unauthorized);
    }
    sqlx::query("UPDATE sessions SET last_used_at = ? WHERE token_hash = ?")
        .bind(time)
        .bind(&hash)
        .execute(db)
        .await?;
    Ok(user)
}

pub async fn revoke_session(db: &SqlitePool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(sha256_hex(token))
        .execute(db)
        .await?;
    Ok(())
}

/// Game sessions one account may hold; the oldest make room for new ones.
const MAX_GAME_SESSIONS: i64 = 10;

/// Opens a Yggdrasil session bound to the client's `client_token` and
/// returns its access token.
pub async fn create_game_session(
    db: &SqlitePool,
    user_id: Uuid,
    client_token: &str,
) -> Result<String> {
    let token = random_token();
    let time = now();
    sqlx::query(
        "INSERT INTO sessions
            (id, user_id, token_hash, kind, client_token, created_at, expires_at, last_used_at)
         VALUES (?, ?, ?, 'game', ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id.to_string())
    .bind(sha256_hex(&token))
    .bind(client_token)
    .bind(time)
    .bind(time + SessionKind::Game.ttl())
    .bind(time)
    .execute(db)
    .await?;
    sqlx::query(
        "DELETE FROM sessions WHERE user_id = ? AND kind = 'game' AND id NOT IN (
            SELECT id FROM sessions WHERE user_id = ? AND kind = 'game'
            ORDER BY created_at DESC, rowid DESC LIMIT ?)",
    )
    .bind(user_id.to_string())
    .bind(user_id.to_string())
    .bind(MAX_GAME_SESSIONS)
    .execute(db)
    .await?;
    Ok(token)
}

/// The active account behind a Yggdrasil access token, with the client
/// token the session was opened with.
pub async fn game_session(db: &SqlitePool, access_token: &str) -> Result<Option<(User, String)>> {
    let row = sqlx::query(
        "SELECT u.*, s.client_token FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.kind = 'game' AND s.expires_at > ?",
    )
    .bind(sha256_hex(access_token))
    .bind(now())
    .fetch_optional(db)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let user = from_row(&row)?;
    if user.status != Status::Active {
        return Ok(None);
    }
    let client_token: Option<String> = row.try_get("client_token")?;
    Ok(Some((user, client_token.unwrap_or_default())))
}

pub async fn revoke_game_sessions(db: &SqlitePool, user_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ? AND kind = 'game'")
        .bind(user_id.to_string())
        .execute(db)
        .await?;
    Ok(())
}

/// Ends every session of the account, except the one whose token is `keep`.
pub async fn revoke_sessions(db: &SqlitePool, user_id: Uuid, keep: Option<&str>) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash IS NOT ?")
        .bind(user_id.to_string())
        .bind(keep.map(sha256_hex))
        .execute(db)
        .await?;
    Ok(())
}

pub async fn purge_expired_sessions(db: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(now())
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn login_and_sessions() {
        let db = crate::db::open_in_memory().await.unwrap();
        assert!(!admin_exists(&db).await.unwrap());
        let admin = create(&db, "Notch", "correct horse", Role::Admin, Status::Active)
            .await
            .unwrap();
        assert!(admin_exists(&db).await.unwrap());

        // Names are unique regardless of case.
        let taken = create(
            &db,
            "notch",
            "another password",
            Role::Player,
            Status::Active,
        )
        .await;
        assert!(matches!(taken, Err(Error::Conflict(_))));

        assert_eq!(
            verify_login(&db, "notch", "correct horse")
                .await
                .unwrap()
                .id,
            admin.id
        );
        assert!(matches!(
            verify_login(&db, "Notch", "wrong horse").await,
            Err(Error::Unauthorized)
        ));
        assert!(matches!(
            verify_login(&db, "nobody", "correct horse").await,
            Err(Error::Unauthorized)
        ));

        let token = create_session(&db, admin.id, SessionKind::Admin)
            .await
            .unwrap();
        assert_eq!(
            authenticate(&db, &token, SessionKind::Admin)
                .await
                .unwrap()
                .id,
            admin.id
        );
        // A token is only good for the kind of session it was issued for.
        assert!(
            authenticate(&db, &token, SessionKind::Launcher)
                .await
                .is_err()
        );
        assert!(authenticate(&db, "0000", SessionKind::Admin).await.is_err());
        revoke_session(&db, &token).await.unwrap();
        assert!(authenticate(&db, &token, SessionKind::Admin).await.is_err());
    }

    #[tokio::test]
    async fn pending_and_invalid_accounts_are_refused() {
        let db = crate::db::open_in_memory().await.unwrap();
        create(&db, "Alex", "long enough", Role::Player, Status::Pending)
            .await
            .unwrap();
        assert!(matches!(
            verify_login(&db, "Alex", "long enough").await,
            Err(Error::Forbidden(_))
        ));
        assert!(
            create(&db, "a", "long enough", Role::Player, Status::Active)
                .await
                .is_err()
        );
        assert!(
            create(
                &db,
                "has space",
                "long enough",
                Role::Player,
                Status::Active
            )
            .await
            .is_err()
        );
        assert!(
            create(&db, "Steve", "short", Role::Player, Status::Active)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn registration_follows_the_policy() {
        use RegistrationPolicy::{ApprovalRequired, Closed, StrictInvite};
        let db = crate::db::open_in_memory().await.unwrap();
        let admin = create(&db, "root", "correct horse", Role::Admin, Status::Active)
            .await
            .unwrap();
        let password = "long enough";

        assert!(register(&db, "Alex", password, Closed, None).await.is_err());
        assert!(
            register(&db, "Alex", password, StrictInvite, None)
                .await
                .is_err()
        );
        let bogus = Some("AAAA-BBBB-CCCC-DDDD");
        assert!(
            register(&db, "Alex", password, StrictInvite, bogus)
                .await
                .is_err()
        );
        // A refused registration leaves nothing behind.
        assert!(find_by_name(&db, "Alex").await.unwrap().is_none());

        let (invite, code) = create_invite(&db, admin.id, "for Alex", Some(1))
            .await
            .unwrap();
        assert_eq!(invite.created_by.as_deref(), Some("root"));
        let typed = code.to_lowercase();
        let alex = register(&db, "Alex", password, StrictInvite, Some(&typed))
            .await
            .unwrap();
        assert_eq!(alex.status, Status::Active);
        // Single use.
        assert!(
            register(&db, "Steve", password, StrictInvite, Some(&code))
                .await
                .is_err()
        );
        let listed = list_invites(&db).await.unwrap();
        assert_eq!(listed[0].used_by.as_deref(), Some("Alex"));

        let (expired, code) = create_invite(&db, admin.id, "", Some(1)).await.unwrap();
        sqlx::query("UPDATE invites SET expires_at = 1 WHERE id = ?")
            .bind(expired.id)
            .execute(&db)
            .await
            .unwrap();
        assert!(
            register(&db, "Steve", password, StrictInvite, Some(&code))
                .await
                .is_err()
        );
        delete_invite(&db, expired.id).await.unwrap();
        assert!(delete_invite(&db, expired.id).await.is_err());

        let steve = register(&db, "Steve", password, ApprovalRequired, None)
            .await
            .unwrap();
        assert_eq!(steve.status, Status::Pending);
        set_status(&db, steve.id, Status::Active).await.unwrap();
        assert!(verify_login(&db, "Steve", password).await.is_ok());
    }

    #[tokio::test]
    async fn reset_codes_work_once_and_sign_out() {
        let db = crate::db::open_in_memory().await.unwrap();
        let alex = create(&db, "Alex", "old password", Role::Player, Status::Active)
            .await
            .unwrap();
        let launcher = SessionKind::Launcher;
        let session = create_session(&db, alex.id, launcher).await.unwrap();
        let (first, _) = create_reset_code(&db, alex.id).await.unwrap();
        let (code, _) = create_reset_code(&db, alex.id).await.unwrap();

        // Only the latest code counts, and only for its own account.
        assert!(
            reset_password(&db, "Alex", &first, "new password")
                .await
                .is_err()
        );
        assert!(
            reset_password(&db, "Nobody", &code, "new password")
                .await
                .is_err()
        );
        reset_password(&db, "alex", &code, "new password")
            .await
            .unwrap();
        assert!(
            reset_password(&db, "Alex", &code, "third password")
                .await
                .is_err()
        );

        assert!(verify_login(&db, "Alex", "old password").await.is_err());
        assert!(verify_login(&db, "Alex", "new password").await.is_ok());
        assert!(authenticate(&db, &session, launcher).await.is_err());

        let kept = create_session(&db, alex.id, launcher).await.unwrap();
        let other = create_session(&db, alex.id, launcher).await.unwrap();
        assert!(
            change_password(&db, alex.id, "wrong", "fourth password", Some(&kept))
                .await
                .is_err()
        );
        change_password(&db, alex.id, "new password", "fourth password", Some(&kept))
            .await
            .unwrap();
        assert!(authenticate(&db, &kept, launcher).await.is_ok());
        assert!(authenticate(&db, &other, launcher).await.is_err());
    }

    #[tokio::test]
    async fn game_sessions_are_capped_and_the_last_admin_is_safe() {
        let db = crate::db::open_in_memory().await.unwrap();
        let admin = create(&db, "root", "correct horse", Role::Admin, Status::Active)
            .await
            .unwrap();
        let first = create_game_session(&db, admin.id, "client").await.unwrap();
        let (user, client) = game_session(&db, &first).await.unwrap().unwrap();
        assert_eq!((user.id, client.as_str()), (admin.id, "client"));
        // Not a launcher token.
        assert!(
            authenticate(&db, &first, SessionKind::Launcher)
                .await
                .is_err()
        );
        let mut last = String::new();
        for _ in 0..MAX_GAME_SESSIONS {
            last = create_game_session(&db, admin.id, "client").await.unwrap();
        }
        assert!(game_session(&db, &first).await.unwrap().is_none());
        assert!(game_session(&db, &last).await.unwrap().is_some());

        assert!(set_status(&db, admin.id, Status::Banned).await.is_err());
        assert!(set_role(&db, admin.id, Role::Player).await.is_err());
        assert!(delete(&db, admin.id).await.is_err());

        let alex = create(&db, "Alex", "long enough", Role::Player, Status::Active)
            .await
            .unwrap();
        let token = create_game_session(&db, alex.id, "c").await.unwrap();
        set_status(&db, alex.id, Status::Banned).await.unwrap();
        assert!(game_session(&db, &token).await.unwrap().is_none());
        delete(&db, alex.id).await.unwrap();
        assert!(find_by_name(&db, "Alex").await.unwrap().is_none());
    }

    #[test]
    fn codes_are_grouped_and_unambiguous() {
        let code = random_code();
        assert_eq!(code.len(), 19);
        assert!(code.split('-').all(|g| g.len() == 4));
        assert!(!code.contains(['0', 'O', '1', 'I']));
        assert_ne!(code, random_code());
        assert_eq!(random_token().len(), 64);
    }
}
