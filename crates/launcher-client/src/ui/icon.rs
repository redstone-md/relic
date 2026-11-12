//! Every icon in the launcher. An icon is a stack of SVG layers, and each
//! layer may move on its own: the gear turns, the arrow leaves the door, the
//! bell... there is no bell, but it would ring.
//!
//! GPUI draws an SVG as a tinted mask and can transform it as a whole, so
//! motion is built from rotation, translation, scale and opacity per layer.
//! Shapes do not morph.

use std::f32::consts::PI;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt, App, ElementId, Hsla, Pixels, Transformation, Window, div,
    ease_in_out, point, px, radians, size, svg,
};

/// Icons are drawn on the usual 24-unit grid.
const GRID: f32 = 24.;

macro_rules! doc {
    ($body:literal) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" "#,
            r#"stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#,
            $body,
            "</svg>"
        )
    };
}

/// How one layer moves while its icon plays. Every motion starts and ends
/// at rest, so an icon can stop being animated at any time without a jump.
#[derive(Clone, Copy)]
enum Motion {
    Still,
    /// Full turns around the center.
    Spin(f32),
    /// Rocks left and right by this many degrees.
    Rock(f32),
    /// Moves out by (x, y) grid units and back.
    Nudge(f32, f32),
    /// Grows to this scale and back.
    Pop(f32),
    /// Fades down to this opacity and back.
    Fade(f32),
}

struct Layer {
    svg: &'static str,
    motion: Motion,
}

const fn layer(svg: &'static str, motion: Motion) -> Layer {
    Layer { svg, motion }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconName {
    Alert,
    ArrowLeft,
    Book,
    Cart,
    Chat,
    Check,
    ChevronRight,
    ExternalLink,
    Globe,
    Heart,
    Key,
    Loader,
    Lock,
    LogOut,
    Map,
    Memory,
    News,
    Plus,
    Puzzle,
    Server,
    Settings,
    Ticket,
    User,
    Users,
    Video,
}

impl IconName {
    /// The icon for a link, by the name an admin gave it in the panel.
    pub fn for_link(name: Option<&str>) -> Self {
        match name
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "discord" | "chat" | "telegram" | "forum" => IconName::Chat,
            "globe" | "site" | "web" | "website" | "home" => IconName::Globe,
            "wiki" | "book" | "rules" | "docs" | "guide" => IconName::Book,
            "shop" | "store" | "cart" | "donate" => IconName::Cart,
            "video" | "youtube" | "twitch" | "stream" => IconName::Video,
            "map" | "dynmap" | "bluemap" => IconName::Map,
            "vote" | "heart" | "support" => IconName::Heart,
            "news" => IconName::News,
            "users" | "community" | "vk" => IconName::Users,
            _ => IconName::ExternalLink,
        }
    }

    fn layers(self) -> &'static [Layer] {
        use Motion::*;
        match self {
            IconName::Alert => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/>"#
                            ),
                            Rock(7.),
                        ),
                        layer(
                            doc!(r#"<path d="M12 9v4"/><path d="M12 17h.01"/>"#),
                            Pop(1.25),
                        ),
                    ]
                }
            }
            IconName::ArrowLeft => {
                &const {
                    [layer(
                        doc!(r#"<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>"#),
                        Nudge(-3., 0.),
                    )]
                }
            }
            IconName::Book => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<path d="M20.001 19A2 2 0 0022 17V5a2 2 0 00-1.999-2L16 3.002A5 5 0 0012 5a5 5 0 00-4-2H4a2 2 0 00-2 2v12a2 2 0 001.999 2H8a5 5 0 014 2 5 5 0 014-2z"/>"#
                            ),
                            Pop(1.08),
                        ),
                        layer(doc!(r#"<path d="M12 5v16"/>"#), Fade(0.3)),
                    ]
                }
            }
            IconName::Cart => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<path d="m2.05 2.05 1.099-.028a1 1 0 0 1 1.008.815l2.69 14.347A1 1 0 0 0 7.83 18H18"/><path d="M4.563 5h16.435a1 1 0 0 1 .981 1.204l-1.026 6.226A2 2 0 0 1 18.962 14H6.25"/>"#
                            ),
                            Nudge(2., 0.),
                        ),
                        layer(
                            doc!(
                                r#"<circle cx="18" cy="20" r="2"/><circle cx="8" cy="20" r="2"/>"#
                            ),
                            Nudge(2., 0.),
                        ),
                    ]
                }
            }
            IconName::Chat => {
                &const {
                    [layer(
                        doc!(
                            r#"<path d="M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719"/>"#
                        ),
                        Rock(9.),
                    )]
                }
            }
            IconName::Check => {
                &const { [layer(doc!(r#"<path d="M20 6 9 17l-5-5"/>"#), Pop(1.25))] }
            }
            IconName::ChevronRight => {
                &const { [layer(doc!(r#"<path d="m9 18 6-6-6-6"/>"#), Nudge(3., 0.))] }
            }
            IconName::ExternalLink => {
                &const {
                    [
                        layer(
                            doc!(r#"<path d="M15 3h6v6"/><path d="M10 14 21 3"/>"#),
                            Nudge(2., -2.),
                        ),
                        layer(
                            doc!(
                                r#"<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>"#
                            ),
                            Still,
                        ),
                    ]
                }
            }
            IconName::Globe => {
                &const {
                    [
                        layer(doc!(r#"<circle cx="12" cy="12" r="10"/>"#), Still),
                        layer(
                            doc!(
                                r#"<path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>"#
                            ),
                            Spin(0.5),
                        ),
                    ]
                }
            }
            IconName::Heart => {
                &const {
                    [layer(
                        doc!(
                            r#"<path d="M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5"/>"#
                        ),
                        Pop(1.2),
                    )]
                }
            }
            IconName::Key => {
                &const {
                    [layer(
                        doc!(
                            r#"<path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/><circle cx="16.5" cy="7.5" r=".5" fill="black"/>"#
                        ),
                        Rock(14.),
                    )]
                }
            }
            IconName::Loader => {
                &const {
                    [layer(
                        doc!(r#"<path d="M21 12a9 9 0 1 1-6.219-8.56"/>"#),
                        Spin(1.),
                    )]
                }
            }
            IconName::Lock => {
                &const {
                    [
                        layer(
                            doc!(r#"<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/>"#),
                            Still,
                        ),
                        layer(
                            doc!(r#"<path d="M7 11V7a5 5 0 0 1 10 0v4"/>"#),
                            Nudge(0., -2.),
                        ),
                    ]
                }
            }
            IconName::LogOut => {
                &const {
                    [
                        layer(
                            doc!(r#"<path d="m16 17 5-5-5-5"/><path d="M21 12H9"/>"#),
                            Nudge(3., 0.),
                        ),
                        layer(
                            doc!(r#"<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>"#),
                            Still,
                        ),
                    ]
                }
            }
            IconName::Map => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<path d="M14.106 5.553a2 2 0 0 0 1.788 0l3.659-1.83A1 1 0 0 1 21 4.619v12.764a1 1 0 0 1-.553.894l-4.553 2.277a2 2 0 0 1-1.788 0l-4.212-2.106a2 2 0 0 0-1.788 0l-3.659 1.83A1 1 0 0 1 3 19.381V6.618a1 1 0 0 1 .553-.894l4.553-2.277a2 2 0 0 1 1.788 0z"/>"#
                            ),
                            Rock(5.),
                        ),
                        layer(
                            doc!(r#"<path d="M15 5.764v15"/><path d="M9 3.236v15"/>"#),
                            Rock(5.),
                        ),
                    ]
                }
            }
            IconName::Memory => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<rect x="2" y="6" width="20" height="10" rx="2"/><path d="M2 11h1.5"/><path d="M20.5 11H22"/>"#
                            ),
                            Still,
                        ),
                        layer(
                            doc!(
                                r#"<path d="M8 12v-2"/><path d="M12 12v-2"/><path d="M16 12v-2"/>"#
                            ),
                            Fade(0.2),
                        ),
                        layer(
                            doc!(
                                r#"<path d="M4 18v-2"/><path d="M8 18v-2"/><path d="M12 18v-2"/><path d="M16 18v-2"/><path d="M20 18v-2"/>"#
                            ),
                            Nudge(0., 1.5),
                        ),
                    ]
                }
            }
            IconName::News => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<path d="M4 22h16a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v16a2 2 0 0 1-4 0v-9a2 2 0 0 1 2-2h2"/>"#
                            ),
                            Still,
                        ),
                        layer(
                            doc!(r#"<rect width="8" height="4" x="10" y="6" rx="1"/>"#),
                            Pop(1.12),
                        ),
                        layer(
                            doc!(r#"<path d="M15 18h-5"/><path d="M18 14h-8"/>"#),
                            Fade(0.25),
                        ),
                    ]
                }
            }
            IconName::Plus => {
                &const {
                    [layer(
                        doc!(r#"<path d="M5 12h14"/><path d="M12 5v14"/>"#),
                        Spin(0.5),
                    )]
                }
            }
            IconName::Puzzle => {
                &const {
                    [layer(
                        doc!(
                            r#"<path d="M15.39 4.39a1 1 0 0 0 1.68-.474 2.5 2.5 0 1 1 3.014 3.015 1 1 0 0 0-.474 1.68l1.683 1.682a2.414 2.414 0 0 1 0 3.414L19.61 15.39a1 1 0 0 1-1.68-.474 2.5 2.5 0 1 0-3.014 3.015 1 1 0 0 1 .474 1.68l-1.683 1.682a2.414 2.414 0 0 1-3.414 0L8.61 19.61a1 1 0 0 0-1.68.474 2.5 2.5 0 1 1-3.014-3.015 1 1 0 0 0 .474-1.68l-1.683-1.682a2.414 2.414 0 0 1 0-3.414L4.39 8.61a1 1 0 0 1 1.68.474 2.5 2.5 0 1 0 3.014-3.015 1 1 0 0 1-.474-1.68l1.683-1.682a2.414 2.414 0 0 1 3.414 0z"/>"#
                        ),
                        Rock(12.),
                    )]
                }
            }
            IconName::Server => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<rect width="20" height="8" x="2" y="2" rx="2" ry="2"/><rect width="20" height="8" x="2" y="14" rx="2" ry="2"/>"#
                            ),
                            Still,
                        ),
                        layer(doc!(r#"<line x1="6" x2="6.01" y1="6" y2="6"/>"#), Fade(0.1)),
                        layer(
                            doc!(r#"<line x1="6" x2="6.01" y1="18" y2="18"/>"#),
                            Pop(1.6),
                        ),
                    ]
                }
            }
            IconName::Settings => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/>"#
                            ),
                            Spin(0.5),
                        ),
                        layer(doc!(r#"<circle cx="12" cy="12" r="3"/>"#), Still),
                    ]
                }
            }
            IconName::Ticket => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<path d="M2 9a3 3 0 0 1 0 6v2a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-2a3 3 0 0 1 0-6V7a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2Z"/>"#
                            ),
                            Rock(8.),
                        ),
                        layer(
                            doc!(r#"<path d="M13 5v2"/><path d="M13 17v2"/><path d="M13 11v2"/>"#),
                            Rock(8.),
                        ),
                    ]
                }
            }
            IconName::User => {
                &const {
                    [
                        layer(
                            doc!(r#"<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/>"#),
                            Still,
                        ),
                        layer(doc!(r#"<circle cx="12" cy="7" r="4"/>"#), Nudge(0., -2.)),
                    ]
                }
            }
            IconName::Users => {
                &const {
                    [
                        layer(
                            doc!(
                                r#"<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/>"#
                            ),
                            Still,
                        ),
                        layer(
                            doc!(
                                r#"<path d="M16 3.128a4 4 0 0 1 0 7.744"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/>"#
                            ),
                            Nudge(2., 0.),
                        ),
                    ]
                }
            }
            IconName::Video => {
                &const {
                    [
                        layer(
                            doc!(r#"<rect x="2" y="6" width="14" height="12" rx="2"/>"#),
                            Still,
                        ),
                        layer(
                            doc!(
                                r#"<path d="m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5"/>"#
                            ),
                            Nudge(2., 0.),
                        ),
                    ]
                }
            }
        }
    }
}

/// When an icon moves.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Playback {
    Rest,
    /// Plays once per distinct value; a hover counter is the usual source.
    Once(usize),
    Loop,
}

#[derive(IntoElement)]
pub struct AnimatedIcon {
    id: ElementId,
    name: IconName,
    size: Pixels,
    color: Hsla,
    playback: Playback,
}

impl AnimatedIcon {
    pub fn new(id: impl Into<ElementId>, name: IconName, color: Hsla) -> Self {
        Self {
            id: id.into(),
            name,
            size: px(18.),
            color,
            playback: Playback::Rest,
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Plays the animation once whenever `beat` changes to a new non-zero
    /// value. Zero keeps the icon at rest.
    pub fn play(mut self, beat: usize) -> Self {
        if beat > 0 && self.playback != Playback::Loop {
            self.playback = Playback::Once(beat);
        }
        self
    }

    /// Keeps the animation running, for anything that is in progress.
    pub fn looping(mut self, looping: bool) -> Self {
        if looping {
            self.playback = Playback::Loop;
        }
        self
    }
}

/// Places a layer for progress `t` (0 to 1) through its motion.
fn pose(layer: gpui_kit::Svg, motion: Motion, t: f32, extent: Pixels) -> gpui_kit::Svg {
    let unit = extent / GRID;
    // There and back again, and a full sine swing.
    let arc = (PI * t).sin();
    let swing = (2. * PI * t).sin();
    match motion {
        Motion::Still => layer,
        Motion::Spin(turns) => {
            layer.with_transformation(Transformation::rotate(radians(2. * PI * turns * t)))
        }
        Motion::Rock(degrees) => layer.with_transformation(Transformation::rotate(radians(
            degrees.to_radians() * swing,
        ))),
        Motion::Nudge(x, y) => layer.with_transformation(Transformation::translate(point(
            unit * x * arc,
            unit * y * arc,
        ))),
        Motion::Pop(scale) => {
            let s = 1. + (scale - 1.) * arc;
            layer.with_transformation(Transformation::scale(size(s, s)))
        }
        Motion::Fade(low) => layer.opacity(1. - (1. - low) * arc),
    }
}

impl RenderOnce for AnimatedIcon {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let Self {
            id,
            name,
            size: icon_size,
            color,
            playback,
        } = self;
        let layers = name.layers().iter().enumerate().map(move |(ix, layer)| {
            let motion = layer.motion;
            let base = svg()
                .data(layer.svg.as_bytes())
                .absolute()
                .top_0()
                .left_0()
                .size(icon_size)
                .text_color(color);
            let animation = match (playback, motion) {
                (Playback::Rest, _) | (_, Motion::Still) => None,
                (Playback::Once(beat), _) => Some((
                    beat,
                    Animation::new(Duration::from_millis(520)).with_easing(ease_in_out),
                )),
                (Playback::Loop, Motion::Spin(_)) => {
                    Some((0, Animation::new(Duration::from_millis(900)).repeat()))
                }
                (Playback::Loop, _) => Some((
                    0,
                    Animation::new(Duration::from_millis(1100))
                        .repeat()
                        .with_easing(ease_in_out),
                )),
            };
            match animation {
                None => base.into_any_element(),
                Some((beat, animation)) => base
                    .with_animation(
                        ElementId::from((id.clone(), format!("{ix}:{beat}"))),
                        animation,
                        move |layer, t| pose(layer, motion, t, icon_size),
                    )
                    .into_any_element(),
            }
        });
        div()
            .relative()
            .flex_none()
            .size(icon_size)
            .children(layers)
    }
}
