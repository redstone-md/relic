//! The main screen, as a board of tiles: the chosen server's scene, the
//! servers beside it, and along the bottom the news, the player and PLAY.

use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Hsla, SharedString, Window, div, img,
    px,
};
use relic_proto::{FileKind, InstanceInfo, InstanceStatus, LoaderKind};

use super::app::{Launcher, PlayState, Screen};
use super::icon::{AnimatedIcon, IconName};
use crate::lang::{self, Msg, date, t, tf};

use super::widgets::{advance, bar, board, hover_beat, pixel, surface, tile};

/// How wide the column of servers is, and PLAY under it.
const SIDE: f32 = 340.;

pub(super) fn loader_name(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Vanilla => "Vanilla",
        LoaderKind::Fabric => "Fabric",
        LoaderKind::Quilt => "Quilt",
        LoaderKind::Forge => "Forge",
        LoaderKind::NeoForge => "NeoForge",
    }
}

fn status_label(status: InstanceStatus) -> &'static str {
    match status {
        InstanceStatus::Running => t(Msg::Online),
        InstanceStatus::Starting => t(Msg::Starting),
        InstanceStatus::Installing => t(Msg::Installing),
        InstanceStatus::Stopping => t(Msg::Stopping),
        InstanceStatus::Stopped => t(Msg::Offline),
        InstanceStatus::Crashed => t(Msg::Down),
    }
}

/// What the game is, in words: `Minecraft 1.21.1 with Fabric`.
fn game_line(instance: &InstanceInfo) -> String {
    match instance.loader.kind {
        LoaderKind::Vanilla => format!("Minecraft {}", instance.minecraft_version),
        kind => tf(
            Msg::GameWithLoader,
            &[&instance.minecraft_version, &loader_name(kind)],
        ),
    }
}

/// Whether the server is up, and how busy.
fn presence(instance: &InstanceInfo) -> String {
    match instance.status {
        InstanceStatus::Running => tf(
            Msg::OnlinePlaying,
            &[&instance.players_online, &instance.players_max],
        ),
        status => status_label(status).to_owned(),
    }
}

/// A small heading with an icon that plays as the section appears.
pub(super) fn heading(
    id: &'static str,
    icon: IconName,
    title: &'static str,
    cx: &App,
) -> AnyElement {
    let text = cx.theme().colors.foreground;
    div()
        .h_flex()
        .items_center()
        .gap_2()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .child(AnimatedIcon::new(id, icon, text).size(px(16.)).play(1))
        .child(title)
        .into_any_element()
}

/// The quiet line that says what a tile holds.
pub(super) fn label(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().colors.muted_foreground)
        .child(text.into())
}

impl Launcher {
    fn status_color(&self, status: InstanceStatus, cx: &App) -> Hsla {
        let c = &cx.theme().colors;
        match status {
            InstanceStatus::Running => c.success,
            InstanceStatus::Crashed => c.danger,
            InstanceStatus::Stopped => c.muted_foreground,
            _ => c.warning,
        }
    }

    fn instance_row(
        &self,
        ix: usize,
        instance: &InstanceInfo,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = self.status_color(instance.status, cx);
        let c = &cx.theme().colors;
        let (accent, ink, dim, hover) = (
            c.primary,
            c.primary_foreground,
            c.muted_foreground,
            c.secondary,
        );
        let instance_id = instance.id.clone();
        let state = if instance.status == InstanceStatus::Running {
            format!("{}/{}", instance.players_online, instance.players_max)
        } else {
            status_label(instance.status).to_owned()
        };

        div()
            .id(("instance", ix))
            .h_flex()
            .flex_none()
            .items_center()
            .gap_3()
            .h(px(48.))
            .px_4()
            .cursor_pointer()
            // The chosen one is the one PLAY starts.
            .when(selected, |el| el.bg(accent).text_color(ink))
            .when(!selected, |el| el.hover(move |style| style.bg(hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select(instance_id.clone(), window, cx);
            }))
            .child(
                div()
                    .flex_none()
                    .size(px(8.))
                    .bg(if selected { ink } else { status }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .child(instance.name.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(if selected { ink } else { dim })
                    .child(state),
            )
            .into_any_element()
    }

    fn servers_tile(&self, cx: &mut Context<Self>) -> AnyElement {
        let handshake = self.handshake.clone();
        let selected = self.selected().map(|i| i.id.clone());
        let instances = handshake
            .as_ref()
            .map(|h| h.instances.as_slice())
            .unwrap_or_default();

        let mut list = div()
            .id("servers")
            .v_flex()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        if instances.is_empty() {
            list = list.child(label(t(Msg::NothingToPlay), cx).px_4().text_sm());
        }
        for (ix, instance) in instances.iter().enumerate() {
            let is_selected = selected.as_deref() == Some(instance.id.as_str());
            list = list.child(self.instance_row(ix, instance, is_selected, cx));
        }
        tile(cx)
            .v_flex()
            .flex_1()
            .min_h_0()
            .child(label(t(Msg::Servers), cx).flex_none().px_4().pt_4().pb_2())
            .child(list)
            .into_any_element()
    }

    fn links_tile(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let handshake = self.handshake.clone()?;
        if handshake.links.is_empty() {
            return None;
        }
        let c = &cx.theme().colors;
        let (dim, text, hover) = (c.muted_foreground, c.foreground, c.secondary);
        let mut list = tile(cx).v_flex().flex_none();
        for (ix, link) in handshake.links.iter().enumerate() {
            let id = ElementId::from(("link", ix));
            let beat = hover_beat(&id, window, cx);
            let url = link.url.clone();
            list = list.child(
                div()
                    .id(id.clone())
                    .h_flex()
                    .flex_none()
                    .items_center()
                    .gap_3()
                    .h(px(40.))
                    .px_4()
                    .text_color(dim)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover).text_color(text))
                    .on_hover(advance(&beat))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_link(&url, cx);
                    }))
                    .child(
                        AnimatedIcon::new(
                            (id, "icon"),
                            IconName::for_link(link.icon.as_deref()),
                            dim,
                        )
                        .size(px(16.))
                        .play(*beat.read(cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(link.label.clone()),
                    ),
            );
        }
        Some(list.into_any_element())
    }

    /// Two small tiles of numbers: who is on, and what the pack is.
    fn facts(&self, cx: &Context<Self>) -> AnyElement {
        let selected = self.selected();
        let fact = |name: &'static str, value: String, note: Option<String>| {
            // The pixel face is wide: long words drop a size to stay inside.
            let size = if value.chars().count() > 8 { 8. } else { 16. };
            tile(cx)
                .v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .justify_between()
                .p_4()
                .child(label(name, cx))
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(pixel(value, size))
                        .children(note.map(|note| label(note, cx).truncate())),
                )
        };
        let (playing, note) = match selected {
            Some(i) if i.status == InstanceStatus::Running => (
                format!("{}/{}", i.players_online, i.players_max),
                Some(t(Msg::PlayingNow).to_owned()),
            ),
            Some(i) => (status_label(i.status).to_uppercase(), None),
            None => ("-".to_owned(), None),
        };
        let (version, pack) = match selected {
            Some(i) => {
                let mods = self.manifest.as_ref().map(|manifest| {
                    manifest
                        .files
                        .iter()
                        .filter(|file| file.kind == FileKind::Mod)
                        .count()
                });
                let loader = loader_name(i.loader.kind);
                let pack = match mods {
                    Some(n) if n > 0 => format!("{loader}, {}", lang::mods(n)),
                    _ => loader.to_owned(),
                };
                (i.minecraft_version.clone(), Some(pack))
            }
            None => ("-".to_owned(), None),
        };
        board(cx)
            .h_flex()
            .flex_none()
            .h(px(120.))
            .child(fact(t(Msg::Server), playing, note))
            .child(fact("Minecraft", version, pack))
            .into_any_element()
    }

    /// The chosen server's picture, with its name on a plate in the corner.
    fn scene_tile(&self, cx: &Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (dim, background) = (c.muted_foreground, surface(cx));
        let mut plate = div()
            .absolute()
            .left_4()
            .bottom_4()
            .max_w(px(460.))
            .v_flex()
            .gap_1()
            .p_4()
            .bg(background);
        match self.selected() {
            Some(instance) => {
                plate = plate
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(8.))
                                    .bg(self.status_color(instance.status, cx)),
                            )
                            .child(presence(instance)),
                    )
                    .child(pixel(instance.name.clone(), 24.))
                    .child(div().text_color(dim).child(game_line(instance)));
                if !instance.description.is_empty() {
                    plate = plate.child(div().pt_1().child(instance.description.clone()));
                }
            }
            None => {
                let server_name = self
                    .handshake
                    .as_ref()
                    .map(|h| h.server_name.clone())
                    .unwrap_or_default();
                plate = plate
                    .child(pixel(server_name, 24.))
                    .child(div().text_color(dim).child(t(Msg::NothingToPlay)));
            }
        }
        tile(cx)
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(self.scene()),
            )
            .child(plate)
            .into_any_element()
    }

    /// The latest news, one line of it. The rest is a click away.
    fn news_tile(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (dim, hover) = (c.muted_foreground, c.secondary);
        let id = ElementId::from("news-tile");
        let beat = hover_beat(&id, window, cx);
        let base = tile(cx)
            .id(id)
            .h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .gap_4()
            .px_5();
        let Some(latest) = self.news.first() else {
            return base
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(label(t(Msg::News), cx))
                        .child(div().text_color(dim).child(t(Msg::NoNews))),
                )
                .into_any_element();
        };
        let latest_id = latest.id;
        base.cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_hover(advance(&beat))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.read_news(Some(latest_id), cx);
            }))
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(label(tf(Msg::NewsDated, &[&date(latest.published_at)]), cx))
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(latest.title.clone()),
                    ),
            )
            .child(
                AnimatedIcon::new("news-open", IconName::ChevronRight, dim)
                    .size(px(20.))
                    .play(*beat.read(cx)),
            )
            .into_any_element()
    }

    /// Who is playing. Their skin is a click away.
    fn me_tile(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (dim, border, hover) = (c.muted_foreground, c.border, c.secondary);
        let name: SharedString = self
            .account
            .as_ref()
            .map(|a| a.username.clone())
            .unwrap_or_default()
            .into();
        let face = match &self.head {
            Some(head) => img(head.clone())
                .flex_none()
                .size(px(48.))
                .into_any_element(),
            None => div()
                .h_flex()
                .flex_none()
                .size(px(48.))
                .items_center()
                .justify_center()
                .bg(border)
                .child(AnimatedIcon::new("face", IconName::User, dim).size(px(24.)))
                .into_any_element(),
        };
        tile(cx)
            .id("me")
            .h_flex()
            .flex_none()
            .w(px(220.))
            .h_full()
            .items_center()
            .gap_3()
            .px_4()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.go(Screen::Skin, cx);
            }))
            .child(face)
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().font_weight(FontWeight::MEDIUM).truncate().child(name))
                    .child(label(t(Msg::ChangeSkin), cx)),
            )
            .into_any_element()
    }

    fn settings_tile(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (text, hover) = (c.foreground, c.secondary);
        let id = ElementId::from("settings");
        let beat = hover_beat(&id, window, cx);
        tile(cx)
            .id(id)
            .h_flex()
            .flex_none()
            .w(px(96.))
            .h_full()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_hover(advance(&beat))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.go(Screen::Settings, cx);
            }))
            .child(
                AnimatedIcon::new("settings-icon", IconName::Settings, text)
                    .size(px(28.))
                    .play(*beat.read(cx)),
            )
            .into_any_element()
    }

    /// PLAY, and while the game is being set up, how far along that is.
    fn play_tile(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (accent, ink, lit, background, border, dim, danger) = (
            c.primary,
            c.primary_foreground,
            c.primary_hover,
            surface(cx),
            c.border,
            c.muted_foreground,
            c.danger,
        );
        let ready = self.selected().is_some();
        let base = div()
            .id("play")
            .flex_none()
            .w(px(SIDE))
            .h_full()
            .overflow_hidden();
        let start = cx.listener(|this, _: &ClickEvent, window, cx| this.play(window, cx));
        match &self.play {
            PlayState::Idle if ready => base
                .h_flex()
                .items_center()
                .justify_center()
                .bg(accent)
                .text_color(ink)
                .cursor_pointer()
                .hover(move |style| style.bg(lit))
                .active(|style| style.opacity(0.85))
                .on_click(start)
                .child(pixel(t(Msg::Play), 32.))
                .into_any_element(),
            PlayState::Idle => base
                .h_flex()
                .items_center()
                .justify_center()
                .bg(border)
                .text_color(dim)
                .child(pixel(t(Msg::Play), 32.))
                .into_any_element(),
            PlayState::Working { label, progress } => base
                .v_flex()
                .justify_center()
                .gap_3()
                .px_5()
                .bg(background)
                .child(div().truncate().child(label.clone()))
                .child(bar("play-progress", *progress, accent, cx))
                .into_any_element(),
            PlayState::Running => base
                .h_flex()
                .items_center()
                .justify_center()
                .bg(background)
                .text_color(accent)
                .child(pixel(t(Msg::Playing), 16.))
                .into_any_element(),
            PlayState::Failed(reason) => base
                .v_flex()
                .justify_center()
                .gap_2()
                .px_4()
                .bg(background)
                .cursor_pointer()
                .hover(move |style| style.bg(border))
                .on_click(start)
                .child(
                    div()
                        .text_xs()
                        .text_color(danger)
                        .line_clamp(2)
                        .child(reason.clone()),
                )
                .child(pixel(t(Msg::TryAgain), 8.).text_color(accent))
                .into_any_element(),
        }
    }

    pub(super) fn home_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        board(cx)
            .v_flex()
            .size_full()
            .child(
                board(cx)
                    .h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.scene_tile(cx))
                    .child(
                        board(cx)
                            .v_flex()
                            .flex_none()
                            .w(px(SIDE))
                            .h_full()
                            .child(self.servers_tile(cx))
                            .children(self.links_tile(window, cx))
                            .child(self.facts(cx)),
                    ),
            )
            .child(
                board(cx)
                    .h_flex()
                    .flex_none()
                    .h(px(96.))
                    .child(self.news_tile(window, cx))
                    .child(self.me_tile(cx))
                    .child(self.settings_tile(window, cx))
                    .child(self.play_tile(cx)),
            )
            .into_any_element()
    }
}
