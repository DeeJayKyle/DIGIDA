//! Scrobbling: the listens the phone plays, sent to Last.fm, Libre.fm, ListenBrainz or Maloja
//! through the desktop's own services in `music::scrobble`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use music::scrobble::{self, Account, Link, Play, Secret};

use crate::{Digida, MobileError, Shared, Song, lock};

/// How a scrobbling service is linked, which says what the settings row asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ScrobbleLink {
    /// Nothing to type: the service's page opens to approve the app.
    Browser,
    /// An API key and secret from the service, then the same approval page.
    Keys,
    /// A token copied from the service's settings page.
    Token,
    /// A server address and one of its API keys.
    Server,
}

/// One scrobbling service as the settings list shows it.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Scrobbler {
    pub id: String,
    pub link: ScrobbleLink,
    /// Where the user makes the key or token the link asks for.
    pub signup: Option<String>,
    pub linked: bool,
    pub enabled: bool,
    /// The account's name on the service, once linked.
    pub name: String,
}

/// The linked accounts, where they are kept, and the listen playing now with how far it got.
pub(crate) struct Scrobbles {
    path: PathBuf,
    accounts: HashMap<String, Account>,
    current: Option<Listen>,
}

/// A listen in progress: what plays, and the furthest point it has reached.
struct Listen {
    play: Play,
    heard: Duration,
}

impl Scrobbles {
    /// The accounts kept at `path`, or none when nothing has been linked yet.
    pub(crate) fn load(path: PathBuf) -> Self {
        let accounts = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            accounts,
            current: None,
        }
    }

    fn save(&self) {
        let written = serde_json::to_vec(&self.accounts)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| write_private(&self.path, &bytes));
        if let Err(error) = written {
            log::warn!("mobile: cannot keep the scrobbling accounts: {error:#}");
        }
    }

    /// The accounts that submit, with their service.
    fn enabled(&self) -> Vec<(Arc<dyn scrobble::Service>, Account)> {
        scrobble::services()
            .into_iter()
            .filter_map(|service| {
                let account = self.accounts.get(service.id())?;
                (account.linked() && account.enabled).then(|| (service, account.clone()))
            })
            .collect()
    }
}

#[uniffi::export]
impl Digida {
    /// Every scrobbling service, in the order the settings list shows them.
    pub fn scrobblers(&self) -> Vec<Scrobbler> {
        let scrobbles = lock(&self.shared.scrobbles);
        scrobble::services()
            .into_iter()
            .map(|service| {
                let account = scrobbles.accounts.get(service.id());
                Scrobbler {
                    id: service.id().to_owned(),
                    link: match service.link() {
                        Link::Browser => ScrobbleLink::Browser,
                        Link::Keys => ScrobbleLink::Keys,
                        Link::Token => ScrobbleLink::Token,
                        Link::Server => ScrobbleLink::Server,
                    },
                    signup: service.signup().map(str::to_owned),
                    linked: account.is_some_and(Account::linked),
                    enabled: account.is_some_and(|account| account.enabled),
                    name: account
                        .map(|account| account.name.clone())
                        .unwrap_or_default(),
                }
            })
            .collect()
    }

    /// Links the service `id` with what its row collected: `key` and `secret` for Last.fm,
    /// `token` for ListenBrainz, `url` and `key` for Maloja, nothing for Libre.fm. Blocks until
    /// the service answers, which for an approval page is once the user approves.
    pub fn link_scrobbler(
        &self,
        id: String,
        key: String,
        secret: String,
        token: String,
        url: String,
    ) -> Result<(), MobileError> {
        let service = scrobble::services()
            .into_iter()
            .find(|service| service.id() == id)
            .ok_or_else(|| MobileError::Failed(format!("there is no scrobbling service {id}")))?;
        let secret = match service.link() {
            Link::Browser => Secret::None,
            Link::Keys => Secret::Keys { key, secret },
            Link::Token => Secret::Token(token),
            Link::Server => Secret::Server { url, key },
        };
        let mut account = self
            .runtime
            .block_on(service.connect(secret))
            .inspect_err(|error| log::warn!("mobile: cannot link {id}: {error:#}"))?;
        account.enabled = true;
        let mut scrobbles = lock(&self.shared.scrobbles);
        scrobbles.accounts.insert(id, account);
        scrobbles.save();
        Ok(())
    }

    /// Forgets the account linked to the service `id`.
    pub fn unlink_scrobbler(&self, id: String) {
        let mut scrobbles = lock(&self.shared.scrobbles);
        scrobbles.accounts.remove(&id);
        scrobbles.save();
    }

    /// Turns submitting to the service `id` on or off, keeping its account.
    pub fn set_scrobbler_enabled(&self, id: String, enabled: bool) {
        let mut scrobbles = lock(&self.shared.scrobbles);
        if let Some(account) = scrobbles.accounts.get_mut(&id) {
            account.enabled = enabled;
        }
        scrobbles.save();
    }
}

/// A new song has started: the one before it is scrobbled if it played long enough, and the
/// services are told what plays now.
pub(crate) fn started(shared: &Arc<Shared>, song: &Song) {
    finish(shared);
    let play = Play {
        artist: song.artist.clone(),
        title: song.title.clone(),
        album: Some(song.album.clone()),
        duration: Duration::from_millis(song.duration_ms),
        at: SystemTime::now(),
    };
    let enabled = {
        let mut scrobbles = lock(&shared.scrobbles);
        scrobbles.current = Some(Listen {
            play: play.clone(),
            heard: Duration::ZERO,
        });
        scrobbles.enabled()
    };
    if enabled.is_empty() {
        return;
    }
    shared.handle.spawn(async move {
        for (service, account) in enabled {
            if let Err(error) = service.now_playing(&account, &play).await {
                log::info!("mobile: {} has no now playing: {error:#}", service.id());
            }
        }
    });
}

/// Records how far the song playing has got.
pub(crate) fn heard(shared: &Shared, position_ms: u64) {
    if let Some(listen) = lock(&shared.scrobbles).current.as_mut() {
        listen.heard = listen.heard.max(Duration::from_millis(position_ms));
    }
}

/// Scrobbles the song playing if it played long enough, and lets it go.
pub(crate) fn finish(shared: &Arc<Shared>) {
    let (listen, enabled) = {
        let mut scrobbles = lock(&shared.scrobbles);
        let Some(listen) = scrobbles.current.take() else {
            return;
        };
        (listen, scrobbles.enabled())
    };
    if enabled.is_empty() || !listen.play.earned(listen.heard) {
        return;
    }
    shared.handle.spawn(async move {
        for (service, account) in enabled {
            match service
                .scrobble(&account, std::slice::from_ref(&listen.play))
                .await
            {
                Ok(()) => log::info!(
                    "mobile: scrobbled {} to {}",
                    listen.play.title,
                    service.id()
                ),
                Err(error) => log::warn!("mobile: cannot scrobble to {}: {error:#}", service.id()),
            }
        }
    });
}

/// Writes `bytes` to `path` readable by the app alone, since it holds session keys.
fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
