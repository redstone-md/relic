use std::io::{Cursor, Write};

use relic_proto::{LoaderKind, Side};
use zip::write::SimpleFileOptions;

use crate::{Confidence, JarInfo, inspect};

fn jar(entries: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let mut cursor = writer.finish().unwrap();
    cursor.set_position(0);
    cursor
}

fn inspect_entries(entries: &[(&str, &[u8])]) -> JarInfo {
    inspect(jar(entries)).unwrap()
}

#[test]
fn fabric_client_environment_is_declared() {
    let info = inspect_entries(&[(
        "fabric.mod.json",
        br#"{"schemaVersion":1,"id":"sodium","name":"Sodium","version":"0.6.5","environment":"client"}"#,
    )]);
    assert_eq!(info.side, Side::Client);
    assert_eq!(info.confidence, Confidence::Declared);
    assert_eq!(info.loaders, [LoaderKind::Fabric]);
    assert_eq!(info.mods[0].id, "sodium");
    assert_eq!(info.mods[0].version, "0.6.5");
}

#[test]
fn fabric_without_environment_defaults_to_both() {
    let info = inspect_entries(&[("fabric.mod.json", br#"{"id":"lithium","version":"1"}"#)]);
    assert_eq!(info.side, Side::Both);
    assert_eq!(info.confidence, Confidence::Declared);
    assert_eq!(info.mods[0].name, "lithium");
}

#[test]
fn fabric_json_with_raw_newline_in_string_still_parses() {
    let info = inspect_entries(&[(
        "fabric.mod.json",
        b"{\"id\":\"x\",\"description\":\"line one\nline two\",\"environment\":\"server\"}",
    )]);
    assert_eq!(info.side, Side::Server);
}

#[test]
fn quilt_dedicated_server_maps_to_server() {
    let info = inspect_entries(&[(
        "quilt.mod.json",
        br#"{"quilt_loader":{"id":"q","version":"2.0","metadata":{"name":"Q Mod"}},
            "minecraft":{"environment":"dedicated_server"}}"#,
    )]);
    assert_eq!(info.side, Side::Server);
    assert_eq!(info.loaders, [LoaderKind::Quilt]);
    assert_eq!(info.mods[0].name, "Q Mod");
}

#[test]
fn forge_without_side_information_is_unknown() {
    let info = inspect_entries(&[(
        "META-INF/mods.toml",
        br#"
modLoader = "javafml"
loaderVersion = "[47,)"
[[mods]]
modId = "create"
version = "0.5.1"
displayName = "Create"
[[dependencies.create]]
modId = "minecraft"
mandatory = true
versionRange = "[1.20.1]"
side = "BOTH"
"#,
    )]);
    assert_eq!(info.side, Side::Unknown);
    assert_eq!(info.confidence, Confidence::None);
    assert_eq!(info.loaders, [LoaderKind::Forge]);
    assert_eq!(info.mods[0].name, "Create");
}

#[test]
fn forge_client_side_only_flag_is_declared() {
    let info = inspect_entries(&[(
        "META-INF/mods.toml",
        b"clientSideOnly = true\n[[mods]]\nmodId = \"hud\"\nversion = \"1\"\n",
    )]);
    assert_eq!(info.side, Side::Client);
    assert_eq!(info.confidence, Confidence::Declared);
}

#[test]
fn neoforge_client_only_platform_dependency_is_a_hint() {
    let info = inspect_entries(&[
        (
            "META-INF/neoforge.mods.toml",
            br#"
[[mods]]
modId = "shiny"
version = "${file.jarVersion}"
[[dependencies.shiny]]
modId = "neoforge"
type = "required"
side = "CLIENT"
[[dependencies.shiny]]
modId = "minecraft"
side = "CLIENT"
[[dependencies.shiny]]
modId = "jei"
side = "BOTH"
"#,
        ),
        (
            "META-INF/MANIFEST.MF",
            b"Manifest-Version: 1.0\r\nImplementation-Version: 3.2.1\r\n",
        ),
    ]);
    assert_eq!(info.side, Side::Client);
    assert_eq!(info.confidence, Confidence::Heuristic);
    assert_eq!(info.loaders, [LoaderKind::NeoForge]);
    assert_eq!(info.mods[0].version, "3.2.1");
}

#[test]
fn forge_ignore_server_version_hints_server() {
    let info = inspect_entries(&[(
        "META-INF/mods.toml",
        b"[[mods]]\nmodId = \"srv\"\nversion = \"1\"\ndisplayTest = \"IGNORE_SERVER_VERSION\"\n",
    )]);
    assert_eq!(info.side, Side::Server);
    assert_eq!(info.confidence, Confidence::Heuristic);
}

#[test]
fn multi_loader_jar_prefers_the_declared_side() {
    let info = inspect_entries(&[
        (
            "fabric.mod.json",
            br#"{"id":"multi","environment":"client"}"#,
        ),
        (
            "META-INF/mods.toml",
            b"[[mods]]\nmodId = \"multi\"\nversion = \"1\"\n",
        ),
    ]);
    assert_eq!(info.side, Side::Client);
    assert_eq!(info.confidence, Confidence::Declared);
    assert_eq!(info.loaders, [LoaderKind::Fabric, LoaderKind::Forge]);
    assert_eq!(info.mods.len(), 1);
}

#[test]
fn legacy_mcmod_info_reads_both_layouts() {
    let bare = inspect_entries(&[(
        "mcmod.info",
        br#"[{"modid":"jei","name":"Just Enough Items","version":"4.16"}]"#,
    )]);
    assert_eq!(bare.mods[0].id, "jei");
    assert_eq!(bare.side, Side::Unknown);
    assert_eq!(bare.loaders, [LoaderKind::Forge]);

    let wrapped = inspect_entries(&[(
        "mcmod.info",
        br#"{"modListVersion":2,"modList":[{"modid":"ic2","name":"IC2","version":"2.8"}]}"#,
    )]);
    assert_eq!(wrapped.mods[0].id, "ic2");
}

#[test]
fn legacy_mod_annotation_with_client_side_only_hints_client() {
    let mut class = b"\xCA\xFE\xBA\xBE....".to_vec();
    class.extend_from_slice(b"Lnet/minecraftforge/fml/common/Mod;");
    class.extend_from_slice(b"..modid..clientSideOnly..");
    let info = inspect_entries(&[
        ("mcmod.info", br#"[{"modid":"minimap"}]"#),
        ("com/example/Other.class", b"\xCA\xFE\xBA\xBEclientSideOnly"),
        ("com/example/Minimap.class", &class),
    ]);
    assert_eq!(info.side, Side::Client);
    assert_eq!(info.confidence, Confidence::Heuristic);
    assert!(info.reason.contains("Minimap.class"));
}

#[test]
fn archive_without_manifest_is_unknown() {
    let info = inspect_entries(&[("assets/pack.png", b"png")]);
    assert_eq!(info.side, Side::Unknown);
    assert!(info.loaders.is_empty());
}

#[test]
fn garbage_input_is_an_error_not_a_panic() {
    assert!(inspect(Cursor::new(b"definitely not a zip".to_vec())).is_err());
}

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];

#[test]
fn fabric_reads_description_icon_and_what_it_needs() {
    let inner = jar(&[(
        "fabric.mod.json",
        br#"{"id":"cloth-basic-math","version":"1","provides":["basic-math"]}"#,
    )])
    .into_inner();
    let info = inspect_entries(&[
        (
            "fabric.mod.json",
            br#"{"id":"minimap","name":"Minimap","version":"1",
                "description":"A map\n in the   corner",
                "icon":{"32":"small.png","128":"assets/minimap/icon.png"},
                "depends":{"minecraft":">=1.21","fabricloader":"*","fabric-api":"*","cloth-config":"*","cloth-basic-math":"*"},
                "recommends":{"modmenu":"*"},
                "provides":["xaerominimap"]}"#,
        ),
        ("assets/minimap/icon.png", PNG),
        ("META-INF/jars/math.jar", &inner),
    ]);
    assert_eq!(info.mods[0].description, "A map in the corner");
    assert_eq!(info.icon.as_deref(), Some(PNG));
    assert_eq!(info.bundled, ["cloth-basic-math", "basic-math"]);
    assert_eq!(
        info.ids(),
        ["minimap", "xaerominimap", "cloth-basic-math", "basic-math"]
    );
    // What it carries inside is not asked of the pack.
    assert_eq!(info.depends(), ["cloth-config", "fabric-api"]);
}

#[test]
fn an_icon_that_is_not_a_png_is_dropped() {
    let info = inspect_entries(&[
        ("fabric.mod.json", br#"{"id":"a","icon":"icon.png"}"#),
        ("icon.png", b"<svg/>"),
    ]);
    assert_eq!(info.icon, None);
}

#[test]
fn forge_reads_required_dependencies_only() {
    let info = inspect_entries(&[
        (
            "META-INF/mods.toml",
            br#"
modLoader = "javafml"
logoFile = "logo.png"
[[mods]]
modId = "jeiaddon"
displayName = "JEI Addon"
description = "Adds things"
[[dependencies.jeiaddon]]
modId = "forge"
mandatory = true
[[dependencies.jeiaddon]]
modId = "jei"
mandatory = true
[[dependencies.jeiaddon]]
modId = "patchouli"
mandatory = false
"#,
        ),
        ("logo.png", PNG),
    ]);
    assert_eq!(info.mods[0].description, "Adds things");
    assert_eq!(info.depends(), ["jei"]);
    assert_eq!(info.icon.as_deref(), Some(PNG));
}

#[test]
fn neoforge_dependency_types_are_respected() {
    let info = inspect_entries(&[(
        "META-INF/neoforge.mods.toml",
        br#"
[[mods]]
modId = "addon"
[[dependencies.addon]]
modId = "neoforge"
type = "required"
[[dependencies.addon]]
modId = "base"
type = "required"
[[dependencies.addon]]
modId = "nice"
type = "optional"
[[dependencies.addon]]
modId = "implied"
"#,
    )]);
    assert_eq!(info.depends(), ["base", "implied"]);
}

#[test]
fn quilt_reads_dependencies_in_both_shapes() {
    let info = inspect_entries(&[(
        "quilt.mod.json",
        br#"{"quilt_loader":{"id":"q","version":"1",
            "metadata":{"name":"Q","description":"Quilted","icon":"q.png"},
            "depends":["quilt_loader",{"id":"quilt:qsl"},{"id":"maybe","optional":true}],
            "provides":[{"id":"old-q"}]}}"#,
    )]);
    assert_eq!(info.mods[0].description, "Quilted");
    assert_eq!(info.depends(), ["qsl"]);
    assert_eq!(info.ids(), ["q", "old-q"]);
}
