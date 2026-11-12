//! Forge for Minecraft 1.12.2 and older: `mcmod.info` plus the `@Mod`
//! annotation on the main class.

use std::io::{Read, Seek};

use relic_proto::{LoaderKind, Side};
use serde_json::Value;

use crate::{Confidence, Finding, ModMeta, is_platform, parse_lenient_json, tidy};

/// Descriptor of the legacy `@Mod` annotation as stored in a class file.
const MOD_ANNOTATION: &[u8] = b"Lnet/minecraftforge/fml/common/Mod;";
/// Class files larger than this are skipped by the annotation scan.
const MAX_CLASS_BYTES: u64 = 2 * 1024 * 1024;
/// Upper bound on class files examined per JAR.
const MAX_CLASSES_SCANNED: usize = 20_000;

pub(crate) fn parse_mcmod_info(text: &str) -> Finding {
    let mut finding = Finding {
        loader: LoaderKind::Forge,
        mods: Vec::new(),
        side: Side::Unknown,
        confidence: Confidence::None,
        reason: "mcmod.info does not state a side; classify manually".to_owned(),
        icon: None,
    };
    let Some(json) = parse_lenient_json(text) else {
        finding.reason = "mcmod.info is not valid JSON".to_owned();
        return finding;
    };

    // Either a bare array, or `{ "modListVersion": 2, "modList": [...] }`.
    let list = match &json {
        Value::Array(items) => items.as_slice(),
        Value::Object(map) => map
            .get("modList")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        _ => &[],
    };
    finding.mods = list
        .iter()
        .filter_map(|entry| {
            let id = entry.get("modid")?.as_str()?.to_owned();
            Some(ModMeta {
                name: entry
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_owned(),
                version: entry
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                description: tidy(
                    entry
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                depends: entry
                    .get("requiredMods")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    // `Forge@[14.23,)` and the like.
                    .map(|dep| dep.split('@').next().unwrap_or(dep).trim().to_owned())
                    .filter(|dep| !dep.is_empty() && !is_platform(dep))
                    .collect(),
                provides: Vec::new(),
                id,
            })
        })
        .collect();
    finding.icon = list
        .iter()
        .find_map(|entry| entry.get("logoFile").and_then(Value::as_str))
        .filter(|path| !path.is_empty())
        .map(str::to_owned);
    finding
}

/// Looks for `clientSideOnly` / `serverSideOnly` on a legacy `@Mod` class.
///
/// This does not decode the annotation: it checks that the member name is
/// present in the constant pool of a class carrying `@Mod`. Both members
/// default to `false`, so a name that appears at all was almost certainly
/// set to `true` — good enough for a hint, never treated as declared.
pub(crate) fn scan_mod_annotation<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Option<(Side, String)> {
    let mut scanned = 0;
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        if !entry.name().ends_with(".class") || entry.size() > MAX_CLASS_BYTES {
            continue;
        }
        scanned += 1;
        if scanned > MAX_CLASSES_SCANNED {
            break;
        }
        let name = entry.name().to_owned();
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        if entry.take(MAX_CLASS_BYTES).read_to_end(&mut bytes).is_err() {
            continue;
        }
        if !contains(&bytes, MOD_ANNOTATION) {
            continue;
        }
        let client = contains(&bytes, b"clientSideOnly");
        let server = contains(&bytes, b"serverSideOnly");
        return match (client, server) {
            (true, false) => Some((Side::Client, format!("@Mod on {name} sets clientSideOnly"))),
            (false, true) => Some((Side::Server, format!("@Mod on {name} sets serverSideOnly"))),
            _ => None,
        };
    }
    None
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
