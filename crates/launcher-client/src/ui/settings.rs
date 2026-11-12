//! Settings: memory, optional mods, password, and the way out.

use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla, SharedString, Window, div, img, px,
    relative,
};

use super::app::{Launcher, MEMORY_RANGE_MB, Screen};
use super::home::{heading, label};
use super::icon::{AnimatedIcon, IconName};
use crate::lang::{self, Msg, t, tf};

use super::widgets::{Btn, FRAME, Field, advance, board, hover_beat, pixel, tile};

/// One block of the memory bar stands for this much.
const BLOCK_MB: u32 = 1024;

/// How wide the narrow column of tiles is.
const SIDE: f32 = 340.;

/// Megabytes the way players talk about them.
fn memory_label(mb: u32) -> String {
    if mb.is_multiple_of(1024) {
        tf(Msg::Gigabytes, &[&(mb / 1024)])
    } else {
        tf(Msg::Gigabytes, &[&format!("{:.1}", f64::from(mb) / 1024.)])
    }
}

/// How much of memory block `ix` is taken when the game gets `mb`.
fn block_share(ix: u32, mb: u32) -> f32 {
    (mb.saturating_sub(ix * BLOCK_MB) as f32 / BLOCK_MB as f32).clamp(0., 1.)
}

/// A line under a heading that says what the setting is for.
fn about(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(cx.theme().colors.muted_foreground)
        .child(text.into())
}

impl Launcher {
    /// Memory: a row of blocks, one per gigabyte. Click one to fill up to it.
    fn memory_tile(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (accent, empty, lit, dim) = (c.primary, c.border, c.foreground, c.muted_foreground);
        let mb = self.config.memory_mb;
        let (least, most) = MEMORY_RANGE_MB;
        let step = |id: &'static str, sign: &'static str, up: bool, at_end: bool| {
            Btn::new(id)
                .label(sign)
                .disabled(at_end)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.nudge_memory(up, cx);
                }))
        };

        let mut blocks = div().h_flex().w_full().h(px(40.)).gap(px(2.));
        for ix in 0..most / BLOCK_MB {
            let upto = (ix + 1) * BLOCK_MB;
            blocks = blocks.child(
                div()
                    .id(("memory-block", ix as usize))
                    .relative()
                    .flex_1()
                    .h_full()
                    .bg(empty)
                    .cursor_pointer()
                    .hover(move |style| style.bg(lit))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_memory(upto, cx);
                    }))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .h_full()
                            .w(relative(block_share(ix, mb)))
                            .bg(accent),
                    ),
            );
        }

        tile(cx)
            .v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .justify_between()
            .gap_4()
            .p_6()
            .child(
                div()
                    .v_flex()
                    .gap_1p5()
                    .child(heading(
                        "memory-heading",
                        IconName::Memory,
                        t(Msg::Memory),
                        cx,
                    ))
                    .child(about(t(Msg::MemoryHint), cx)),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(pixel(memory_label(mb), 32.)))
                    .child(step("memory-less", "-", false, mb <= least))
                    .child(step("memory-more", "+", true, mb >= most)),
            )
            .child(
                div().v_flex().gap_2().child(blocks).child(
                    div()
                        .h_flex()
                        .justify_between()
                        .text_xs()
                        .text_color(dim)
                        .child(memory_label(BLOCK_MB))
                        .child(t(Msg::BlockIsGigabyte))
                        .child(memory_label(most)),
                ),
            )
            .into_any_element()
    }

    /// The extras this server offers: how many are on. A click opens the
    /// catalog where they are picked.
    fn mods_tile(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (text, dim, hover) = (c.foreground, c.muted_foreground, c.secondary);
        let id = ElementId::from("mods");
        let beat = hover_beat(&id, window, cx);
        let cards = self.mod_cards();
        let any = !cards.is_empty();
        let state: SharedString = if any {
            let on = cards.iter().filter(|card| card.on()).count();
            format!("{on} {}", tf(Msg::ModsOn, &[&cards.len()])).into()
        } else {
            t(Msg::NoMods).into()
        };
        tile(cx)
            .id(id.clone())
            .h_flex()
            .flex_none()
            .h(px(56.))
            .items_center()
            .gap_2()
            .px_6()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .when(any, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.go(Screen::Mods, cx);
                    }))
            })
            .on_hover(advance(&beat))
            .child(AnimatedIcon::new((id, "icon"), IconName::Puzzle, text).play(*beat.read(cx)))
            .child(t(Msg::OptionalMods))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_right()
                    .text_color(dim)
                    .truncate()
                    .child(state),
            )
            .when(any, |row| {
                row.child(AnimatedIcon::new("mods-open", IconName::ChevronRight, dim))
            })
            .into_any_element()
    }

    /// The language in use. A click opens the list of them all.
    fn language_tile(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (text, dim, hover) = (c.foreground, c.muted_foreground, c.secondary);
        let id = ElementId::from("language");
        let beat = hover_beat(&id, window, cx);
        let chosen: SharedString = match self.language() {
            Some(lang) => lang.name().into(),
            None => tf(Msg::LanguageFollowsSystem, &[&lang::current().name()]).into(),
        };
        tile(cx)
            .id(id.clone())
            .h_flex()
            .flex_none()
            .h(px(56.))
            .items_center()
            .gap_2()
            .px_6()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_hover(advance(&beat))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.go(Screen::Language, cx);
            }))
            .child(AnimatedIcon::new((id, "icon"), IconName::Globe, text).play(*beat.read(cx)))
            .child(t(Msg::Language))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_right()
                    .text_color(dim)
                    .truncate()
                    .child(chosen),
            )
            .child(AnimatedIcon::new(
                "language-open",
                IconName::ChevronRight,
                dim,
            ))
            .into_any_element()
    }

    /// Where the chosen modpack is on disk, and the ways to see and move it.
    fn folder_tile(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let dim = cx.theme().colors.muted_foreground;
        let path: SharedString = self.pack_dir().display().to_string().into();
        let moved = self.config.game_dir.is_some();
        let playing = self.playing();
        tile(cx)
            .v_flex()
            .flex_1()
            .min_h_0()
            .gap_3()
            .p_6()
            .child(
                div()
                    .v_flex()
                    .gap_1p5()
                    .child(heading(
                        "folder-heading",
                        IconName::Server,
                        t(Msg::GameFolder),
                        cx,
                    ))
                    .child(about(t(Msg::GameFolderHint), cx)),
            )
            .child(
                div()
                    .id("folder-path")
                    .w_full()
                    .text_xs()
                    .text_color(dim)
                    .overflow_x_scroll()
                    .whitespace_nowrap()
                    .child(path),
            )
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Btn::new("open-folder")
                            .label(t(Msg::OpenFolder))
                            .icon(IconName::ExternalLink)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.open_folder(cx);
                            })),
                    )
                    .child(
                        Btn::new("change-folder")
                            .label(t(Msg::ChangeFolder))
                            .quiet()
                            .disabled(playing)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.choose_folder(window, cx);
                            })),
                    )
                    .when(moved, |row| {
                        row.child(
                            Btn::new("default-folder")
                                .label(t(Msg::DefaultFolder))
                                .quiet()
                                .disabled(playing)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.set_folder(None, cx);
                                })),
                        )
                    }),
            )
            .into_any_element()
    }

    fn password_tile(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let field = |field: Field| div().flex_1().min_w_0().child(field);
        tile(cx)
            .v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_4()
            .p_6()
            .child(
                div()
                    .v_flex()
                    .gap_1p5()
                    .child(heading(
                        "password-heading",
                        IconName::Lock,
                        t(Msg::Password),
                        cx,
                    ))
                    .child(about(t(Msg::PasswordHint), cx)),
            )
            .child(
                div()
                    .h_flex()
                    .items_start()
                    .gap_3()
                    .child(field(
                        Field::new(
                            "current-password",
                            t(Msg::CurrentPassword),
                            IconName::Lock,
                            &self.current_password,
                        )
                        .disabled(self.busy),
                    ))
                    .child(field(
                        Field::new(
                            "new-password",
                            t(Msg::NewPassword),
                            IconName::Key,
                            &self.new_password,
                        )
                        .disabled(self.busy),
                    )),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_4()
                    .child(
                        Btn::new("change-password")
                            .label(t(Msg::ChangePassword))
                            .icon(IconName::Key)
                            .loading(self.busy)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.change_password(window, cx);
                            })),
                    )
                    .child(div().flex_1().min_w_0().children(self.feedback(cx))),
            )
            .into_any_element()
    }

    /// Who is signed in and where, over the two ways out.
    fn account_tiles(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (dim, border, text, danger, hover) = (
            c.muted_foreground,
            c.border,
            c.foreground,
            c.danger,
            c.secondary,
        );
        let address = self
            .config
            .server
            .as_ref()
            .map(|server| server.address.clone())
            .unwrap_or_default();
        let name: SharedString = self
            .account
            .as_ref()
            .map(|account| account.username.clone())
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
                .child(AnimatedIcon::new("settings-face", IconName::User, dim).size(px(24.)))
                .into_any_element(),
        };

        let way_out = |id: &'static str,
                       icon: IconName,
                       title: &'static str,
                       color: Hsla,
                       window: &mut Window,
                       cx: &mut Context<Self>| {
            let id = ElementId::from(id);
            let beat = hover_beat(&id, window, cx);
            tile(cx)
                .id(id.clone())
                .h_flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .gap_2()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_hover(advance(&beat))
                .child(AnimatedIcon::new((id, "icon"), icon, color).play(*beat.read(cx)))
                .child(title)
        };
        let sign_out = way_out(
            "logout",
            IconName::LogOut,
            t(Msg::SignOut),
            danger,
            window,
            cx,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.logout(cx)));
        let other = way_out(
            "change-server",
            IconName::Globe,
            t(Msg::OtherServer),
            text,
            window,
            cx,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.change_server(cx)));

        div()
            .v_flex()
            .flex_none()
            .w(px(SIDE))
            .h_full()
            .gap(FRAME)
            .child(
                tile(cx)
                    .v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_4()
                    .p_6()
                    .child(heading(
                        "account-heading",
                        IconName::User,
                        t(Msg::Account),
                        cx,
                    ))
                    .child(
                        div().h_flex().items_center().gap_3().child(face).child(
                            div()
                                .v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(div().font_weight(FontWeight::MEDIUM).truncate().child(name))
                                .child(label(tf(Msg::OnServer, &[&address]), cx).truncate()),
                        ),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .flex_none()
                    .h(px(56.))
                    .gap(FRAME)
                    .child(sign_out)
                    .child(other),
            )
            .into_any_element()
    }

    pub(super) fn settings_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = || div().h_flex().w_full().gap(FRAME);
        board(cx)
            .v_flex()
            .size_full()
            .child(self.header(t(Msg::Settings), cx))
            .child(
                row().flex_1().min_h_0().child(self.memory_tile(cx)).child(
                    div()
                        .v_flex()
                        .flex_none()
                        .w(px(SIDE))
                        .h_full()
                        .gap(FRAME)
                        .child(self.mods_tile(window, cx))
                        .child(self.folder_tile(cx))
                        .child(self.language_tile(window, cx)),
                ),
            )
            .child(
                row()
                    .flex_none()
                    .h(px(248.))
                    .child(self.password_tile(cx))
                    .child(self.account_tiles(window, cx)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_reads_in_gigabytes() {
        assert_eq!(memory_label(4096), "4 GB");
        assert_eq!(memory_label(2560), "2.5 GB");
    }

    #[test]
    fn memory_fills_its_blocks_in_order() {
        assert_eq!(block_share(0, 2560), 1.);
        assert_eq!(block_share(1, 2560), 1.);
        assert_eq!(block_share(2, 2560), 0.5);
        assert_eq!(block_share(3, 2560), 0.);
    }
}
