//! The few controls every screen shares.

use std::rc::Rc;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt, App, ClickEvent, Div, ElementId, Entity, FontWeight, Hsla, Pixels,
    SharedString, Window, div, px, relative,
};

use super::icon::{AnimatedIcon, IconName};

/// The pixel face, for names and numbers.
pub const DISPLAY: &str = "Press Start 2P";

/// The seam between tiles: one line of the canvas, no more.
pub const FRAME: Pixels = px(1.);

/// What tiles lie on, and what a tile is filled with: one step apart, the
/// tile always the lighter of the two.
fn layers(cx: &App) -> (Hsla, Hsla) {
    let base = cx.theme().colors.background;
    let shifted = |by: f32| Hsla {
        l: (base.l + by).clamp(0., 1.),
        ..base
    };
    if base.l < 0.5 {
        (base, shifted(0.055))
    } else {
        (shifted(-0.07), base)
    }
}

/// The color behind the tiles.
pub fn canvas(cx: &App) -> Hsla {
    layers(cx).0
}

/// The fill of a tile.
pub fn surface(cx: &App) -> Hsla {
    layers(cx).1
}

/// A line in the pixel face. It is drawn on an 8 px grid, so `size` should
/// be a multiple of 8 to keep its edges on whole pixels.
pub fn pixel(text: impl Into<SharedString>, size: f32) -> Div {
    div()
        .font_family(DISPLAY)
        .text_size(px(size))
        .line_height(px(size * 1.5))
        .child(text.into())
}

/// One cell of a screen. Cells have no frame: they are a step lighter
/// than the board they sit on, with [`FRAME`] between them.
pub fn tile(cx: &App) -> Div {
    div().bg(surface(cx)).overflow_hidden()
}

/// A board of [`tile`]s.
pub fn board(cx: &App) -> Div {
    div().bg(canvas(cx)).gap(FRAME)
}

/// A square progress bar. Without a known share, a block walks along it.
pub fn bar(id: impl Into<ElementId>, done: Option<f32>, fill: Hsla, cx: &App) -> Div {
    let track = div()
        .relative()
        .w_full()
        .h(px(12.))
        .bg(canvas(cx))
        .overflow_hidden();
    match done {
        Some(done) => track.child(div().h_full().w(relative(done.clamp(0., 1.))).bg(fill)),
        None => track.child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(0.2))
                .bg(fill)
                .with_animation(
                    id.into(),
                    Animation::new(std::time::Duration::from_millis(1200)).repeat(),
                    // Five places to stand, visited in order.
                    |block, at| block.left(relative((at * 5.).floor().min(4.) * 0.2)),
                ),
        ),
    }
}

/// Counts how many times the pointer has entered an element. Icons play
/// once per count, so each hover replays them and a replay always finishes.
pub fn hover_beat(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<usize> {
    window.use_keyed_state(ElementId::from((id.clone(), "beat")), cx, |_, _| 0usize)
}

/// A hover listener that advances a [`hover_beat`].
pub fn advance(beat: &Entity<usize>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    let beat = beat.clone();
    move |hovered, _, cx| {
        if *hovered {
            beat.update(cx, |beat, cx| {
                *beat += 1;
                cx.notify();
            });
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Primary,
    Ghost,
    /// No frame: for secondary actions that should not compete.
    Quiet,
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Btn {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<IconName>,
    kind: Kind,
    loading: bool,
    disabled: bool,
    full: bool,
    on_click: Option<OnClick>,
}

impl Btn {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            icon: None,
            kind: Kind::Ghost,
            loading: false,
            disabled: false,
            full: false,
            on_click: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn primary(mut self) -> Self {
        self.kind = Kind::Primary;
        self
    }

    pub fn quiet(mut self) -> Self {
        self.kind = Kind::Quiet;
        self
    }

    /// Shows a spinner and ignores clicks.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn full(mut self) -> Self {
        self.full = true;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Btn {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let beat = hover_beat(&self.id, window, cx);
        let theme = cx.theme();
        let c = &theme.colors;
        let (bg, hover, fg, border): (Hsla, Hsla, Hsla, Hsla) = match self.kind {
            Kind::Primary => (c.primary, c.primary_hover, c.primary_foreground, c.primary),
            Kind::Ghost => (c.secondary, c.secondary_hover, c.foreground, c.secondary),
            Kind::Quiet => {
                let clear = c.secondary.opacity(0.);
                (clear, c.secondary_hover, c.muted_foreground, clear)
            }
        };
        let inert = self.disabled || self.loading;
        let icon_size = px(16.);
        let icon = if self.loading {
            Some(IconName::Loader)
        } else {
            self.icon
        };

        div()
            .id(self.id.clone())
            .h_flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap_2()
            .h(px(40.))
            .px(if self.label.is_some() {
                px(16.)
            } else {
                px(10.)
            })
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(fg)
            .font_weight(FontWeight::MEDIUM)
            .text_sm()
            .when(self.full, |el| el.w_full())
            .when(inert, |el| el.opacity(0.6))
            .when(!inert, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .active(|style| style.opacity(0.85))
                    .on_hover(advance(&beat))
                    .when_some(self.on_click, |el, on_click| {
                        el.on_click(move |event, window, cx| on_click(event, window, cx))
                    })
            })
            .when_some(icon, |el, icon| {
                el.child(
                    AnimatedIcon::new((self.id.clone(), "icon"), icon, fg)
                        .size(icon_size)
                        .play(*beat.read(cx))
                        .looping(self.loading),
                )
            })
            .when_some(self.label, |el, label| el.child(label))
    }
}

/// A labelled text input with an icon that reacts to the pointer.
#[derive(IntoElement)]
pub struct Field {
    id: ElementId,
    label: SharedString,
    icon: IconName,
    input: Entity<InputState>,
    disabled: bool,
}

impl Field {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        icon: IconName,
        input: &Entity<InputState>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon,
            input: input.clone(),
            disabled: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Field {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let beat = hover_beat(&self.id, window, cx);
        let dim = cx.theme().colors.muted_foreground;
        div()
            .id(self.id.clone())
            .v_flex()
            .w_full()
            .gap_1()
            .on_hover(advance(&beat))
            .child(div().text_xs().text_color(dim).child(self.label))
            .child(
                Input::new(&self.input).disabled(self.disabled).prefix(
                    AnimatedIcon::new((self.id.clone(), "icon"), self.icon, dim)
                        .size(px(16.))
                        .play(*beat.read(cx)),
                ),
            )
    }
}

/// One line of feedback under a form: what went wrong, or what happened.
pub fn message(id: &'static str, text: SharedString, is_error: bool, cx: &App) -> impl IntoElement {
    let c = &cx.theme().colors;
    let color = if is_error { c.danger } else { c.success };
    let icon = if is_error {
        IconName::Alert
    } else {
        IconName::Check
    };
    div()
        .h_flex()
        .w_full()
        .items_start()
        .gap_2()
        .text_sm()
        .text_color(color)
        // Plays once when the message appears, and again when its text changes.
        .child(
            AnimatedIcon::new((ElementId::from(id), text.clone()), icon, color)
                .size(px(16.))
                .play(1),
        )
        .child(div().flex_1().min_w_0().child(text))
}

type OnChange = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

/// An on/off switch: a block that sits left when off and right when on.
#[derive(IntoElement)]
pub struct Toggle {
    id: ElementId,
    on: bool,
    on_change: Option<OnChange>,
}

impl Toggle {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            on: false,
            on_change: None,
        }
    }

    pub fn checked(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Toggle {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = &cx.theme().colors;
        let (frame, block) = if self.on {
            (c.primary, c.primary)
        } else {
            (c.muted_foreground, c.muted_foreground)
        };
        let on = self.on;
        div()
            .id(self.id)
            .h_flex()
            .flex_none()
            .w(px(44.))
            .h(px(24.))
            .p(px(3.))
            .border_2()
            .border_color(frame)
            .cursor_pointer()
            .when(on, |el| el.justify_end())
            .child(div().w(px(16.)).h_full().bg(block))
            .when_some(self.on_change, |el, on_change| {
                el.on_click(move |_, window, cx| on_change(&!on, window, cx))
            })
    }
}
