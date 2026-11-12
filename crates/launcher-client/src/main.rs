//! Relic launcher: connects to a server, signs the player in and starts
//! the game the server describes.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod lang;
mod ui;

use std::borrow::Cow;

use gpui_kit::component::TitleBar;
use gpui_kit::prelude::*;
use gpui_kit::{Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            // The launcher's own faces: a pixel one for names and numbers, a
            // monospace one for everything read at length.
            let fonts: [&'static [u8]; 3] = [
                include_bytes!("../assets/fonts/PressStart2P-Regular.ttf"),
                include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
                include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
            ];
            cx.text_system()
                .add_fonts(fonts.into_iter().map(Cow::Borrowed).collect())
                .expect("the built-in fonts load");
            let bounds = Bounds::centered(None, size(px(1040.), px(660.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(900.), px(580.))),
                // The launcher draws its own title bar; the name is still what
                // the taskbar shows.
                titlebar: Some(TitlebarOptions {
                    title: Some("Relic".into()),
                    ..TitleBar::title_bar_options()
                }),
                ..TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::app::Launcher::new(window, cx))
            })
            .expect("the launcher window opens");
        });
}
