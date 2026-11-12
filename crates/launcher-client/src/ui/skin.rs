//! The player's skin: the figure wearing it, and the way to change it.

use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, FontWeight, Window, div, px};

use super::app::Launcher;
use super::home::label;
use crate::lang::{Msg, t};

use super::widgets::{board, tile};

impl Launcher {
    /// Classic arms or slim ones: two halves of one frame.
    fn arms(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (accent, ink, hover) = (c.primary, c.primary_foreground, c.secondary);
        let half = |id: &'static str, name: &'static str, slim: bool| {
            let chosen = self.slim == slim;
            div()
                .id(id)
                .h_flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .when(chosen, |el| el.bg(accent).text_color(ink))
                .when(!chosen, |el| el.hover(move |style| style.bg(hover)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.slim = slim;
                    cx.notify();
                }))
                .child(name)
        };
        div()
            .h_flex()
            .flex_none()
            .h(px(48.))
            .border_1()
            .border_color(cx.theme().colors.border)
            .child(half("arms-classic", t(Msg::ArmsClassic), false))
            .child(
                div()
                    .flex_none()
                    .w(px(1.))
                    .h_full()
                    .bg(cx.theme().colors.border),
            )
            .child(half("arms-slim", t(Msg::ArmsSlim), true))
            .into_any_element()
    }

    pub(super) fn skin_screen(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (accent, ink, lit, dim, danger, hover, border) = (
            c.primary,
            c.primary_foreground,
            c.primary_hover,
            c.muted_foreground,
            c.danger,
            c.secondary,
            c.border,
        );
        let has_skin = self
            .account
            .as_ref()
            .is_some_and(|account| account.skin_sha256.is_some());
        let busy = self.busy;

        let upload = div()
            .id("skin-upload")
            .h_flex()
            .flex_1()
            .h_full()
            .items_center()
            .justify_center()
            .font_weight(FontWeight::MEDIUM)
            .when(busy, |el| el.bg(border).text_color(dim))
            .when(!busy, |el| {
                el.bg(accent)
                    .text_color(ink)
                    .cursor_pointer()
                    .hover(move |style| style.bg(lit))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.choose_skin(window, cx);
                    }))
            })
            .child(t(Msg::UploadSkin));
        let remove = has_skin.then(|| {
            tile(cx)
                .id("skin-remove")
                .h_flex()
                .flex_none()
                .w(px(128.))
                .h_full()
                .items_center()
                .justify_center()
                .font_weight(FontWeight::MEDIUM)
                .text_color(if busy { dim } else { danger })
                .when(!busy, |el| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.remove_skin(window, cx);
                        }))
                })
                .child(t(Msg::Remove))
        });

        let side = board(cx)
            .v_flex()
            .flex_none()
            .w(px(380.))
            .h_full()
            .child(self.header(t(Msg::Skin), cx))
            .child(
                tile(cx)
                    .v_flex()
                    .flex_none()
                    .gap_3()
                    .p_4()
                    .child(label(t(Msg::Arms), cx))
                    .child(self.arms(cx))
                    .child(label(t(Msg::ArmsHint), cx)),
            )
            .child(
                tile(cx)
                    .v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .p_4()
                    .child(label(t(Msg::File), cx))
                    .child(t(Msg::SkinFileHint))
                    .child(div().text_color(dim).child(t(Msg::TurnHint)))
                    .children(self.feedback(cx)),
            )
            .child(
                board(cx)
                    .h_flex()
                    .flex_none()
                    .h(px(72.))
                    .child(upload)
                    .children(remove),
            );

        board(cx)
            .h_flex()
            .size_full()
            .child(
                tile(cx).relative().flex_1().min_w_0().h_full().child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .child(self.doll.clone()),
                ),
            )
            .child(side)
            .into_any_element()
    }
}
