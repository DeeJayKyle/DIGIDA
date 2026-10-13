use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use librespot_core::{Session, SpotifyUri};
use librespot_playback::config::{Bitrate, PlayerConfig};
use librespot_playback::mixer::NoOpVolume;
use librespot_playback::player::{Player, PlayerEvent};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::Device;
use crate::audio::{Chain, Volume};
use crate::sink::Cue;
use crate::spectrum::Spectrum;
use crate::spotify::connect::Connect;
use crate::spotify::sink::OutputSink;
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
    devices: UnboundedReceiver<Vec<Device>>,
    cue: Cue,
    written: UnboundedReceiver<()>,
    /// The `Playing` or `Seeked` waiting for the sink; a newer one replaces an older one.
    held: Option<PlaybackEvent>,
    /// Position reports seen while the cue refuses writes. A seek or load that took keeps the
    /// player thread busy, so a run of them means it never left the old position.
    stale: u8,
    /// The play request that last ended or went unavailable.
    ended: Option<u64>,
}

/// Spirc stops the player after a track ends or goes unavailable, which reports `Stopped` for
/// the same play request. The end already reached the state, so a second one would skip a
/// track. A `Stopped` with no end before it is a remote skip and passes.
fn repeats_an_end(ended: &mut Option<u64>, event: &PlayerEvent) -> bool {
    match event {
        PlayerEvent::EndOfTrack {
            play_request_id, ..
        }
        | PlayerEvent::Unavailable {
            play_request_id, ..
        } => {
            *ended = Some(*play_request_id);
            false
        }
        PlayerEvent::Stopped {
            play_request_id, ..
        } => ended.take() == Some(*play_request_id),
        _ => false,
    }
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
                    let event = event?;
                    if repeats_an_end(&mut self.ended, &event) {
                        continue;
                    }
                    let Some(event) = translate(event) else { continue };
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
                        PlaybackEvent::Position { .. } | PlaybackEvent::Length { .. } => {
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
                        return Some(event);
                    }
                }
                changed = self.output.recv() => {
                    changed?;
                    return Some(PlaybackEvent::OutputChanged);
                }
                // Without Connect the sender is gone, which must not end the events.
                Some(devices) = self.devices.recv() => {
                    return Some(PlaybackEvent::Devices(devices));
                }
            }
        }
    }
}

/// Builds a Spotify engine on a connected session.
pub struct Factory {
    session: Session,
    /// The runtime Spotify Connect runs on: engines start on the UI thread, outside it.
    handle: Option<Handle>,
}

impl Factory {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            handle: Handle::try_current().ok(),
        }
    }
}

impl PlaybackFactory for Factory {
    fn start(&self, config: PlaybackConfig) -> (Box<dyn MusicPlayer>, Box<dyn PlaybackEvents>) {
        let (engine, events) = Engine::start(self.session.clone(), self.handle.as_ref(), config);
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
    connect: Connect,
}

impl Engine {
    fn start(session: Session, handle: Option<&Handle>, config: PlaybackConfig) -> (Self, Events) {
        let volume = Volume::new(config.gain);
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
        let player = Player::new(
            player_config,
            session.clone(),
            Box::new(NoOpVolume),
            move || OutputSink::boxed(sink_cue, chain, output_tx.clone()),
        );
        let (devices_tx, devices_rx) = unbounded_channel();
        let connect = Connect::start(handle, &session, player.clone(), devices_tx);

        let events = Events {
            player: player.get_player_event_channel(),
            output: output_rx,
            devices: devices_rx,
            cue: cue.clone(),
            written,
            held: None,
            stale: 0,
            ended: None,
        };
        let engine = Self {
            player,
            volume,
            cue,
            spectrum,
            gapless: config.gapless,
            connect,
        };
        (engine, events)
    }

    fn load(&self, track_id: &str, at: Duration, seamless: bool) -> Result<()> {
        let uri = track_uri(track_id)?;

        if !seamless || !self.gapless {
            self.cue.clear();
        }
        let at = at.as_millis() as u32;
        if !self.connect.load(&uri, true, at) {
            self.player.load(uri, true, at);
        }
        Ok(())
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        let uri = track_uri(track_id)?;

        self.cue.clear();
        let at = at.as_millis() as u32;
        if !self.connect.load(&uri, false, at) {
            self.player.load(uri, false, at);
        }
        Ok(())
    }

    fn preload(&self, track_id: &str) -> Result<()> {
        self.player.preload(track_uri(track_id)?);
        Ok(())
    }

    /// Through Spirc when it loaded the track: it ignores a resume it did not ask for.
    /// Pause and seek go straight to the player, and Spirc follows their events.
    fn play(&self) {
        if !self.connect.play() {
            self.player.play();
        }
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

    fn transfer(&self, device_id: &str) {
        self.connect.transfer(device_id);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> SpotifyUri {
        track_uri("4uLU6hMCjMI75M1A2tKUQC").unwrap()
    }

    fn end(play_request_id: u64) -> PlayerEvent {
        PlayerEvent::EndOfTrack {
            play_request_id,
            track_id: uri(),
        }
    }

    fn stop(play_request_id: u64) -> PlayerEvent {
        PlayerEvent::Stopped {
            play_request_id,
            track_id: uri(),
        }
    }

    #[test]
    fn drops_the_stop_spirc_sends_after_an_end() {
        let mut ended = None;
        assert!(!repeats_an_end(&mut ended, &end(7)));
        assert!(repeats_an_end(&mut ended, &stop(7)));
        // Only once: a later stop for the same request is a real one.
        assert!(!repeats_an_end(&mut ended, &stop(7)));
    }

    #[test]
    fn passes_a_remote_skip() {
        let mut ended = None;
        assert!(!repeats_an_end(&mut ended, &stop(3)));
    }

    #[test]
    fn passes_a_stop_for_another_request() {
        let mut ended = None;
        assert!(!repeats_an_end(&mut ended, &end(1)));
        assert!(!repeats_an_end(&mut ended, &stop(2)));
    }
}
