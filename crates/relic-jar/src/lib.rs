//! Reads mod metadata out of a JAR without unpacking it to disk and works out
//! which side (client / server / both) the mod belongs on.
//!
//! Only Fabric and Quilt declare a side reliably. Forge and NeoForge have no
//! mod-level side field, so for them the result is usually a hint or
//! [`Side::Unknown`], and an admin override decides.

use std::io::{Read, Seek};
use std::path::Path;

use relic_proto::{LoaderKind, Side};
use serde::{Deserialize, Serialize};

mod fabric;
mod forge;
mod legacy;

/// Metadata entries larger than this are ignored (guards against zip bombs).
const MAX_META_BYTES: u64 = 4 * 1024 * 1024;
/// A mod's icon is a small picture; anything bigger is not shown.
const MAX_ICON_BYTES: u64 = 1024 * 1024;
/// JARs packed inside a JAR are opened to learn which mods they bring.
const MAX_NESTED_JARS: usize = 96;
const MAX_NESTED_BYTES: u64 = 48 * 1024 * 1024;
const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a readable zip/jar archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// How much the detected [`Side`] can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Nothing usable was found.
    None,
    /// Inferred from indirect signals; worth an admin's glance.
    Heuristic,
    /// The mod's own manifest states it.
    Declared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Ids of the mods this one does not load without. The game, the
    /// loader and Java are left out.
    #[serde(default)]
    pub depends: Vec<String>,
    /// Other ids this mod answers to.
    #[serde(default)]
    pub provides: Vec<String>,
}

/// Dependencies every installation has, whatever the pack.
const PLATFORM: &[&str] = &[
    "minecraft",
    "java",
    "fabricloader",
    "fabric-loader",
    "quilt_loader",
    "quilt_base",
    "forge",
    "neoforge",
    "fml",
    "javafml",
    "lowcodefml",
    "mixinextras",
];

pub(crate) fn is_platform(id: &str) -> bool {
    PLATFORM.contains(&id.to_ascii_lowercase().as_str())
}

/// Trims a description to something a tile can show.
pub(crate) fn tidy(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(400) {
        Some((cut, _)) => format!("{}…", flat[..cut].trim_end()),
        None => flat,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JarInfo {
    /// Loaders this JAR carries a manifest for (multi-loader JARs list several).
    pub loaders: Vec<LoaderKind>,
    pub mods: Vec<ModMeta>,
    pub side: Side,
    pub confidence: Confidence,
    /// Human-readable explanation of how `side` was decided.
    pub reason: String,
    /// The first mod's icon, when it is a PNG of a sane size.
    #[serde(skip)]
    pub icon: Option<Vec<u8>>,
    /// Ids of the mods in JARs packed inside this one.
    #[serde(default)]
    pub bundled: Vec<String>,
}

impl JarInfo {
    fn unknown(reason: impl Into<String>) -> Self {
        Self {
            loaders: Vec::new(),
            mods: Vec::new(),
            side: Side::Unknown,
            confidence: Confidence::None,
            reason: reason.into(),
            icon: None,
            bundled: Vec::new(),
        }
    }

    /// Every mod id this JAR satisfies: its own, the ones it answers to and
    /// the ones it carries inside.
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        let own = self
            .mods
            .iter()
            .flat_map(|m| std::iter::once(&m.id).chain(&m.provides));
        for id in own.chain(&self.bundled) {
            if !id.is_empty() && !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// Ids this JAR needs from other files of the pack.
    pub fn depends(&self) -> Vec<String> {
        let own = self.ids();
        let mut out: Vec<String> = Vec::new();
        for id in self.mods.iter().flat_map(|m| &m.depends) {
            if !own.contains(id) && !out.contains(id) {
                out.push(id.clone());
            }
        }
        // JSON key order depends on how serde_json is built; the answer must not.
        out.sort();
        out
    }
}

/// One loader's view of the JAR.
struct Finding {
    loader: LoaderKind,
    mods: Vec<ModMeta>,
    side: Side,
    confidence: Confidence,
    reason: String,
    /// Archive path of the mod's picture.
    icon: Option<String>,
}

pub fn inspect_path(path: impl AsRef<Path>) -> Result<JarInfo, Error> {
    let file = std::fs::File::open(path)?;
    inspect(std::io::BufReader::new(file))
}

pub fn inspect<R: Read + Seek>(reader: R) -> Result<JarInfo, Error> {
    inspect_at(reader, 0)
}

fn inspect_at<R: Read + Seek>(reader: R, depth: u8) -> Result<JarInfo, Error> {
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut findings: Vec<Finding> = Vec::new();

    if let Some(text) = read_entry(&mut archive, "fabric.mod.json") {
        findings.push(fabric::parse_fabric(&text));
    }
    if let Some(text) = read_entry(&mut archive, "quilt.mod.json") {
        findings.push(fabric::parse_quilt(&text));
    }

    let jar_version = read_entry(&mut archive, "META-INF/MANIFEST.MF")
        .and_then(|m| forge::implementation_version(&m));
    if let Some(text) = read_entry(&mut archive, "META-INF/neoforge.mods.toml") {
        findings.push(forge::parse_mods_toml(
            &text,
            LoaderKind::NeoForge,
            jar_version.as_deref(),
        ));
    }
    if let Some(text) = read_entry(&mut archive, "META-INF/mods.toml") {
        findings.push(forge::parse_mods_toml(
            &text,
            LoaderKind::Forge,
            jar_version.as_deref(),
        ));
    }

    // Pre-1.13 Forge. Only consulted when no modern manifest exists, because
    // it needs a scan over the class files.
    if findings.is_empty()
        && let Some(text) = read_entry(&mut archive, "mcmod.info")
    {
        let mut finding = legacy::parse_mcmod_info(&text);
        if let Some((side, reason)) = legacy::scan_mod_annotation(&mut archive) {
            finding.side = side;
            finding.confidence = Confidence::Heuristic;
            finding.reason = reason;
        }
        findings.push(finding);
    }

    let bundled = if depth == 0 {
        nested_ids(&mut archive)
    } else {
        Vec::new()
    };
    if findings.is_empty() {
        let mut info = JarInfo::unknown("no mod manifest found in the archive");
        info.bundled = bundled;
        return Ok(info);
    }
    let icon = findings
        .iter()
        .find_map(|finding| finding.icon.clone())
        .and_then(|path| read_icon(&mut archive, &path));
    let mut info = merge(findings);
    info.icon = icon;
    info.bundled = bundled;
    Ok(info)
}

/// Mod ids of the JARs packed inside (Fabric's `META-INF/jars`, Forge's
/// `META-INF/jarjar`), one level deep.
fn nested_ids<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Vec<String> {
    let names: Vec<String> = archive
        .file_names()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".jar")
                && (lower.starts_with("meta-inf/jars/") || lower.starts_with("meta-inf/jarjar/"))
        })
        .take(MAX_NESTED_JARS)
        .map(str::to_owned)
        .collect();
    let mut ids = Vec::new();
    for name in names {
        let Some(bytes) = read_bytes(archive, &name, MAX_NESTED_BYTES) else {
            continue;
        };
        let Ok(inner) = inspect_at(std::io::Cursor::new(bytes), 1) else {
            continue;
        };
        for id in inner.ids() {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

fn read_bytes<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    max: u64,
) -> Option<Vec<u8>> {
    let entry = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    entry.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}

fn read_icon<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, path: &str) -> Option<Vec<u8>> {
    let path = path.trim_start_matches("./").trim_start_matches('/');
    read_bytes(archive, path, MAX_ICON_BYTES).filter(|bytes| bytes.starts_with(PNG_SIGNATURE))
}

/// Combines per-loader findings: the most trustworthy side wins; equally
/// trusted findings that disagree collapse to `Unknown`.
fn merge(findings: Vec<Finding>) -> JarInfo {
    let best = findings
        .iter()
        .map(|f| f.confidence)
        .max()
        .unwrap_or(Confidence::None);
    let mut top = findings.iter().filter(|f| f.confidence == best);
    let first = top.next().expect("findings is non-empty");
    let (side, confidence, reason) = if top.all(|f| f.side == first.side) {
        (first.side, best, first.reason.clone())
    } else {
        (
            Side::Unknown,
            Confidence::None,
            "manifests for different loaders disagree about the side".to_owned(),
        )
    };

    let mut loaders = Vec::new();
    let mut mods: Vec<ModMeta> = Vec::new();
    for finding in findings {
        if !loaders.contains(&finding.loader) {
            loaders.push(finding.loader);
        }
        for meta in finding.mods {
            if !mods.iter().any(|m| m.id == meta.id) {
                mods.push(meta);
            }
        }
    }
    JarInfo {
        loaders,
        mods,
        side,
        confidence,
        reason,
        icon: None,
        bundled: Vec::new(),
    }
}

/// Reads a text entry, tolerating a BOM and invalid UTF-8. Missing, oversized
/// or unreadable entries yield `None`.
fn read_entry<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let bytes = read_bytes(archive, name, MAX_META_BYTES)?;
    let text = String::from_utf8_lossy(&bytes);
    Some(text.trim_start_matches('\u{feff}').to_owned())
}

/// Parses JSON the way mod authors actually write it: raw line breaks and
/// tabs inside strings are common and rejected by a strict parser.
fn parse_lenient_json(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str(text).ok().or_else(|| {
        let cleaned: String = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        serde_json::from_str(&cleaned).ok()
    })
}

#[cfg(test)]
mod tests;
