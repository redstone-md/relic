//! Fabric (`fabric.mod.json`) and Quilt (`quilt.mod.json`).

use relic_proto::{LoaderKind, Side};
use serde_json::Value;

use crate::{Confidence, Finding, ModMeta, is_platform, parse_lenient_json, tidy};

/// `"icon": "assets/x/icon.png"`, or a map from size to path: the largest.
fn icon_of(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(path) => Some(path.clone()),
        Value::Object(sizes) => sizes
            .iter()
            .filter_map(|(size, path)| Some((size.parse::<u32>().ok()?, path.as_str()?)))
            .max_by_key(|(size, _)| *size)
            .map(|(_, path)| path.to_owned()),
        _ => None,
    }
}

/// Ids out of a list whose items are strings or `{ "id": ... }` objects.
/// Items marked optional are skipped.
fn id_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| match item {
            Value::String(id) => Some(id.as_str()),
            Value::Object(map) => {
                if map.get("optional").and_then(Value::as_bool) == Some(true) {
                    return None;
                }
                map.get("id").and_then(Value::as_str)
            }
            _ => None,
        })
        // Quilt writes `group:id`.
        .map(|id| id.rsplit(':').next().unwrap_or(id).to_owned())
        .collect()
}

fn required(ids: Vec<String>) -> Vec<String> {
    ids.into_iter().filter(|id| !is_platform(id)).collect()
}

fn str_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value, |v, key| v.get(key))
        .and_then(Value::as_str)
}

fn unparsable(loader: LoaderKind, file: &str) -> Finding {
    Finding {
        loader,
        mods: Vec::new(),
        side: Side::Unknown,
        confidence: Confidence::None,
        icon: None,
        reason: format!("{file} is not valid JSON"),
    }
}

pub(crate) fn parse_fabric(text: &str) -> Finding {
    let Some(json) = parse_lenient_json(text) else {
        return unparsable(LoaderKind::Fabric, "fabric.mod.json");
    };
    let id = str_at(&json, &["id"]).unwrap_or_default().to_owned();
    let depends = json
        .get("depends")
        .and_then(Value::as_object)
        .map(|deps| deps.keys().cloned().collect())
        .unwrap_or_default();
    let meta = ModMeta {
        name: str_at(&json, &["name"]).unwrap_or(&id).to_owned(),
        version: str_at(&json, &["version"]).unwrap_or_default().to_owned(),
        description: tidy(str_at(&json, &["description"]).unwrap_or_default()),
        depends: required(depends),
        provides: id_list(json.get("provides")),
        id,
    };
    let icon = icon_of(json.get("icon"));

    // The field is optional and defaults to "*".
    let (side, reason) = match str_at(&json, &["environment"]) {
        None => (
            Side::Both,
            "fabric.mod.json: environment omitted (defaults to \"*\")",
        ),
        Some("*") => (Side::Both, "fabric.mod.json: environment = \"*\""),
        Some("client") => (Side::Client, "fabric.mod.json: environment = \"client\""),
        Some("server") => (Side::Server, "fabric.mod.json: environment = \"server\""),
        Some(_) => (
            Side::Unknown,
            "fabric.mod.json: unrecognised environment value",
        ),
    };
    Finding {
        icon,
        loader: LoaderKind::Fabric,
        mods: vec![meta],
        confidence: if side == Side::Unknown {
            Confidence::None
        } else {
            Confidence::Declared
        },
        side,
        reason: reason.to_owned(),
    }
}

pub(crate) fn parse_quilt(text: &str) -> Finding {
    let Some(json) = parse_lenient_json(text) else {
        return unparsable(LoaderKind::Quilt, "quilt.mod.json");
    };
    let id = str_at(&json, &["quilt_loader", "id"])
        .unwrap_or_default()
        .to_owned();
    let meta = ModMeta {
        name: str_at(&json, &["quilt_loader", "metadata", "name"])
            .unwrap_or(&id)
            .to_owned(),
        version: str_at(&json, &["quilt_loader", "version"])
            .unwrap_or_default()
            .to_owned(),
        description: tidy(
            str_at(&json, &["quilt_loader", "metadata", "description"]).unwrap_or_default(),
        ),
        depends: required(id_list(
            json.get("quilt_loader").and_then(|q| q.get("depends")),
        )),
        provides: id_list(json.get("quilt_loader").and_then(|q| q.get("provides"))),
        id,
    };
    let icon = icon_of(
        json.get("quilt_loader")
            .and_then(|q| q.get("metadata"))
            .and_then(|m| m.get("icon")),
    );

    let (side, reason) = match str_at(&json, &["minecraft", "environment"]) {
        None => (
            Side::Both,
            "quilt.mod.json: minecraft.environment omitted (defaults to \"*\")",
        ),
        Some("*") => (Side::Both, "quilt.mod.json: minecraft.environment = \"*\""),
        Some("client") => (
            Side::Client,
            "quilt.mod.json: minecraft.environment = \"client\"",
        ),
        Some("dedicated_server") => (
            Side::Server,
            "quilt.mod.json: minecraft.environment = \"dedicated_server\"",
        ),
        Some(_) => (
            Side::Unknown,
            "quilt.mod.json: unrecognised minecraft.environment value",
        ),
    };
    Finding {
        icon,
        loader: LoaderKind::Quilt,
        mods: vec![meta],
        confidence: if side == Side::Unknown {
            Confidence::None
        } else {
            Confidence::Declared
        },
        side,
        reason: reason.to_owned(),
    }
}
