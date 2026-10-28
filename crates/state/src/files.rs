//! Saving songs from the listener's own Subsonic or Navidrome server as files in their music
//! folder. Only a server that holds the listener's own music hands its files over; a streaming
//! service's tracks only ever play inside the app.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui::{Context, Entity, Task};
use music::{MusicApi, Track};

use crate::{Io, Outcome, Session, Toast, Toasts};

/// The one provider whose files can be saved.
const SUBSONIC: &str = "subsonic";
/// The folder under the music folder that saved files go to.
const FOLDER: &str = "DIGIDA";
/// How long a name a saved file may have before it is cut, well under any file system's limit.
const NAME_LIMIT: usize = 120;

/// Saves the files of songs, albums and playlists from the listener's own server.
pub struct SavedFiles {
    session: Entity<Session>,
    /// The save running now. One runs at a time, since each writes a whole batch.
    saving: Option<Task<()>>,
}

/// What a save is asked to take.
enum Batch {
    Tracks(Vec<Track>),
    Album(String),
    Playlist(String),
}

impl SavedFiles {
    pub fn new(session: Entity<Session>) -> Self {
        Self {
            session,
            saving: None,
        }
    }

    /// Whether the signed-in service can hand its files over.
    pub fn offered(&self, cx: &gpui::App) -> bool {
        self.session.read(cx).provider_slug() == Some(SUBSONIC)
    }

    /// Saves the files of `tracks`.
    pub fn save_tracks(&mut self, tracks: Vec<Track>, cx: &mut Context<Self>) {
        self.save(Batch::Tracks(tracks), cx);
    }

    /// Saves the files of every song on the album `id`.
    pub fn save_album(&mut self, id: String, cx: &mut Context<Self>) {
        self.save(Batch::Album(id), cx);
    }

    /// Saves the files of every song on the playlist `id`.
    pub fn save_playlist(&mut self, id: String, cx: &mut Context<Self>) {
        self.save(Batch::Playlist(id), cx);
    }

    fn save(&mut self, batch: Batch, cx: &mut Context<Self>) {
        if self.saving.is_some() {
            Toasts::show(Outcome::Warning, "files-busy", cx);
            return;
        }
        let Some(api) = self
            .offered(cx)
            .then(|| self.session.read(cx).client())
            .flatten()
        else {
            return;
        };
        let folder = dirs::audio_dir()
            .or_else(|| dirs::home_dir().map(|home| home.join("Music")))
            .unwrap_or_else(std::env::temp_dir)
            .join(FOLDER);
        Toasts::show(Outcome::Done, "files-saving", cx);
        let work = Io::global(cx).spawn(async move { save_batch(api, batch, &folder).await });
        self.saving = Some(cx.spawn(async move |this, cx| {
            let outcome = work.await;
            this.update(cx, |this, cx| {
                this.saving = None;
                let toast = match outcome {
                    Ok(Ok((saved, 0, folder))) => Toast::new(Outcome::Done, "files-saved")
                        .count(saved)
                        .detail(folder.display().to_string()),
                    Ok(Ok((saved, failed, folder))) => {
                        Toast::new(Outcome::Warning, "files-saved-some")
                            .count(saved + failed)
                            .detail(failure_detail(failed, &folder))
                    }
                    Ok(Err(error)) => {
                        log::warn!("files: cannot save: {error:#}");
                        Toast::new(Outcome::Failed, "files-failed").detail(format!("{error:#}"))
                    }
                    Err(error) => {
                        log::warn!("files: the save stopped: {error:#}");
                        Toast::new(Outcome::Failed, "files-failed")
                    }
                };
                Toasts::post(toast, cx);
            })
            .ok();
        }));
    }
}

/// Writes every track of `batch` into `folder`, by artist and album, answering how many were
/// saved and how many failed.
async fn save_batch(
    api: Arc<dyn MusicApi>,
    batch: Batch,
    folder: &Path,
) -> Result<(usize, usize, PathBuf)> {
    let tracks = match batch {
        Batch::Tracks(tracks) => tracks,
        Batch::Album(id) => api.album_tracks(&id).await?,
        Batch::Playlist(id) => api.playlist_tracks(&id).await?,
    };
    let (mut saved, mut failed) = (0, 0);
    for track in tracks {
        let Some(id) = track.id.clone() else {
            continue;
        };
        match save_one(api.as_ref(), &id, &track, folder).await {
            Ok(path) => {
                log::info!("files: saved {}", path.display());
                saved += 1;
            }
            Err(error) => {
                log::warn!("files: cannot save {}: {error:#}", track.name);
                failed += 1;
            }
        }
    }
    Ok((saved, failed, folder.to_owned()))
}

async fn save_one(api: &dyn MusicApi, id: &str, track: &Track, folder: &Path) -> Result<PathBuf> {
    let (bytes, extension) = api
        .original_file(id)
        .await?
        .context("only songs on your own server can be saved as files")?;
    let folder = folder.join(clean(&track.artists)).join(clean(&track.album));
    std::fs::create_dir_all(&folder).context("cannot make the folder")?;
    let path = folder.join(format!("{}.{}", clean(&track.name), clean(&extension)));
    std::fs::write(&path, bytes).context("cannot write the file")?;
    Ok(path)
}

/// The detail of a save that partly failed: how many failed, and where the rest went.
fn failure_detail(failed: usize, folder: &Path) -> String {
    let mut args = i18n::FluentArgs::new();
    args.set("count", failed as i64);
    args.set("folder", folder.display().to_string());
    i18n::lookup("files-failed-count", Some(&args)).to_string()
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
