//! The Theme section of Appearance: one card per theme on a rail that scrolls sideways, each
//! card a soft, flat miniature of the Sonora window painted in that theme.

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Div, Pixels, SharedString, div, linear_color_stop, linear_gradient, px,
    relative, svg,
};
use i18n::t;
use state::ThemeFamily;
use ui::{
    ActiveTheme as _, Menu, MenuItem, OptionCard, Picker, Rail, Theme, ThemeKind, ThemeOverrides,
};

use super::SettingsView;

/// How wide a theme card stands.
const CARD: Pixels = px(150.);
/// How tall a theme card's picture stands, its border included.
const THUMB: Pixels = px(96.);
/// How thick the card's border is, which the miniature sits inside.
const EDGE: Pixels = px(1.);
/// The miniature window inside the card's border, in whole pixels.
const WIDTH: f32 = 148.;
const HEIGHT: f32 = 94.;
/// How many variants of a family its card shows side by side at most.
const SLICES: usize = 4;
/// The side of a variant's swatch in the menu.
const SWATCH: f32 = 14.;
/// How large the open folder glyph draws on its card.
const GLYPH: Pixels = px(18.);
/// How much of the theme's corner radius the miniature's tiles keep.
const RADIUS_SCALE: f32 = 0.3;
/// How strongly the bars show against what they sit on, the first bar of the sidebar standing
/// for the active entry, and how much of its strength the accent tile keeps.
const BAR: f32 = 0.2;
const ACTIVE_BAR: f32 = 0.4;
const ACCENT: f32 = 0.9;

/// The sidebar's width. It runs the full height, so it alone carries the left corners.
const SIDEBAR: f32 = 38.;
/// The columns a cut between two palettes may run down: the sidebar's edge and the middle of
/// each gap between two tiles, so a cut never splits a tile or the play button.
const SEAMS: [f32; 3] = [SIDEBAR, 76., 110.];
/// How far a slice runs on under the slice after it.
const SEAM_OVERLAP: f32 = 1.;
/// The sidebar's entries and the content's heading, as left, top, width and height.
const ENTRIES: [[f32; 4]; 3] = [[8., 14., 22., 5.], [8., 25., 16., 5.], [8., 36., 19., 5.]];
const HEADING: [f32; 4] = [45., 14., 34., 5.];
/// The row of tiles under the heading by their left edges, the first one in the accent.
const TILES: [f32; 3] = [45., 79., 113.];
const TILE_TOP: f32 = 27.;
const TILE: f32 = 28.;
/// The player floating at the foot of the content, and its play button.
const PLAYER: [f32; 4] = [45., 66., 96., 14.];
const PLAY: [f32; 4] = [50., 70., 6., 6.];

/// One card of the rail: a built-in theme or a family of theme files.
enum Entry {
    Default,
    Ambient,
    Family(ThemeFamily),
}

impl SettingsView {
    /// The light/dark policy is independent of the selected colour family.
    pub(super) fn appearance_cards(
        &self,
        window: &gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chosen = self.settings.read(cx).appearance_mode();
        let radius = cx.theme().radius;
        super::rows::grid(ThemeKind::ALL.len() as u16, window, cx)
            .children(ThemeKind::ALL.map(|kind| {
                let palettes = match kind {
                    ThemeKind::System => vec![Theme::dark(), Theme::light()],
                    _ => vec![Theme::for_kind(kind)],
                };
                OptionCard::new(kind.id())
                    .thumb_height(THUMB)
                    .thumb(sliced(&palettes, radius))
                    .label(kind.label())
                    .selected(chosen == kind)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_appearance_mode(kind, cx));
                    }))
            }))
            .into_any_element()
    }

    pub(super) fn theme_cards(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let (selected, chosen_family, families) = {
            let settings = self.settings.read(cx);
            (
                settings.theme().to_owned(),
                settings.theme_family().map(|family| family.id.clone()),
                settings.theme_families().to_vec(),
            )
        };

        let mode = self.settings.read(cx).appearance_mode();
        let mut cards = [Entry::Default, Entry::Ambient]
            .into_iter()
            .chain(
                families
                    .into_iter()
                    .filter(|family| family.variants_for(mode).next().is_some())
                    .map(Entry::Family),
            )
            .map(|entry| {
                let chosen = match &entry {
                    Entry::Default => selected == "default",
                    Entry::Ambient => selected == "ambient",
                    Entry::Family(family) => chosen_family.as_ref() == Some(&family.id),
                };
                self.theme_card(entry, chosen, cx)
            })
            .collect::<Vec<_>>();

        if selected != "default" && selected != "ambient" && chosen_family.is_none() {
            cards.push(
                OptionCard::new("unavailable-theme")
                    .w(CARD)
                    .thumb_height(THUMB)
                    .label(t!("theme-unavailable", name = selected.clone()))
                    .selected(true)
                    .disabled(true)
                    .into_any_element(),
            );
        }

        let settings = self.settings.clone();
        cards.push(
            OptionCard::new("open-theme-folder")
                .w(CARD)
                .thumb_height(THUMB)
                .dashed()
                .thumb(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path(icons::path("icons/folder-plus.svg"))
                                .size(GLYPH)
                                .text_color(theme.muted_foreground),
                        ),
                )
                .label(t!("settings-theme-folder"))
                .on_click(move |_, _, cx| {
                    let path =
                        settings.update(cx, |settings, _| settings.ensure_themes_directory());
                    if let Err(error) = open::that_detached(&path) {
                        log::warn!("settings: cannot open {}: {error}", path.display());
                    }
                })
                .into_any_element(),
        );

        // the ring around the chosen card stands outside it, so the rail leaves it room
        let room = theme.metrics.pad;
        Rail::new("theme-cards")
            .py(room)
            .arrows_at(room + THUMB / 2.)
            .children(cards)
            .into_any_element()
    }

    /// One card: its miniature and its name. A family of several variants names the variant a
    /// click picks after its own name, and that line opens the menu of all of them.
    fn theme_card(&self, entry: Entry, chosen: bool, cx: &mut Context<Self>) -> AnyElement {
        let radius = cx.theme().radius;

        match entry {
            Entry::Default | Entry::Ambient => {
                let ambient = matches!(entry, Entry::Ambient);
                let settings = self.settings.read(cx);
                let kind = settings.resolved_appearance();
                let mut palette = Theme::for_kind(kind);
                let (id, label) = match ambient {
                    true => ("ambient", t!("settings-ambient")),
                    false => ("default", t!("settings-theme-default")),
                };
                if ambient {
                    let sampled = self
                        .previews
                        .playing_or_picked(cx)
                        .cover
                        .as_deref()
                        .and_then(|cover| ui::cover_palette(cover, cx))
                        .unwrap_or_default();
                    palette = Theme::for_look(
                        ui::Look {
                            ambient: true,
                            ambient_strength: 1.,
                            tint: Some(sampled.primary.unwrap_or(palette.selection)),
                            tint_secondary: sampled.secondary,
                            tint_background: sampled.background,
                            ..settings.look()
                        },
                        &ThemeOverrides::default(),
                    );
                    palette.background = palette.ambient_palette.background;
                }
                OptionCard::new(id)
                    .w(CARD)
                    .thumb_height(THUMB)
                    .thumb(sliced(&[palette], radius))
                    .label(label)
                    .selected(chosen)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.set_theme(id, cx));
                    }))
                    .into_any_element()
            }
            Entry::Family(family) => {
                let mode = self.settings.read(cx).appearance_mode();
                let variants = family.variants_for(mode);
                let palettes = variants
                    .clone()
                    .take(SLICES)
                    .map(|variant| variant.palette())
                    .collect::<Vec<_>>();
                let several = variants.count() > 1;
                // the variant on show while the family is chosen, else the one a click would pick
                let settings = self.settings.read(cx);
                let picked = match chosen {
                    true => settings.theme_variant().map(|(_, variant)| variant),
                    false => settings.family_variant(&family.id),
                }
                .map(|variant| variant.name.clone());
                let label = match &picked {
                    Some(name) if family.variants.len() > 1 => {
                        format!("{} {name}", family.name)
                    }
                    _ => family.name.clone(),
                };
                let id = family.id.clone();

                let card =
                    OptionCard::new(SharedString::from(format!("theme-family:{}", family.id)))
                        .w(CARD)
                        .thumb_height(THUMB)
                        .thumb(sliced(&palettes, radius))
                        .label(label)
                        .selected(chosen)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.settings.update(cx, |settings, cx| {
                                settings.set_theme_family(&id, cx);
                            });
                            cx.notify();
                        }));
                if mode == ThemeKind::System {
                    let controls = div().flex().flex_col().items_center().gap_1().children(
                        [ThemeKind::Light, ThemeKind::Dark]
                            .into_iter()
                            .filter_map(|kind| self.card_variant_picker(&family, kind, cx)),
                    );
                    return card.controls(controls).into_any_element();
                }
                card.when(several && picked.is_some(), |card| {
                    let kind = self.settings.read(cx).resolved_appearance();
                    let key = SharedString::from(format!("theme-variants:{}", family.id));
                    let menu = Menu::new("theme-variants-menu")
                        .w(Picker::NARROW)
                        .when(self.popovers.shows(&key), |menu| {
                            menu.items(self.variant_items(&family, kind, cx))
                        });
                    card.variant_menu(key, &self.popovers, menu)
                })
                .into_any_element()
            }
        }
    }

    /// Every System theme card exposes independent light and dark choices before selection.
    fn card_variant_picker(
        &self,
        family: &ThemeFamily,
        kind: ThemeKind,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let chosen = self.settings.read(cx).variant_for(&family.id, kind)?;
        let key = SharedString::from(format!("theme-variant:{}:{}", family.id, kind.id()));
        let label = t!(
            "settings-theme-variant-mode",
            mode = kind.label().to_string(),
            name = chosen.name.clone()
        );
        let open = self.popovers.shows(&key);
        Some(
            Picker::new(key, &self.popovers, label)
                .when(open, |picker| {
                    picker.items(self.variant_items(family, kind, cx))
                })
                .into_any_element(),
        )
    }

    /// Shares variant selection and remembered-mode handling between both card dropdown layouts.
    fn variant_items(
        &self,
        family: &ThemeFamily,
        kind: ThemeKind,
        cx: &mut Context<Self>,
    ) -> Vec<MenuItem> {
        let chosen = self
            .settings
            .read(cx)
            .variant_for(&family.id, kind)
            .map(|variant| variant.id.clone());
        family
            .variants_for(kind)
            .map(|variant| {
                let id = variant.id.clone();
                let family_id = family.id.clone();
                MenuItem::new(
                    SharedString::from(format!("theme-variant:{}", variant.id)),
                    variant.name.clone(),
                )
                .leading(swatch(&variant.palette()))
                .checked(chosen.as_deref() == Some(variant.id.as_str()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.set_theme_variant(&family_id, &id, cx)
                    });
                    this.popovers.close();
                }))
            })
            .collect()
    }
}

/// The miniature cut into one vertical slice per palette, left to right, each slice the same
/// window painted in its own palette so the layout runs on across the cuts. The cuts run down
/// `SEAMS` and show only as the change of colour. `radius` is the card's, whose inside the
/// miniature's outer corners follow.
fn sliced(palettes: &[Theme], radius: Pixels) -> Div {
    let count = palettes.len().max(1);
    let cuts = (1..count).map(|index| {
        let even = WIDTH * index as f32 / count as f32;
        SEAMS
            .into_iter()
            .min_by(|a, b| (a - even).abs().total_cmp(&(b - even).abs()))
            .unwrap_or(even)
    });
    let edges: Vec<f32> = std::iter::once(0.)
        .chain(cuts)
        .chain(std::iter::once(WIDTH))
        .collect();
    let mark = (radius * RADIUS_SCALE).floor();
    // the card does not clip to its corners, so the miniature rounds its own inside the border
    let outer = (radius - EDGE).max(Pixels::ZERO).min(px(SIDEBAR / 2.));

    div().relative().size_full().children(
        palettes
            .iter()
            .zip(edges.windows(2))
            // each slice runs one pixel on under the next, so a cut that lands between pixels
            // while the rail scrolls never opens a seam, but stops short of the far corners,
            // whose soft edges only the last slice draws
            .map(|(palette, edge)| {
                let left = edge[0];
                let right = match edge[1] < WIDTH {
                    true => edge[1] + SEAM_OVERLAP,
                    false => WIDTH,
                };
                rect([left, 0., right - left, HEIGHT])
                    .overflow_hidden()
                    .child(
                        miniature(palette, outer, mark)
                            .w(relative(WIDTH / (right - left)))
                            .left(relative(-left / (right - left))),
                    )
            }),
    )
}

/// The Sonora window in `palette`'s colours as a few soft, flat shapes, `WIDTH` by `HEIGHT`
/// pixels: the sidebar with three entries, a heading over a row of tiles, the first in the
/// accent, and a floating player with its play button. Nothing is outlined. The window and the
/// sidebar round the outer corners by `outer`, and `mark` rounds the tiles.
fn miniature(palette: &Theme, outer: Pixels, mark: Pixels) -> Div {
    let panel = match palette.sidebar == palette.background {
        true => palette.secondary,
        false => palette.sidebar,
    };
    let bar = |shape: [f32; 4], strength: f32| {
        rect(shape)
            .rounded_full()
            .bg(palette.foreground.opacity(strength))
    };

    div()
        .absolute()
        .top_0()
        .size_full()
        .rounded(outer)
        .bg(palette.background)
        .when_some(palette.tint_background, |this, color| {
            this.bg(linear_gradient(
                135.,
                linear_color_stop(color, 0.),
                linear_color_stop(palette.background, 1.),
            ))
        })
        .child(rect([0., 0., SIDEBAR, HEIGHT]).rounded_l(outer).bg(panel))
        .children(ENTRIES.into_iter().enumerate().map(|(index, entry)| {
            bar(
                entry,
                match index {
                    0 => ACTIVE_BAR,
                    _ => BAR,
                },
            )
        }))
        .child(bar(HEADING, ACTIVE_BAR))
        .children(TILES.into_iter().enumerate().map(|(index, left)| {
            let fill = match index {
                0 => palette.primary.opacity(ACCENT),
                _ => palette.muted,
            };
            rect([left, TILE_TOP, TILE, TILE]).rounded(mark).bg(fill)
        }))
        .child(rect(PLAYER).rounded_full().bg(panel))
        .child(rect(PLAY).rounded_full().bg(palette.primary))
}

/// A variant's swatch in the menu: a circle cut on the diagonal into its page colour at the top
/// left and its accent at the bottom right. The cut blends over one pixel so its edge stays
/// smooth.
fn swatch(palette: &Theme) -> Div {
    let blend = 0.5 / SWATCH;
    div()
        .size(px(SWATCH))
        .flex_none()
        .rounded_full()
        .bg(linear_gradient(
            135.,
            linear_color_stop(palette.background, 0.5 - blend),
            linear_color_stop(palette.primary, 0.5 + blend),
        ))
}

/// A proportional box expressed in the miniature’s reference coordinates.
fn rect([x, y, width, height]: [f32; 4]) -> Div {
    div()
        .absolute()
        .left(relative(x / WIDTH))
        .top(relative(y / HEIGHT))
        .w(relative(width / WIDTH))
        .h(relative(height / HEIGHT))
}
