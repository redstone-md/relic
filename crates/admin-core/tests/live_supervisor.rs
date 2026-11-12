//! Boots a real Fabric server under the supervisor. Needs network access and
//! a few hundred megabytes of downloads, so it only runs on request:
//!
//! ```text
//! cargo test -p admin-core --test live_supervisor -- --ignored --nocapture
//! ```
//!
//! `RELIC_LIVE_DIR` keeps downloads between runs. Running this accepts the
//! Minecraft EULA for the throwaway test instance.

use std::path::PathBuf;
use std::time::Duration;

use admin_core::blobs::BlobStore;
use admin_core::instances::{self, NewInstance};
use admin_core::modpack::Modpack;
use admin_core::supervisor::{Paths, RuntimeInfo, Supervisor};
use relic_mc::{http, loaders, server};
use relic_proto::{InstanceStatus, Loader, LoaderKind};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "network"]
async fn fabric_server_boots_accepts_commands_and_stops() {
    let guard = tempfile::tempdir().unwrap();
    let root = std::env::var_os("RELIC_LIVE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| guard.path().to_owned());

    let client = http::client();
    let mc = "1.21.1";
    let versions = loaders::versions(&client, LoaderKind::Fabric, mc)
        .await
        .unwrap();
    let loader = Loader {
        kind: LoaderKind::Fabric,
        version: loaders::pick_default(&versions).unwrap().version.clone(),
    };

    let db = admin_core::db::open_in_memory().await.unwrap();
    let blobs = BlobStore::open(guard.path().join("blobs")).await.unwrap();
    let id = "live-fabric";
    instances::create(
        &db,
        NewInstance {
            id: id.into(),
            name: "Live test".into(),
            description: String::new(),
            minecraft_version: mc.into(),
            loader,
            java_major: 21,
            port: 25599,
            memory_mb: 2048,
        },
    )
    .await
    .unwrap();

    let supervisor = Supervisor::new(
        db.clone(),
        Modpack::new(db, blobs),
        client,
        Paths {
            instances: root.clone(),
            runtimes: root.join("runtimes"),
        },
    );
    let dir = supervisor.instance_dir(id);
    tokio::fs::create_dir_all(&dir).await.unwrap();
    server::accept_eula(&dir).await.unwrap();

    let mut events = supervisor.events();
    supervisor.start(id).await.unwrap();
    assert!(
        supervisor.start(id).await.is_err(),
        "double start must be refused"
    );

    let wait_for = |want: InstanceStatus| {
        let supervisor = supervisor.clone();
        async move {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
            loop {
                let info: RuntimeInfo = supervisor.info(id);
                if info.status == want {
                    return info;
                }
                assert!(
                    info.status != InstanceStatus::Crashed,
                    "instance crashed: {:?}\n{:#?}",
                    info.last_error,
                    supervisor.console(id, 0).0
                );
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "timed out waiting for {want:?}"
                );
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    };

    let info = wait_for(InstanceStatus::Running).await;
    assert!(info.pid.is_some());
    assert_eq!(info.players_max, 20);

    supervisor.command(id, "list").await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let (history, _rx) = supervisor.console(id, 0);
    for line in &history {
        println!("{:>4} {}", line.seq, line.text);
    }
    assert!(history.iter().any(|l| l.text.contains("players online")));

    supervisor.stop(id).await.unwrap();
    assert_eq!(supervisor.info(id).status, InstanceStatus::Stopped);
    assert!(events.try_recv().is_ok(), "status changes are broadcast");

    // A stopped instance can be started again by the same supervisor.
    tokio::time::sleep(Duration::from_millis(300)).await;
    supervisor.restart(id).await.unwrap();
    wait_for(InstanceStatus::Running).await;
    supervisor.shutdown().await;
    assert_eq!(supervisor.info(id).status, InstanceStatus::Stopped);
}
