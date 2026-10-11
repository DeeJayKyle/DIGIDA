use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use librespot_core::{Session, SpotifyUri};
use librespot_playback::config::{Bitrate, PlayerConfig};
use librespot_playback::mixer::NoOpVolume;
use librespot_playback::player::{Player, PlayerEvent};
use librespot_playback::{NUM_CHANNELS, SAMPLE_RATE};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::audio::{Chain, Volume};
use crate::sink::Cue;
use crate::spectrum::Spectrum;
use crate::spotify::sink::{OutputSink, RateControl};
use crate::{
    PlaybackConfig, PlaybackEvent, PlaybackEvents, PlaybackFactory, Player as MusicPlayer,
};

const TRACK_PREFIX: &str = "spotify:track:";
/// Position reports that can still come from before a seek or load: one already in the channel
/// and one for a packet decoded before the player read the command.
const STALE_POSITIONS: u8 = 2;

/// The engine's events, with one correction: librespot says `Playing` when the decoder is
/// ready and `Seeked` when it has moved, both before it has buffered or written anything. Each
/// waits here for the sink's next write, which the same player thread makes strictly after it.
pub struct Events {
    player: UnboundedReceiver<PlayerEvent>,
    output: UnboundedReceiver<()>,
    cue: Cue,
    written: UnboundedReceiver<()>,
    /// The `Playing` or `Seeked` waiting for the sink; a newer one replaces an older one.
    held: Option<PlaybackEvent>,
    /// Position reports seen while the cue refuses writes. A seek or load that took keeps the
    /// player thread busy, so a run of them means it never left the old position.
    stale: u8,
    rate: RateControl,
    heard: Option<HeardPosition>,
}

/// Tracks the position that has reached the speakers, accounting for samples already queued.
struct HeardPosition {
    at: Duration,
    played: u64,
    speed: f32,
}

impl HeardPosition {
    /// Advances from samples consumed by the output, rather than librespot's decode head.
    fn advance(&mut self, cue: &Cue, speed: f32) -> Duration {
        let played = cue.played();
        let frames = played.saturating_sub(self.played) / u64::from(NUM_CHANNELS);
        self.at +=
            Duration::from_secs_f64(frames as f64 / f64::from(SAMPLE_RATE) * f64::from(self.speed));
        self.played = played;
        self.speed = speed;
        self.at
    }
}

/// Reads the selected tempo without holding the control lock during audio processing.
fn speed(rate: &RateControl) -> f32 {
    rate.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .0
        .clamp(0.5, 2.)
}

#[async_trait]
impl PlaybackEvents for Events {
    async fn next(&mut self) -> Option<PlaybackEvent> {
        loop {
            // librespot's event always precedes the write it led to, so it is taken first when
            // both are ready at once.
            tokio::select! {
                biased;
                event = self.player.recv() => {
                    let Some(event) = translate(event?) else { continue };
                    match event {
                        PlaybackEvent::Playing { .. } | PlaybackEvent::Seeked { .. } => {
                            self.cue.arm();
                            self.stale = 0;
                            self.held = Some(event);
                        }
                        PlaybackEvent::Position { .. } if self.cue.cleared() => {
                            self.stale += 1;
                            if self.stale > STALE_POSITIONS {
                                log::warn!("playback: the engine kept its old position, letting audio through");
                                self.cue.arm();
                                self.stale = 0;
                            }
                            return Some(event);
                        }
                        PlaybackEvent::Position { id, at } => {
                            let at = self
                                .heard
                                .as_mut()
                                .map(|heard| heard.advance(&self.cue, speed(&self.rate)))
                                .unwrap_or(at);
                            return Some(PlaybackEvent::Position { id, at });
                        }
                        PlaybackEvent::Length { .. } => {
                            return Some(event);
                        }
                        event => {
                            self.held = None;
                            self.stale = 0;
                            return Some(event);
                        }
                    }
                }
                written = self.written.recv() => {
                    written?;
                    if let Some(event) = self.held.take() {
                        let at = match &event {
                            PlaybackEvent::Playing { at, .. }
                            | PlaybackEvent::Seeked { at, .. } => Some(*at),
                            _ => None,
                        };
                        self.heard = at.map(|at| HeardPosition {
                            at,
                            played: self.cue.played(),
                            speed: speed(&self.rate),
                        });
                        return Some(event);
                    }
                }
                changed = self.output.recv() => {
                    changed?;
                    return Some(PlaybackEvent::OutputChanged);
                }
            }
        }
    }
}

/// Builds a Spotify engine on a connected session.
pub struct Factory(Session);

impl Factory {
    pub fn new(session: Session) -> Self {
        Self(session)
    }
}

impl PlaybackFactory for Factory {
    fn start(&self, config: PlaybackConfig) -> (Box<dyn MusicPlayer>, Box<dyn PlaybackEvents>) {
        let (engine, events) = Engine::start(self.0.clone(), config);
        (Box::new(engine), Box::new(events))
    }
}

/// The librespot player with our sink behind it. Every method queues a command to librespot's
/// own thread, which answers through `Events`.
pub struct Engine {
    player: Arc<Player>,
    volume: Volume,
    cue: Cue,
    spectrum: Spectrum,
    gapless: bool,
    rate: RateControl,
}

impl Engine {
    fn start(session: Session, config: PlaybackConfig) -> (Self, Events) {
        let volume = Volume::new(config.gain);
        let rate: RateControl = Arc::new(Mutex::new((1., 0.)));
        let (cue, written) = Cue::new();
        let spectrum = Spectrum::new();

        let player_config = PlayerConfig {
            bitrate: Bitrate::Bitrate320,
            normalisation: config.normalisation,
            gapless: config.gapless,
            position_update_interval: Some(config.position_interval),
            ..Default::default()
        };

        let sink_cue = cue.clone();
        let chain = Chain {
            volume: volume.clone(),
            equalizer: config.equalizer.clone(),
            spectrum: spectrum.clone(),
        };
        let (output_tx, output_rx) = unbounded_channel();
        let sink_rate = rate.clone();
        let player = Player::new(player_config, session, Box::new(NoOpVolume), move || {
            OutputSink::boxed(sink_cue, chain, output_tx.clone(), sink_rate.clone())
        });

        let events = Events {
            player: player.get_player_event_channel(),
            output: output_rx,
            cue: cue.clone(),
            written,
            held: None,
            stale: 0,
            rate: rate.clone(),
            heard: None,
        };
        let engine = Self {
            player,
            volume,
            cue,
            spectrum,
            gapless: config.gapless,
            rate,
        };
        (engine, events)
    }

    fn load(&self, track_id: &str, at: Duration, seamless: bool) -> Result<()> {
        let uri = track_uri(track_id)?;

        if !seamless || !self.gapless {
            self.cue.clear();
        }
        self.player.load(uri, true, at.as_millis() as u32);
        Ok(())
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        let uri = track_uri(track_id)?;

        self.cue.clear();
        self.player.load(uri, false, at.as_millis() as u32);
        Ok(())
    }

    fn preload(&self, track_id: &str) -> Result<()> {
        self.player.preload(track_uri(track_id)?);
        Ok(())
    }

    fn play(&self) {
        self.player.play();
    }

    fn pause(&self) {
        self.player.pause();
    }

    fn seek(&self, position: Duration) {
        self.player.seek(position.as_millis() as u32);
        self.cue.clear();
    }

    fn set_gain(&self, gain: f32) {
        self.volume.set(gain);
    }

    fn spectrum(&self) -> Spectrum {
        self.spectrum.clone()
    }
}

impl MusicPlayer for Engine {
    fn load(&self, track_id: &str, at: Duration, seamless: bool) -> Result<()> {
        self.load(track_id, at, seamless)
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        self.load_paused_at(track_id, at)
    }

    fn preload(&self, track_id: &str, _segue: bool) -> Result<()> {
        self.preload(track_id)
    }

    fn play(&self) {
        self.play();
    }

    fn pause(&self) {
        self.pause();
    }

    fn seek(&self, position: Duration) {
        self.seek(position);
    }

    fn set_gain(&self, gain: f32) {
        self.set_gain(gain);
    }

    fn set_speed_pitch(&self, speed: f32, pitch_semitones: f32) {
        *self.rate.lock().unwrap_or_else(PoisonError::into_inner) =
            (speed.clamp(0.5, 2.), pitch_semitones.clamp(-12., 12.));
    }

    fn spectrum(&self) -> Option<Spectrum> {
        Some(self.spectrum())
    }
}

fn track_uri(track_id: &str) -> Result<SpotifyUri> {
    SpotifyUri::from_uri(&format!("{TRACK_PREFIX}{track_id}"))
        .with_context(|| format!("{track_id} is not a track id"))
}

/// librespot's event as ours, or `None` for the ones the state has no use for. A denied audio
/// key is `Refused` and one refused for now is `Throttled`. Any other unavailability names the
/// track.
fn translate(event: PlayerEvent) -> Option<PlaybackEvent> {
    let millis = |position_ms: u32| Duration::from_millis(position_ms as u64);
    let track_id = |uri: SpotifyUri| uri.to_id().ok();

    match event {
        PlayerEvent::Loading {
            track_id: uri,
            position_ms,
            ..
        } => Some(PlaybackEvent::Loading {
            id: track_id(uri),
            at: millis(position_ms),
        }),
        PlayerEvent::Playing {
            track_id: uri,
            position_ms,
            ..
        } => Some(PlaybackEvent::Playing {
            id: track_id(uri),
            at: millis(position_ms),
        }),
        PlayerEvent::Paused {
            track_id: uri,
            position_ms,
            ..
        } => Some(PlaybackEvent::Paused {
            id: track_id(uri),
            at: millis(position_ms),
        }),
        PlayerEvent::PositionChanged {
            track_id: uri,
            position_ms,
            ..
        }
        | PlayerEvent::PositionCorrection {
            track_id: uri,
            position_ms,
            ..
        } => Some(PlaybackEvent::Position {
            id: track_id(uri),
            at: millis(position_ms),
        }),
        PlayerEvent::Seeked {
            track_id: uri,
            position_ms,
            ..
        } => Some(PlaybackEvent::Seeked {
            id: track_id(uri),
            at: millis(position_ms),
        }),
        PlayerEvent::Stopped { track_id: uri, .. }
        | PlayerEvent::EndOfTrack { track_id: uri, .. } => {
            Some(PlaybackEvent::Ended { id: track_id(uri) })
        }
        PlayerEvent::Unavailable { denied: true, .. } => Some(PlaybackEvent::Refused),
        PlayerEvent::Unavailable {
            throttled: true,
            track_id: uri,
            ..
        } => Some(PlaybackEvent::Throttled { id: track_id(uri) }),
        PlayerEvent::Unavailable { track_id: uri, .. } => {
            Some(PlaybackEvent::Unavailable { id: track_id(uri) })
        }
        _ => None,
    }
}
