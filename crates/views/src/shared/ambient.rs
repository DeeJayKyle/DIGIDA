//! The ambient background: one flowing colour field behind the app, five soft
//! blobs under a heavy blur, adapted to the base theme so text stays readable. `Root` owns the
//! one entity and blends it over the base theme, title bar included. Root divides
//! the window opacity between the base and this field so transparency stays intact.
//!
//! Ambient strength controls its contribution throughout browsing and fullscreen.
//! The animation preference decides whether the field drifts.
//!
//! Field colours share the theme transition; only blob positions animate here.

use std::cell::Cell;
use std::f32::consts::TAU;
use std::rc::Rc;
use std::time::Instant;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, BoxShadow, Context, Hsla, Pixels, Render, Task, Window, canvas, div,
    point, px,
};
use state::Sonora;
use ui::{ActiveTheme as _, AmbientPalette, CoverPalette, Theme};

/// How many blobs make up the field.
pub(crate) const BLOBS: usize = 5;
/// The widest blur, in device pixels, the renderer still runs at full resolution. It picks
/// the buffer from the radius: four or less keeps every pixel, eight halves the frame, and
/// anything wider drops to a quarter. The field is already drawn at `DOWNSCALE`, so a wider
/// blur would shrink it a second time and the upscale would crawl over every seam. The discs
/// carry the falloff instead.
const BLUR_FULL: f32 = 4.;
/// How many times smaller the field is drawn than it is shown. The layer is
/// laid out at this fraction of the window and the compositor scales it back
/// up with a bilinear sample, so the discs and the blur cost a sixteenth of
/// the pixels. Nothing in the field is sharper than the blur, so the upscale
/// shows nothing.
const DOWNSCALE: f32 = 4.;
/// One blob: base centre (fractions of the layer), diameter (fraction of the
/// layer's smaller side), drift period in seconds, phase, drift amplitude
/// (fractions).
///
/// A centre travels at 2 pi A W / T at its fastest, where A is its amplitude and W the width
/// it drifts across. On a 2560 wide window these periods put that around 65 pixels a second,
/// so a frame at the display rate moves a blob half a pixel. What reads as steps at this speed
/// is not the clock but the eight bit banding underneath, which holds a contour still and then
/// jumps it a whole band at once. The dither over the field is what deals with that.
const SPECS: [Spec; BLOBS] = [
    (0.22, 0.30, 1.10, 34., 0.0, 0.13, 0.10),
    (0.80, 0.24, 1.00, 27., 1.7, 0.11, 0.13),
    (0.52, 0.68, 1.20, 41., 3.4, 0.14, 0.09),
    (0.12, 0.78, 0.90, 24., 5.1, 0.10, 0.12),
    (0.85, 0.72, 0.72, 31., 2.5, 0.12, 0.11),
];

/// Concentric discs faking a radial falloff. They are what keeps the field smooth now that
/// the blur stays narrow enough to run at full resolution, and at `DOWNSCALE` a disc costs a
/// sixteenth of the fill it did at window size, so the stack can afford to be dense. The
/// spacing left between them has to stay inside the blur's reach or they read as rings.
const DISCS: usize = 24;
/// Opacity ramp from the outermost disc to the core. The discs stack, so a disc is far
/// fainter than the field it builds up to.
const DISC_FAINT: f32 = 0.027;
const DISC_STRONG: f32 = 0.079;
/// A framed field's blob, drawn as the blurred shadow of a circle rather than through a blur
/// layer: how much of the blob's span the sharp circle takes, how far its blur reaches as a
/// share of that span, and how strong its core is, which together match the stacked discs.
const CORE: f32 = 0.5;
const SOFT: f32 = 0.15;
const PEAK: f32 = 0.72;

/// One entry of `SPECS`.
type Spec = (f32, f32, f32, f32, f32, f32, f32);

/// The flowing colour field. One sits behind the whole window in fullscreen, and a preview can
/// hold another of its own made with `Ambient::framed`.
pub(crate) struct Ambient {
    started: Instant,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The palette of the cover a framed field follows, `None` for the one behind the window.
    framed: Option<Sampled>,
}

/// The palette of the cover a framed field follows, sampled by the field itself.
#[derive(Default)]
struct Sampled {
    cover: Option<String>,
    palette: CoverPalette,
    _task: Option<Task<()>>,
}

impl Ambient {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        // Turning the drift back on has no frame of its own to land in: with
        // motion off nothing asks for one.
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();

        let now = Instant::now();
        Self {
            started: now,
            bounds: Rc::new(Cell::new(Bounds::default())),
            framed: None,
        }
    }

    /// A field for a preview that fills a frame rather than the window. It takes its colours
    /// from the cover `follow` hands it whatever the adaptive theme says, and paints without a
    /// blur layer, so it fills its bounds edge to edge and the frame has to round it.
    pub(crate) fn framed(cx: &mut Context<Self>) -> Self {
        Self {
            framed: Some(Sampled::default()),
            ..Self::new(cx)
        }
    }

    /// Samples `cover` for a framed field once it differs from the one sampled last. A field
    /// behind the window takes its colours from the theme and ignores this.
    pub(crate) fn follow(&mut self, cover: Option<String>, cx: &mut Context<Self>) {
        let Some(sampled) = self.framed.as_mut() else {
            return;
        };
        if sampled.cover == cover {
            return;
        }
        sampled.cover = cover.clone();
        let Some(cover) = cover else {
            sampled._task = None;
            sampled.palette = CoverPalette::default();
            cx.notify();
            return;
        };
        let palette = ui::palette(cover, cx);
        sampled._task = Some(cx.spawn(async move |this, cx| {
            let palette = palette.await;
            this.update(cx, |this, cx| {
                if let Some(sampled) = this.framed.as_mut() {
                    sampled.palette = palette;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// The theme with the tint this field is painted from: the theme itself behind the window,
    /// and the sampled cover in a frame.
    fn tinted(&self, theme: Theme) -> Theme {
        match &self.framed {
            None => theme,
            Some(sampled) => Theme {
                tint: sampled.palette.primary,
                tint_secondary: sampled.palette.secondary,
                tint_background: sampled.palette.background,
                ..theme
            },
        }
    }
}

impl Render for Ambient {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = match self.framed.is_some() {
            true => Theme::for_kind(Sonora::global(cx).settings.read(cx).resolved_appearance()),
            false => *cx.theme(),
        };
        // Without motion the field is a still gradient: no frame requests, so
        // fullscreen stops redrawing once settled. Either the system preference
        // or the setting of its own is enough to stop it.
        let animates =
            ui::motion::animates(cx) && Sonora::global(cx).settings.read(cx).ambient_motion();
        // The drift asks for the next frame off the display's own clock rather than a timer of
        // its own. A timer is never in phase with the refresh, so a blob that moves a fraction
        // of a pixel per frame lands on one vsync, skips the next and doubles the one after,
        // which reads as a twitch however slow the motion is. Cost belongs in the field below,
        // not in the frame rate.
        let elapsed = match animates {
            true => {
                window.request_animation_frame();
                self.started.elapsed().as_secs_f32()
            }
            false => 0.,
        };
        let AmbientPalette { colors, background } = match self.framed.is_some() {
            true => AmbientPalette::new(&self.tinted(theme)),
            false => theme.ambient_palette,
        };
        // Root supplies the remaining base contribution, preserving the window opacity.
        let clarity = match theme.transparent {
            true => theme.background.a,
            false => 1.,
        } * match self.framed.is_some() {
            true => 1.,
            false => Sonora::global(cx).settings.read(cx).ambient_strength(),
        };
        if self.framed.is_some() {
            return self.flat(elapsed, colors, background, clarity, window);
        }
        // Geometry resolves against the layer's own pixel bounds, measured a
        // frame ago by the canvas, so the discs stay circular at any window
        // aspect ratio. The field is laid out at a fraction of those bounds
        // and scaled back up from the top left corner.
        let bounds = self.bounds.get();
        let wide = bounds.size.width.as_f32().max(1.) / DOWNSCALE;
        let high = bounds.size.height.as_f32().max(1.) / DOWNSCALE;
        // The field reaches into the corners the chrome rounds, and GPUI clips only to a
        // rectangle, so the sheet takes that radius itself.
        let radius = window_radius(window, cx);

        div()
            .id("ambient")
            .absolute()
            .inset_0()
            .overflow_hidden()
            .when_some(radius, |this, radius| this.rounded(radius))
            .opacity(clarity)
            .child(measure(&self.bounds))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .when_some(radius, |this, radius| this.rounded(radius))
                    .bg(background),
            )
            // The blur spreads the blobs past any shape they could be cut to, so a rounded window
            // fades them out toward every edge instead. The fade sits on an unblurred layer the
            // size of the window, since a blurred one measures it from past its own blur reach.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .when_some(radius, |this, radius| {
                        this.fade_edges(radius * 2., radius * 2.)
                            .fade_sides(radius * 2., radius * 2.)
                    })
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .w(px(wide))
                            .h(px(high))
                            .layer_scale(DOWNSCALE)
                            .layer_scale_origin(point(0., 0.))
                            .blur(px(BLUR_FULL / window.scale_factor()))
                            .children(SPECS.iter().zip(colors).map(|(spec, color)| {
                                let (x, y, grown) = drift(*spec, elapsed, wide, high);
                                div()
                                    .absolute()
                                    .left(px(x - grown / 2.))
                                    .top(px(y - grown / 2.))
                                    .size(px(grown))
                                    .children((0..DISCS).map(move |step| {
                                        let fraction = 1.
                                            - step as f32 / DISCS as f32 * (1. - 1. / DISCS as f32);
                                        let opacity = DISC_FAINT
                                            + step as f32 / (DISCS as f32 - 1.)
                                                * (DISC_STRONG - DISC_FAINT);
                                        let stepped = grown * fraction;
                                        div()
                                            .absolute()
                                            .left(px((grown - stepped) / 2.))
                                            .top(px((grown - stepped) / 2.))
                                            .size(px(stepped))
                                            .rounded_full()
                                            .bg(color.opacity(opacity))
                                    }))
                            })),
                    ),
            )
            // Every buffer the field passes through holds eight bits a channel, and a gradient
            // this wide and this dark steps through only a few dozen of them, so its steps read
            // as bands that slide with the blobs. The dither scatters each step over
            // neighbouring pixels instead.
            .child(ui::grain(window))
            .into_any_element()
    }
}

impl Ambient {
    /// The field of a framed preview, painted with no layer of its own so it is the same
    /// picture wherever the frame sits, under a scroller's fade included. Each blob is the
    /// blurred shadow of a circle, which falls off as smoothly as the blurred discs do. The
    /// frame clips it only to a rectangle, so its owner covers the rounded corners.
    fn flat(
        &self,
        elapsed: f32,
        colors: [Hsla; BLOBS],
        background: Hsla,
        clarity: f32,
        window: &Window,
    ) -> AnyElement {
        let bounds = self.bounds.get();
        let wide = bounds.size.width.as_f32().max(1.);
        let high = bounds.size.height.as_f32().max(1.);

        div()
            .id("ambient")
            .absolute()
            .inset_0()
            .overflow_hidden()
            .opacity(clarity)
            .child(measure(&self.bounds))
            .child(div().absolute().inset_0().bg(background))
            .children(SPECS.iter().zip(colors).map(|(spec, color)| {
                let (x, y, span) = drift(*spec, elapsed, wide, high);
                let core = span * CORE;
                // a drop shadow is cut away under its own element, so the circle stands off
                // past the field's left edge and throws its shadow back onto the blob's place
                let away = wide + span;
                div()
                    .absolute()
                    .left(px(x - core / 2. - away))
                    .top(px(y - core / 2.))
                    .size(px(core))
                    .rounded_full()
                    .shadow(vec![BoxShadow {
                        color: color.opacity(PEAK),
                        offset: point(px(away), px(0.)),
                        blur_radius: px(span * SOFT),
                        spread_radius: px(0.),
                        inset: false,
                    }])
            }))
            .child(ui::grain(window))
            .into_any_element()
    }
}

/// Whether the ambient background is on. Fullscreen reads it for the frosted
/// glass on its controls as well, which only has the field to blur.
pub(crate) fn shown(cx: &App) -> bool {
    let settings = Sonora::global(cx).settings.read(cx);
    settings.ambient_theme()
}

/// The radius of the window's own corners, where the chrome rounds them.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn window_radius(window: &Window, cx: &App) -> Option<Pixels> {
    crate::chrome::window_radius(Sonora::global(cx).settings.read(cx), cx, window)
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
fn window_radius(_window: &Window, _cx: &App) -> Option<Pixels> {
    None
}

/// A canvas that stores the bounds it is laid out at in `bounds`, which the field reads a
/// frame later to size its blobs.
fn measure(bounds: &Rc<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    let bounds = bounds.clone();
    canvas(
        move |got, window, _| {
            if bounds.replace(got) != got {
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// Where a blob stands `elapsed` seconds into its drift across a field `wide` by `high`, and
/// how wide it spans, all in the field's own units.
fn drift(spec: Spec, elapsed: f32, wide: f32, high: f32) -> (f32, f32, f32) {
    let (base_x, base_y, size, period, phase, amp_x, amp_y) = spec;
    let spin = TAU * elapsed / period;
    let x = base_x + amp_x * (spin + phase).sin();
    let y = base_y + amp_y * (spin * 0.83 + phase * 1.7).cos();
    let span = wide.min(high) * size * (1. + 0.12 * (spin * 0.6 + phase * 2.3).sin());
    (x * wide, y * high, span)
}
