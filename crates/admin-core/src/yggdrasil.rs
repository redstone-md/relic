//! The parts of a Yggdrasil server (as authlib-injector specifies it) that
//! are not HTTP: the signing key, signed profiles and the join handshake.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::rngs::SysRng;
use rsa::PrivateKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use serde_json::{Value, json};
use sha1::Sha1;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::users::User;

/// The size the specification recommends, and the one Mojang uses.
pub const KEY_BITS: usize = 4096;
/// How long a server has to confirm a join the client announced.
const JOIN_TTL: Duration = Duration::from_secs(30);

fn key_error(err: impl std::fmt::Display) -> Error {
    Error::Other(anyhow::anyhow!("signing key: {err}"))
}

/// A fresh RSA private key as PKCS#8 PEM. Slow: seconds at [`KEY_BITS`].
pub fn generate_key_pem(bits: usize) -> Result<String> {
    let key = PrivateKey::new(&mut SysRng, bits).map_err(key_error)?;
    Ok(key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(key_error)?
        .to_string())
}

/// The key that signs profile properties. Clients trust textures only when
/// the signature checks out against the public half from the API metadata.
pub struct Keys {
    private: PrivateKey,
    public_pem: String,
}

impl Keys {
    pub fn from_pem(pem: &str) -> Result<Self> {
        let private = PrivateKey::from_pkcs8_pem(pem).map_err(key_error)?;
        let public_pem = private
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .map_err(key_error)?;
        Ok(Self {
            private,
            public_pem,
        })
    }

    /// Reads the key at `path`, creating it on first run. The key must not
    /// change afterwards: clients cache the public half.
    pub async fn load_or_create(path: &Path) -> Result<Self> {
        match tokio::fs::read_to_string(path).await {
            Ok(pem) => return Self::from_pem(&pem),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        tracing::info!("generating the profile signing key, this takes a moment");
        let pem = tokio::task::spawn_blocking(|| generate_key_pem(KEY_BITS))
            .await
            .map_err(key_error)??;
        write_private(path, &pem).await?;
        Self::from_pem(&pem)
    }

    /// `-----BEGIN PUBLIC KEY-----` PEM, as the API metadata publishes it.
    pub fn public_pem(&self) -> &str {
        &self.public_pem
    }

    /// SHA1withRSA over `text`, Base64-encoded.
    pub fn sign(&self, text: &str) -> Result<String> {
        let signature = self
            .private
            .pkcs1v15_sign::<Sha1>(text.as_bytes())
            .map_err(key_error)?;
        Ok(BASE64.encode(signature))
    }
}

/// Writes the key readable by the daemon's user only.
async fn write_private(path: &Path, pem: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).await?;
    file.write_all(pem.as_bytes()).await?;
    file.sync_all().await?;
    Ok(())
}

/// A UUID the way Yggdrasil writes it: 32 hex digits, no hyphens.
pub fn unsigned_id(id: Uuid) -> String {
    id.simple().to_string()
}

/// `{id, name}`: a profile without its properties.
pub fn brief_profile(user: &User) -> Value {
    json!({ "id": unsigned_id(user.id), "name": user.username })
}

/// A profile with its `textures` property. `textures_url` is the public
/// prefix texture hashes are appended to. With `keys`, every property is
/// signed, which game clients require of profiles a server hands them.
pub fn full_profile(user: &User, textures_url: &str, keys: Option<&Keys>) -> Result<Value> {
    let mut textures = serde_json::Map::new();
    if let Some(hash) = &user.skin_sha256 {
        let mut skin = json!({ "url": format!("{textures_url}/{hash}") });
        if user.skin_model == "slim" {
            skin["metadata"] = json!({ "model": "slim" });
        }
        textures.insert("SKIN".into(), skin);
    }
    if let Some(hash) = &user.cape_sha256 {
        textures.insert(
            "CAPE".into(),
            json!({ "url": format!("{textures_url}/{hash}") }),
        );
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let payload = json!({
        "timestamp": timestamp,
        "profileId": unsigned_id(user.id),
        "profileName": user.username,
        "textures": textures,
    });

    let mut properties = Vec::new();
    for (name, value) in [
        ("textures", BASE64.encode(payload.to_string())),
        ("uploadableTextures", "skin,cape".to_owned()),
    ] {
        let mut property = json!({ "name": name, "value": value });
        if let Some(keys) = keys {
            property["signature"] = Value::String(keys.sign(&value)?);
        }
        properties.push(property);
    }
    Ok(json!({
        "id": unsigned_id(user.id),
        "name": user.username,
        "properties": properties,
    }))
}

struct Join {
    user_id: Uuid,
    ip: String,
    /// The address the client reached the API by, for texture URLs.
    origin: String,
    at: Instant,
}

/// Joins that clients announced and servers have yet to confirm. Lives in
/// memory: an entry is only good for half a minute.
#[derive(Default)]
pub struct Joins {
    pending: Mutex<HashMap<String, Join>>,
}

impl Joins {
    pub fn announce(&self, server_id: &str, user_id: Uuid, ip: String, origin: String) {
        let mut pending = self.pending.lock().expect("joins lock");
        pending.retain(|_, join| join.at.elapsed() < JOIN_TTL);
        pending.insert(
            server_id.to_owned(),
            Join {
                user_id,
                ip,
                origin,
                at: Instant::now(),
            },
        );
    }

    /// The account that announced `server_id`, if it did so recently and,
    /// when the server passes the player's address along, from that address.
    pub fn confirm(&self, server_id: &str, ip: Option<&str>) -> Option<(Uuid, String)> {
        let pending = self.pending.lock().expect("joins lock");
        let join = pending.get(server_id)?;
        let fresh = join.at.elapsed() < JOIN_TTL;
        let same_address = ip.is_none_or(|ip| ip == join.ip);
        (fresh && same_address).then(|| (join.user_id, join.origin.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{Role, Status};

    fn user() -> User {
        User {
            id: Uuid::new_v4(),
            username: "Alex".into(),
            role: Role::Player,
            status: Status::Active,
            skin_sha256: Some("ab".repeat(32)),
            skin_model: "slim".into(),
            cape_sha256: None,
            created_at: 0,
        }
    }

    #[test]
    fn profiles_are_signed_with_the_published_key() {
        use rsa::pkcs8::DecodePublicKey;

        let keys = Keys::from_pem(&generate_key_pem(2048).unwrap()).unwrap();
        assert!(keys.public_pem().starts_with("-----BEGIN PUBLIC KEY-----"));
        let user = user();
        let profile = full_profile(
            &user,
            "https://mc.example/api/yggdrasil/textures",
            Some(&keys),
        )
        .unwrap();
        assert_eq!(profile["id"].as_str().unwrap().len(), 32);

        let public = rsa::PublicKey::from_public_key_pem(keys.public_pem()).unwrap();
        for property in profile["properties"].as_array().unwrap() {
            let value = property["value"].as_str().unwrap();
            let signature = BASE64
                .decode(property["signature"].as_str().unwrap())
                .unwrap();
            public
                .pkcs1v15_verify::<Sha1>(value.as_bytes(), &signature)
                .unwrap();
        }

        let textures = &profile["properties"][0];
        assert_eq!(textures["name"], "textures");
        let payload: Value =
            serde_json::from_slice(&BASE64.decode(textures["value"].as_str().unwrap()).unwrap())
                .unwrap();
        assert_eq!(payload["profileName"], "Alex");
        let skin = &payload["textures"]["SKIN"];
        assert_eq!(skin["metadata"]["model"], "slim");
        assert!(skin["url"].as_str().unwrap().ends_with(&"ab".repeat(32)));
        assert!(payload["textures"].get("CAPE").is_none());

        let unsigned = full_profile(&user, "http://x", None).unwrap();
        assert!(unsigned["properties"][0].get("signature").is_none());
    }

    #[test]
    fn joins_are_confirmed_once_announced() {
        let joins = Joins::default();
        let id = Uuid::new_v4();
        assert_eq!(joins.confirm("abc", None), None);
        let origin = "https://mc.example".to_owned();
        joins.announce("abc", id, "10.0.0.7".into(), origin.clone());
        let joined = Some((id, origin));
        assert_eq!(joins.confirm("abc", None), joined);
        assert_eq!(joins.confirm("abc", Some("10.0.0.7")), joined);
        assert_eq!(joins.confirm("abc", Some("10.0.0.8")), None);
        assert_eq!(joins.confirm("other", None), None);
    }
}
