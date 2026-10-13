use std::time::Instant;

use gpui::prelude::*;
use gpui::{Context, Entity, Hsla, Render, Window, div};
use state::{AppSettings, Playback, PlaybackState, Sonora};
use ui::ActiveTheme as _;
use ui::motion::animates;

/// How many of the analyzer's lowest bands drive the glow, about 100 to 215 Hz, where the kick
/// and the bass line sit.
const BASS_BANDS: usize = 6;
/// How fast the running bass level catches up with the bass, per second. The glow draws what
/// rises above it, so a hit flares and a held note fades back out.
const FLOOR_RATE: f32 = 4.;
/// How far the bass has to rise above its running level to light the glow fully.
const GAIN: f32 = 3.;
/// How fast the drawn pulse rises, per second.
const RISE: f32 = 40.;
/// How fast the drawn pulse falls, per second.
const FALL: f32 = 10.;
/// The longest frame gap fed into one step, in seconds, so a stalled window does not jump.
const STEP_MAX: f32 = 0.08;
/// The pulse under which nothing is drawn and no more frames are asked for.
const PULSE_MIN: f32 = 0.01;
/// The glow's opacity at full pulse.
const OPACITY: f32 = 0.8;
/// The glow's blur radius, as a share of the artwork side.
const BLUR: f32 = 0.1;
/// How far past the artwork the glow grows at full pulse, as a share of its side.
const SWELL: f32 = 0.08;

/// A theme-tinted glow behind the player bar's artwork that flares with the bass. It is its own
/// entity so the frames it asks for while music plays repaint the glow alone, not the whole
/// player bar. It follows the album pulse setting and stays off under reduced motion.
pub(crate) struct ArtworkGlow {
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    /// The running bass level the pulse is measured against.
    floor: f32,
    /// The pulse as drawn, from 0 to 1.
    pulse: f32,
    /// When the pulse last moved.
    beat: Option<Instant>,
}

impl ArtworkGlow {
    pub(crate) fn new(playback: Entity<Playback>, cx: &mut Context<Self>) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        Self {
            playback,
            settings,
            floor: 0.,
            pulse: 0.,
            beat: None,
        }
    }

    /// Moves the running level and the pulse on by the time since the last frame.
    fn step(&mut self, bass: f32) {
        let now = Instant::now();
        let step = self.beat.replace(now).map_or(0., |beat| {
            now.duration_since(beat).as_secs_f32().min(STEP_MAX)
        });
        self.floor += (bass - self.floor) * ease(FLOOR_RATE, step);
        let target = ((bass - self.floor) * GAIN).clamp(0., 1.);
        let rate = match target > self.pulse {
            true => RISE,
            false => FALL,
        };
        self.pulse += (target - self.pulse) * ease(rate, step);
    }
}

impl Render for ArtworkGlow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let allowed = self.settings.read(cx).album_pulse() && animates(cx);
        let playback = self.playback.read(cx);
        let playing = allowed && *playback.state() == PlaybackState::Playing;
        let bass = match playing {
            true => playback.spectrum(cx).map_or(0., |spectrum| {
                let bands = spectrum.bands();
                let bass = &bands[..BASS_BANDS.min(bands.len())];
                bass.iter().sum::<f32>() / bass.len().max(1) as f32
            }),
            false => 0.,
        };
        self.step(bass);

        let shown = allowed && self.pulse > PULSE_MIN;
        if playing || shown {
            window.request_animation_frame();
        }

        let theme = cx.theme();
        // The player bar draws its artwork at this side, and the glow sits in a box of the same.
        let side = ui::snapped(theme.metrics.row, window);
        let glow = Hsla {
            a: OPACITY * self.pulse,
            ..theme.foreground
        };

        div().absolute().inset_0().when(shown, |this| {
            this.child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(ui::artwork_radius(cx))
                    .bg(glow)
                    .blur(side * BLUR)
                    .layer_scale(1. + SWELL * self.pulse),
            )
        })
    }
}

/// The share of the gap an exponential follower at `rate` per second closes in `step` seconds.
fn ease(rate: f32, step: f32) -> f32 {
    1. - (-rate * step).exp()
}
