use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use admin_core::api::{self, App};
use admin_core::supervisor::AuthAgent;
use admin_core::users;

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
}

async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let data_dir = std::env::var_os("RELIC_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"));
    let bind: SocketAddr = std::env::var("RELIC_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse()
        .map_err(|e| anyhow::anyhow!("RELIC_BIND is not a socket address: {e}"))?;

    let app = App::open(&data_dir, env_flag("RELIC_TRUST_PROXY")).await?;
    let rescanned = app.modpack.rescan().await?;
    if rescanned > 0 {
        tracing::info!(
            files = rescanned,
            "read mod names, pictures and dependencies"
        );
    }
    let swept = app.modpack.collect_garbage().await?;
    tracing::info!(data_dir = %data_dir.display(), orphan_blobs_removed = swept, "storage ready");

    if let Some(token) = app.issue_setup_token().await? {
        // Printed, not logged: it must reach the operator even with logging
        // turned down, and should not land in log aggregation.
        println!(
            "\n  No administrator yet. Open the panel and finish setup with this token:\n\n      {token}\n"
        );
    }

    app.backups.spawn_scheduler();
    let db = app.db.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            tick.tick().await;
            if let Err(e) = users::purge_expired_sessions(&db).await {
                tracing::warn!(error = %e, "session cleanup failed");
            }
        }
    });
    // Game servers verify players against this daemon, over loopback unless
    // it listens on one address only.
    app.keys().await?;
    let local = match bind.ip() {
        ip if ip.is_unspecified() => "127.0.0.1".to_owned(),
        std::net::IpAddr::V6(ip) => format!("[{ip}]"),
        ip => ip.to_string(),
    };
    app.supervisor.set_auth_agent(AuthAgent {
        dir: std::path::absolute(&data_dir)?.join("authlib-injector"),
        api_url: format!("http://{local}:{}/api/yggdrasil", bind.port()),
    });
    if let Err(e) = app.supervisor.autostart().await {
        tracing::warn!(error = %e, "autostart failed");
    }

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");
    let service = api::router(app.clone()).into_make_service_with_connect_info::<SocketAddr>();
    // Not a graceful drain: open console sockets would hold it forever.
    tokio::select! {
        result = axum::serve(listener, service) => result?,
        () = shutdown_signal() => tracing::info!("shutting down, stopping instances"),
    }
    app.supervisor.shutdown().await;
    Ok(())
}
