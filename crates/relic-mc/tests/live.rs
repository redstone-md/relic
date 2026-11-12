//! Tests against the real upstream services. They need network access and
//! download hundreds of megabytes, so they only run on request:
//!
//! ```text
//! cargo test -p relic-mc --test live -- --ignored --test-threads=1
//! ```
//!
//! `RELIC_LIVE_DIR` keeps downloads between runs.

use std::path::PathBuf;

use relic_mc::server::{InstallRequest, ServerLaunch, install};
use relic_mc::{http, java, loaders, mojang};
use relic_proto::{Loader, LoaderKind};

fn work_dir() -> (PathBuf, Option<tempfile::TempDir>) {
    match std::env::var_os("RELIC_LIVE_DIR") {
        Some(dir) => (PathBuf::from(dir), None),
        None => {
            let tmp = tempfile::tempdir().unwrap();
            (tmp.path().to_owned(), Some(tmp))
        }
    }
}

#[tokio::test]
#[ignore = "network"]
async fn metadata_endpoints_answer_in_the_expected_shape() {
    let client = http::client();

    let manifest = mojang::version_manifest(&client).await.unwrap();
    assert!(manifest.versions.iter().any(|v| v.id == "1.12.2"));
    assert_eq!(
        mojang::version_json(&client, "1.21.1")
            .await
            .unwrap()
            .java_major(),
        21
    );
    assert_eq!(
        mojang::version_json(&client, "1.12.2")
            .await
            .unwrap()
            .java_major(),
        8
    );

    for (kind, mc) in [
        (LoaderKind::Fabric, "1.21.1"),
        (LoaderKind::Quilt, "1.20.1"),
        (LoaderKind::Forge, "1.20.1"),
        (LoaderKind::Forge, "1.12.2"),
        (LoaderKind::NeoForge, "1.21.1"),
        (LoaderKind::NeoForge, manifest.latest.release.as_str()),
    ] {
        let list = loaders::versions(&client, kind, mc).await.unwrap();
        let default =
            loaders::pick_default(&list).unwrap_or_else(|| panic!("no {kind:?} versions for {mc}"));
        println!(
            "{kind:?} {mc}: {} builds, default {}",
            list.len(),
            default.version
        );
    }
    assert!(
        loaders::versions(&client, LoaderKind::Fabric, "1.12.2")
            .await
            .unwrap()
            .is_empty()
    );

    let platform = java::Platform::current().unwrap();
    for major in [8, 17, 21, 25] {
        let package = java::resolve(&client, major, platform).await.unwrap();
        assert_eq!(package.sha256.len(), 64, "{package:?}");
    }
}

async fn install_and_check(kind: LoaderKind, mc: &str) {
    let client = http::client();
    let (root, _guard) = work_dir();
    let loader = if kind == LoaderKind::Vanilla {
        Loader {
            kind,
            version: String::new(),
        }
    } else {
        let list = loaders::versions(&client, kind, mc).await.unwrap();
        Loader {
            kind,
            version: loaders::pick_default(&list).unwrap().version.clone(),
        }
    };
    let dir = root.join(format!("{}-{mc}", kind.as_str()));
    let request = InstallRequest {
        dir: &dir,
        runtimes: &root.join("runtimes"),
        minecraft_version: mc,
        loader: &loader,
    };
    let installed = install(&client, &request).await.unwrap();
    println!("{installed:?}");
    match &installed.launch {
        ServerLaunch::Jar { jar } => assert!(dir.join(jar).is_file()),
        ServerLaunch::ArgsFile { dir: args } => assert!(dir.join(args).is_dir()),
    }
    // A second call must recognise the existing installation.
    assert_eq!(install(&client, &request).await.unwrap(), installed);
}

#[tokio::test]
#[ignore = "network"]
async fn installs_vanilla() {
    install_and_check(LoaderKind::Vanilla, "1.21.1").await;
}

#[tokio::test]
#[ignore = "network"]
async fn installs_fabric() {
    install_and_check(LoaderKind::Fabric, "1.21.1").await;
}

#[tokio::test]
#[ignore = "network"]
async fn installs_quilt() {
    install_and_check(LoaderKind::Quilt, "1.20.1").await;
}

#[tokio::test]
#[ignore = "network"]
async fn installs_neoforge() {
    install_and_check(LoaderKind::NeoForge, "1.21.1").await;
}

#[tokio::test]
#[ignore = "network"]
async fn installs_forge_modern() {
    install_and_check(LoaderKind::Forge, "1.20.1").await;
}

#[tokio::test]
#[ignore = "network"]
async fn installs_forge_legacy() {
    install_and_check(LoaderKind::Forge, "1.12.2").await;
}
