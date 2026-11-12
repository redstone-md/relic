//! Launcher version documents (`versions/<id>/<id>.json`): the format Mojang
//! publishes and every mod loader extends. Parsing, inheritance, the
//! OS rules, and argument templates.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mojang::{Download, JavaVersion};
use crate::{Error, Result};

const DEFAULT_LIBRARY_REPO: &str = "https://libraries.minecraft.net/";

/// The operating system as version documents name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Os {
    /// `windows`, `osx` or `linux`.
    pub name: &'static str,
    /// `x86`, `x86_64` or `arm64`.
    pub arch: &'static str,
}

impl Os {
    pub fn current() -> Self {
        Self {
            name: match std::env::consts::OS {
                "windows" => "windows",
                "macos" => "osx",
                _ => "linux",
            },
            arch: match std::env::consts::ARCH {
                "x86" => "x86",
                "aarch64" => "arm64",
                _ => "x86_64",
            },
        }
    }

    pub fn classpath_separator(self) -> &'static str {
        if self.name == "windows" { ";" } else { ":" }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// `allow` or `disallow`.
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    /// Launcher features (demo mode, custom resolution, quick play). Relic
    /// offers none of them through rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

impl Rule {
    fn matches(&self, os: Os) -> bool {
        if let Some(features) = &self.features
            && features.values().any(|wanted| *wanted)
        {
            return false;
        }
        match &self.os {
            None => true,
            Some(rule) => {
                rule.name.as_deref().is_none_or(|name| name == os.name)
                    && rule.arch.as_deref().is_none_or(|arch| arch == os.arch)
            }
        }
    }
}

/// Whether something guarded by `rules` applies here. No rules means yes;
/// otherwise the last rule that matches decides, and no match means no.
pub fn allowed(rules: &[Rule], os: Os) -> bool {
    if rules.is_empty() {
        return true;
    }
    rules
        .iter()
        .rev()
        .find(|rule| rule.matches(os))
        .is_some_and(|rule| rule.action == "allow")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    /// Path under the libraries directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Empty for files an installer produces locally.
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    /// Maven coordinates: `group:artifact:version[:classifier][@ext]`.
    pub name: String,
    /// Repository base, in the short form Fabric and Quilt use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,
    /// Pre-1.19 native bundles: OS name to classifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
}

/// One file a library contributes, and where to get it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFile {
    /// Path under the libraries directory, forward slashes.
    pub path: String,
    /// `None` when the file can only come from an installer.
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl Library {
    pub fn applies(&self, os: Os) -> bool {
        allowed(self.rules.as_deref().unwrap_or_default(), os)
    }

    /// `group:artifact[:classifier]`: what makes two entries the same
    /// library in different versions.
    pub fn key(&self) -> String {
        let name = self.name.split('@').next().unwrap_or(&self.name);
        let mut parts = name.split(':');
        let group = parts.next().unwrap_or_default();
        let artifact = parts.next().unwrap_or_default();
        let _version = parts.next();
        match parts.next() {
            Some(classifier) => format!("{group}:{artifact}:{classifier}"),
            None => format!("{group}:{artifact}"),
        }
    }

    fn file_of(&self, artifact: &Artifact, fallback_path: Result<String>) -> Result<LibraryFile> {
        let path = match &artifact.path {
            Some(path) => path.clone(),
            None => fallback_path?,
        };
        Ok(LibraryFile {
            path,
            url: (!artifact.url.is_empty()).then(|| artifact.url.clone()),
            sha1: artifact.sha1.clone(),
            size: artifact.size,
        })
    }

    /// The JAR that goes on the classpath, if this library has one.
    pub fn artifact(&self) -> Result<Option<LibraryFile>> {
        if let Some(downloads) = &self.downloads {
            return match &downloads.artifact {
                Some(artifact) => self.file_of(artifact, maven_path(&self.name)).map(Some),
                // Only native bundles.
                None => Ok(None),
            };
        }
        if self.natives.is_some() {
            return Ok(None);
        }
        let path = maven_path(&self.name)?;
        let base = self.url.as_deref().unwrap_or(DEFAULT_LIBRARY_REPO);
        let base = base.trim_end_matches('/');
        Ok(Some(LibraryFile {
            url: Some(format!("{base}/{path}")),
            path,
            sha1: self.sha1.clone(),
            size: self.size,
        }))
    }

    /// The bundle of native libraries to unpack for `os`, if any.
    pub fn native(&self, os: Os) -> Result<Option<LibraryFile>> {
        let Some(classifier) = self.natives.as_ref().and_then(|n| n.get(os.name)) else {
            return Ok(None);
        };
        let bits = if os.arch == "x86" { "32" } else { "64" };
        let classifier = classifier.replace("${arch}", bits);
        let found = self
            .downloads
            .as_ref()
            .and_then(|d| d.classifiers.as_ref())
            .and_then(|c| c.get(&classifier));
        match found {
            Some(artifact) => self
                .file_of(artifact, maven_path(&format!("{}:{classifier}", self.name)))
                .map(Some),
            None => Ok(None),
        }
    }
}

/// Repository-relative path of Maven coordinates
/// (`group:artifact:version[:classifier][@ext]`).
pub fn maven_path(coordinates: &str) -> Result<String> {
    let bad = || Error::Decode {
        url: coordinates.to_owned(),
        message: "not Maven coordinates".into(),
    };
    let (name, extension) = coordinates.split_once('@').unwrap_or((coordinates, "jar"));
    let parts: Vec<&str> = name.split(':').collect();
    let (group, artifact, version, classifier) = match parts.as_slice() {
        [group, artifact, version] => (group, artifact, version, None),
        [group, artifact, version, classifier] => (group, artifact, version, Some(classifier)),
        _ => return Err(bad()),
    };
    // Coordinates come from documents on the network and become file paths.
    let unsafe_part =
        |part: &str| part.is_empty() || part.contains(['/', '\\']) || part == "." || part == "..";
    if parts.iter().any(|part| unsafe_part(part))
        || unsafe_part(extension)
        || group.split('.').any(unsafe_part)
    {
        return Err(bad());
    }
    let file = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{extension}"),
        None => format!("{artifact}-{version}.{extension}"),
    };
    Ok(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

/// One entry of `arguments.game` or `arguments.jvm`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Guarded { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

/// The argument templates that apply on `os`, in order.
pub fn select(arguments: &[Argument], os: Os) -> Vec<String> {
    let mut out = Vec::new();
    for argument in arguments {
        match argument {
            Argument::Plain(value) => out.push(value.clone()),
            Argument::Guarded { rules, value } if allowed(rules, os) => match value {
                OneOrMany::One(value) => out.push(value.clone()),
                OneOrMany::Many(values) => out.extend(values.iter().cloned()),
            },
            Argument::Guarded { .. } => {}
        }
    }
    out
}

/// Fills `${name}` placeholders. Unknown ones are left as they are, which
/// is what the official launcher does.
pub fn substitute(template: &str, values: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                match values.get(&after[..end]) {
                    Some(value) => out.push_str(value),
                    None => out.push_str(&rest[start..start + 2 + end + 1]),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggingClient {
    /// JVM argument template with a `${path}` placeholder.
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<Download>,
}

/// A version document. Fields this crate has no use for are kept in `rest`
/// so a document written back to disk loses nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    #[serde(
        rename = "inheritsFrom",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub inherits_from: Option<String>,
    #[serde(rename = "mainClass", default, skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    /// The pre-1.13 form of game arguments: one space-separated string.
    #[serde(
        rename = "minecraftArguments",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub minecraft_arguments: Option<String>,
    #[serde(
        rename = "assetIndex",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<ProfileDownloads>,
    #[serde(
        rename = "javaVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub java_version: Option<JavaVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,
    #[serde(flatten)]
    pub rest: serde_json::Map<String, Value>,
}

impl Profile {
    /// Java major version to run with. Versions published before Mojang
    /// recorded the requirement ran on Java 8.
    pub fn java_major(&self) -> u32 {
        self.java_version.as_ref().map_or(8, |j| j.major_version)
    }

    /// Lays this document over the one it inherits from, the way the
    /// official launcher does: its libraries come first and replace the
    /// parent's copies of the same library, its arguments are appended, and
    /// anything it does not say is taken from the parent.
    pub fn over(self, parent: Profile) -> Profile {
        let mut libraries = self.libraries;
        let mine: std::collections::HashSet<String> = libraries.iter().map(Library::key).collect();
        libraries.extend(
            parent
                .libraries
                .into_iter()
                .filter(|library| !mine.contains(&library.key())),
        );

        let arguments = match (parent.arguments, self.arguments) {
            (Some(mut base), Some(more)) => {
                base.game.extend(more.game);
                base.jvm.extend(more.jvm);
                Some(base)
            }
            (base, more) => more.or(base),
        };

        let mut rest = parent.rest;
        rest.extend(self.rest);
        Profile {
            id: self.id,
            inherits_from: None,
            main_class: self.main_class.or(parent.main_class),
            kind: self.kind.or(parent.kind),
            libraries,
            arguments,
            minecraft_arguments: self.minecraft_arguments.or(parent.minecraft_arguments),
            asset_index: self.asset_index.or(parent.asset_index),
            downloads: self.downloads.or(parent.downloads),
            java_version: self.java_version.or(parent.java_version),
            logging: self.logging.or(parent.logging),
            rest,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: Os = Os {
        name: "windows",
        arch: "x86_64",
    };
    const MAC: Os = Os {
        name: "osx",
        arch: "arm64",
    };

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn last_matching_rule_decides() {
        assert!(allowed(&[], WINDOWS));
        let only_mac = rules(r#"[{"action":"allow","os":{"name":"osx"}}]"#);
        assert!(allowed(&only_mac, MAC));
        assert!(!allowed(&only_mac, WINDOWS));

        let not_mac = rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#);
        assert!(allowed(&not_mac, WINDOWS));
        assert!(!allowed(&not_mac, MAC));

        let demo = rules(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
        assert!(!allowed(&demo, WINDOWS));
    }

    #[test]
    fn maven_coordinates_become_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.5").unwrap(),
            "net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip").unwrap(),
            "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
        );
        assert!(maven_path("just-a-name").is_err());
        assert!(maven_path("a:b:../../../evil").is_err());
        assert!(maven_path("a..b:c:1").is_err());
    }

    #[test]
    fn libraries_resolve_in_all_three_dialects() {
        // Mojang: explicit download.
        let mojang: Library = serde_json::from_str(
            r#"{"name":"com.mojang:brigadier:1.3.10","downloads":{"artifact":{
                "path":"com/mojang/brigadier/1.3.10/brigadier-1.3.10.jar",
                "url":"https://libraries.minecraft.net/x.jar","sha1":"ab","size":3}}}"#,
        )
        .unwrap();
        let file = mojang.artifact().unwrap().unwrap();
        assert_eq!(
            file.url.as_deref(),
            Some("https://libraries.minecraft.net/x.jar")
        );
        assert_eq!(file.sha1.as_deref(), Some("ab"));

        // Fabric: coordinates and a repository.
        let fabric: Library = serde_json::from_str(
            r#"{"name":"net.fabricmc:intermediary:1.21.1","url":"https://maven.fabricmc.net/"}"#,
        )
        .unwrap();
        assert_eq!(
            fabric.artifact().unwrap().unwrap().url.as_deref(),
            Some(
                "https://maven.fabricmc.net/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar"
            )
        );

        // Forge: produced by the installer, nowhere to download it from.
        let forge: Library = serde_json::from_str(
            r#"{"name":"net.minecraftforge:forge:1.20.1-47.2.0:client","downloads":{"artifact":{
                "path":"net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-client.jar",
                "url":"","sha1":"cd","size":9}}}"#,
        )
        .unwrap();
        assert_eq!(forge.artifact().unwrap().unwrap().url, None);
        assert_eq!(forge.key(), "net.minecraftforge:forge:client");
    }

    #[test]
    fn legacy_natives_pick_the_classifier_for_the_os() {
        let library: Library = serde_json::from_str(
            r#"{"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209",
                "natives":{"windows":"natives-windows-${arch}","osx":"natives-osx"},
                "extract":{"exclude":["META-INF/"]},
                "downloads":{"classifiers":{
                    "natives-windows-64":{"path":"p/win64.jar","url":"https://example.invalid/w","sha1":"aa","size":1},
                    "natives-osx":{"path":"p/osx.jar","url":"https://example.invalid/o","sha1":"bb","size":1}}}}"#,
        )
        .unwrap();
        assert_eq!(library.artifact().unwrap(), None);
        assert_eq!(
            library.native(WINDOWS).unwrap().unwrap().path,
            "p/win64.jar"
        );
        assert_eq!(library.native(MAC).unwrap().unwrap().path, "p/osx.jar");
        let linux = Os {
            name: "linux",
            arch: "x86_64",
        };
        assert_eq!(library.native(linux).unwrap(), None);
    }

    #[test]
    fn arguments_follow_rules_and_fill_placeholders() {
        let arguments: Vec<Argument> = serde_json::from_str(
            r#"["--username","${auth_player_name}",
                {"rules":[{"action":"allow","os":{"name":"osx"}}],"value":["-XstartOnFirstThread"]},
                {"rules":[{"action":"allow","os":{"name":"windows"}}],"value":"-Dwin=1"},
                {"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"}]"#,
        )
        .unwrap();
        assert_eq!(
            select(&arguments, WINDOWS),
            ["--username", "${auth_player_name}", "-Dwin=1"]
        );
        assert_eq!(
            select(&arguments, MAC),
            ["--username", "${auth_player_name}", "-XstartOnFirstThread"]
        );

        let values = HashMap::from([("auth_player_name", "Steve".to_owned())]);
        assert_eq!(substitute("${auth_player_name}", &values), "Steve");
        assert_eq!(
            substitute("-Dname=${auth_player_name}:${unknown}", &values),
            "-Dname=Steve:${unknown}"
        );
        assert_eq!(substitute("broken ${tail", &values), "broken ${tail");
    }

    #[test]
    fn a_loader_profile_lays_over_the_game() {
        let game: Profile = serde_json::from_str(
            r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","type":"release",
                "assetIndex":{"id":"17","sha1":"aa","url":"https://example.invalid/17.json"},
                "arguments":{"game":["--version","${version_name}"],"jvm":["-cp","${classpath}"]},
                "libraries":[{"name":"org.ow2.asm:asm:9.6"},{"name":"com.mojang:brigadier:1.3.10"}]}"#,
        )
        .unwrap();
        let loader: Profile = serde_json::from_str(
            r#"{"id":"fabric-loader-0.16.5-1.21.1","inheritsFrom":"1.21.1",
                "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
                "arguments":{"game":[],"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main "]},
                "libraries":[{"name":"org.ow2.asm:asm:9.7","url":"https://maven.fabricmc.net/"}]}"#,
        )
        .unwrap();
        let merged = loader.over(game);
        assert_eq!(merged.id, "fabric-loader-0.16.5-1.21.1");
        assert_eq!(
            merged.main_class.as_deref(),
            Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
        );
        let names: Vec<&str> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            ["org.ow2.asm:asm:9.7", "com.mojang:brigadier:1.3.10"]
        );
        let arguments = merged.arguments.unwrap();
        assert_eq!(arguments.jvm.len(), 3);
        assert_eq!(arguments.game.len(), 2);
        assert_eq!(merged.asset_index.unwrap().id, "17");
        assert_eq!(merged.kind.as_deref(), Some("release"));
    }
}
