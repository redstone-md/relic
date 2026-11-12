//! The news, in full: every item down the side, the chosen one beside it.

use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, FontWeight, ObjectFit, Window, div, img, px};

use super::app::Launcher;
use super::home::label;
use crate::lang::{Msg, date, t};

use super::widgets::{board, pixel, tile};

impl Launcher {
    pub(super) fn news_screen(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (accent, ink, dim, hover) = (
            c.primary,
            c.primary_foreground,
            c.muted_foreground,
            c.secondary,
        );
        let news = self.news.clone();
        // The one the player picked, or failing that the newest.
        let open = news
            .iter()
            .find(|item| Some(item.id) == self.news_open)
            .or_else(|| news.first());

        let mut list = tile(cx)
            .id("news-list")
            .v_flex()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        for (ix, item) in news.iter().enumerate() {
            let chosen = open.is_some_and(|open| open.id == item.id);
            let id = item.id;
            list = list.child(
                div()
                    .id(("news-row", ix))
                    .v_flex()
                    .flex_none()
                    .gap_1()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(cx.theme().colors.border)
                    .cursor_pointer()
                    .when(chosen, |el| el.bg(accent).text_color(ink))
                    .when(!chosen, |el| el.hover(move |style| style.bg(hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.news_open = Some(id);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_xs()
                            .text_color(if chosen { ink } else { dim })
                            .child(date(item.published_at)),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(item.title.clone()),
                    ),
            );
        }

        let mut article = tile(cx)
            .id("news-article")
            .flex_1()
            .min_w_0()
            .h_full()
            .p_8()
            .overflow_y_scroll();
        article = match open {
            Some(item) => article.child(
                div()
                    .v_flex()
                    .w_full()
                    .max_w(px(640.))
                    .gap_4()
                    .children(
                        item.image_url
                            .as_deref()
                            .and_then(|url| self.picture(url))
                            .map(|picture| {
                                img(picture)
                                    .w_full()
                                    .h(px(280.))
                                    .object_fit(ObjectFit::Cover)
                            }),
                    )
                    .child(label(date(item.published_at), cx))
                    .child(pixel(item.title.clone(), 16.))
                    .child(TextView::markdown(
                        ("news", item.id as u64),
                        item.body.clone(),
                    )),
            ),
            None => article.child(div().text_color(dim).child(t(Msg::NoNews))),
        };

        board(cx)
            .h_flex()
            .size_full()
            .child(
                board(cx)
                    .v_flex()
                    .flex_none()
                    .w(px(340.))
                    .h_full()
                    .child(self.header(t(Msg::News), cx))
                    .child(list),
            )
            .child(article)
            .into_any_element()
    }
}
