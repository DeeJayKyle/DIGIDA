//! Downloads, in two kinds. An offline download keeps a song in the encrypted vault, where only
//! DIGIDA can play it, as the desktop keeps its downloads. A saved file is the song's own file
//! from the listener's Subsonic server, written into a music folder on the phone.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use music::Fraction;
use serde::{Deserialize, Serialize};

use crate::{Digida, SPOTIFY, Shared, Song, YOUTUBE, api_of, lock, matched};

/// How long a name a saved file may have before it is cut, well under any file system's limit.
const NAME_LIMIT: usize = 120;

/// What one download is doing.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum DownloadState {
    Queued,
    /// Arriving, with how much has, from 0 to 1, once the player can tell.
    Running {
        progress: Option<f32>,
    },
    Done,
    Failed {
        reason: String,
    },
}

/// One song in the downloads list, and the file it was saved to when it was saved as one.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Download {
    pub song: Song,
    pub state: DownloadState,
    pub file: Option<String>,
}

/// A song downloaded for good, and where it plays from: the engine that holds it, under the id
/// that engine knows it by, which differs for a Spotify song kept from its YouTube match.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Kept {
    pub song: Song,
    pub engine: String,
    pub engine_id: String,
    pub file: Option<String>,
}

/// What is waiting to download: a song, and the folder it goes to when it is saved as a file.
struct Wanted {
    song: Song,
    folder: Option<PathBuf>,
}

/// The downloads list: what is kept, what waits, what runs and what failed this run.
pub(crate) struct Downloads {
    path: PathBuf,
    kept: Vec<Kept>,
    queue: VecDeque<Wanted>,
    running: Option<(Song, Fraction)>,
    failed: Vec<(Song, String)>,
}

impl Downloads {
    /// The downloads listed at `path`, or none before the first.
    pub(crate) fn load(path: PathBuf) -> Self {
        let kept = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            kept,
            queue: VecDeque::new(),
            running: None,
            failed: Vec::new(),
        }
    }

    fn save(&self) {
        let written = serde_json::to_vec(&self.kept)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| Ok(std::fs::write(&self.path, bytes)?));
        if let Err(error) = written {
            log::warn!("mobile: cannot keep the downloads list: {error:#}");
        }
    }

    /// Whether `song` is kept, waiting or arriving, so asking for it again does nothing.
    fn has(&self, song: &Song) -> bool {
        let same = |other: &Song| other.id == song.id && other.provider == song.provider;
        self.kept.iter().any(|kept| same(&kept.song))
            || self.queue.iter().any(|wanted| same(&wanted.song))
            || self
                .running
                .as_ref()
                .is_some_and(|(running, _)| same(running))
    }

    /// The engine and id `song` plays from offline, when it is kept in the vault.
    pub(crate) fn offline(&self, song: &Song) -> Option<(String, String)> {
        self.kept
            .iter()
            .find(|kept| {
                kept.file.is_none()
                    && kept.song.id == song.id
                    && kept.song.provider == song.provider
            })
            .map(|kept| (kept.engine.clone(), kept.engine_id.clone()))
    }
}

#[uniffi::export]
impl Digida {
    /// Keeps `songs` in the vault to play with no network, one after another. Songs already kept
    /// or on their way are left as they are.
    pub fn download(&self, songs: Vec<Song>) {
        self.enqueue(songs, None);
    }

    /// Saves the files of `songs` from the listener's Subsonic server into `folder`. Songs from
    /// any other service are refused, since their audio only ever plays inside the app.
    pub fn save_files(&self, songs: Vec<Song>, folder: String) {
        self.enqueue(songs, Some(PathBuf::from(folder)));
    }

    /// Everything in the downloads list: what is arriving and waiting first, then what failed,
    /// then what is kept.
    pub fn downloads(&self) -> Vec<Download> {
        let downloads = lock(&self.shared.downloads);
        let running = downloads.running.iter().map(|(song, progress)| Download {
            song: song.clone(),
            state: DownloadState::Running {
                progress: progress.get(),
            },
            file: None,
        });
        let queued = downloads.queue.iter().map(|wanted| Download {
            song: wanted.song.clone(),
            state: DownloadState::Queued,
            file: None,
        });
        let failed = downloads.failed.iter().map(|(song, reason)| Download {
            song: song.clone(),
            state: DownloadState::Failed {
                reason: reason.clone(),
            },
            file: None,
        });
        let kept = downloads.kept.iter().rev().map(|kept| Download {
            song: kept.song.clone(),
            state: DownloadState::Done,
            file: kept.file.clone(),
        });
        running.chain(queued).chain(failed).chain(kept).collect()
    }

    /// Whether `song` is kept to play offline.
    pub fn is_downloaded(&self, song: Song) -> bool {
        lock(&self.shared.downloads).offline(&song).is_some()
    }

    /// Takes `song` out of the downloads: its vault entry goes, and a saved file is deleted.
    pub fn remove_download(&self, song: Song) {
        let removed = {
            let mut downloads = lock(&self.shared.downloads);
            downloads
                .failed
                .retain(|(failed, _)| failed.id != song.id || failed.provider != song.provider);
            let at = downloads
                .kept
                .iter()
                .position(|kept| kept.song.id == song.id && kept.song.provider == song.provider);
            let removed = at.map(|at| downloads.kept.remove(at));
            downloads.save();
            removed
        };
        let Some(kept) = removed else {
            return;
        };
        match kept.file {
            Some(file) => {
                if let Err(error) = std::fs::remove_file(&file) {
                    log::warn!("mobile: cannot delete {file}: {error}");
                }
            }
            None => {
                let player = lock(&self.shared.slots)
                    .get(&kept.engine)
                    .and_then(|slot| slot.player.clone());
                if let Some(player) = player {
                    player.discard(&kept.engine_id);
                }
            }
        }
    }

    /// Forgets the downloads that failed, so the list shows only what is kept.
    pub fn clear_failed_downloads(&self) {
        lock(&self.shared.downloads).failed.clear();
    }
}

impl Digida {
    fn enqueue(&self, songs: Vec<Song>, folder: Option<PathBuf>) {
        {
            let mut downloads = lock(&self.shared.downloads);
            for song in songs {
                if !downloads.has(&song) {
                    downloads.queue.push_back(Wanted {
                        song,
                        folder: folder.clone(),
                    });
                }
            }
        }
        next(&self.shared);
    }
}

/// Starts the next download when none is running, and the one after it once it ends.
fn next(shared: &Arc<Shared>) {
    let wanted = {
        let mut downloads = lock(&shared.downloads);
        if downloads.running.is_some() {
            return;
        }
        let Some(wanted) = downloads.queue.pop_front() else {
            return;
        };
        let progress = Fraction::default();
        downloads.running = Some((wanted.song.clone(), progress.clone()));
        (wanted, progress)
    };
    let shared = shared.clone();
    shared.handle.clone().spawn(async move {
        let (wanted, progress) = wanted;
        let song = wanted.song.clone();
        let outcome = match wanted.folder {
            Some(folder) => save_file(&shared, &song, &folder).await,
            None => keep(&shared, &song, progress).await,
        };
        {
            let mut downloads = lock(&shared.downloads);
            downloads.running = None;
            match outcome {
                Ok(kept) => {
                    log::info!("mobile: downloaded {} from {}", song.title, song.provider);
                    downloads.kept.push(kept);
                    downloads.save();
                }
                Err(error) => {
                    log::warn!("mobile: cannot download {}: {error:#}", song.title);
                    downloads.failed.push((song, format!("{error:#}")));
                }
            }
        }
        next(&shared);
    });
}

/// Fetches `song` into the vault through the engine that plays it.
async fn keep(shared: &Shared, song: &Song, progress: Fraction) -> anyhow::Result<Kept> {
    // a Spotify song without Premium is kept from its YouTube match, the one it plays from
    let (engine, engine_id) = match song.provider == SPOTIFY && shared.spotify.free() {
        true => {
            let id = matched(shared, song)
                .await
                .ok_or_else(|| anyhow::anyhow!("YouTube Music has no match for this song"))?;
            (YOUTUBE.to_owned(), id)
        }
        false => (song.provider.clone(), song.id.clone()),
    };
    let player = lock(&shared.slots)
        .get(&engine)
        .and_then(|slot| slot.player.clone())
        .ok_or_else(|| anyhow::anyhow!("{engine} is not signed in"))?;
    player.download(&engine_id, progress).await?;
    Ok(Kept {
        song: song.clone(),
        engine,
        engine_id,
        file: None,
    })
}

/// Writes the file the listener's own server holds for `song` into `folder`.
async fn save_file(shared: &Shared, song: &Song, folder: &Path) -> anyhow::Result<Kept> {
    let api = api_of(shared, &song.provider)
        .ok_or_else(|| anyhow::anyhow!("{} is not signed in", song.provider))?;
    let (bytes, extension) = api
        .original_file(&song.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("only songs on your own server can be saved as files"))?;
    let folder = folder.join(clean(&song.artist)).join(clean(&song.album));
    std::fs::create_dir_all(&folder)?;
    let file = folder.join(format!("{}.{}", clean(&song.title), clean(&extension)));
    std::fs::write(&file, bytes)?;
    Ok(Kept {
        song: song.clone(),
        engine: song.provider.clone(),
        engine_id: song.id.clone(),
        file: Some(file.to_string_lossy().into_owned()),
    })
}

/// `name` made safe as one part of a path: no separators or characters a file system refuses,
/// and never empty.
fn clean(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .take(NAME_LIMIT)
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_owned();
    match cleaned.is_empty() {
        true => "Unknown".to_owned(),
        false => cleaned,
    }
}
