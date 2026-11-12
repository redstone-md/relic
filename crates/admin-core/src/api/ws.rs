//! Live streams for the panel: instance consoles and state changes.

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

use super::AppState;
use crate::error::Result;
use crate::instances;
use crate::supervisor::{ConsoleLine, Event};

const PING_EVERY: Duration = Duration::from_secs(30);

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ConsoleOut<'a> {
    History { lines: &'a [ConsoleLine] },
    Line { line: &'a ConsoleLine },
    Error { message: String },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ConsoleIn {
    Command { line: String },
}

async fn send_json<T: Serialize>(socket: &mut WebSocket, value: &T) -> bool {
    match serde_json::to_string(value) {
        Ok(json) => socket.send(Message::text(json)).await.is_ok(),
        Err(_) => false,
    }
}

pub async fn console(
    State(app): State<AppState>,
    Path(id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    instances::get(&app.db, &id).await?;
    Ok(upgrade.on_upgrade(move |socket| console_socket(socket, app, id)))
}

async fn console_socket(mut socket: WebSocket, app: AppState, id: String) {
    let (history, mut lines) = app.supervisor.console(&id, 0);
    let mut last = history.last().map_or(0, |l| l.seq);
    if !send_json(&mut socket, &ConsoleOut::History { lines: &history }).await {
        return;
    }
    drop(history);

    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    loop {
        tokio::select! {
            line = lines.recv() => match line {
                Ok(line) => {
                    if line.seq > last {
                        last = line.seq;
                        if !send_json(&mut socket, &ConsoleOut::Line { line: &line }).await {
                            break;
                        }
                    }
                }
                // Fell behind the broadcast: catch up from the ring buffer.
                Err(RecvError::Lagged(_)) => {
                    let (missed, fresh) = app.supervisor.console(&id, last);
                    lines = fresh;
                    for line in &missed {
                        last = line.seq;
                        if !send_json(&mut socket, &ConsoleOut::Line { line }).await {
                            return;
                        }
                    }
                }
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let outcome = match serde_json::from_str::<ConsoleIn>(text.as_str()) {
                        Ok(ConsoleIn::Command { line }) => {
                            app.supervisor.command(&id, &line).await.map_err(|e| e.to_string())
                        }
                        Err(_) => Err("unrecognised message".to_owned()),
                    };
                    if let Err(message) = outcome
                        && !send_json(&mut socket, &ConsoleOut::Error { message }).await
                    {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EventOut {
    /// Events were dropped; the client should fetch state again.
    Resync,
}

pub async fn events(State(app): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| events_socket(socket, app))
}

async fn events_socket(mut socket: WebSocket, app: AppState) {
    let mut events = app.supervisor.events();
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    loop {
        tokio::select! {
            event = events.recv() => {
                let sent = match event {
                    Ok(event @ Event::Runtime { .. }) => send_json(&mut socket, &event).await,
                    Err(RecvError::Lagged(_)) => send_json(&mut socket, &EventOut::Resync).await,
                    Err(RecvError::Closed) => false,
                };
                if !sent {
                    break;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
        }
    }
}
