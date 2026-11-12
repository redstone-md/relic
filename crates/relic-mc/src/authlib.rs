//! authlib-injector: the Java agent that points Minecraft's authentication
//! at a Yggdrasil server other than Mojang's. Servers and clients both run
//! with it.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::fs;

use crate::Result;
use crate::http::{Checksum, download, get_json};

const LATEST: &str = "https://authlib-injector.yushi.moe/artifact/latest.json";
const FILE_NAME: &str = "authlib-injector.jar";

#[derive(Deserialize)]
struct Release {
    download_url: String,
    checksums: Checksums,
}

#[derive(Deserialize)]
struct Checksums {
    sha256: String,
}

/// Path of the agent JAR inside `dir`, downloading the current release on
/// first use. A JAR that is already there is used as is, so starting a game
/// never depends on the download site being reachable.
pub async fn ensure_agent(client: &reqwest::Client, dir: &Path) -> Result<PathBuf> {
    let jar = dir.join(FILE_NAME);
    if fs::try_exists(&jar).await? {
        return Ok(jar);
    }
    let release: Release = get_json(client, LATEST).await?;
    download(
        client,
        &release.download_url,
        &jar,
        &Checksum::Sha256(release.checksums.sha256),
    )
    .await?;
    Ok(jar)
}

/// The JVM argument that loads the agent against the Yggdrasil API at `api_url`.
pub fn jvm_argument(jar: &Path, api_url: &str) -> String {
    format!("-javaagent:{}={api_url}", jar.display())
}
