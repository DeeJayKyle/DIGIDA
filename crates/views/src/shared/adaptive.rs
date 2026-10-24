use gpui::{App, Context, Entity, Task};
use state::{AppSettings, Playback, Queue, Reloaded, Sonora};
use ui::{CoverPalette, Look, Theme, ThemeOverrides};

/// Drives the theme's tint from the playing track's cover, and keeps the next
/// track's cover sampled ahead of time so a track change recolours on the frame
/// it happens rather than once a download and a decode have come back.
pub struct Adaptive {
    playback: Entity<Playback>,
    queue: Entity<Queue>,
    settings: Entity<AppSettings>,
    cover: Option<String>,
    /// The palette the theme is wearing, so a change that is not a track change can be put
    /// back on without sampling the cover again.
    painted: CoverPalette,
    /// The requested palette, kept so unrelated settings do not restart a running fade.
    applied: Option<(Look, ThemeOverrides)>,
    ahead: Option<Ahead>,
    task: Option<Task<()>>,
}

/// The cover of the track queued after the current one, with the palette once
/// the sampling has finished. The task is held so that requeueing cancels a
/// sampling that is no longer wanted; it is never cleared from inside itself.
struct Ahead {
    cover: String,
    palette: Option<CoverPalette>,
    _task: Task<()>,
}

impl Adaptive {
    pub fn new(playback: Entity<Playback>, cx: &mut Context<Self>) -> Self {
        let sonora = Sonora::global(cx);
        let (queue, settings) = (sonora.queue.clone(), sonora.settings.clone());
        let devices = sonora.devices.clone();
        cx.observe(&playback, |this, _, cx| this.sync(false, cx))
            .detach();
        cx.observe(&settings, |this, _, cx| {
            let settings = this.settings.read(cx);
            let sliding = this.applied.as_ref().is_some_and(|(look, _)| {
                look.kind == settings.look().kind
                    && look.ambient == settings.ambient_theme()
                    && look.ambient_strength != settings.ambient_strength()
            });
            this.sync(sliding, cx);
        })
        .detach();
        cx.observe(&devices, |this, _, cx| {
            this.sync(false, cx);
            this.look_ahead(cx);
        })
        .detach();
        // A reload can change what the theme is built from without changing its kind or tint,
        // so forget what was worn and paint it again.
        cx.subscribe(&settings, |this, _, _: &Reloaded, cx| {
            this.applied = None;
            this.sync(false, cx);
        })
        .detach();
        cx.observe(&queue, |this, _, cx| this.look_ahead(cx))
            .detach();

        let mut adaptive = Self {
            playback,
            queue,
            settings,
            cover: None,
            painted: CoverPalette::default(),
            applied: None,
            ahead: None,
            task: None,
        };
        adaptive.sync(false, cx);
        adaptive
    }

    /// Resamples the cover and hands the palette to the theme. `instant` skips the fade, for
    /// a change that is not a track change.
    fn sync(&mut self, instant: bool, cx: &mut Context<Self>) {
        let cover = self
            .settings
            .read(cx)
            .ambient_theme()
            .then(|| {
                self.playback
                    .read(cx)
                    .shown_track(cx)
                    .and_then(|track| track.cover)
            })
            .flatten();

        if cover == self.cover {
            // A settings change can alter the theme without changing the sampled cover.
            self.apply(self.painted, instant, cx);
            return;
        }
        self.cover = cover.clone();

        let Some(cover) = cover else {
            self.task = None;
            self.apply(CoverPalette::default(), instant, cx);
            self.look_ahead(cx);
            return;
        };

        // The track we sampled ahead is usually the one that just started, so
        // the recolour lands without a round trip.
        let ready = self
            .ahead
            .take_if(|ahead| ahead.cover == cover && ahead.palette.is_some())
            .and_then(|ahead| ahead.palette);
        match ready {
            Some(palette) => {
                self.task = None;
                self.apply(palette, instant, cx);
            }
            None => {
                let palette = ui::palette(cover, cx);
                self.task = Some(cx.spawn(async move |this, cx| {
                    let palette = palette.await;
                    this.update(cx, |this, cx| this.apply(palette, instant, cx))
                        .ok();
                }));
            }
        }
        self.look_ahead(cx);
    }

    /// Puts the palette and the kind on the theme, as a fade or as a swap on the spot.
    fn apply(&mut self, palette: CoverPalette, instant: bool, cx: &mut App) {
        self.painted = palette;
        let (look, overrides) = {
            let settings = self.settings.read(cx);
            (
                Look {
                    tint: palette.primary,
                    tint_secondary: palette.secondary,
                    tint_background: palette.background,
                    ..settings.look()
                },
                settings.theme_overrides(),
            )
        };

        if self
            .applied
            .as_ref()
            .is_some_and(|(last, colors)| *last == look && *colors == overrides)
        {
            return;
        }
        self.applied = Some((look, overrides.clone()));
        match instant {
            true => Theme::set(look, &overrides, cx),
            false => Theme::fade(look, &overrides, cx),
        }
    }

    /// Samples the cover of the track queued next, so its palette is waiting
    /// when it starts. A shuffle or a jump elsewhere in the library simply
    /// misses and falls back to sampling on the change.
    fn look_ahead(&mut self, cx: &mut Context<Self>) {
        if !self.settings.read(cx).ambient_theme() {
            self.ahead = None;
            return;
        }
        let next = match self.playback.read(cx).steered(cx) {
            Some(remote) => remote
                .upcoming
                .first()
                .and_then(|upcoming| upcoming.track.cover.clone()),
            None => {
                let queue = self.queue.read(cx);
                queue
                    .upcoming()
                    .next()
                    .or_else(|| queue.similar().next())
                    .and_then(|track| track.cover.clone())
            }
        };
        let Some(cover) = next else {
            self.ahead = None;
            return;
        };
        if self
            .ahead
            .as_ref()
            .is_some_and(|ahead| ahead.cover == cover)
        {
            return;
        }

        let palette = ui::palette(cover.clone(), cx);
        let wanted = cover.clone();
        let task = cx.spawn(async move |this, cx| {
            let palette = palette.await;
            this.update(cx, |this, _| {
                let Some(ahead) = this.ahead.as_mut().filter(|ahead| ahead.cover == wanted) else {
                    return;
                };
                ahead.palette = Some(palette);
            })
            .ok();
        });
        self.ahead = Some(Ahead {
            cover,
            palette: None,
            _task: task,
        });
    }
}
