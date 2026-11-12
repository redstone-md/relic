//! Forge 1.13+ (`META-INF/mods.toml`) and NeoForge
//! (`META-INF/neoforge.mods.toml`).
//!
//! Neither format has a mod-level side field. What exists:
//! * `clientSideOnly = true` at the top level (newer Forge) — an explicit
//!   statement, treated as declared;
//! * `side` on the dependency entries for the game or the loader — a hint;
//! * `displayTest = "IGNORE_SERVER_VERSION"` — the mod tells the client not
//!   to expect it, which in practice means server-only — a hint.

use relic_proto::{LoaderKind, Side};
use toml::{Table, Value};

use crate::{Confidence, Finding, ModMeta, is_platform, tidy};

/// The mods `id` cannot load without, from `[[dependencies.<id>]]`.
fn required(doc: &Table, id: &str) -> Vec<String> {
    let entries = doc
        .get("dependencies")
        .and_then(Value::as_table)
        .and_then(|deps| deps.get(id))
        .and_then(Value::as_array);
    let mut out: Vec<String> = Vec::new();
    for dep in entries.into_iter().flatten() {
        let Some(target) = dep.get("modId").and_then(Value::as_str) else {
            continue;
        };
        // Forge says `mandatory = true`, NeoForge `type = "required"`,
        // which is also what it assumes when nothing is said.
        let needed = match (
            dep.get("mandatory").and_then(Value::as_bool),
            dep.get("type").and_then(Value::as_str),
        ) {
            (Some(mandatory), _) => mandatory,
            (None, Some(kind)) => kind.eq_ignore_ascii_case("required"),
            (None, None) => true,
        };
        if needed && !is_platform(target) && !out.iter().any(|known| known == target) {
            out.push(target.to_owned());
        }
    }
    out
}

/// Dependency targets whose `side` says something about the mod itself.
const PLATFORM_IDS: &[&str] = &["minecraft", "forge", "neoforge"];

/// `Implementation-Version` from `META-INF/MANIFEST.MF`, which is what the
/// `${file.jarVersion}` placeholder in `mods.toml` resolves to.
pub(crate) fn implementation_version(manifest: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim().eq_ignore_ascii_case("Implementation-Version"))
            .then(|| value.trim().to_owned())
            .filter(|v| !v.is_empty())
    })
}

pub(crate) fn parse_mods_toml(
    text: &str,
    loader: LoaderKind,
    jar_version: Option<&str>,
) -> Finding {
    let file = match loader {
        LoaderKind::NeoForge => "neoforge.mods.toml",
        _ => "mods.toml",
    };
    let Ok(doc) = toml::from_str::<Table>(text) else {
        return Finding {
            loader,
            mods: Vec::new(),
            side: Side::Unknown,
            confidence: Confidence::None,
            icon: None,
            reason: format!("{file} is not valid TOML"),
        };
    };

    let mods: Vec<ModMeta> = doc
        .get("mods")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry.get("modId")?.as_str()?.to_owned();
            let version = match entry.get("version").and_then(Value::as_str) {
                Some(v) if v.contains("${") => jar_version.unwrap_or_default().to_owned(),
                Some(v) => v.to_owned(),
                None => String::new(),
            };
            Some(ModMeta {
                name: entry
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_owned(),
                version,
                description: tidy(
                    entry
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                depends: required(&doc, &id),
                provides: Vec::new(),
                id,
            })
        })
        .collect();
    let icon = doc
        .get("mods")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|entry| entry.get("logoFile").and_then(Value::as_str))
        .or_else(|| doc.get("logoFile").and_then(Value::as_str))
        .map(str::to_owned);

    let (side, confidence, reason) = detect_side(&doc, &mods, file);
    Finding {
        loader,
        mods,
        side,
        confidence,
        reason,
        icon,
    }
}

fn detect_side(doc: &Table, mods: &[ModMeta], file: &str) -> (Side, Confidence, String) {
    if doc.get("clientSideOnly").and_then(Value::as_bool) == Some(true) {
        return (
            Side::Client,
            Confidence::Declared,
            format!("{file}: clientSideOnly = true"),
        );
    }

    // `side` on the minecraft / loader dependency of each mod in the file.
    let mut platform_sides: Vec<Side> = Vec::new();
    if let Some(deps) = doc.get("dependencies").and_then(Value::as_table) {
        for meta in mods {
            let entries = deps.get(&meta.id).and_then(Value::as_array);
            for dep in entries.into_iter().flatten() {
                let target = dep.get("modId").and_then(Value::as_str).unwrap_or_default();
                if !PLATFORM_IDS.contains(&target) {
                    continue;
                }
                match dep.get("side").and_then(Value::as_str) {
                    Some(s) if s.eq_ignore_ascii_case("CLIENT") => {
                        platform_sides.push(Side::Client);
                    }
                    Some(s) if s.eq_ignore_ascii_case("SERVER") => {
                        platform_sides.push(Side::Server);
                    }
                    _ => platform_sides.push(Side::Both),
                }
            }
        }
    }
    if let Some(&first) = platform_sides.first()
        && first != Side::Both
        && platform_sides.iter().all(|&s| s == first)
    {
        return (
            first,
            Confidence::Heuristic,
            format!(
                "{file}: every game/loader dependency is restricted to side {}",
                first.as_str().to_uppercase()
            ),
        );
    }

    let display_test = doc
        .get("mods")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("displayTest").and_then(Value::as_str))
        .next();
    if display_test == Some("IGNORE_SERVER_VERSION") {
        return (
            Side::Server,
            Confidence::Heuristic,
            format!("{file}: displayTest = \"IGNORE_SERVER_VERSION\""),
        );
    }

    (
        Side::Unknown,
        Confidence::None,
        format!("{file} does not state a side; classify manually"),
    )
}
