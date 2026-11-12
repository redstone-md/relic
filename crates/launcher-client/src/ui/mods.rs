//! The extras a server offers, as a catalog: each mod by its own name and
//! picture, with what it brings along said in words. Whatever a mod cannot
//! load without is installed with it, so no choice here breaks the pack.

use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ElementId, FontWeight, ObjectFit, SharedString, Window, div, img, px};
use relic_proto::{Manifest, ManifestFile};

use super::app::{Launcher, Screen};
use super::home::label;
use super::icon::{AnimatedIcon, IconName};
use super::widgets::{FRAME, Toggle, board, tile};
use crate::backend::sync;
use crate::lang::{Msg, t, tf};

/// How many mods stand side by side.
const COLUMNS: usize = 2;

/// The last part of a pack path, without its extension: what a mod is
/// called when it does not say.
pub(super) fn file_title(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".jar").unwrap_or(name)
}

fn title(file: &ManifestFile) -> String {
    match file.name.trim() {
        "" => file_title(&file.path).to_owned(),
        name => name.to_owned(),
    }
}

/// Why a mod is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum State {
    Off,
    /// The player switched it on.
    Picked,
    /// Other mods the player picked cannot load without it.
    NeededBy(Vec<String>),
    /// The pack's own files cannot load without it.
    NeededByPack,
}

/// One optional file of the pack, ready to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Card {
    pub path: String,
    pub title: String,
    pub description: String,
    pub icon: Option<String>,
    pub state: State,
    /// Names of the optional files that come along with this one.
    pub brings: Vec<String>,
}

impl Card {
    pub fn on(&self) -> bool {
        self.state != State::Off
    }
}

/// Every optional file `path` pulls in, itself left out.
fn closure<'a>(path: &'a str, optional: &BTreeMap<&'a str, &'a ManifestFile>) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![path];
    while let Some(at) = queue.pop() {
        for needed in optional.get(at).into_iter().flat_map(|file| &file.requires) {
            if let Some((&key, _)) = optional.get_key_value(needed.as_str())
                && key != path
                && seen.insert(key)
            {
                queue.push(key);
            }
        }
    }
    seen
}

/// The catalog of a pack, given what the player switched on.
pub(super) fn catalog(manifest: &Manifest, picked: &BTreeSet<String>) -> Vec<Card> {
    let optional: BTreeMap<&str, &ManifestFile> = manifest
        .files
        .iter()
        .filter(|file| file.optional)
        .map(|file| (file.path.as_str(), file))
        .collect();
    let by_pack = sync::resolve(manifest, &BTreeSet::new());
    let pulls: BTreeMap<&str, BTreeSet<&str>> = optional
        .keys()
        .map(|&path| (path, closure(path, &optional)))
        .collect();
    let name = |path: &str| {
        optional
            .get(path)
            .map(|file| title(file))
            .unwrap_or_default()
    };

    let mut cards: Vec<Card> = optional
        .iter()
        .map(|(&path, file)| {
            let needed_by: Vec<String> = pulls
                .iter()
                .filter(|(other, pulled)| picked.contains(**other) && pulled.contains(path))
                .map(|(other, _)| name(other))
                .collect();
            let state = if by_pack.contains(path) {
                State::NeededByPack
            } else if !needed_by.is_empty() {
                State::NeededBy(needed_by)
            } else if picked.contains(path) {
                State::Picked
            } else {
                State::Off
            };
            Card {
                path: path.to_owned(),
                title: title(file),
                description: file.description.clone(),
                icon: file.icon.clone(),
                state,
                brings: pulls[path].iter().map(|other| name(other)).collect(),
            }
        })
        .collect();
    cards.sort_by_key(|card| card.title.to_lowercase());
    cards
}

impl Launcher {
    /// The catalog of the pack in view.
    pub(super) fn mod_cards(&self) -> Vec<Card> {
        let (Some(manifest), Some(instance)) = (&self.manifest, self.selected()) else {
            return Vec::new();
        };
        let none = BTreeSet::new();
        let picked = self.config.optional.get(&instance.id).unwrap_or(&none);
        catalog(manifest, picked)
    }

    fn mod_card(&self, ix: usize, card: Card, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (dim, well, accent) = (c.muted_foreground, c.secondary, c.primary);
        let id = ElementId::from(("mod", ix));
        let picture = card
            .icon
            .as_deref()
            .and_then(|sha256| self.picture(&super::app::icon_key(sha256)));
        let face = div()
            .flex_none()
            .size(px(64.))
            .h_flex()
            .items_center()
            .justify_center()
            .bg(well)
            .map(|el| match picture {
                Some(picture) => el.child(img(picture).size_full().object_fit(ObjectFit::Contain)),
                None => el.child(
                    AnimatedIcon::new((id.clone(), "icon"), IconName::Puzzle, dim).size(px(24.)),
                ),
            });

        // One line under the description says what the choice drags along,
        // or why there is no choice.
        let note: Option<(SharedString, bool)> = match &card.state {
            State::NeededByPack => Some((t(Msg::NeededByPack).into(), true)),
            State::NeededBy(mods) => Some((tf(Msg::NeededBy, &[&mods.join(", ")]).into(), true)),
            _ if !card.brings.is_empty() => Some((
                tf(Msg::AlsoInstalls, &[&card.brings.join(", ")]).into(),
                false,
            )),
            _ => None,
        };
        let locked = matches!(card.state, State::NeededBy(_) | State::NeededByPack);
        let control = if locked {
            div()
                .flex_none()
                .w(px(44.))
                .h_flex()
                .justify_center()
                .child(
                    AnimatedIcon::new((id.clone(), "lock"), IconName::Lock, accent).size(px(16.)),
                )
                .into_any_element()
        } else {
            let path = card.path.clone();
            Toggle::new((id.clone(), "switch"))
                .checked(card.on())
                .on_change(cx.listener(move |this, on: &bool, _, cx| {
                    this.set_optional(&path, *on, cx);
                }))
                .into_any_element()
        };

        tile(cx)
            .h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .gap_4()
            .px_4()
            .child(face)
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(card.title),
                    )
                    .when(!card.description.is_empty(), |el| {
                        el.child(label(card.description, cx).line_clamp(2))
                    })
                    .when_some(note, |el, (note, strong)| {
                        el.child(
                            div()
                                .text_xs()
                                .truncate()
                                .text_color(if strong { accent } else { dim })
                                .child(note),
                        )
                    }),
            )
            .child(control)
            .into_any_element()
    }

    pub(super) fn mods_screen(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let cards = self.mod_cards();
        let mut grid = div()
            .id("mods")
            .v_flex()
            .flex_1()
            .min_h_0()
            .gap(FRAME)
            .overflow_y_scroll();
        let mut ix = 0;
        let mut cards = cards.into_iter().peekable();
        while cards.peek().is_some() {
            let mut row = div().h_flex().flex_none().w_full().h(px(112.)).gap(FRAME);
            for _ in 0..COLUMNS {
                row = match cards.next() {
                    Some(card) => {
                        ix += 1;
                        row.child(self.mod_card(ix, card, cx))
                    }
                    // A short last row still keeps the width of its tiles.
                    None => row.child(tile(cx).flex_1().min_w_0().h_full()),
                };
            }
            grid = grid.child(row);
        }
        // Whatever height the mods leave is one tile, with the one rule
        // worth knowing written in it.
        grid = grid.child(
            tile(cx)
                .flex_1()
                .w_full()
                .min_h(px(56.))
                .p_6()
                .child(label(t(Msg::ModsRule), cx)),
        );

        board(cx)
            .v_flex()
            .size_full()
            .child(self.header_to(t(Msg::OptionalMods), Screen::Settings, cx))
            .child(grid)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use relic_proto::{FileKind, JavaRuntime, Loader, LoaderKind, ModPolicy, SyncAction};

    use super::*;

    fn file(path: &str, name: &str, optional: bool, requires: &[&str]) -> ManifestFile {
        ManifestFile {
            path: path.to_owned(),
            sha256: "0".repeat(64),
            size: 1,
            kind: FileKind::Mod,
            action: SyncAction::Enforce,
            optional,
            name: name.to_owned(),
            description: String::new(),
            icon: None,
            requires: requires.iter().map(|path| (*path).to_owned()).collect(),
        }
    }

    fn pack(files: Vec<ManifestFile>) -> Manifest {
        Manifest {
            instance_id: "main".into(),
            build_id: uuid::Uuid::nil(),
            minecraft_version: "1.21.1".into(),
            loader: Loader {
                kind: LoaderKind::Fabric,
                version: "0.16.0".into(),
            },
            java_runtime: JavaRuntime {
                major_version: 21,
                vendor: "temurin".into(),
            },
            policy: ModPolicy::Strict,
            files,
            protected_paths: Vec::new(),
            locked_dirs: Vec::new(),
        }
    }

    fn picked(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|path| (*path).to_owned()).collect()
    }

    #[test]
    fn mods_are_named_after_their_file_when_they_do_not_say() {
        assert_eq!(file_title("mods/sodium-0.6.jar"), "sodium-0.6");
        assert_eq!(file_title("config/options.txt"), "options.txt");
        assert_eq!(file_title("loose.jar"), "loose");
        assert_eq!(title(&file("mods/a-1.0.jar", " ", true, &[])), "a-1.0");
        assert_eq!(
            title(&file("mods/a-1.0.jar", "Minimap", true, &[])),
            "Minimap"
        );
    }

    #[test]
    fn a_mod_says_what_it_brings_and_a_library_says_who_needs_it() {
        let manifest = pack(vec![
            file("mods/map.jar", "Minimap", true, &["mods/cloth.jar"]),
            file("mods/cloth.jar", "Cloth Config", true, &["mods/arch.jar"]),
            file("mods/arch.jar", "Architectury", true, &[]),
            file("mods/zoom.jar", "Zoom", true, &[]),
            file("mods/core.jar", "Core", false, &[]),
        ]);
        let cards = catalog(&manifest, &picked(&["mods/map.jar"]));
        let by_title = |title: &str| cards.iter().find(|card| card.title == title).unwrap();
        // Required files are not a choice.
        assert_eq!(cards.len(), 4);
        assert_eq!(by_title("Minimap").state, State::Picked);
        assert_eq!(by_title("Minimap").brings, ["Architectury", "Cloth Config"]);
        assert_eq!(
            by_title("Cloth Config").state,
            State::NeededBy(vec!["Minimap".into()])
        );
        assert_eq!(
            by_title("Architectury").state,
            State::NeededBy(vec!["Minimap".into()])
        );
        assert_eq!(by_title("Zoom").state, State::Off);

        // A library switched on by hand stays the player's choice.
        let cards = catalog(&manifest, &picked(&["mods/cloth.jar"]));
        let cloth = cards
            .iter()
            .find(|card| card.title == "Cloth Config")
            .unwrap();
        assert_eq!(cloth.state, State::Picked);
    }

    #[test]
    fn what_the_pack_itself_needs_cannot_be_switched_off() {
        let manifest = pack(vec![
            file("mods/core.jar", "Core", false, &["mods/lib.jar"]),
            file("mods/lib.jar", "Lib", true, &[]),
        ]);
        let cards = catalog(&manifest, &BTreeSet::new());
        assert_eq!(cards[0].state, State::NeededByPack);
        assert!(cards[0].on());
    }

    #[test]
    fn mods_that_need_each_other_do_not_loop() {
        let manifest = pack(vec![
            file("mods/a.jar", "A", true, &["mods/b.jar"]),
            file("mods/b.jar", "B", true, &["mods/a.jar"]),
        ]);
        let cards = catalog(&manifest, &picked(&["mods/a.jar"]));
        assert_eq!(cards[0].brings, ["B"]);
        assert_eq!(cards[1].state, State::NeededBy(vec!["A".into()]));
    }
}
