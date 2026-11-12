//! The languages the launcher speaks, each by its own name, to pick from.

use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, FontWeight, SharedString, Window, div, px};

use super::app::{Launcher, Screen};
use super::widgets::{FRAME, board, tile};
use crate::lang::{Lang, Msg, t, tf};

/// How many languages stand side by side.
const COLUMNS: usize = 4;

impl Launcher {
    pub(super) fn language_screen(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (accent, ink, hover) = (c.primary, c.primary_foreground, c.secondary);
        let chosen = self.language();
        // The system's own choice first, then every language there is.
        let choices: Vec<Option<Lang>> =
            std::iter::once(None).chain(Lang::all().map(Some)).collect();
        let follows: SharedString =
            tf(Msg::LanguageFollowsSystem, &[&Lang::system().name()]).into();

        let mut grid = div()
            .id("languages")
            .v_flex()
            .flex_1()
            .min_h_0()
            .gap(FRAME)
            .overflow_y_scroll();
        for (row, line) in choices.chunks(COLUMNS).enumerate() {
            let mut tiles = div().h_flex().flex_none().w_full().h(px(56.)).gap(FRAME);
            for (column, &choice) in line.iter().enumerate() {
                let name: SharedString = match choice {
                    Some(lang) => lang.name().into(),
                    None => follows.clone(),
                };
                let is_chosen = choice == chosen;
                tiles = tiles.child(
                    tile(cx)
                        .id(("language", row * COLUMNS + column))
                        .h_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .when(is_chosen, |el| el.bg(accent).text_color(ink))
                        .when(!is_chosen, |el| el.hover(move |style| style.bg(hover)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.set_language(choice, window, cx);
                        }))
                        .child(div().px_3().truncate().child(name)),
                );
            }
            // A short last row still keeps the width of its tiles.
            for _ in line.len()..COLUMNS {
                tiles = tiles.child(tile(cx).flex_1().min_w_0().h_full());
            }
            grid = grid.child(tiles);
        }
        // Whatever height the languages leave is one empty tile.
        grid = grid.child(tile(cx).flex_1().w_full());

        board(cx)
            .v_flex()
            .size_full()
            .child(self.header_to(t(Msg::Language), Screen::Settings, cx))
            .child(grid)
            .into_any_element()
    }
}
