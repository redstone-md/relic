//! Self-update from GitHub releases. A release binary is only ever swapped
//! in after its minisign signature checks out against the key this build
//! was compiled with; builds made without one never update themselves.

use std::path::Path;

use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::Deserialize;

/// `owner/name` of the repository whose releases this build follows.
const REPO: Option<&str> = option_env!("RELIC_UPDATE_REPO");
/// The base64 line of the minisign public key releases are signed with.
const PUBLIC_KEY: Option<&str> = option_env!("RELIC_UPDATE_PUBLIC_KEY");

/// A launcher is tens of megabytes; anything far beyond is not one.
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not reach the update server: {0}")]
    Network(#[from] relic_mc::reqwest::Error),
    #[error("the update server answered with {0}")]
    Status(relic_mc::reqwest::StatusCode),
    #[error("the update is not signed by this launcher's publisher")]
    Signature,
    #[error("the update is larger than a launcher can be")]
    TooLarge,
    #[error("could not install the update: {0}")]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// A newer release that has a build for this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    tag: String,
    binary: String,
    signature: String,
}

/// Where this build gets updates from, if it was built to get any.
fn source() -> Option<(&'static str, &'static str)> {
    let repo = REPO.map(str::trim).filter(|r| !r.is_empty())?;
    let key = PUBLIC_KEY.map(str::trim).filter(|k| !k.is_empty())?;
    Some((repo, key))
}

/// The name release builds for this machine are published under.
fn asset_name() -> String {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let suffix = std::env::consts::EXE_SUFFIX;
    format!("relic-launcher-{os}-{arch}{suffix}")
}

/// What a release has to offer a launcher of version `current`.
fn pick(release: Release, repo: &str, asset: &str, current: &Version) -> Option<Update> {
    let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
    if version <= *current {
        return None;
    }
    // Downloads only ever come from the repository's own release, whatever
    // the API answer says.
    let prefix = format!(
        "https://github.com/{repo}/releases/download/{}/",
        release.tag_name
    );
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name && a.browser_download_url.starts_with(&prefix))
            .map(|a| a.browser_download_url.clone())
    };
    Some(Update {
        binary: url(asset)?,
        signature: url(&format!("{asset}.minisig"))?,
        tag: release.tag_name,
        version,
    })
}

/// Checks `binary` against its minisign signature. The signed comment has
/// to name this very file and release, so that a genuine signature of an
/// older or different build cannot be passed off as this one.
fn verify(binary: &[u8], signature: &str, key: &str, expected_comment: &str) -> Result<()> {
    let key = PublicKey::from_base64(key).map_err(|_| Error::Signature)?;
    let signature = Signature::decode(signature).map_err(|_| Error::Signature)?;
    key.verify(binary, &signature, false)
        .map_err(|_| Error::Signature)?;
    if signature.trusted_comment() != expected_comment {
        return Err(Error::Signature);
    }
    Ok(())
}

async fn download(client: &relic_mc::reqwest::Client, url: &str, limit: u64) -> Result<Vec<u8>> {
    let mut response = client.get(url).send().await?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status()));
    }
    if response.content_length().is_some_and(|len| len > limit) {
        return Err(Error::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if (bytes.len() + chunk.len()) as u64 > limit {
            return Err(Error::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Looks for a release newer than this build. `Ok(None)` also covers builds
/// that do not update themselves.
pub async fn check() -> Result<Option<Update>> {
    let Some((repo, _)) = source() else {
        return Ok(None);
    };
    let Ok(current) = Version::parse(env!("CARGO_PKG_VERSION")) else {
        return Ok(None);
    };
    let response = relic_mc::http::client()
        .get(format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status()));
    }
    let release: Release = response.json().await?;
    Ok(pick(release, repo, &asset_name(), &current))
}

/// Downloads `update`, verifies it and puts it in place of the running
/// executable. The new version runs from the next start on.
pub async fn install(update: &Update, data_dir: &Path) -> Result<()> {
    let Some((_, key)) = source() else {
        return Err(Error::Signature);
    };
    let client = relic_mc::http::client();
    let signature = download(&client, &update.signature, MAX_SIGNATURE_BYTES).await?;
    let signature = String::from_utf8(signature).map_err(|_| Error::Signature)?;
    let binary = download(&client, &update.binary, MAX_BINARY_BYTES).await?;

    let asset = asset_name();
    let expected = format!("{asset} {}", update.tag);
    let dir = data_dir.join("update");
    tokio::task::spawn_blocking(move || {
        verify(&binary, &signature, key, &expected)?;
        std::fs::create_dir_all(&dir)?;
        let staged = dir.join(&asset);
        std::fs::write(&staged, &binary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
        }
        let replaced = self_replace::self_replace(&staged);
        let _ = std::fs::remove_file(&staged);
        Ok(replaced?)
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e)))?
}

/// Starts the (updated) launcher again; the caller then quits.
pub fn restart() -> std::io::Result<()> {
    std::process::Command::new(std::env::current_exe()?)
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The test vector published with the minisign-verify crate.
    const KEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\n\
        RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n\
        trusted comment: timestamp:1633700835\tfile:test\tprehashed\n\
        wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==";
    const COMMENT: &str = "timestamp:1633700835\tfile:test\tprehashed";

    #[test]
    fn a_genuine_signature_is_accepted() {
        verify(b"test", SIGNATURE, KEY, COMMENT).unwrap();
    }

    #[test]
    fn tampering_is_rejected() {
        let rejected = |r: Result<()>| matches!(r, Err(Error::Signature));
        assert!(rejected(verify(b"tesT", SIGNATURE, KEY, COMMENT)));
        // A real signature, but of some other release.
        assert!(rejected(verify(
            b"test",
            SIGNATURE,
            KEY,
            "relic-launcher-linux-x86_64 v9.9.9"
        )));
        let forged = SIGNATURE.replace("file:test", "file:best");
        assert!(rejected(verify(
            b"test",
            &forged,
            KEY,
            "timestamp:1633700835\tfile:best\tprehashed"
        )));
        assert!(rejected(verify(b"test", "nonsense", KEY, COMMENT)));
        assert!(rejected(verify(b"test", SIGNATURE, "nonsense", COMMENT)));
    }

    fn release(tag: &str, assets: &[(&str, &str)]) -> Release {
        Release {
            tag_name: tag.to_owned(),
            assets: assets
                .iter()
                .map(|(name, url)| Asset {
                    name: (*name).to_owned(),
                    browser_download_url: (*url).to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn only_newer_complete_releases_are_offered() {
        let current = Version::new(0, 2, 0);
        let base = "https://github.com/o/r/releases/download/v0.3.0";
        let bin = format!("{base}/app");
        let sig = format!("{base}/app.minisig");
        let full = [("app", bin.as_str()), ("app.minisig", sig.as_str())];

        let update = pick(release("v0.3.0", &full), "o/r", "app", &current).unwrap();
        assert_eq!(update.version, Version::new(0, 3, 0));
        assert_eq!(update.binary, bin);
        assert_eq!(update.signature, sig);

        // Same or older, unsigned, for another machine, or not a version.
        assert_eq!(pick(release("v0.2.0", &full), "o/r", "app", &current), None);
        assert_eq!(pick(release("v0.1.9", &full), "o/r", "app", &current), None);
        assert_eq!(
            pick(release("v0.3.0", &full[..1]), "o/r", "app", &current),
            None
        );
        assert_eq!(
            pick(release("v0.3.0", &full), "o/r", "other", &current),
            None
        );
        assert_eq!(pick(release("latest", &full), "o/r", "app", &current), None);
    }

    #[test]
    fn downloads_must_come_from_the_release_itself() {
        let current = Version::new(0, 2, 0);
        let elsewhere = [
            ("app", "https://evil.example/app"),
            ("app.minisig", "https://evil.example/app.minisig"),
        ];
        assert_eq!(
            pick(release("v0.3.0", &elsewhere), "o/r", "app", &current),
            None
        );
    }

    #[test]
    fn release_assets_are_named_after_the_machine() {
        let name = asset_name();
        assert!(name.starts_with("relic-launcher-"));
        assert_eq!(cfg!(windows), name.ends_with(".exe"));
    }
}
