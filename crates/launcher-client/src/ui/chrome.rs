//! The strip along the top of the window: whose launcher this is, a place
//! to drag the window by, and the buttons that minimize, maximize and
//! close it.

use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, MouseButton, Window, WindowControlArea, div, img, px};

use super::app::{Launcher, PlayState, Screen};
use super::icon::{AnimatedIcon, IconName};
use crate::lang::{Msg, tf};

use super::widgets::{board, pixel, tile};

/// Thin on purpose: the screens need the room more.
const HEIGHT: f32 = 24.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Minimize,
    Maximize,
    Close,
}

impl Control {
    fn area(self) -> WindowControlArea {
        match self {
            Control::Minimize => WindowControlArea::Min,
            Control::Maximize => WindowControlArea::Max,
            Control::Close => WindowControlArea::Close,
        }
    }
}

impl Launcher {
    fn control(&self, control: Control, cx: &Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (ink, hover) = match control {
            Control::Close => (c.foreground, c.danger),
            _ => (c.foreground, c.border),
        };
        // Drawn from blocks, like everything else here.
        let glyph = match control {
            Control::Minimize => div().mt(px(8.)).w(px(10.)).h(px(2.)).bg(ink),
            Control::Maximize => div().size(px(10.)).border_2().border_color(ink),
            Control::Close => pixel("x", 8.).text_color(ink),
        };
        let id = match control {
            Control::Minimize => "window-minimize",
            Control::Maximize => "window-maximize",
            Control::Close => "window-close",
        };
        div()
            .id(id)
            .h_flex()
            .flex_none()
            .w(px(34.))
            .h_full()
            .items_center()
            .justify_center()
            .hover(move |style| style.bg(hover))
            // Windows acts on these areas itself; elsewhere the click is ours.
            .window_control_area(control.area())
            .when(cfg!(target_os = "linux"), |el| {
                el.on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_: &ClickEvent, window, cx| match control {
                    Control::Minimize => window.minimize_window(),
                    Control::Maximize => window.zoom_window(),
                    Control::Close => {
                        window.remove_window();
                        cx.stop_propagation();
                    }
                })
            })
            .child(glyph)
            .into_any_element()
    }

    /// The top of a screen that is not home: the way back, and its name.
    pub(super) fn header(&self, title: &'static str, cx: &mut Context<Self>) -> AnyElement {
        self.header_to(title, Screen::Home, cx)
    }

    /// A header whose arrow leads back to `back`.
    pub(super) fn header_to(
        &self,
        title: &'static str,
        back: Screen,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &cx.theme().colors;
        let (text, hover) = (c.foreground, c.secondary);
        board(cx)
            .h_flex()
            .flex_none()
            .h(px(72.))
            .child(
                tile(cx)
                    .id("back")
                    .h_flex()
                    .flex_none()
                    .w(px(72.))
                    .h_full()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.go(back, cx);
                    }))
                    .child(AnimatedIcon::new("back-icon", IconName::ArrowLeft, text).size(px(24.))),
            )
            .child(
                tile(cx)
                    .h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .px_5()
                    .child(pixel(title, 16.)),
            )
            .into_any_element()
    }

    pub(super) fn title_bar(&self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &cx.theme().colors;
        let (surface, accent, on_accent) = (c.secondary, c.primary, c.primary_foreground);
        let name = match (&self.handshake, self.screen) {
            (
                Some(handshake),
                Screen::Home
                | Screen::Settings
                | Screen::Language
                | Screen::Mods
                | Screen::News
                | Screen::Skin,
            ) => handshake.server_name.clone(),
            _ => "Relic".to_owned(),
        };
        let mac = cfg!(target_os = "macos");

        let brand = div()
            .id("window-drag")
            .h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .gap_2()
            // macOS keeps its own three buttons on the left.
            .pl(if mac { px(72.) } else { px(10.) })
            .window_control_area(WindowControlArea::Drag)
            .when(!cfg!(target_os = "windows"), |el| {
                el.on_mouse_down(MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
            })
            .children(
                self.logo
                    .clone()
                    .map(|logo| img(logo).h(px(14.)).max_w(px(56.))),
            )
            .child(pixel(name, 8.).truncate());

        div()
            .h_flex()
            .flex_none()
            .w_full()
            .h(px(HEIGHT))
            .bg(surface)
            .child(brand)
            .when_some(self.updated.clone(), |el, version| {
                // Restarting mid-install would leave the game half set up.
                let busy = matches!(self.play, PlayState::Working { .. });
                el.child(
                    div()
                        .id("restart-to-update")
                        .h_flex()
                        .flex_none()
                        .h_full()
                        .items_center()
                        .px_3()
                        .bg(accent)
                        .text_color(on_accent)
                        .text_xs()
                        .child(tf(Msg::RestartToUpdate, &[&version]))
                        .when(busy, |el| el.opacity(0.6))
                        .when(!busy, |el| {
                            el.cursor_pointer().on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.restart(cx);
                                },
                            ))
                        }),
                )
            })
            .when(!mac, |el| {
                el.child(self.control(Control::Minimize, cx))
                    .child(self.control(Control::Maximize, cx))
                    .child(self.control(Control::Close, cx))
            })
            .into_any_element()
    }
}
