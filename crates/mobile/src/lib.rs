//! The music core for the Android app. It owns a tokio runtime, the local library and one
//! playback engine, and keeps the queue itself, since the desktop's queue lives in GPUI state.
//! Every call returns at once or blocks only on the library; what playback does arrives at the
//! [`Listener`] as a [`Status`].

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use music::equalizer::{Curve, EqualizerWriter, Preamp};
use music::local::LocalProvider;
use music::{
    Album, Analyzer, Meter, MusicProvider as _, PlaybackConfig, PlaybackEvent, Player,
    ProviderSession, Quality, SignIn, Track, Transcode,
};
use storage::{Cache, Database};
use tokio::runtime::Runtime;

uniffi::setup_scaffolding!();

/// How often the engine reports where it is in a track.
const POSITION_INTERVAL: Duration = Duration::from_millis(250);
/// How far into a track Previous restarts it rather than going back a track.
const RESTART: Duration = Duration::from_secs(3);

/// Why a call into the core failed, as one line of English for a log or a toast.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MobileError {
    Failed(String),
}

impl std::fmt::Display for MobileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl From<anyhow::Error> for MobileError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(format!("{error:#}"))
    }
}

/// One track as the app lists it. `cover` is a file path or a URL.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: Option<String>,
    pub cover: Option<String>,
    pub duration_ms: u64,
}

/// One album as the app lists it.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Release {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub cover: Option<String>,
    pub year: i32,
    pub track_count: u32,
}

/// What the engine is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlayState {
    Idle,
    Loading,
    Playing,
    Paused,
}

/// Everything the player screen draws from: the transport state, the track, where it stands
/// and where it sits in the queue.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Status {
    pub state: PlayState,
    pub song: Option<Song>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub index: u32,
    pub queue_length: u32,
}

/// Told every time the [`Status`] changes, on a core thread: the app hops to its own.
#[uniffi::export(with_foreign)]
pub trait Listener: Send + Sync {
    fn changed(&self, status: Status);
}

/// The queue the core plays through, with the song now playing at `index`.
#[derive(Default)]
struct Queue {
    songs: Vec<Song>,
    index: usize,
}

/// The core. One per app process.
#[derive(uniffi::Object)]
pub struct Digida {
    runtime: Runtime,
    provider: LocalProvider,
    session: Mutex<Option<ProviderSession>>,
    player: Arc<Mutex<Option<Box<dyn Player>>>>,
    queue: Arc<Mutex<Queue>>,
    status: Arc<Mutex<Status>>,
    listener: Arc<Mutex<Option<Arc<dyn Listener>>>>,
    /// Keeps the equalizer's one writer alive, flat and off, for as long as the engine reads it.
    _equalizer: EqualizerWriter,
    config: PlaybackConfig,
}

#[uniffi::export]
impl Digida {
    /// Opens the core over `data_dir`, where the library database lives, and `cache_dir`,
    /// where scanned covers and the scan index go. Both are the app's own private folders.
    #[uniffi::constructor]
    pub fn new(data_dir: String, cache_dir: String) -> Result<Arc<Self>, MobileError> {
        logging();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("digida-core")
            .build()
            .map_err(anyhow::Error::from)?;
        let data = PathBuf::from(data_dir);
        let cache = PathBuf::from(cache_dir);
        std::fs::create_dir_all(&data).map_err(anyhow::Error::from)?;
        std::fs::create_dir_all(&cache).map_err(anyhow::Error::from)?;
        let provider = LocalProvider::new(
            cache.join("local"),
            Database::at(data.join("state.sqlite")),
            Cache::at(cache.join("cache.sqlite")),
        );
        let equalizer = EqualizerWriter::new(
            false,
            Curve::new(Preamp::Manual(0.), Vec::new())
                .map_err(|_| MobileError::Failed("cannot build a flat equalizer".to_owned()))?,
        );
        let config = PlaybackConfig {
            normalisation: false,
            gapless: true,
            position_interval: POSITION_INTERVAL,
            gain: 1.,
            equalizer: equalizer.reader(),
            transcode: Transcode::default(),
            quality: Quality::default(),
            meter: Meter::default(),
            analyzer: Analyzer::default(),
            vault: None,
        };

        Ok(Arc::new(Self {
            runtime,
            provider,
            session: Mutex::new(None),
            player: Arc::new(Mutex::new(None)),
            queue: Arc::new(Mutex::new(Queue::default())),
            status: Arc::new(Mutex::new(idle())),
            listener: Arc::new(Mutex::new(None)),
            _equalizer: equalizer,
            config,
        }))
    }

    /// Hands the core the one listener it tells about playback, replacing any before it.
    pub fn set_listener(&self, listener: Arc<dyn Listener>) {
        *lock(&self.listener) = Some(listener.clone());
        listener.changed(lock(&self.status).clone());
    }

    /// Scans `folders` into the library, replacing what an earlier scan found, and starts the
    /// engine the first time. Blocks until the scan ends, so the app calls it off its main thread.
    pub fn scan(&self, folders: Vec<String>) -> Result<(), MobileError> {
        let paths = folders.into_iter().map(PathBuf::from).collect();
        let (_prompts, input) = tokio::sync::mpsc::unbounded_channel();
        let session = self.runtime.block_on(self.provider.sign_in(
            SignIn::Path(paths),
            Arc::new(|_| {}),
            input,
        ))?;

        if lock(&self.player).is_none() {
            let (player, events) = session.playback.start(self.config.clone());
            *lock(&self.player) = Some(player);
            self.listen(events);
        }
        *lock(&self.session) = Some(session);
        Ok(())
    }

    /// Every album in the library, in the order the provider lists them.
    pub fn albums(&self) -> Result<Vec<Release>, MobileError> {
        let api = self.api()?;
        let albums = self.runtime.block_on(api.all_albums())?;
        Ok(albums.into_iter().map(release).collect())
    }

    /// Every track in the library.
    pub fn songs(&self) -> Result<Vec<Song>, MobileError> {
        let api = self.api()?;
        let tracks = self.runtime.block_on(api.all_tracks())?;
        Ok(tracks.into_iter().filter_map(song).collect())
    }

    /// The tracks of one album, in disc and track order.
    pub fn album_songs(&self, album_id: String) -> Result<Vec<Song>, MobileError> {
        let api = self.api()?;
        let tracks = self.runtime.block_on(api.album_tracks(&album_id))?;
        Ok(tracks.into_iter().filter_map(song).collect())
    }

    /// Replaces the queue with `songs` and plays the one at `start`.
    pub fn play(&self, songs: Vec<Song>, start: u32) {
        {
            let mut queue = lock(&self.queue);
            queue.index = (start as usize).min(songs.len().saturating_sub(1));
            queue.songs = songs;
        }
        load_current(&self.player, &self.queue, &self.status, &self.listener);
    }

    /// Pauses a playing track and resumes a paused one.
    pub fn toggle(&self) {
        let playing = lock(&self.status).state == PlayState::Playing;
        if let Some(player) = lock(&self.player).as_ref() {
            match playing {
                true => player.pause(),
                false => player.play(),
            }
        }
    }

    /// Plays the next song in the queue, if there is one.
    pub fn next(&self) {
        let moved = {
            let mut queue = lock(&self.queue);
            let more = queue.index + 1 < queue.songs.len();
            if more {
                queue.index += 1;
            }
            more
        };
        if moved {
            load_current(&self.player, &self.queue, &self.status, &self.listener);
        }
    }

    /// Restarts the song a few seconds in, and goes back a song otherwise.
    pub fn previous(&self) {
        let position = Duration::from_millis(lock(&self.status).position_ms);
        let back = {
            let mut queue = lock(&self.queue);
            let back = position < RESTART && queue.index > 0;
            if back {
                queue.index -= 1;
            }
            back
        };
        match back {
            true => load_current(&self.player, &self.queue, &self.status, &self.listener),
            false => self.seek(0),
        }
    }

    /// Moves to `position_ms` in the current song.
    pub fn seek(&self, position_ms: u64) {
        if let Some(player) = lock(&self.player).as_ref() {
            player.seek(Duration::from_millis(position_ms));
        }
    }

    /// The status as it stands, for a screen that comes up between changes.
    pub fn status(&self) -> Status {
        lock(&self.status).clone()
    }
}

impl Digida {
    /// The library of the scanned session, or why there is none yet.
    fn api(&self) -> Result<Arc<dyn music::MusicApi>, MobileError> {
        lock(&self.session)
            .as_ref()
            .map(|session| session.api.clone())
            .ok_or_else(|| MobileError::Failed("no library has been scanned yet".to_owned()))
    }

    /// Follows the engine's events for as long as it runs, folding each into the status and
    /// moving on through the queue when a song ends.
    fn listen(&self, mut events: Box<dyn music::PlaybackEvents>) {
        let player = self.player.clone();
        let queue = self.queue.clone();
        let status = self.status.clone();
        let listener = self.listener.clone();

        self.runtime.spawn(async move {
            while let Some(event) = events.next().await {
                let ended = matches!(
                    event,
                    PlaybackEvent::Ended { .. } | PlaybackEvent::Unavailable { .. }
                );
                let changed = {
                    let mut status = lock(&status);
                    fold(&mut status, &event)
                };
                if changed {
                    tell(&status, &listener);
                }
                if ended {
                    let more = {
                        let mut queue = lock(&queue);
                        let more = queue.index + 1 < queue.songs.len();
                        if more {
                            queue.index += 1;
                        }
                        more
                    };
                    match more {
                        true => load_current(&player, &queue, &status, &listener),
                        false => {
                            lock(&status).state = PlayState::Idle;
                            tell(&status, &listener);
                        }
                    }
                }
            }
            log::warn!("mobile: the playback engine stopped");
        });
    }
}

/// Loads the queue's current song into the engine and puts it in the status.
fn load_current(
    player: &Mutex<Option<Box<dyn Player>>>,
    queue: &Mutex<Queue>,
    status: &Mutex<Status>,
    listener: &Mutex<Option<Arc<dyn Listener>>>,
) {
    let (song, index, length) = {
        let queue = lock(queue);
        (
            queue.songs.get(queue.index).cloned(),
            queue.index,
            queue.songs.len(),
        )
    };
    let Some(song) = song else {
        return;
    };
    {
        let mut status = lock(status);
        status.state = PlayState::Loading;
        status.position_ms = 0;
        status.duration_ms = song.duration_ms;
        status.index = index as u32;
        status.queue_length = length as u32;
        status.song = Some(song.clone());
    }
    tell(status, listener);

    if let Some(player) = lock(player).as_ref()
        && let Err(error) = player.load(&song.id, Duration::ZERO, false)
    {
        log::warn!("mobile: cannot load {}: {error:#}", song.id);
    }
}

/// Folds one engine event into the status, answering whether anything the app draws moved.
fn fold(status: &mut Status, event: &PlaybackEvent) -> bool {
    let at = |at: &Duration| at.as_millis() as u64;
    match event {
        PlaybackEvent::Loading { at: position, .. } => {
            status.state = PlayState::Loading;
            status.position_ms = at(position);
        }
        PlaybackEvent::Playing { at: position, .. } => {
            status.state = PlayState::Playing;
            status.position_ms = at(position);
        }
        PlaybackEvent::Paused { at: position, .. } => {
            status.state = PlayState::Paused;
            status.position_ms = at(position);
        }
        PlaybackEvent::Position { at: position, .. }
        | PlaybackEvent::Seeked { at: position, .. } => status.position_ms = at(position),
        PlaybackEvent::Length { duration, .. } => status.duration_ms = at(duration),
        _ => return false,
    }
    true
}

/// Tells the listener the status as it stands.
fn tell(status: &Mutex<Status>, listener: &Mutex<Option<Arc<dyn Listener>>>) {
    let snapshot = lock(status).clone();
    let listener = lock(listener).clone();
    if let Some(listener) = listener {
        listener.changed(snapshot);
    }
}

fn idle() -> Status {
    Status {
        state: PlayState::Idle,
        song: None,
        position_ms: 0,
        duration_ms: 0,
        index: 0,
        queue_length: 0,
    }
}

/// A track the app can list, which needs an id to be played by.
fn song(track: Track) -> Option<Song> {
    Some(Song {
        id: track.id?,
        title: track.name,
        artist: track.artists,
        album: track.album,
        album_id: track.album_id,
        cover: track.cover,
        duration_ms: track.duration.as_millis() as u64,
    })
}

fn release(album: Album) -> Release {
    Release {
        id: album.id,
        title: album.name,
        artist: album.artists,
        cover: album.cover_large.or(album.cover),
        year: album.year,
        track_count: album.track_count,
    }
}

/// Locks a mutex, taking it over from a thread that panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Sends the log to logcat on Android, and nowhere elsewhere.
fn logging() {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("digida"),
    );
}
