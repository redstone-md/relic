//! From the PLAY button to a running game: fetch the manifest, install
//! what is missing, sync the pack, sign in and start Java.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use crate::lang::{Msg, t, tf};

use relic_mc::client::{self, LaunchOptions, PrepareRequest, Stage};
use relic_proto::{InstanceInfo, InstanceStatus};
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::api::{self, Client};
use super::sync::{self, PRIVATE_DIR};

/// What the UI hears about a launch, in order. `Failed` and `Exited` are
/// final.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Step {
        label: String,
        /// 0 to 1, when it can be told.
        progress: Option<f32>,
    },
    /// The game process is up.
    Started,
    /// The game ended; `crashed` holds what to tell the player if it did
    /// not end well.
    Exited { crashed: Option<String> },
    Failed {
        reason: String,
        /// The session is gone; the player has to sign in again.
        unauthorized: bool,
    },
}

pub struct Launch {
    pub client: Client,
    pub instance: InstanceInfo,
    pub yggdrasil_url: String,
    pub memory_mb: u32,
    /// Optional files the player switched on.
    pub optional: BTreeSet<String>,
    /// The launcher's data directory.
    pub data_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Api(#[from] api::Error),
    #[error(transparent)]
    Install(#[from] relic_mc::Error),
    #[error(transparent)]
    Sync(#[from] sync::Error),
    #[error("{}", tf(Msg::StartFailed, &[.0]))]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

/// Where one instance of one server keeps its game files. Servers are
/// told apart by a digest of their origin, which is not a usable directory
/// name itself.
pub fn game_dir(data_dir: &Path, origin: &str, instance: &str) -> Option<PathBuf> {
    let unusable = instance.is_empty()
        || instance == "."
        || instance == ".."
        || instance.contains(['/', '\\', ':', '\0']);
    if unusable {
        return None;
    }
    let server = hex::encode(&Sha256::digest(origin.as_bytes())[..8]);
    Some(data_dir.join("instances").join(server).join(instance))
}

/// `host:port` as the panel reports it.
fn join_target(address: &str) -> Option<(&str, u16)> {
    let (host, port) = address.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty()).then_some(())?;
    Some((host, port.parse().ok()?))
}

fn megabytes(bytes: u64) -> String {
    let megabytes = format!("{:.1}", bytes as f64 / (1024. * 1024.));
    tf(Msg::Megabytes, &[&megabytes])
}

/// What the installer says it is doing, in the player's language. The
/// installer names its stages in English; one it adds later is shown as it
/// comes.
fn stage_label(label: &str) -> String {
    match label {
        "Installing mod loader" => t(Msg::InstallingLoader).to_owned(),
        "Checking game files" => t(Msg::CheckingGameFiles).to_owned(),
        "Downloading Java" => t(Msg::DownloadingJava).to_owned(),
        other => format!("{other}…"),
    }
}

/// Starts a launch on the I/O runtime. The receiver ends after the final
/// event.
pub fn start(launch: Launch) -> UnboundedReceiver<Event> {
    let (events, receiver) = unbounded_channel();
    super::spawn(async move {
        if let Err(err) = run(launch, &events).await {
            let unauthorized = matches!(&err, Error::Api(api) if api.is_unauthorized());
            let _ = events.send(Event::Failed {
                reason: err.to_string(),
                unauthorized,
            });
        }
    });
    receiver
}

fn step(events: &UnboundedSender<Event>, label: &str) {
    let _ = events.send(Event::Step {
        label: label.to_owned(),
        progress: None,
    });
}

async fn run(launch: Launch, events: &UnboundedSender<Event>) -> Result<(), Error> {
    let Launch {
        client,
        instance,
        yggdrasil_url,
        memory_mb,
        optional,
        data_dir,
    } = launch;
    let game_dir = game_dir(&data_dir, client.origin(), &instance.id)
        .ok_or_else(|| Error::Other(t(Msg::InstanceNameUnusable).into()))?;
    let root = data_dir.join("game");
    let private = game_dir.join(PRIVATE_DIR);
    tokio::fs::create_dir_all(&private).await?;

    step(events, t(Msg::CheckingUpdates));
    let manifest = client.manifest(&instance.id).await?;

    // Thousands of small files go by; the UI only needs whole percents.
    let last = Mutex::new((String::new(), u64::MAX));
    let report = |label: &str, done: u64, total: u64| {
        let percent = (done * 100).checked_div(total).unwrap_or(u64::MAX);
        let mut last = last.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if last.0 != label || last.1 != percent {
            *last = (label.to_owned(), percent);
            let _ = events.send(Event::Step {
                label: label.to_owned(),
                progress: (total > 0).then(|| done as f32 / total as f32),
            });
        }
    };

    let http = relic_mc::http::client();
    let prepared = client::prepare(
        &http,
        &PrepareRequest {
            root: &root,
            runtimes: &data_dir.join("runtimes"),
            minecraft_version: &manifest.minecraft_version,
            loader: &manifest.loader,
        },
        &|stage: Stage| report(&stage_label(stage.label), stage.done, stage.total),
    )
    .await?;

    step(events, t(Msg::CheckingMods));
    let (plan, mut state) = {
        let (manifest, game_dir) = (manifest.clone(), game_dir.clone());
        tokio::task::spawn_blocking(move || {
            let mut state = sync::State::load(&game_dir);
            sync::plan(&manifest, &optional, &game_dir, &mut state).map(|plan| (plan, state))
        })
        .await
        .map_err(|e| Error::Other(e.to_string()))??
    };
    let total = megabytes(plan.bytes());
    sync::apply(
        &client,
        &instance.id,
        &game_dir,
        &plan,
        &mut state,
        &|done, all| report(&tf(Msg::DownloadingMods, &[&total]), done, all),
    )
    .await?;

    step(events, t(Msg::SigningInToGame));
    let agent = relic_mc::authlib::ensure_agent(&http, &root.join("authlib")).await?;
    let session = client.game_session().await?;

    let jvm_args = [relic_mc::authlib::jvm_argument(&agent, &yggdrasil_url)];
    // The instance as the launcher last saw it may be minutes old; only a
    // server that is up right now is worth joining.
    let instance = match client.handshake().await {
        Ok(fresh) => fresh
            .instances
            .into_iter()
            .find(|i| i.id == instance.id)
            .unwrap_or(instance),
        Err(_) => instance,
    };
    let join = (instance.status == InstanceStatus::Running)
        .then(|| join_target(&instance.address))
        .flatten();
    let args = client::launch_arguments(
        &prepared,
        &LaunchOptions {
            game_dir: &game_dir,
            memory_mb,
            player_name: &session.profile.name,
            uuid: &session.profile.id.replace('-', ""),
            access_token: &session.access_token,
            jvm_args: &jvm_args,
            join,
        },
    )?;

    step(events, t(Msg::StartingGame));
    let log_path = private.join("game.log");
    let log = std::fs::File::create(&log_path)?;
    let mut command = Command::new(&prepared.java);
    // Java 9 and newer read arguments from a file, which keeps the access
    // token out of the process list and the command line short enough for
    // Windows.
    let args_file = private.join("launch.args");
    if prepared.java_major >= 9 {
        tokio::fs::write(&args_file, client::argument_file(&args)).await?;
        command.arg(format!("@{}", args_file.display()));
    } else {
        command.args(&args);
    }
    command
        .current_dir(&game_dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn()?;
    let _ = events.send(Event::Started);

    // The token in the arguments file is only needed while Java starts.
    let cleanup = super::spawn({
        let args_file = args_file.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = tokio::fs::remove_file(args_file).await;
        }
    });
    let status = child.wait().await?;
    cleanup.abort();
    let _ = tokio::fs::remove_file(&args_file).await;

    let crashed = (!status.success()).then(|| {
        let log = log_path.display();
        match status.code() {
            Some(code) => tf(Msg::StoppedWithCode, &[&code, &log]),
            None => tf(Msg::StoppedUnexpectedly, &[&log]),
        }
    });
    let _ = events.send(Event::Exited { crashed });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_and_instances_get_their_own_directories() {
        let data = Path::new("data");
        let a = game_dir(data, "https://a.example", "main").unwrap();
        let b = game_dir(data, "https://b.example", "main").unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with(data.join("instances")));
        assert!(a.ends_with("main"));
        assert_eq!(game_dir(data, "https://a.example", "../x"), None);
        assert_eq!(game_dir(data, "https://a.example", ""), None);
    }

    #[test]
    fn game_addresses_split_into_host_and_port() {
        assert_eq!(join_target("mc.example:25565"), Some(("mc.example", 25565)));
        assert_eq!(join_target("[::1]:25570"), Some(("::1", 25570)));
        assert_eq!(join_target("mc.example"), None);
        assert_eq!(join_target(":25565"), None);
    }
}
