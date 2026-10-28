//! Playing one service's tracks from another's: a Spotify account without Premium, which
//! librespot will not play for, plays each track from its match on YouTube Music. The library,
//! search and everything else stay the first service's; only the audio comes from the second.
//!
//! A [`StandIn`] is the first service's playback. It looks each track up by its title, artist
//! and length on the second service, plays the match there, and reports the match's events under
//! the first service's ids, so the app never sees the swap. Matches are remembered on disk, so a
//! downloaded track still plays, under its own id, with no network.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::future::BoxFuture;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{
    Fraction, MusicApi, PlaybackConfig, PlaybackEvent, PlaybackEvents, PlaybackFactory, Player,
    Spectrum,
};

/// How far apart two recordings' lengths may be and still be the same track.
const MATCH_LENGTH: Duration = Duration::from_secs(15);

/// One track to look for: what it is called, by whom, and how long it runs, zero when unknown.
pub struct Wanted<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub duration: Duration,
}

/// The track on `api` that is `wanted`, looked for by its title and first artist. Labels a
/// service adds in brackets or after a dash, like "(Live)" or "- Remastered", are left out of the
/// search, since an upload rarely carries them. A result about as long whose title and artist
/// agree most wins; failing one, a result whose title holds the whole title and names the
/// artist, however long it runs, since a live or cover take seldom keeps the length.
pub async fn matched(api: &dyn MusicApi, wanted: &Wanted<'_>) -> Option<String> {
    let core = core_title(wanted.title);
    let lead = wanted
        .artist
        .split([',', '&'])
        .next()
        .unwrap_or(wanted.artist)
        .trim();
    let tracks = api
        .search(&format!("{core} {lead}"))
        .await
        .inspect_err(|error| log::warn!("standin: cannot search for a match: {error:#}"))
        .ok()?;
    let title = words(&core);
    let artist = words(lead);
    let scored: Vec<Candidate> = tracks
        .into_iter()
        .filter_map(|track| {
            let length = track.duration;
            Some(Candidate {
                close: wanted.duration.is_zero()
                    || length.is_zero()
                    || length.abs_diff(wanted.duration) <= MATCH_LENGTH,
                named: overlap(&title, &words(&track.name)),
                by: overlap(
                    &artist,
                    &words(&format!("{} {}", track.artists, track.name)),
                ),
                id: track.id?,
            })
        })
        .collect();
    let best = |keep: fn(&Candidate) -> bool| {
        scored
            .iter()
            .filter(|candidate| keep(candidate))
            .max_by(|a, b| a.score().total_cmp(&b.score()))
            .map(|candidate| candidate.id.clone())
    };
    best(|candidate| candidate.close && candidate.named > 0.5)
        .or_else(|| best(|candidate| candidate.named >= 1. && candidate.by > 0.))
}

/// The playback of a service whose tracks play from their matches on another.
pub struct StandIn {
    /// The service whose ids the app plays, asked what each track is.
    origin: Arc<dyn MusicApi>,
    /// The service the audio comes from, searched for each match.
    source: Arc<dyn MusicApi>,
    playback: Arc<dyn PlaybackFactory>,
    matches: Arc<Matches>,
    handle: tokio::runtime::Handle,
}

impl StandIn {
    /// Plays `origin`'s tracks from their matches on `source`, played by `playback`. Matches are
    /// kept in `file`. It runs its lookups on the tokio runtime it is made on.
    pub fn new(
        origin: Arc<dyn MusicApi>,
        source: Arc<dyn MusicApi>,
        playback: Arc<dyn PlaybackFactory>,
        file: PathBuf,
    ) -> Self {
        Self {
            origin,
            source,
            playback,
            matches: Arc::new(Matches::load(file)),
            handle: tokio::runtime::Handle::current(),
        }
    }
}

impl PlaybackFactory for StandIn {
    fn start(&self, config: PlaybackConfig) -> (Box<dyn Player>, Box<dyn PlaybackEvents>) {
        let (inner, events) = self.playback.start(config);
        let (told, heard) = unbounded_channel();
        let player = StandInPlayer {
            inner: Arc::from(inner),
            lookup: Arc::new(Lookup {
                origin: self.origin.clone(),
                source: self.source.clone(),
                matches: self.matches.clone(),
            }),
            told,
            handle: self.handle.clone(),
        };
        let events = StandInEvents {
            inner: events,
            heard,
            matches: self.matches.clone(),
        };
        (Box::new(player), Box::new(events))
    }
}

/// Finds and remembers the match of a track.
struct Lookup {
    origin: Arc<dyn MusicApi>,
    source: Arc<dyn MusicApi>,
    matches: Arc<Matches>,
}

impl Lookup {
    /// The match of the origin's track `id`, from memory or by asking both services.
    async fn find(&self, id: &str) -> Result<String> {
        if let Some(found) = self.matches.source_of(id) {
            return Ok(found);
        }
        let track = self.origin.track(id).await?;
        let wanted = Wanted {
            title: &track.name,
            artist: &track.artists,
            duration: track.duration,
        };
        let found = matched(self.source.as_ref(), &wanted)
            .await
            .ok_or_else(|| anyhow!("no match for {} by {}", track.name, track.artists))?;
        log::info!("standin: {id} plays as {found}");
        self.matches.keep(id, &found);
        Ok(found)
    }
}

/// The player the app holds: it takes the origin's ids and drives the source's player.
struct StandInPlayer {
    inner: Arc<dyn Player>,
    lookup: Arc<Lookup>,
    /// Where a lookup that failed reports, as the event a failed load would have sent.
    told: UnboundedSender<PlaybackEvent>,
    handle: tokio::runtime::Handle,
}

impl StandInPlayer {
    /// Runs `then` with the match of `id`, at once when it is known and after a lookup
    /// otherwise. A track with no match reports itself unavailable.
    fn with_match(
        &self,
        id: &str,
        then: impl FnOnce(&dyn Player, &str) -> Result<()> + Send + 'static,
    ) -> Result<()> {
        if let Some(found) = self.lookup.matches.source_of(id) {
            return then(self.inner.as_ref(), &found);
        }
        let (inner, lookup, told, id) = (
            self.inner.clone(),
            self.lookup.clone(),
            self.told.clone(),
            id.to_owned(),
        );
        self.handle.spawn(async move {
            let outcome = match lookup.find(&id).await {
                Ok(found) => then(inner.as_ref(), &found),
                Err(error) => Err(error),
            };
            if let Err(error) = outcome {
                log::warn!("standin: cannot play {id}: {error:#}");
                told.send(PlaybackEvent::Unavailable { id: Some(id) }).ok();
            }
        });
        Ok(())
    }
}

impl Player for StandInPlayer {
    fn load(&self, track_id: &str, at: Duration, seamless: bool) -> Result<()> {
        self.with_match(track_id, move |inner, found| {
            inner.load(found, at, seamless)
        })
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        self.with_match(track_id, move |inner, found| {
            inner.load_paused_at(found, at)
        })
    }

    fn preload(&self, track_id: &str, segue: bool) -> Result<()> {
        self.with_match(track_id, move |inner, found| inner.preload(found, segue))
    }

    fn play(&self) {
        self.inner.play();
    }

    fn pause(&self) {
        self.inner.pause();
    }

    fn seek(&self, position: Duration) {
        self.inner.seek(position);
    }

    fn set_gain(&self, gain: f32) {
        self.inner.set_gain(gain);
    }

    fn spectrum(&self) -> Option<Spectrum> {
        self.inner.spectrum()
    }

    fn download(&self, track_id: &str, progress: Fraction) -> BoxFuture<'static, Result<()>> {
        let (inner, lookup, id) = (self.inner.clone(), self.lookup.clone(), track_id.to_owned());
        Box::pin(async move {
            let found = lookup.find(&id).await?;
            inner.download(&found, progress).await
        })
    }

    fn downloaded(&self, track_id: &str) -> bool {
        self.lookup
            .matches
            .source_of(track_id)
            .is_some_and(|found| self.inner.downloaded(&found))
    }

    fn plays_offline(&self, track_id: &str) -> bool {
        self.lookup
            .matches
            .source_of(track_id)
            .is_some_and(|found| self.inner.plays_offline(&found))
    }

    fn discard(&self, track_id: &str) {
        if let Some(found) = self.lookup.matches.source_of(track_id) {
            self.inner.discard(&found);
        }
    }
}

/// The source player's events, under the origin's ids, and the failed lookups among them.
struct StandInEvents {
    inner: Box<dyn PlaybackEvents>,
    heard: UnboundedReceiver<PlaybackEvent>,
    matches: Arc<Matches>,
}

#[async_trait]
impl PlaybackEvents for StandInEvents {
    async fn next(&mut self) -> Option<PlaybackEvent> {
        tokio::select! {
            Some(event) = self.heard.recv() => Some(event),
            event = self.inner.next() => event.map(|event| renamed(event, &self.matches)),
        }
    }
}

/// The matches found so far, both ways, and the file they are kept in.
struct Matches {
    file: PathBuf,
    /// The source's id of each origin track.
    sources: Mutex<HashMap<String, String>>,
}

impl Matches {
    fn load(file: PathBuf) -> Self {
        let sources = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            file,
            sources: Mutex::new(sources),
        }
    }

    fn source_of(&self, origin: &str) -> Option<String> {
        self.sources.lock().ok()?.get(origin).cloned()
    }

    fn origin_of(&self, source: &str) -> Option<String> {
        let sources = self.sources.lock().ok()?;
        sources
            .iter()
            .find(|(_, found)| *found == source)
            .map(|(origin, _)| origin.clone())
    }

    fn keep(&self, origin: &str, source: &str) {
        let Ok(mut sources) = self.sources.lock() else {
            return;
        };
        sources.insert(origin.to_owned(), source.to_owned());
        let written = serde_json::to_vec(&*sources)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| Ok(std::fs::write(&self.file, bytes)?));
        if let Err(error) = written {
            log::warn!("standin: cannot keep the matches: {error:#}");
        }
    }
}

/// `event` under the origin's id of the track it is about.
fn renamed(event: PlaybackEvent, matches: &Matches) -> PlaybackEvent {
    let origin = |id: Option<String>| id.map(|id| matches.origin_of(&id).unwrap_or(id));
    match event {
        PlaybackEvent::Loading { id, at } => PlaybackEvent::Loading { id: origin(id), at },
        PlaybackEvent::Playing { id, at } => PlaybackEvent::Playing { id: origin(id), at },
        PlaybackEvent::Paused { id, at } => PlaybackEvent::Paused { id: origin(id), at },
        PlaybackEvent::Position { id, at } => PlaybackEvent::Position { id: origin(id), at },
        PlaybackEvent::Seeked { id, at } => PlaybackEvent::Seeked { id: origin(id), at },
        PlaybackEvent::Length { id, duration } => PlaybackEvent::Length {
            id: origin(id),
            duration,
        },
        PlaybackEvent::Downloaded { id } => PlaybackEvent::Downloaded { id: origin(id) },
        PlaybackEvent::Ended { id } => PlaybackEvent::Ended { id: origin(id) },
        PlaybackEvent::Unavailable { id } => PlaybackEvent::Unavailable { id: origin(id) },
        PlaybackEvent::Throttled { id } => PlaybackEvent::Throttled { id: origin(id) },
        PlaybackEvent::Signal { id, track } => PlaybackEvent::Signal {
            id: matches.origin_of(&id).unwrap_or(id),
            track,
        },
        other => other,
    }
}

/// One search result weighed as a match: whether it is about as long, how much of the title and
/// of the artist it holds, from none at zero to all at one, and its id.
struct Candidate {
    close: bool,
    named: f64,
    by: f64,
    id: String,
}

impl Candidate {
    fn score(&self) -> f64 {
        self.named + self.by
    }
}

/// A title without the labels a service adds to it: whatever is in brackets, and whatever follows
/// a spaced dash. A title that is nothing but labels stays as it was.
fn core_title(title: &str) -> String {
    let mut core = String::new();
    let mut depth = 0usize;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            c if depth == 0 => core.push(c),
            _ => {}
        }
    }
    let core = core.split(" - ").next().unwrap_or(&core).trim().to_owned();
    match core.is_empty() {
        true => title.to_owned(),
        false => core,
    }
}

/// The lowercase words of `text`, with punctuation dropped.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// How many of `wanted`'s words `found` holds, from none at zero to all at one.
fn overlap(wanted: &[String], found: &[String]) -> f64 {
    if wanted.is_empty() {
        return 0.;
    }
    let held = wanted.iter().filter(|word| found.contains(word)).count();
    held as f64 / wanted.len() as f64
}
