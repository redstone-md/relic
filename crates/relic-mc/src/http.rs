//! HTTP client construction and checksum-verified downloads.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::{Error, Result};

const ATTEMPTS: u32 = 4;

/// A client identifying itself generically; upstream APIs (Modrinth, Fabric
/// meta) ask automated clients for a descriptive user agent.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("relic/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .expect("static client configuration is valid")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checksum {
    Sha1(String),
    Sha256(String),
    /// The source publishes no digest. Only for trusted HTTPS origins.
    None,
}

enum Hasher {
    Sha1(Sha1),
    Sha256(Sha256),
    None,
}

impl Hasher {
    fn update(&mut self, bytes: &[u8]) {
        match self {
            Hasher::Sha1(h) => h.update(bytes),
            Hasher::Sha256(h) => h.update(bytes),
            Hasher::None => {}
        }
    }

    fn finish(self) -> String {
        match self {
            Hasher::Sha1(h) => hex::encode(h.finalize()),
            Hasher::Sha256(h) => hex::encode(h.finalize()),
            Hasher::None => String::new(),
        }
    }
}

fn http_err(url: &str) -> impl FnOnce(reqwest::Error) -> Error + '_ {
    move |source| Error::Http {
        url: url.to_owned(),
        source,
    }
}

/// Whether another attempt could plausibly succeed.
fn retryable(err: &Error) -> bool {
    match err {
        Error::Http { .. } | Error::Checksum { .. } => true,
        Error::Status { status, .. } => *status == 429 || *status >= 500,
        _ => false,
    }
}

async fn with_retries<T, F, Fut>(mut attempt: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut tries = 0;
    loop {
        tries += 1;
        match attempt().await {
            Ok(value) => return Ok(value),
            Err(err) if tries < ATTEMPTS && retryable(&err) => {
                tracing::debug!(%err, tries, "retrying request");
                tokio::time::sleep(Duration::from_millis(400 * 2u64.pow(tries))).await;
            }
            Err(err) => return Err(err),
        }
    }
}

async fn send(client: &reqwest::Client, url: &str) -> Result<reqwest::Response> {
    let response = client.get(url).send().await.map_err(http_err(url))?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status {
            url: url.to_owned(),
            status: status.as_u16(),
        });
    }
    Ok(response)
}

pub async fn get_json<T: DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    with_retries(|| async {
        let bytes = send(client, url)
            .await?
            .bytes()
            .await
            .map_err(http_err(url))?;
        serde_json::from_slice(&bytes).map_err(|e| Error::Decode {
            url: url.to_owned(),
            message: e.to_string(),
        })
    })
    .await
}

pub async fn get_text(client: &reqwest::Client, url: &str) -> Result<String> {
    with_retries(|| async { send(client, url).await?.text().await.map_err(http_err(url)) }).await
}

/// Hex digest of a file on disk, in the algorithm of `like`.
pub async fn file_digest(path: &Path, like: &Checksum) -> Result<String> {
    let mut hasher = match like {
        Checksum::Sha1(_) => Hasher::Sha1(Sha1::new()),
        Checksum::Sha256(_) => Hasher::Sha256(Sha256::new()),
        Checksum::None => return Ok(String::new()),
    };
    let mut file = fs::File::open(path).await?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = tokio::io::AsyncReadExt::read(&mut file, &mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finish())
}

/// Downloads `url` to `dest`, verifying the digest before the file appears
/// under its final name. If `dest` already holds the right content, nothing
/// is fetched. Returns whether a download happened.
pub async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    checksum: &Checksum,
) -> Result<bool> {
    let expected = match checksum {
        Checksum::Sha1(h) | Checksum::Sha256(h) => h.to_ascii_lowercase(),
        Checksum::None => String::new(),
    };
    if *checksum != Checksum::None
        && fs::try_exists(dest).await?
        && file_digest(dest, checksum).await? == expected
    {
        return Ok(false);
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = std::path::PathBuf::from(part);

    let result = with_retries(|| async {
        let response = send(client, url).await?;
        let mut hasher = match checksum {
            Checksum::Sha1(_) => Hasher::Sha1(Sha1::new()),
            Checksum::Sha256(_) => Hasher::Sha256(Sha256::new()),
            Checksum::None => Hasher::None,
        };
        let mut file = fs::File::create(&part).await?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(http_err(url))?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        drop(file);

        let actual = hasher.finish();
        if actual != expected {
            return Err(Error::Checksum {
                url: url.to_owned(),
                expected: expected.clone(),
                actual,
            });
        }
        Ok(())
    })
    .await;

    if let Err(err) = result {
        let _ = fs::remove_file(&part).await;
        return Err(err);
    }
    // `rename` does not replace an existing file on Windows.
    let _ = fs::remove_file(dest).await;
    fs::rename(&part, dest).await?;
    Ok(true)
}
