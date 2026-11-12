use std::io::{Cursor, Write};

use relic_proto::{FileKind, Loader, LoaderKind, ModPolicy, Side, SyncAction};

use super::*;
use crate::instances::NewInstance;

struct Fixture {
    modpack: Modpack,
    db: SqlitePool,
    _dir: tempfile::TempDir,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::open_in_memory().await.unwrap();
    let blobs = BlobStore::open(dir.path().join("blobs")).await.unwrap();
    for (id, port) in [("main", 25565), ("second", 25566)] {
        instances::create(
            &db,
            NewInstance {
                id: id.to_owned(),
                name: id.to_owned(),
                description: String::new(),
                minecraft_version: "1.21.1".to_owned(),
                loader: Loader {
                    kind: LoaderKind::Fabric,
                    version: "0.16.9".to_owned(),
                },
                java_major: 21,
                port,
                memory_mb: 4096,
            },
        )
        .await
        .unwrap();
    }
    Fixture {
        modpack: Modpack::new(db.clone(), blobs),
        db,
        _dir: dir,
    }
}

fn jar(entry: &str, content: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(entry, zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(content.as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

fn fabric_jar(id: &str, environment: &str) -> Vec<u8> {
    jar(
        "fabric.mod.json",
        &format!(r#"{{"id":"{id}","name":"{id}","version":"1.0","environment":"{environment}"}}"#),
    )
}

fn forge_jar(id: &str) -> Vec<u8> {
    jar(
        "META-INF/mods.toml",
        &format!("[[mods]]\nmodId = \"{id}\"\nversion = \"1.0\"\n"),
    )
}

async fn add(f: &Fixture, instance: &str, path: &str, bytes: &[u8]) -> FileRecord {
    f.modpack
        .add_file(instance, path, bytes, None, FileHints::default())
        .await
        .unwrap()
}

fn paths(files: &[FileRecord]) -> Vec<&str> {
    files.iter().map(|f| f.path.as_str()).collect()
}

#[tokio::test]
async fn files_are_split_between_client_and_server_by_side() {
    let f = fixture().await;
    add(&f, "main", "mods/client.jar", &fabric_jar("c", "client")).await;
    add(&f, "main", "mods/server.jar", &fabric_jar("s", "server")).await;
    let both = add(&f, "main", "mods/both.jar", &fabric_jar("b", "*")).await;
    add(&f, "main", "config/b.toml", b"x = 1").await;
    add(&f, "main", "shaderpacks/nice.zip", b"zip").await;

    assert_eq!(both.mod_id.as_deref(), Some("b"));
    assert_eq!(both.detect_confidence, Confidence::Declared);

    let manifest = f.modpack.manifest("main").await.unwrap();
    let client: Vec<&str> = manifest.files.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(
        client,
        [
            "config/b.toml",
            "mods/both.jar",
            "mods/client.jar",
            "shaderpacks/nice.zip"
        ]
    );
    assert_eq!(manifest.minecraft_version, "1.21.1");
    assert_eq!(manifest.java_runtime.major_version, 21);
    assert_eq!(manifest.policy, ModPolicy::Optional);
    assert!(manifest.protected_paths.iter().any(|p| p == "options.txt"));

    let config = &manifest.files[0];
    assert_eq!(config.kind, FileKind::Config);
    assert_eq!(config.action, SyncAction::SyncOnce);
    assert_eq!(config.size, 5);
    assert_eq!(manifest.files[1].action, SyncAction::Enforce);

    let server = f.modpack.server_files("main").await.unwrap();
    assert_eq!(
        paths(&server),
        ["config/b.toml", "mods/both.jar", "mods/server.jar"]
    );
}

#[tokio::test]
async fn unknown_side_is_deployed_nowhere_until_overridden() {
    let f = fixture().await;
    let file = add(&f, "main", "mods/create.jar", &forge_jar("create")).await;
    assert_eq!(file.side, Side::Unknown);

    assert!(f.modpack.manifest("main").await.unwrap().files.is_empty());
    assert!(f.modpack.server_files("main").await.unwrap().is_empty());
    assert_eq!(
        paths(&f.modpack.unclassified("main").await.unwrap()),
        ["mods/create.jar"]
    );
    assert!(
        !f.modpack
            .is_client_blob("main", &file.sha256)
            .await
            .unwrap()
    );

    f.modpack
        .set_side_override(&file.sha256, Some(Side::Both))
        .await
        .unwrap();
    let file = f.modpack.get_file("main", "mods/create.jar").await.unwrap();
    assert_eq!(file.side, Side::Both);
    assert_eq!(file.side_source, SideSource::Override);
    assert_eq!(file.detected_side, Side::Unknown);
    assert_eq!(f.modpack.manifest("main").await.unwrap().files.len(), 1);
    assert_eq!(f.modpack.server_files("main").await.unwrap().len(), 1);
    assert!(
        f.modpack
            .is_client_blob("main", &file.sha256)
            .await
            .unwrap()
    );

    f.modpack
        .set_side_override(&file.sha256, None)
        .await
        .unwrap();
    assert!(f.modpack.manifest("main").await.unwrap().files.is_empty());
}

#[tokio::test]
async fn override_follows_the_content_across_instances_and_reuploads() {
    let f = fixture().await;
    let bytes = forge_jar("jei");
    let first = add(&f, "main", "mods/jei.jar", &bytes).await;
    f.modpack
        .set_side_override(&first.sha256, Some(Side::Client))
        .await
        .unwrap();

    let elsewhere = add(&f, "second", "mods/renamed-jei.jar", &bytes).await;
    assert_eq!(elsewhere.side, Side::Client);
    assert_eq!(elsewhere.side_source, SideSource::Override);
}

#[tokio::test]
async fn build_id_changes_on_every_change_to_the_file_set() {
    let f = fixture().await;
    let build = |f: &Fixture| {
        let db = f.db.clone();
        async move { instances::get(&db, "main").await.unwrap().build_id }
    };
    let mut seen = vec![build(&f).await];
    let mut expect_new = |id| {
        assert!(!seen.contains(&id), "build_id was not regenerated");
        seen.push(id);
    };

    let file = add(&f, "main", "mods/a.jar", &fabric_jar("a", "*")).await;
    expect_new(build(&f).await);

    f.modpack
        .set_file_options("main", "mods/a.jar", None, Some(true))
        .await
        .unwrap();
    expect_new(build(&f).await);

    f.modpack
        .set_side_override(&file.sha256, Some(Side::Server))
        .await
        .unwrap();
    expect_new(build(&f).await);

    let other_before = instances::get(&f.db, "second").await.unwrap().build_id;
    f.modpack.remove_file("main", "mods/a.jar").await.unwrap();
    expect_new(build(&f).await);
    assert_eq!(
        instances::get(&f.db, "second").await.unwrap().build_id,
        other_before,
        "an unrelated instance must keep its build_id"
    );
}

#[tokio::test]
async fn replacing_a_file_keeps_admin_settings() {
    let f = fixture().await;
    add(&f, "main", "mods/a.jar", &fabric_jar("a", "*")).await;
    f.modpack
        .set_file_options("main", "mods/a.jar", Some(SyncAction::SyncOnce), Some(true))
        .await
        .unwrap();

    let replaced = add(&f, "main", "mods/a.jar", &fabric_jar("a2", "client")).await;
    assert_eq!(replaced.mod_id.as_deref(), Some("a2"));
    assert_eq!(replaced.side, Side::Client);
    assert!(replaced.optional);
    assert_eq!(replaced.sync_action, SyncAction::SyncOnce);
    assert_eq!(f.modpack.list_files("main").await.unwrap().len(), 1);
}

#[tokio::test]
async fn import_hints_beat_detection() {
    let f = fixture().await;
    let file = f
        .modpack
        .add_file(
            "main",
            "mods/create.jar",
            &forge_jar("create")[..],
            None,
            FileHints {
                side: Some(Side::Both),
                optional: Some(true),
            },
        )
        .await
        .unwrap();
    assert_eq!(file.side, Side::Both);
    assert_eq!(file.detect_confidence, Confidence::Declared);
    assert!(file.optional);
}

#[tokio::test]
async fn garbage_collection_removes_only_unreferenced_blobs() {
    let f = fixture().await;
    let shared = fabric_jar("shared", "*");
    let kept = add(&f, "main", "mods/shared.jar", &shared).await;
    add(&f, "second", "mods/shared.jar", &shared).await;
    let dropped = add(&f, "main", "mods/gone.jar", &fabric_jar("gone", "*")).await;

    f.modpack
        .remove_file("main", "mods/gone.jar")
        .await
        .unwrap();
    f.modpack
        .remove_file("main", "mods/shared.jar")
        .await
        .unwrap();

    assert_eq!(f.modpack.collect_garbage().await.unwrap(), 1);
    assert!(!f.modpack.blobs().exists(&dropped.sha256).await.unwrap());
    assert!(f.modpack.blobs().exists(&kept.sha256).await.unwrap());
    assert_eq!(f.modpack.collect_garbage().await.unwrap(), 0);
}

#[tokio::test]
async fn corrupt_jar_is_stored_as_unknown_instead_of_failing() {
    let f = fixture().await;
    let file = add(&f, "main", "mods/broken.jar", b"not a zip").await;
    assert_eq!(file.side, Side::Unknown);
    assert!(!file.detect_reason.is_empty());
}

#[tokio::test]
async fn unknown_instance_and_bad_paths_are_rejected() {
    let f = fixture().await;
    let err = f
        .modpack
        .add_file("nope", "mods/a.jar", &b"x"[..], None, FileHints::default())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound("instance")));

    for bad in [
        "../x.jar",
        "/etc/passwd",
        "mods//a.jar",
        "C:/x.jar",
        "mods/..",
        "",
        "mods/a.jar.",
    ] {
        assert!(normalize_path(bad).is_err(), "{bad:?} should be rejected");
    }
    assert_eq!(normalize_path("mods\\a.jar").unwrap(), "mods/a.jar");
    assert_eq!(
        normalize_path("config/sub dir/a.toml").unwrap(),
        "config/sub dir/a.toml"
    );
}
