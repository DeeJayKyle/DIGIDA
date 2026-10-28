//! The music core for the Android app. It owns a tokio runtime, every provider the desktop app
//! has, a session and a playback engine per provider once signed in, and the queue, since the
//! desktop's queue lives in GPUI state. Library calls block until the provider answers, so the
//! app makes them off its main thread; what playback and sign-in do arrives at the
//! [`Listener`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use music::equalizer::{Curve, EqualizerWriter, Preamp};
use music::local::LocalProvider;
use music::{
    Album, Analyzer, Meter, MusicApi, MusicProvider, PlaybackConfig, PlaybackEvent, Player,
    Playlist as Listing, ProviderSession, Quality, Shape, SignIn, SignInPrompt, Track, Transcode,
};
use storage::{Cache, Database};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedSender;

uniffi::setup_scaffolding!();

/// How often an engine reports where it is in a track.
const POSITION_INTERVAL: Duration = Duration::from_millis(250);
/// How far into a track Previous restarts it rather than going back a track.
const RESTART: Duration = Duration::from_secs(3);
/// The slug of the provider that plays files on the phone.
const LOCAL: &str = "local";

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

/// One track as the app lists it, with the provider that plays it. `cover` is a file path or
/// a URL.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Song {
    pub id: String,
    pub provider: String,
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

/// One playlist as the app lists it.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub cover: Option<String>,
    pub track_count: u32,
}

/// The ways a provider can be signed in to, as the app offers them.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Method {
    /// The provider's own sign-in in a browser.
    Browser,
    /// Browsing without an account.
    Guest,
    /// Cookies from a signed-in browser session, pasted or caught by a web view.
    Cookies,
    /// A server address, user name and password, as a self-hosted server wants.
    Credentials {
        server: String,
        username: String,
        password: String,
    },
}

/// A provider the app can sign in to, and how far along it is.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Service {
    pub slug: String,
    pub name: String,
    pub signed_in: bool,
    /// Whether a sign-in is kept on the phone that [`Digida::restore`] can bring back.
    pub stored: bool,
    pub methods: Vec<Method>,
    pub account: Option<String>,
    /// Whether the session is an account's rather than a guest's, which has no library of its
    /// own to list, only search.
    pub authenticated: bool,
}

/// How to catch a provider's sign-in cookies in a web view: open `url`, and once the cookies
/// for `domain` carry one of the `proof` names the user is through, and those cookies, as a
/// `Cookie` header, are the answer to [`Prompt::Cookies`]. `agent` replaces the web view's own
/// user agent where the provider wants one.
#[derive(Clone, Debug, uniffi::Record)]
pub struct WebLogin {
    pub url: String,
    pub landing: String,
    pub domain: String,
    pub proof: Vec<String>,
    pub agent: Option<String>,
}

/// An account a sign-in asks the listener to pick.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub detail: Option<String>,
}

/// What a sign-in asks of the listener while it runs. The answer, where one is wanted, goes
/// back through [`Digida::answer`].
#[derive(Clone, Debug, uniffi::Enum)]
pub enum Prompt {
    /// Open this page in a browser; the sign-in finishes when it redirects back.
    Url { url: String },
    /// Enter `code` on the page at `url`.
    Code { code: String, url: String },
    /// Answer with the cookies of a signed-in session.
    Cookies,
    /// Answer with the id of one of these accounts.
    Accounts { accounts: Vec<Account> },
}

/// What an engine is doing.
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

/// Told about playback and sign-in on a core thread: the app hops to its own.
#[uniffi::export(with_foreign)]
pub trait Listener: Send + Sync {
    fn changed(&self, status: Status);
    fn prompt(&self, provider: String, prompt: Prompt);
}

/// The queue the core plays through, with the song now playing at `index`.
#[derive(Default)]
struct Queue {
    songs: Vec<Song>,
    index: usize,
}

/// One provider and, once signed in, its session and the engine it plays through.
struct Slot {
    provider: Arc<dyn MusicProvider>,
    session: Option<ProviderSession>,
    player: Option<Arc<dyn Player>>,
}

/// What the event loops and the transport share.
struct Shared {
    slots: Mutex<HashMap<String, Slot>>,
    /// The provider whose engine plays the queue now.
    playing: Mutex<Option<String>>,
    queue: Mutex<Queue>,
    status: Mutex<Status>,
    listener: Mutex<Option<Arc<dyn Listener>>>,
}

/// The core. One per app process.
#[derive(uniffi::Object)]
pub struct Digida {
    runtime: Runtime,
    shared: Arc<Shared>,
    /// The provider the library screens show.
    active: Mutex<String>,
    /// Where an answer to a running sign-in's prompt goes.
    answers: Mutex<Option<UnboundedSender<String>>>,
    /// Keeps the equalizer's one writer alive, flat and off, for as long as the engines read it.
    _equalizer: EqualizerWriter,
    config: PlaybackConfig,
}

#[uniffi::export]
impl Digida {
    /// Opens the core over `data_dir`, where sign-ins and the library database live, and
    /// `cache_dir`, where covers and the scan index go. Both are the app's private folders.
    #[uniffi::constructor]
    pub fn new(data_dir: String, cache_dir: String) -> Result<Arc<Self>, MobileError> {
        logging();
        #[cfg(target_os = "android")]
        trust(std::path::Path::new(&cache_dir));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("digida-core")
            .build()
            .map_err(anyhow::Error::from)?;
        let data = PathBuf::from(data_dir);
        let cache = PathBuf::from(cache_dir);
        std::fs::create_dir_all(&data).map_err(anyhow::Error::from)?;
        std::fs::create_dir_all(&cache).map_err(anyhow::Error::from)?;
        // the music engine keeps sign-ins and downloads under these, since Android names no
        // platform folders of its own
        storage::folders::set(data.clone(), cache.clone());

        // Providers read their settings from the environment as on the desktop, so they are
        // built on the runtime that some of them start tasks on.
        let providers: Vec<Arc<dyn MusicProvider>> = runtime.block_on(async {
            vec![
                Arc::new(LocalProvider::new(
                    cache.join("local"),
                    Database::at(data.join("state.sqlite")),
                    Cache::at(cache.join("cache.sqlite")),
                )) as Arc<dyn MusicProvider>,
                Arc::new(music::spotify::SpotifyProvider::from_env()),
                Arc::new(music::youtube::YouTubeProvider::new()),
                Arc::new(music::apple::AppleProvider::new()),
                Arc::new(music::deezer::DeezerProvider::new()),
                Arc::new(music::tidal::TidalProvider::new()),
                Arc::new(music::subsonic::SubsonicProvider::new()),
            ]
        });
        let slots = providers
            .into_iter()
            .map(|provider| {
                (
                    provider.slug().to_owned(),
                    Slot {
                        provider,
                        session: None,
                        player: None,
                    },
                )
            })
            .collect();

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
            shared: Arc::new(Shared {
                slots: Mutex::new(slots),
                playing: Mutex::new(None),
                queue: Mutex::new(Queue::default()),
                status: Mutex::new(idle()),
                listener: Mutex::new(None),
            }),
            active: Mutex::new(LOCAL.to_owned()),
            answers: Mutex::new(None),
            _equalizer: equalizer,
            config,
        }))
    }

    /// Hands the core the one listener it tells about playback and sign-in, replacing any
    /// before it.
    pub fn set_listener(&self, listener: Arc<dyn Listener>) {
        *lock(&self.shared.listener) = Some(listener.clone());
        listener.changed(lock(&self.shared.status).clone());
    }

    /// Every provider, local files first, with how far along each is.
    pub fn services(&self) -> Vec<Service> {
        let slots = lock(&self.shared.slots);
        let mut services: Vec<Service> = slots
            .iter()
            .map(|(slug, slot)| Service {
                slug: slug.clone(),
                name: slot.provider.name().to_owned(),
                signed_in: slot.session.is_some(),
                stored: slot.provider.stored(),
                methods: slot
                    .provider
                    .sign_in_options()
                    .into_iter()
                    .filter_map(method)
                    .collect(),
                account: slot
                    .session
                    .as_ref()
                    .map(|session| session.profile.display_name.clone()),
                authenticated: slot
                    .session
                    .as_ref()
                    .is_some_and(|session| session.authenticated),
            })
            .collect();
        services.sort_by_key(|service| (service.slug != LOCAL, service.name.clone()));
        services
    }

    /// Brings back every sign-in kept on the phone, answering the slugs that came back. One
    /// provider failing leaves the others alone.
    pub fn restore(&self) -> Vec<String> {
        let providers: Vec<(String, Arc<dyn MusicProvider>)> = lock(&self.shared.slots)
            .iter()
            .filter(|(slug, slot)| *slug != LOCAL && slot.provider.stored())
            .map(|(slug, slot)| (slug.clone(), slot.provider.clone()))
            .collect();
        let mut restored = Vec::new();
        for (slug, provider) in providers {
            match self.runtime.block_on(provider.restore()) {
                Ok(Some(session)) => {
                    self.keep(&slug, session);
                    restored.push(slug);
                }
                Ok(None) => {}
                Err(error) => log::warn!("mobile: cannot restore {slug}: {error:#}"),
            }
        }
        restored
    }

    /// Signs in to `slug` by `method`, blocking until it is done. Prompts the sign-in raises go
    /// to the listener, and its answers come back through [`Self::answer`].
    pub fn sign_in(&self, slug: String, method: Method) -> Result<(), MobileError> {
        let provider = self.provider(&slug)?;
        let (answers, input) = tokio::sync::mpsc::unbounded_channel();
        *lock(&self.answers) = Some(answers);
        let shared = self.shared.clone();
        let asker = slug.clone();
        let prompt = Arc::new(move |prompt: SignInPrompt| {
            let listener = lock(&shared.listener).clone();
            if let Some(listener) = listener {
                listener.prompt(asker.clone(), relay(prompt));
            }
        });
        let signed = self
            .runtime
            .block_on(provider.sign_in(sign_in(method), prompt, input));
        *lock(&self.answers) = None;
        let session = signed
            .inspect_err(|error| log::warn!("mobile: cannot sign in to {slug}: {error:#}"))?;
        self.keep(&slug, session);
        Ok(())
    }

    /// How `slug` signs in through a web view, for a provider that takes cookies.
    pub fn web_login(&self, slug: String) -> Option<WebLogin> {
        let web = self.provider(&slug).ok()?.web_sign_in()?;
        Some(WebLogin {
            url: web.url.to_owned(),
            landing: web.landing.to_owned(),
            domain: web.domain.to_owned(),
            proof: web.proof.iter().map(|name| (*name).to_owned()).collect(),
            agent: web.agent.map(str::to_owned),
        })
    }

    /// Answers the prompt a running sign-in raised: the cookies it asked for, or the id of
    /// the account picked.
    pub fn answer(&self, text: String) {
        if let Some(answers) = lock(&self.answers).as_ref() {
            answers.send(text).ok();
        }
    }

    /// Gives up on a running sign-in.
    pub fn cancel_sign_in(&self, slug: String) {
        *lock(&self.answers) = None;
        if let Ok(provider) = self.provider(&slug) {
            provider.abandon();
        }
    }

    /// Signs out of `slug` and forgets its kept sign-in.
    pub fn sign_out(&self, slug: String) {
        let mut slots = lock(&self.shared.slots);
        if let Some(slot) = slots.get_mut(&slug) {
            if let Some(player) = slot.player.take() {
                player.pause();
            }
            slot.session = None;
            slot.provider.sign_out();
        }
    }

    /// Scans `folders` into the local library, replacing what an earlier scan found.
    pub fn scan(&self, folders: Vec<String>) -> Result<(), MobileError> {
        let provider = self.provider(LOCAL)?;
        let paths = folders.into_iter().map(PathBuf::from).collect();
        let (_answers, input) = tokio::sync::mpsc::unbounded_channel();
        let session = self.runtime.block_on(provider.sign_in(
            SignIn::Path(paths),
            Arc::new(|_| {}),
            input,
        ))?;
        self.keep(LOCAL, session);
        Ok(())
    }

    /// Shows `slug`'s library on the library screens.
    pub fn set_active(&self, slug: String) {
        *lock(&self.active) = slug;
    }

    /// The provider the library screens show.
    pub fn active(&self) -> String {
        lock(&self.active).clone()
    }

    /// The albums of the shown library: everything a catalog holds, or what was saved.
    pub fn albums(&self) -> Result<Vec<Release>, MobileError> {
        let (api, shape, _) = self.library()?;
        let albums = self
            .runtime
            .block_on(async {
                match shape.is_catalog() {
                    true => api.all_albums().await,
                    false => api.saved_albums().await,
                }
            })
            .inspect_err(|error| log::warn!("mobile: cannot list albums: {error:#}"))?;
        Ok(albums.into_iter().map(release).collect())
    }

    /// The tracks of the shown library: everything a catalog holds, or what was saved.
    pub fn songs(&self) -> Result<Vec<Song>, MobileError> {
        let (api, shape, slug) = self.library()?;
        let tracks = self.runtime.block_on(async {
            match shape.is_catalog() {
                true => api.all_tracks().await,
                false => api.saved_tracks().await,
            }
        })?;
        Ok(songs(tracks, &slug))
    }

    /// The playlists of the shown library.
    pub fn playlists(&self) -> Result<Vec<Playlist>, MobileError> {
        let (api, _, _) = self.library()?;
        let playlists = self.runtime.block_on(api.playlists())?;
        Ok(playlists.into_iter().map(list).collect())
    }

    /// The tracks of one album, in disc and track order.
    pub fn album_songs(&self, album_id: String) -> Result<Vec<Song>, MobileError> {
        let (api, _, slug) = self.library()?;
        let tracks = self.runtime.block_on(api.album_tracks(&album_id))?;
        Ok(songs(tracks, &slug))
    }

    /// The tracks of one playlist, in its order.
    pub fn playlist_songs(&self, playlist_id: String) -> Result<Vec<Song>, MobileError> {
        let (api, _, slug) = self.library()?;
        let tracks = self.runtime.block_on(api.playlist_tracks(&playlist_id))?;
        Ok(songs(tracks, &slug))
    }

    /// The tracks the shown provider finds for `query`.
    pub fn search(&self, query: String) -> Result<Vec<Song>, MobileError> {
        let (api, _, slug) = self.library()?;
        let tracks = self
            .runtime
            .block_on(api.search(&query))
            .inspect_err(|error| log::warn!("mobile: {slug} cannot search: {error:#}"))?;
        Ok(songs(tracks, &slug))
    }

    /// Replaces the queue with `songs` and plays the one at `start`.
    pub fn play(&self, songs: Vec<Song>, start: u32) {
        {
            let mut queue = lock(&self.shared.queue);
            queue.index = (start as usize).min(songs.len().saturating_sub(1));
            queue.songs = songs;
        }
        load_current(&self.shared);
    }

    /// Pauses a playing track and resumes a paused one.
    pub fn toggle(&self) {
        let playing = lock(&self.shared.status).state == PlayState::Playing;
        if let Some(player) = current_player(&self.shared) {
            match playing {
                true => player.pause(),
                false => player.play(),
            }
        }
    }

    /// Plays the next song in the queue, if there is one.
    pub fn next(&self) {
        if advance(&self.shared) {
            load_current(&self.shared);
        }
    }

    /// Restarts the song a few seconds in, and goes back a song otherwise.
    pub fn previous(&self) {
        let position = Duration::from_millis(lock(&self.shared.status).position_ms);
        let back = {
            let mut queue = lock(&self.shared.queue);
            let back = position < RESTART && queue.index > 0;
            if back {
                queue.index -= 1;
            }
            back
        };
        match back {
            true => load_current(&self.shared),
            false => self.seek(0),
        }
    }

    /// Moves to `position_ms` in the current song.
    pub fn seek(&self, position_ms: u64) {
        if let Some(player) = current_player(&self.shared) {
            player.seek(Duration::from_millis(position_ms));
        }
    }

    /// The status as it stands, for a screen that comes up between changes.
    pub fn status(&self) -> Status {
        lock(&self.shared.status).clone()
    }
}

impl Digida {
    /// The provider named `slug`, or why there is none.
    fn provider(&self, slug: &str) -> Result<Arc<dyn MusicProvider>, MobileError> {
        lock(&self.shared.slots)
            .get(slug)
            .map(|slot| slot.provider.clone())
            .ok_or_else(|| MobileError::Failed(format!("there is no provider called {slug}")))
    }

    /// The shown provider's library, its shape and its slug, or why it has none yet.
    fn library(&self) -> Result<(Arc<dyn MusicApi>, Shape, String), MobileError> {
        let slug = lock(&self.active).clone();
        lock(&self.shared.slots)
            .get(&slug)
            .and_then(|slot| slot.session.as_ref())
            .map(|session| (session.api.clone(), session.shape, slug.clone()))
            .ok_or_else(|| MobileError::Failed(format!("{slug} is not signed in")))
    }

    /// Keeps a fresh session for `slug` and starts its engine the first time, listening to it
    /// for as long as it runs.
    fn keep(&self, slug: &str, session: ProviderSession) {
        let started = {
            let mut slots = lock(&self.shared.slots);
            let Some(slot) = slots.get_mut(slug) else {
                return;
            };
            let started = match slot.player.is_none() {
                true => {
                    let (player, events) = session.playback.start(self.config.clone());
                    slot.player = Some(Arc::from(player));
                    Some(events)
                }
                false => None,
            };
            slot.session = Some(session);
            started
        };
        if let Some(events) = started {
            listen(&self.runtime, self.shared.clone(), slug.to_owned(), events);
        }
    }
}

/// Follows one engine's events for as long as it runs. Only the engine playing the queue moves
/// the status; when its song ends the queue moves on.
fn listen(
    runtime: &Runtime,
    shared: Arc<Shared>,
    slug: String,
    mut events: Box<dyn music::PlaybackEvents>,
) {
    runtime.spawn(async move {
        while let Some(event) = events.next().await {
            if lock(&shared.playing).as_deref() != Some(slug.as_str()) {
                continue;
            }
            let ended = matches!(
                event,
                PlaybackEvent::Ended { .. } | PlaybackEvent::Unavailable { .. }
            );
            let refusal = match &event {
                PlaybackEvent::Unavailable { .. } => Some("cannot play the track"),
                PlaybackEvent::Throttled { .. } => Some("was turned down for now"),
                PlaybackEvent::Refused => Some("was refused"),
                PlaybackEvent::Gated => Some("needs something before it plays"),
                _ => None,
            };
            if let Some(refusal) = refusal {
                log::warn!("mobile: the {slug} engine {refusal} ({:?})", event.id());
            }
            let changed = fold(&mut lock(&shared.status), &event);
            if changed {
                tell(&shared);
            }
            if ended {
                match advance(&shared) {
                    true => load_current(&shared),
                    false => {
                        lock(&shared.status).state = PlayState::Idle;
                        tell(&shared);
                    }
                }
            }
        }
        log::warn!("mobile: the {slug} engine stopped");
    });
}

/// Moves the queue on a song, answering whether there was one to move to.
fn advance(shared: &Shared) -> bool {
    let mut queue = lock(&shared.queue);
    let more = queue.index + 1 < queue.songs.len();
    if more {
        queue.index += 1;
    }
    more
}

/// The engine playing the queue now.
fn current_player(shared: &Shared) -> Option<Arc<dyn Player>> {
    let playing = lock(&shared.playing).clone()?;
    lock(&shared.slots)
        .get(&playing)
        .and_then(|slot| slot.player.clone())
}

/// Loads the queue's current song into its provider's engine, pausing another provider's
/// engine that was playing, and puts the song in the status.
fn load_current(shared: &Shared) {
    let (song, index, length) = {
        let queue = lock(&shared.queue);
        (
            queue.songs.get(queue.index).cloned(),
            queue.index,
            queue.songs.len(),
        )
    };
    let Some(song) = song else {
        return;
    };
    let before = lock(&shared.playing).replace(song.provider.clone());
    if let Some(before) = before.filter(|before| *before != song.provider)
        && let Some(player) = lock(&shared.slots)
            .get(&before)
            .and_then(|slot| slot.player.clone())
    {
        player.pause();
    }
    {
        let mut status = lock(&shared.status);
        status.state = PlayState::Loading;
        status.position_ms = 0;
        status.duration_ms = song.duration_ms;
        status.index = index as u32;
        status.queue_length = length as u32;
        status.song = Some(song.clone());
    }
    tell(shared);

    let player = lock(&shared.slots)
        .get(&song.provider)
        .and_then(|slot| slot.player.clone());
    match player {
        Some(player) => {
            log::info!("mobile: loading {} from {}", song.id, song.provider);
            if let Err(error) = player.load(&song.id, Duration::ZERO, false) {
                log::warn!("mobile: cannot load {}: {error:#}", song.id);
            }
        }
        None => log::warn!(
            "mobile: {} is not signed in, so {} cannot play",
            song.provider,
            song.id
        ),
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
fn tell(shared: &Shared) {
    let snapshot = lock(&shared.status).clone();
    let listener = lock(&shared.listener).clone();
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

/// The method the app offers for one of a provider's sign-in options, leaving out a folder
/// pick, which only local files take and through [`Digida::scan`].
fn method(option: SignIn) -> Option<Method> {
    match option {
        SignIn::Default => Some(Method::Browser),
        SignIn::Anonymous => Some(Method::Guest),
        SignIn::Secret => Some(Method::Cookies),
        SignIn::Credentials {
            server,
            username,
            password,
        } => Some(Method::Credentials {
            server,
            username,
            password,
        }),
        SignIn::Path(_) => None,
    }
}

/// The provider's own sign-in for a method the app chose.
fn sign_in(method: Method) -> SignIn {
    match method {
        Method::Browser => SignIn::Default,
        Method::Guest => SignIn::Anonymous,
        Method::Cookies => SignIn::Secret,
        Method::Credentials {
            server,
            username,
            password,
        } => SignIn::Credentials {
            server,
            username,
            password,
        },
    }
}

/// A sign-in prompt in the shape the app reads.
fn relay(prompt: SignInPrompt) -> Prompt {
    match prompt {
        SignInPrompt::Url(url) => Prompt::Url { url },
        SignInPrompt::Code { code, url } => Prompt::Code { code, url },
        SignInPrompt::Secret => Prompt::Cookies,
        SignInPrompt::Accounts(accounts) => Prompt::Accounts {
            accounts: accounts
                .into_iter()
                .map(|account| Account {
                    id: account.id,
                    name: account.name,
                    detail: account.detail,
                })
                .collect(),
        },
    }
}

/// The tracks of `provider` the app can list, which need an id to be played by.
fn songs(tracks: Vec<Track>, provider: &str) -> Vec<Song> {
    tracks
        .into_iter()
        .filter_map(|track| {
            Some(Song {
                id: track.id?,
                provider: provider.to_owned(),
                title: track.name,
                artist: track.artists,
                album: track.album,
                album_id: track.album_id,
                cover: track.cover,
                duration_ms: track.duration.as_millis() as u64,
            })
        })
        .collect()
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

fn list(playlist: Listing) -> Playlist {
    Playlist {
        id: playlist.id,
        title: playlist.name,
        owner: playlist.owner,
        cover: playlist.cover,
        track_count: playlist.track_count,
    }
}

/// Registers the app with the parts of the core that call back into Android: the audio output,
/// which finds the phone's speakers through the process's context, and the certificate
/// verifier, which checks every HTTPS connection against the phone's own trust store. Called
/// once per process from `Tls.init` in Kotlin, before anything plays or reaches the network.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_deejaykyle_digida_Tls_init<'local>(
    mut env: jni::EnvUnowned<'local>,
    _class: jni::objects::JClass<'local>,
    context: jni::objects::JObject<'local>,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let vm = env.get_java_vm()?;
        // The global reference is never released: the context has to outlive every audio
        // stream, and the process is what ends it.
        let app = env.new_global_ref(&context)?.into_raw();
        // SAFETY: both pointers are valid for the life of the process, the VM because there is
        // one per process and the context because its global reference is never deleted, and
        // this runs once, before anything reads the context.
        unsafe { ndk_context::initialize_android_context(vm.get_raw().cast(), app.cast()) };
        rustls_platform_verifier::android::init_with_env(env, context)
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

/// Gathers the phone's trusted certificates into one file and points `SSL_CERT_FILE` at it.
/// Spotify's client looks for certificates where a Linux system keeps them, and Android keeps
/// them elsewhere, one file per certificate: in the Conscrypt module since Android 14, and under
/// `/system` before it. A certificate file holds its PEM block beside a text dump, which the
/// readers skip.
#[cfg(target_os = "android")]
fn trust(cache: &std::path::Path) {
    let stores = [
        "/apex/com.android.conscrypt/cacerts",
        "/system/etc/security/cacerts",
    ];
    let Some(entries) = stores
        .iter()
        .find_map(|store| std::fs::read_dir(store).ok())
    else {
        log::warn!("mobile: the phone names no trusted certificates");
        return;
    };
    let mut bundle = Vec::new();
    for entry in entries.flatten() {
        if let Ok(certificate) = std::fs::read(entry.path()) {
            bundle.extend_from_slice(&certificate);
            bundle.push(b'\n');
        }
    }
    let path = cache.join("ca-bundle.pem");
    if let Err(error) = std::fs::create_dir_all(cache).and_then(|()| std::fs::write(&path, &bundle))
    {
        log::warn!("mobile: cannot write the certificate bundle: {error}");
        return;
    }
    // SAFETY: this runs in the core's constructor before the core starts a thread of its own,
    // and nothing on the Java side reads the process environment.
    unsafe { std::env::set_var("SSL_CERT_FILE", &path) };
}

/// Locks a mutex, taking it over from a thread that panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// How many of the latest log lines the diagnostics keep.
const LOG_LINES: usize = 400;

/// The latest log lines, for the diagnostics screen to show and copy.
static RECENT: Mutex<std::collections::VecDeque<String>> =
    Mutex::new(std::collections::VecDeque::new());

/// Keeps the latest log lines for the diagnostics and, on Android, hands every line to logcat
/// as well.
struct Recorder {
    #[cfg(target_os = "android")]
    logcat: android_logger::AndroidLogger,
}

impl log::Log for Recorder {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        #[cfg(target_os = "android")]
        self.logcat.log(record);
        let mut recent = lock(&RECENT);
        if recent.len() == LOG_LINES {
            recent.pop_front();
        }
        recent.push_back(format!(
            "{} {}: {}",
            record.level(),
            record.target(),
            record.args()
        ));
    }

    fn flush(&self) {}
}

/// Starts keeping the log, once per process.
fn logging() {
    let recorder = Recorder {
        #[cfg(target_os = "android")]
        logcat: android_logger::AndroidLogger::new(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("digida"),
        ),
    };
    if log::set_logger(Box::leak(Box::new(recorder))).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// The latest lines of the core's log, oldest first, for the diagnostics screen.
#[uniffi::export]
pub fn recent_log() -> Vec<String> {
    lock(&RECENT).iter().cloned().collect()
}
