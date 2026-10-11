//! librespot's `Sink` over the shared paced output. librespot decodes on its own thread, so
//! every write here is finished audio; the pacing and the device watch live in `crate::sink`.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use librespot_playback::audio_backend::{Sink, SinkError, SinkResult};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use librespot_playback::{NUM_CHANNELS, SAMPLE_RATE};
use tokio::sync::mpsc::UnboundedSender;

use crate::audio::Chain;
use crate::sink::{Cue, Paced, packet, watch_for_output};

const DEFAULT_QUEUE_CHUNKS: usize = 26;
const RATE_QUEUE_CHUNKS: usize = 8;
const PROCESS_FRAMES: usize = 4096;

/// The current tempo and pitch, shared between the playback engine and its sink.
pub(crate) type RateControl = Arc<Mutex<(f32, f32)>>;

pub struct OutputSink {
    paced: Paced,
    cue: Cue,
    rate: RateControl,
    processor: soundtouch::SoundTouch,
    speed: f32,
    pitch: f32,
    processed_active: bool,
    generation: u32,
    scratch: Vec<f32>,
}

impl OutputSink {
    /// The sink librespot's player builder asks for. Without an output device it gets a silent
    /// one, so playback state still moves.
    pub fn boxed(
        cue: Cue,
        chain: Chain,
        changed: UnboundedSender<()>,
        rate: RateControl,
    ) -> Box<dyn Sink> {
        let generation = cue.generation();
        match Paced::open(cue.clone(), chain, changed.clone()) {
            Ok(paced) => {
                let mut processor = soundtouch::SoundTouch::new();
                processor
                    .set_channels(NUM_CHANNELS as u32)
                    .set_sample_rate(SAMPLE_RATE);
                Box::new(Self {
                    paced,
                    cue,
                    rate,
                    processor,
                    speed: 1.,
                    pitch: 0.,
                    processed_active: false,
                    generation,
                    scratch: vec![0.; PROCESS_FRAMES * NUM_CHANNELS as usize],
                })
            }
            Err(error) => {
                log::error!("sink: cannot open an output device: {error:#}");
                watch_for_output(changed);
                Box::new(Silence(cue))
            }
        }
    }
}

impl Sink for OutputSink {
    /// librespot decodes everything at `SAMPLE_RATE`, so this is the one place the output
    /// follows it, before the first packet of a play.
    fn start(&mut self) -> SinkResult<()> {
        self.paced
            .fit(SAMPLE_RATE)
            .and_then(|()| self.paced.play())
            .map_err(|error| SinkError::OnWrite(error.to_string()))
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.paced.pause();
        Ok(())
    }

    fn write(&mut self, packeted: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        let samples = packeted
            .samples()
            .map_err(|error| SinkError::OnWrite(error.to_string()))?;
        let samples = converter.f64_to_f32(samples);
        let generation = self.cue.generation();
        if generation != self.generation {
            self.processor.clear();
            self.generation = generation;
        }
        if self.cue.cleared() {
            return Ok(());
        }
        let (speed, pitch) = *self.rate.lock().unwrap_or_else(PoisonError::into_inner);
        if speed != self.speed || pitch != self.pitch {
            self.speed = speed.clamp(0.5, 2.);
            self.pitch = pitch.clamp(-12., 12.);
            self.processed_active |= self.speed != 1. || self.pitch != 0.;
            self.processor
                .set_tempo(f64::from(self.speed))
                .set_pitch(2_f64.powf(f64::from(self.pitch) / 12.));
        }
        self.paced.set_queue_limit(match self.processed_active {
            true => RATE_QUEUE_CHUNKS,
            false => DEFAULT_QUEUE_CHUNKS,
        });

        if self.processed_active {
            let channels = NUM_CHANNELS as usize;
            self.processor
                .put_samples(&samples, samples.len() / channels);
            let mut processed = Vec::new();
            loop {
                let frames = self
                    .processor
                    .receive_samples(&mut self.scratch, PROCESS_FRAMES);
                if frames == 0 {
                    break;
                }
                processed.extend_from_slice(&self.scratch[..frames * channels]);
            }
            self.write_samples(&processed)?;
        } else {
            self.write_samples(&samples)?;
        }

        self.paced
            .drain()
            .map_err(|error| SinkError::OnWrite(error.to_string()))
    }
}

impl OutputSink {
    /// Queues interleaved samples without blocking the librespot audio callback.
    fn write_samples(&mut self, samples: &[f32]) -> SinkResult<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let Some(samples) = packet(samples, NUM_CHANNELS as u16, SAMPLE_RATE) else {
            return Ok(());
        };
        self.paced
            .write(samples)
            .map_err(|error| SinkError::OnWrite(error.to_string()))
    }
}

/// Swallows the audio when no output device opens. It still answers the cue, so playback state
/// moves on rather than waiting for sound that cannot come, and it takes as long over a packet
/// as the packet lasts: without the queue to pace it, the player would otherwise run a track
/// out at decoding speed and be sitting on the end of the queue by the time a device is back.
struct Silence(Cue);

impl Sink for Silence {
    fn write(&mut self, packeted: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
        if let Ok(samples) = packeted.samples() {
            std::thread::sleep(lasts(samples.len()));
        }
        self.0.admit();
        Ok(())
    }
}

/// How long `samples` of librespot's audio plays for, counted across every channel.
fn lasts(samples: usize) -> Duration {
    Duration::from_secs_f64(samples as f64 / f64::from(NUM_CHANNELS as u32 * SAMPLE_RATE))
}
