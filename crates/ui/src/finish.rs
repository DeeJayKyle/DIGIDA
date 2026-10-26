//! The finish: the material every surface is drawn in. Each one re-tones the palette it sits on
//! and decides how a raised or pressed surface separates from what is under it, through
//! shadows, highlights and gradients. Colours still come from the theme, so a theme file and
//! the adaptive tint dress every finish alike.

use gpui::prelude::*;
use gpui::{
    App, Background, BoxShadow, Hsla, Pixels, SharedString, hsla, linear_color_stop,
    linear_gradient, point, px, rgb, rgba,
};
use i18n::t;

use crate::theme::{ActiveTheme as _, Theme};

/// The smallest radius clay and neumorphism round to, since a square pillow reads as a slab.
const SOFT_RADIUS: Pixels = px(12.);
/// How much rounder clay draws than the corner setting asks.
const CLAY_ROUNDING: f32 = 1.5;
/// How much of the page a glass surface lets through.
const GLASS_PAGE: f32 = 0.38;
const GLASS_SIDEBAR: f32 = 0.24;
/// How far lighter the top of a skeuomorphic surface is than its bottom, in lightness.
const SKEUO_RISE: f32 = 0.045;

/// The material surfaces are drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Finish {
    /// Frosted panes over a drifting colour field, edged with light.
    #[default]
    Glass,
    /// Soft, puffy pastel surfaces with a lit top and a shaded underside.
    Clay,
    /// Real-world materials: lit gradients, bevelled edges and grained paper.
    Skeuo,
    /// Surfaces pushed out of and pressed into one continuous material.
    Neu,
}

/// How far a surface stands off what is under it: a control, a card or panel, or something
/// raised over the page like a menu or a dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Low,
    Mid,
    High,
}

impl Finish {
    pub const ALL: [Self; 4] = [Self::Glass, Self::Clay, Self::Skeuo, Self::Neu];

    pub fn id(self) -> &'static str {
        match self {
            Self::Glass => "glass",
            Self::Clay => "clay",
            Self::Skeuo => "skeuo",
            Self::Neu => "neu",
        }
    }

    pub fn label(self) -> SharedString {
        match self {
            Self::Glass => t!("finish-glass"),
            Self::Clay => t!("finish-clay"),
            Self::Skeuo => t!("finish-skeuo"),
            Self::Neu => t!("finish-neu"),
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "clay" => Self::Clay,
            "skeuo" => Self::Skeuo,
            "neu" => Self::Neu,
            _ => Self::Glass,
        }
    }

    /// The radius surfaces round to under this finish, from the one the corner setting asks.
    pub fn radius(self, radius: Pixels) -> Pixels {
        match self {
            Self::Clay => (radius * CLAY_ROUNDING).max(SOFT_RADIUS),
            Self::Neu => radius.max(SOFT_RADIUS),
            Self::Glass | Self::Skeuo => radius,
        }
    }

    /// Whether the drifting colour field is painted behind the whole window, which is what
    /// glass has to blur.
    pub fn backdrop(self) -> bool {
        self == Self::Glass
    }

    /// Whether the page carries a paper grain.
    pub fn grained(self) -> bool {
        self == Self::Skeuo
    }

    /// Re-tones a neutral palette for this finish. Runs before the cover tint and the theme
    /// file, so both still have the last word on colour.
    pub(crate) fn tone(self, theme: &mut Theme) {
        let dark = theme.background.l < 0.5;
        let set = |theme: &mut Theme, palette: Palette| palette.apply(theme);

        match (self, dark) {
            (Self::Glass, true) => set(
                theme,
                Palette {
                    background: rgb(0x0b0b12).into(),
                    surface: rgba(0xffffff12).into(),
                    raised: rgb(0x15151f).into(),
                    muted: rgba(0xffffff1c).into(),
                    border: rgba(0xffffff24).into(),
                    sidebar: rgb(0x0b0b12).into(),
                    accent: None,
                },
            ),
            (Self::Glass, false) => set(
                theme,
                Palette {
                    background: rgb(0xf4f5fb).into(),
                    surface: rgba(0xffffff8c).into(),
                    raised: rgb(0xfbfbff).into(),
                    muted: rgba(0xffffffa6).into(),
                    border: rgba(0xffffffcc).into(),
                    sidebar: rgb(0xf4f5fb).into(),
                    accent: None,
                },
            ),
            (Self::Clay, true) => set(
                theme,
                Palette {
                    background: rgb(0x1c1a2b).into(),
                    surface: rgb(0x2a2740).into(),
                    raised: rgb(0x2d2a45).into(),
                    muted: rgb(0x35314f).into(),
                    border: rgba(0xffffff0f).into(),
                    sidebar: rgb(0x211e33).into(),
                    accent: Some((rgb(0xbfaeff).into(), rgb(0x1c1a2b).into())),
                },
            ),
            (Self::Clay, false) => set(
                theme,
                Palette {
                    background: rgb(0xf0ebfd).into(),
                    surface: rgb(0xfdfcff).into(),
                    raised: rgb(0xffffff).into(),
                    muted: rgb(0xe4dcf8).into(),
                    border: rgba(0x7c5cff1f).into(),
                    sidebar: rgb(0xe9e2fa).into(),
                    accent: Some((rgb(0x7c5cff).into(), rgb(0xffffff).into())),
                },
            ),
            (Self::Skeuo, true) => set(
                theme,
                Palette {
                    background: rgb(0x1f1c19).into(),
                    surface: rgb(0x2f2b27).into(),
                    raised: rgb(0x2b2723).into(),
                    muted: rgb(0x3b3631).into(),
                    border: rgba(0x00000099).into(),
                    sidebar: rgb(0x181613).into(),
                    accent: Some((rgb(0xe0aa4a).into(), rgb(0x1f1c19).into())),
                },
            ),
            (Self::Skeuo, false) => set(
                theme,
                Palette {
                    background: rgb(0xe9e4da).into(),
                    surface: rgb(0xf7f3eb).into(),
                    raised: rgb(0xfaf7f1).into(),
                    muted: rgb(0xd9d2c4).into(),
                    border: rgba(0x5a46325c).into(),
                    sidebar: rgb(0xddd6c8).into(),
                    accent: Some((rgb(0x8c5a2b).into(), rgb(0xfff8ee).into())),
                },
            ),
            (Self::Neu, true) => set(
                theme,
                Palette {
                    background: rgb(0x24272d).into(),
                    surface: rgb(0x24272d).into(),
                    raised: rgb(0x272a31).into(),
                    muted: rgb(0x2c3037).into(),
                    border: rgba(0xffffff08).into(),
                    sidebar: rgb(0x24272d).into(),
                    accent: Some((rgb(0x7aa7ff).into(), rgb(0x14171c).into())),
                },
            ),
            (Self::Neu, false) => set(
                theme,
                Palette {
                    background: rgb(0xe3e8ef).into(),
                    surface: rgb(0xe3e8ef).into(),
                    raised: rgb(0xe8edf3).into(),
                    muted: rgb(0xd9dfe7).into(),
                    border: rgba(0xffffff66).into(),
                    sidebar: rgb(0xe3e8ef).into(),
                    accent: Some((rgb(0x4a7bea).into(), rgb(0xffffff).into())),
                },
            ),
        }
    }

    /// Thins the page and the sidebar so the colour field shows through glass. Runs after the
    /// theme file, so a theme's opaque background still turns to glass.
    pub(crate) fn glaze(self, theme: &mut Theme) {
        if self != Self::Glass {
            return;
        }
        theme.background = theme.background.opacity(GLASS_PAGE);
        theme.sidebar = theme.sidebar.opacity(GLASS_SIDEBAR);
        theme.table_head = theme.table_head.opacity(GLASS_PAGE);
    }
}

/// The colours a finish lays over a neutral palette. `accent` replaces the primary fill and
/// its glyph colour, and is left alone by finishes that keep the theme's own.
struct Palette {
    background: Hsla,
    surface: Hsla,
    raised: Hsla,
    muted: Hsla,
    border: Hsla,
    sidebar: Hsla,
    accent: Option<(Hsla, Hsla)>,
}

impl Palette {
    fn apply(self, theme: &mut Theme) {
        let dark = self.background.l < 0.5;
        let lift = |amount: f32| match dark {
            true => hsla(0., 0., 1., amount),
            false => hsla(0., 0., 0., amount),
        };

        theme.background = self.background;
        theme.secondary = self.surface;
        theme.secondary_hover = lift(0.07);
        theme.secondary_active = lift(0.12);
        theme.muted = self.muted;
        theme.border = self.border;
        theme.popover = self.raised;
        theme.sidebar = self.sidebar;
        theme.sidebar_accent = lift(0.08);
        theme.sidebar_border = self.border;
        theme.title_bar_border = self.border;
        theme.table_head = self.background.opacity(0.85);
        theme.table_row_border = self.border;
        theme.table_hover = lift(0.06);
        if let Some((primary, foreground)) = self.accent {
            theme.primary = primary;
            theme.primary_foreground = foreground;
            theme.primary_hover = primary.opacity(0.82);
            theme.progress_bar = primary;
            theme.selection = primary;
            theme.table_active = primary.opacity(0.18);
            theme.table_active_border = primary;
        }
    }
}

/// The shadows a surface at `depth` casts and catches under the active finish. Glass is lit
/// along its top edge, clay is puffed by an inner light and an inner shade, skeuomorphism sits
/// on a crisp contact shadow under a bevel, and neumorphism pushes out of the page with a light
/// above-left and a shade below-right.
pub fn raised_shadows(theme: &Theme, depth: Depth) -> Vec<BoxShadow> {
    let dark = theme.background.l < 0.5;
    let (near, far) = match depth {
        Depth::Low => (px(3.), px(6.)),
        Depth::Mid => (px(6.), px(14.)),
        Depth::High => (px(10.), px(28.)),
    };

    match theme.finish {
        Finish::Glass => {
            let mut shadows = vec![
                shadow(
                    px(0.),
                    px(1.),
                    px(0.),
                    white(match dark {
                        true => 0.14,
                        false => 0.7,
                    }),
                )
                .inset(),
            ];
            if depth != Depth::Low {
                shadows.push(shadow(
                    px(0.),
                    near,
                    far * 2.,
                    black(match dark {
                        true => 0.35,
                        false => 0.12,
                    }),
                ));
            }
            shadows
        }
        Finish::Clay => {
            let shade = match dark {
                true => black(0.45),
                false => Hsla {
                    a: 0.22,
                    ..theme.primary
                },
            };
            vec![
                shadow(px(0.), near * 1.5, far * 1.6, shade),
                shadow(near, near, far, white(if dark { 0.07 } else { 0.85 })).inset(),
                shadow(-near, -near, far, black(if dark { 0.35 } else { 0.08 })).inset(),
            ]
        }
        Finish::Skeuo => vec![
            shadow(px(0.), px(1.), px(0.), white(if dark { 0.09 } else { 0.8 })).inset(),
            shadow(
                px(0.),
                px(-1.),
                px(0.),
                black(if dark { 0.35 } else { 0.08 }),
            )
            .inset(),
            shadow(
                px(0.),
                px(1.),
                px(1.5),
                black(if dark { 0.7 } else { 0.25 }),
            ),
            shadow(px(0.), near, far, black(if dark { 0.4 } else { 0.14 })),
        ],
        Finish::Neu => vec![
            shadow(
                near,
                near,
                far,
                match dark {
                    true => black(0.5),
                    false => Hsla::from(rgba(0xa3b1c6aa)),
                },
            ),
            shadow(-near, -near, far, white(if dark { 0.045 } else { 0.95 })),
        ],
    }
}

/// The shadows a surface at `depth` wears while pressed or selected: the raised look turned
/// inward, so the surface sinks into the page rather than standing off it.
pub fn pressed_shadows(theme: &Theme, depth: Depth) -> Vec<BoxShadow> {
    let dark = theme.background.l < 0.5;
    let (near, far) = match depth {
        Depth::Low => (px(2.), px(5.)),
        Depth::Mid => (px(4.), px(9.)),
        Depth::High => (px(6.), px(14.)),
    };

    match theme.finish {
        Finish::Glass => vec![shadow(px(0.), px(1.), px(3.), black(0.25)).inset()],
        Finish::Clay => vec![
            shadow(near, near, far, black(if dark { 0.4 } else { 0.12 })).inset(),
            shadow(-near, -near, far, white(if dark { 0.05 } else { 0.7 })).inset(),
        ],
        Finish::Skeuo => vec![
            shadow(px(0.), px(2.), px(4.), black(if dark { 0.6 } else { 0.28 })).inset(),
            shadow(px(0.), px(1.), px(0.), white(if dark { 0.06 } else { 0.6 })),
        ],
        Finish::Neu => vec![
            shadow(
                near,
                near,
                far,
                match dark {
                    true => black(0.5),
                    false => Hsla::from(rgba(0xa3b1c6cc)),
                },
            )
            .inset(),
            shadow(-near, -near, far, white(if dark { 0.05 } else { 0.95 })).inset(),
        ],
    }
}

/// The fill a surface of `color` is painted with: a lit gradient under skeuomorphism, the
/// colour itself anywhere else. A translucent colour stays flat, since a gradient over glass
/// reads as a smear.
pub fn surface_fill(theme: &Theme, color: Hsla) -> Background {
    match theme.finish == Finish::Skeuo && color.a > 0.5 {
        true => linear_gradient(
            180.,
            linear_color_stop(lighten(color, SKEUO_RISE), 0.),
            linear_color_stop(lighten(color, -SKEUO_RISE), 1.),
        ),
        false => color.into(),
    }
}

/// Stands an element off the page at `depth` in the active finish.
pub fn raised<E: Styled>(element: E, depth: Depth, cx: &App) -> E {
    element.shadow(raised_shadows(cx.theme(), depth))
}

/// Sinks an element into the page at `depth` in the active finish.
pub fn pressed<E: Styled>(element: E, depth: Depth, cx: &App) -> E {
    element.shadow(pressed_shadows(cx.theme(), depth))
}

/// Marks an element as the chosen one of a list, like the current page in the sidebar: lit
/// from above on glass, and pressed into the page by every finish that has relief to give.
pub fn chosen<E: Styled>(element: E, cx: &App) -> E {
    match cx.theme().finish {
        Finish::Glass => raised(element, Depth::Low, cx),
        _ => pressed(element, Depth::Low, cx),
    }
}

/// Whether a surface that is only there to separate itself from the page should carry the
/// finish's own relief. Neumorphism and clay shape a surface by its shadows alone, so they
/// stand every surface off the page; glass and skeuomorphism draw an edge as well.
pub fn sculpted(cx: &App) -> bool {
    matches!(cx.theme().finish, Finish::Clay | Finish::Neu)
}

fn shadow(x: Pixels, y: Pixels, blur: Pixels, color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(x, y),
        blur_radius: blur,
        spread_radius: px(0.),
        inset: false,
    }
}

fn white(alpha: f32) -> Hsla {
    hsla(0., 0., 1., alpha)
}

fn black(alpha: f32) -> Hsla {
    hsla(0., 0., 0., alpha)
}

fn lighten(color: Hsla, amount: f32) -> Hsla {
    Hsla {
        l: (color.l + amount).clamp(0., 1.),
        ..color
    }
}
