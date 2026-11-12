//! Runs the Minecraft server processes: installation on first start, console
//! capture, graceful stop, crash detection and restart with backoff.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use relic_mc::reqwest;
use relic_mc::server::{self, InstallRequest};
use relic_proto::InstanceStatus;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{Notify, broadcast, watch};

use crate::deploy;
use crate::error::{Error, Result, now};
use crate::instances;
use crate::modpack::Modpack;

const CONSOLE_LINES: usize = 2000;
const MAX_LINE_CHARS: usize = 4000;
const STOP_GRACE: Duration = Duration::from_secs(60);
/// Consecutive crashes after which the supervisor stops trying.
const MAX_RESTARTS: u32 = 5;
/// A run at least this long counts as healthy and resets the crash counter.
const HEALTHY_RUN: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConsoleLine {
    /// Increases by one per line of an instance; lets clients resume a stream.
    pub seq: u64,
    /// Unix seconds.
    pub time: i64,
    pub text: String,
    /// Whether the line comes from Relic itself rather than the server.
    pub system: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeInfo {
    pub status: InstanceStatus,
    pub players: Vec<String>,
    pub players_max: u32,
    pub pid: Option<u32>,
    /// Unix seconds of the current process start.
    pub started_at: Option<i64>,
    /// Why the instance is not running, when that was not requested.
    pub last_error: Option<String>,
    /// Instance-relative path of the crash report of the last crash.
    pub crash_report: Option<String>,
}

impl RuntimeInfo {
    fn stopped() -> Self {
        Self {
            status: InstanceStatus::Stopped,
            players: Vec::new(),
            players_max: 0,
            pid: None,
            started_at: None,
            last_error: None,
            crash_report: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Status or player list of an instance changed.
    Runtime {
        instance_id: String,
        info: RuntimeInfo,
    },
}

struct Console {
    next_seq: u64,
    lines: VecDeque<ConsoleLine>,
}

struct Slot {
    id: String,
    info: watch::Sender<RuntimeInfo>,
    console: Mutex<Console>,
    console_tx: broadcast::Sender<ConsoleLine>,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    /// Whether the admin wants the instance up. Cleared by `stop`.
    desired: Mutex<bool>,
    /// Whether a runner task currently owns the slot.
    active: Mutex<bool>,
    /// Wakes the runner out of a backoff sleep.
    wake: Notify,
    /// Tells the runner to kill the process.
    kill: Notify,
}

impl Slot {
    fn new(id: &str) -> Arc<Self> {
        Arc::new(Self {
            id: id.to_owned(),
            info: watch::Sender::new(RuntimeInfo::stopped()),
            console: Mutex::new(Console {
                next_seq: 1,
                lines: VecDeque::new(),
            }),
            console_tx: broadcast::Sender::new(1024),
            stdin: tokio::sync::Mutex::new(None),
            desired: Mutex::new(false),
            active: Mutex::new(false),
            wake: Notify::new(),
            kill: Notify::new(),
        })
    }

    fn push_line(&self, text: String, system: bool) {
        let mut console = self.console.lock().expect("console lock");
        let line = ConsoleLine {
            seq: console.next_seq,
            time: now(),
            text,
            system,
        };
        console.next_seq += 1;
        if console.lines.len() == CONSOLE_LINES {
            console.lines.pop_front();
        }
        console.lines.push_back(line.clone());
        // Sent under the lock so subscribers see lines in `seq` order.
        let _ = self.console_tx.send(line);
    }

    fn note(&self, text: impl Into<String>) {
        self.push_line(format!("[relic] {}", text.into()), true);
    }

    fn desired(&self) -> bool {
        *self.desired.lock().expect("desired lock")
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub instances: PathBuf,
    pub runtimes: PathBuf,
}

struct Inner {
    db: SqlitePool,
    modpack: Modpack,
    http: reqwest::Client,
    paths: Paths,
    slots: Mutex<HashMap<String, Arc<Slot>>>,
    events: broadcast::Sender<Event>,
    /// Set once the daemon knows where its own Yggdrasil API listens.
    auth: Mutex<Option<AuthAgent>>,
}

/// Makes every server authenticate players against this daemon instead of
/// Mojang, through the authlib-injector Java agent.
#[derive(Debug, Clone)]
pub struct AuthAgent {
    /// Where the agent JAR is kept.
    pub dir: PathBuf,
    /// The Yggdrasil API as the server processes reach it.
    pub api_url: String,
}

#[derive(Clone)]
pub struct Supervisor {
    inner: Arc<Inner>,
}

impl Supervisor {
    pub fn new(db: SqlitePool, modpack: Modpack, http: reqwest::Client, paths: Paths) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                modpack,
                http,
                paths,
                slots: Mutex::new(HashMap::new()),
                events: broadcast::Sender::new(256),
                auth: Mutex::new(None),
            }),
        }
    }

    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.inner.paths.instances.join(id)
    }

    pub fn set_auth_agent(&self, agent: AuthAgent) {
        *self.inner.auth.lock().expect("auth lock") = Some(agent);
    }

    pub fn events(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    fn slot(&self, id: &str) -> Arc<Slot> {
        let mut slots = self.inner.slots.lock().expect("slots lock");
        slots
            .entry(id.to_owned())
            .or_insert_with(|| Slot::new(id))
            .clone()
    }

    pub fn info(&self, id: &str) -> RuntimeInfo {
        self.slot(id).info.borrow().clone()
    }

    /// Buffered console lines with `seq > after`, plus a receiver for
    /// everything that follows. Lines may arrive on the receiver that are
    /// already in the returned history; drop those by `seq`.
    pub fn console(
        &self,
        id: &str,
        after: u64,
    ) -> (Vec<ConsoleLine>, broadcast::Receiver<ConsoleLine>) {
        let slot = self.slot(id);
        let console = slot.console.lock().expect("console lock");
        let rx = slot.console_tx.subscribe();
        let history = console
            .lines
            .iter()
            .filter(|l| l.seq > after)
            .cloned()
            .collect();
        (history, rx)
    }

    /// Starts the instance in the background. Installation, if still needed,
    /// happens first and is reported through the status and the console.
    pub async fn start(&self, id: &str) -> Result<()> {
        instances::get(&self.inner.db, id).await?;
        let slot = self.slot(id);
        {
            let mut active = slot.active.lock().expect("active lock");
            if *active {
                return Err(Error::Conflict("instance is already running".into()));
            }
            *active = true;
            *slot.desired.lock().expect("desired lock") = true;
        }
        let inner = self.inner.clone();
        tokio::spawn(async move {
            inner.run(&slot).await;
            *slot.active.lock().expect("active lock") = false;
        });
        Ok(())
    }

    /// Asks the server to shut down and waits for it; kills it if it has not
    /// exited after the grace period.
    pub async fn stop(&self, id: &str) -> Result<()> {
        let slot = self.slot(id);
        *slot.desired.lock().expect("desired lock") = false;
        if !*slot.active.lock().expect("active lock") {
            return Ok(());
        }
        slot.wake.notify_one();
        self.inner
            .set_status_if(&slot, InstanceStatus::Stopping, |s| {
                matches!(s, InstanceStatus::Starting | InstanceStatus::Running)
            });
        let _ = write_stdin(&slot, "stop").await;

        let mut info = slot.info.subscribe();
        let halted =
            |i: &RuntimeInfo| matches!(i.status, InstanceStatus::Stopped | InstanceStatus::Crashed);
        if tokio::time::timeout(STOP_GRACE, info.wait_for(halted))
            .await
            .is_err()
        {
            slot.note("server did not stop in time, killing it");
            slot.kill.notify_one();
            let _ = tokio::time::timeout(Duration::from_secs(15), info.wait_for(halted)).await;
        }
        Ok(())
    }

    pub async fn restart(&self, id: &str) -> Result<()> {
        self.stop(id).await?;
        // The runner clears `active` right after publishing the final status.
        let slot = self.slot(id);
        for _ in 0..50 {
            if !*slot.active.lock().expect("active lock") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        self.start(id).await
    }

    /// Sends one console command to a running server.
    pub async fn command(&self, id: &str, line: &str) -> Result<()> {
        let line = line.trim();
        if line.is_empty() || line.contains(['\n', '\r']) {
            return Err(Error::invalid("command must be a single non-empty line"));
        }
        let slot = self.slot(id);
        slot.push_line(format!("> {line}"), true);
        write_stdin(&slot, line).await
    }

    /// Starts every instance marked for automatic start.
    pub async fn autostart(&self) -> Result<()> {
        for instance in instances::list(&self.inner.db).await? {
            if instance.auto_start
                && let Err(err) = self.start(&instance.id).await
            {
                tracing::warn!(instance = %instance.id, %err, "autostart failed");
            }
        }
        Ok(())
    }

    /// Stops all running instances; used on daemon shutdown.
    pub async fn shutdown(&self) {
        let ids: Vec<String> = {
            let slots = self.inner.slots.lock().expect("slots lock");
            slots.keys().cloned().collect()
        };
        let stops = ids.iter().map(|id| self.stop(id));
        futures_util::future::join_all(stops).await;
    }

    /// Forgets the runtime state of a deleted instance.
    pub fn forget(&self, id: &str) {
        self.inner.slots.lock().expect("slots lock").remove(id);
    }
}

async fn write_stdin(slot: &Slot, line: &str) -> Result<()> {
    let mut guard = slot.stdin.lock().await;
    let stdin = guard
        .as_mut()
        .ok_or_else(|| Error::Conflict("instance is not running".into()))?;
    stdin.write_all(format!("{line}\n").as_bytes()).await?;
    stdin.flush().await?;
    Ok(())
}

enum Exit {
    /// The process ended because it was asked to, or exited cleanly.
    Clean,
    Crashed(String),
}

impl Inner {
    fn publish(&self, slot: &Slot, change: impl FnOnce(&mut RuntimeInfo)) {
        let mut changed = false;
        slot.info.send_if_modified(|info| {
            let before = info.clone();
            change(info);
            changed = *info != before;
            changed
        });
        if changed {
            let _ = self.events.send(Event::Runtime {
                instance_id: slot.id.clone(),
                info: slot.info.borrow().clone(),
            });
        }
    }

    fn set_status(&self, slot: &Slot, status: InstanceStatus) {
        self.publish(slot, |info| info.status = status);
    }

    fn set_status_if(
        &self,
        slot: &Slot,
        status: InstanceStatus,
        when: impl Fn(InstanceStatus) -> bool,
    ) {
        self.publish(slot, |info| {
            if when(info.status) {
                info.status = status;
            }
        });
    }

    /// Owns the slot until the instance is down for good.
    async fn run(&self, slot: &Slot) {
        let mut crashes = 0u32;
        self.publish(slot, |info| {
            info.last_error = None;
            info.crash_report = None;
        });
        loop {
            let begun = Instant::now();
            let exit = match self.run_once(slot).await {
                Ok(exit) => exit,
                Err(err) => Exit::Crashed(err.to_string()),
            };
            *slot.stdin.lock().await = None;
            let message = match exit {
                Exit::Clean => break,
                Exit::Crashed(message) => message,
            };
            slot.note(&message);
            if begun.elapsed() >= HEALTHY_RUN {
                crashes = 0;
            }
            crashes += 1;
            self.publish(slot, |info| {
                info.status = InstanceStatus::Crashed;
                info.players.clear();
                info.pid = None;
                info.started_at = None;
                info.last_error = Some(message.clone());
            });

            let auto_restart = instances::get(&self.db, &slot.id)
                .await
                .is_ok_and(|i| i.auto_restart);
            if !auto_restart || !slot.desired() {
                return;
            }
            if crashes >= MAX_RESTARTS {
                slot.note(format!("giving up after {crashes} consecutive crashes"));
                return;
            }
            let delay =
                Duration::from_secs(5 * 3u64.pow(crashes - 1)).min(Duration::from_secs(300));
            slot.note(format!("restarting in {} s", delay.as_secs()));
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = slot.wake.notified() => {}
            }
            if !slot.desired() {
                break;
            }
        }
        *slot.desired.lock().expect("desired lock") = false;
        self.publish(slot, |info| {
            info.status = InstanceStatus::Stopped;
            info.players.clear();
            info.pid = None;
            info.started_at = None;
        });
    }

    /// Prepares the instance directory, runs the server once and waits for it.
    async fn run_once(&self, slot: &Slot) -> Result<Exit> {
        let instance = instances::get(&self.db, &slot.id).await?;
        let dir = self.paths.instances.join(&instance.id);

        self.set_status(slot, InstanceStatus::Installing);
        let needs_install = server::installed(&dir)
            .await
            .ok()
            .flatten()
            .is_none_or(|i| {
                i.minecraft_version != instance.minecraft_version || i.loader != instance.loader
            });
        if needs_install {
            slot.note(format!(
                "installing Minecraft {} ({} {})",
                instance.minecraft_version,
                instance.loader.kind.as_str(),
                instance.loader.version
            ));
        }
        let request = InstallRequest {
            dir: &dir,
            runtimes: &self.paths.runtimes,
            minecraft_version: &instance.minecraft_version,
            loader: &instance.loader,
        };
        let installed = server::install(&self.http, &request)
            .await
            .map_err(|e| Error::Other(anyhow::anyhow!("installation failed: {e}")))?;
        let java =
            relic_mc::java::ensure_runtime(&self.http, &self.paths.runtimes, installed.java_major)
                .await
                .map_err(|e| Error::Other(anyhow::anyhow!("Java runtime unavailable: {e}")))?;

        if !server::eula_accepted(&dir).await {
            return Err(Error::Conflict(
                "the Minecraft EULA has not been accepted for this instance".into(),
            ));
        }
        let report = deploy::deploy(&self.modpack, &instance.id, &dir).await?;
        if report.written + report.removed > 0 {
            slot.note(format!(
                "pack files: {} written, {} removed",
                report.written, report.removed
            ));
        }
        if report.unclassified > 0 {
            slot.note(format!(
                "{} file(s) have no side assigned and were not deployed",
                report.unclassified
            ));
        }
        let auth = self.auth.lock().expect("auth lock").clone();
        let agent = match &auth {
            Some(auth) => {
                let jar = relic_mc::authlib::ensure_agent(&self.http, &auth.dir)
                    .await
                    .map_err(|e| {
                        Error::Other(anyhow::anyhow!("authentication agent unavailable: {e}"))
                    })?;
                Some(relic_mc::authlib::jvm_argument(&jar, &auth.api_url))
            }
            None => None,
        };
        let properties = dir.join("server.properties");
        let mut pairs = vec![("server-port", instance.port.to_string())];
        if agent.is_some() {
            // Accounts are checked against this daemon; never let a server
            // fall back to trusting whatever name a client claims.
            pairs.push(("online-mode", "true".to_owned()));
        }
        deploy::set_properties(&properties, &pairs).await?;
        let players_max = deploy::get_property(&properties, "max-players")
            .await
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);

        if !slot.desired() {
            return Ok(Exit::Clean);
        }

        // Make the JVM write its console as UTF-8 regardless of the locale.
        let mut extra = vec![
            "-Dfile.encoding=UTF-8".to_owned(),
            "-Dstdout.encoding=UTF-8".to_owned(),
            "-Dstderr.encoding=UTF-8".to_owned(),
        ];
        extra.extend(agent);
        let command = server::launch_command(&installed, &java, instance.memory_mb, &extra);

        let started = SystemTime::now();
        let mut child = Command::new(&command.program)
            .args(&command.args)
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        *slot.stdin.lock().await = child.stdin.take();
        self.publish(slot, |info| {
            info.status = InstanceStatus::Starting;
            info.players.clear();
            info.players_max = players_max;
            info.pid = child.id();
            info.started_at = Some(now());
            info.last_error = None;
        });
        slot.note("server process started");

        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let read_out = self.pump(slot, stdout);
        let read_err = self.pump(slot, stderr);
        let wait = async {
            tokio::select! {
                status = child.wait() => status,
                () = slot.kill.notified() => {
                    let _ = child.start_kill();
                    child.wait().await
                }
            }
        };
        let (status, (), ()) = tokio::join!(wait, read_out, read_err);
        let status = status?;

        // A zero exit code also covers `/stop` typed in the game or console.
        if status.success() || !slot.desired() {
            slot.note(format!("server stopped ({status})"));
            return Ok(Exit::Clean);
        }
        let crash_report = newest_crash_report(&dir, started);
        self.publish(slot, |info| info.crash_report = crash_report.clone());
        Ok(Exit::Crashed(match crash_report {
            Some(path) => format!("server crashed ({status}), see {path}"),
            None => format!("server crashed ({status})"),
        }))
    }

    /// Copies one output stream of the server into the console, updating
    /// status and player list from what it prints.
    async fn pump(&self, slot: &Slot, stream: impl AsyncRead + Unpin) {
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let text = String::from_utf8_lossy(&buf);
            let text = text.trim_end_matches(['\n', '\r']);
            let text: String = text.chars().take(MAX_LINE_CHARS).collect();
            match parse_line(&text) {
                Some(LogEvent::Ready) => self.set_status_if(slot, InstanceStatus::Running, |s| {
                    s == InstanceStatus::Starting
                }),
                Some(LogEvent::Joined(name)) => self.publish(slot, |info| {
                    let mut set: BTreeSet<String> = info.players.drain(..).collect();
                    set.insert(name.to_owned());
                    info.players = set.into_iter().collect();
                }),
                Some(LogEvent::Left(name)) => {
                    self.publish(slot, |info| info.players.retain(|p| p != name));
                }
                None => {}
            }
            slot.push_line(text, false);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LogEvent<'a> {
    Ready,
    Joined(&'a str),
    Left(&'a str),
}

fn is_username(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Recognises the few server log lines the supervisor cares about. Only
/// lines logged by the server thread at INFO level count, and the message
/// must match exactly, so chat cannot forge them: chat lines start with the
/// sender in angle brackets.
fn parse_line(line: &str) -> Option<LogEvent<'_>> {
    let (prefix, message) = line.split_once("]: ")?;
    if !prefix.contains("Server thread/INFO") {
        return None;
    }
    if message.starts_with("Done (") && message.contains("For help") {
        return Some(LogEvent::Ready);
    }
    if let Some(name) = message.strip_suffix(" joined the game") {
        return is_username(name).then_some(LogEvent::Joined(name));
    }
    if let Some(name) = message.strip_suffix(" left the game") {
        return is_username(name).then_some(LogEvent::Left(name));
    }
    None
}

/// The crash report written since `since`, as an instance-relative path.
fn newest_crash_report(dir: &Path, since: SystemTime) -> Option<String> {
    std::fs::read_dir(dir.join("crash-reports"))
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            let name = entry.file_name().into_string().ok()?;
            (modified >= since && name.ends_with(".txt")).then_some((modified, name))
        })
        .max()
        .map(|(_, name)| format!("crash-reports/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_ready_join_and_leave_across_log_formats() {
        assert_eq!(
            parse_line(r#"[12:00:01] [Server thread/INFO]: Done (3.412s)! For help, type "help""#),
            Some(LogEvent::Ready)
        );
        assert_eq!(
            parse_line(
                r#"[12:00:01] [Server thread/INFO] [minecraft/DedicatedServer]: Done (9.1s)! For help, type "help""#
            ),
            Some(LogEvent::Ready)
        );
        assert_eq!(
            parse_line("[12:00:05] [Server thread/INFO]: Steve_1 joined the game"),
            Some(LogEvent::Joined("Steve_1"))
        );
        assert_eq!(
            parse_line(
                "[12:00:09] [Server thread/INFO] [minecraft/MinecraftServer]: Steve_1 left the game"
            ),
            Some(LogEvent::Left("Steve_1"))
        );
    }

    #[test]
    fn chat_cannot_forge_events() {
        for line in [
            "[12:00:05] [Server thread/INFO]: <Steve> Alex joined the game",
            "[12:00:05] [Server thread/INFO]: [Not Secure] <Steve> Alex left the game",
            r#"[12:00:05] [Server thread/INFO]: <Steve> Done (1s)! For help, type "help""#,
            "[12:00:05] [Async Chat Thread - #0/INFO]: Alex joined the game",
            "Alex joined the game",
        ] {
            assert_eq!(parse_line(line), None, "{line}");
        }
    }

    #[test]
    fn console_keeps_a_bounded_history_with_increasing_seq() {
        let slot = Slot::new("main");
        for i in 0..(CONSOLE_LINES + 10) {
            slot.push_line(format!("line {i}"), false);
        }
        let console = slot.console.lock().unwrap();
        assert_eq!(console.lines.len(), CONSOLE_LINES);
        assert_eq!(console.lines.front().unwrap().seq, 11);
        assert_eq!(
            console.lines.back().unwrap().seq,
            (CONSOLE_LINES + 10) as u64
        );
    }
}
