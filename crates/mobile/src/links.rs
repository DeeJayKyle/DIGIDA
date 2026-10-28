//! Opening a link someone shared, from any service the app knows, as the song, album, artist or
//! playlist it points at.

use music::link::Linked;

use crate::{Digida, MobileError, Playlist, Release, Song, list, lock, release, songs};

/// What a shared link points at, ready for the screen that shows it.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum Opened {
    PlaySong {
        song: Song,
    },
    ShowAlbum {
        provider: String,
        release: Release,
    },
    ShowArtist {
        provider: String,
        id: String,
        name: String,
    },
    ShowPlaylist {
        provider: String,
        playlist: Playlist,
    },
}

#[uniffi::export]
impl Digida {
    /// Reads `link` against every service and fetches what it points at from the one it belongs
    /// to. That service has to be signed in; a short link is followed first.
    pub fn open_link(&self, link: String) -> Result<Opened, MobileError> {
        let (slug, linked) = match self.linked(&link) {
            Some((slug, Linked::Short)) => {
                let followed = self
                    .runtime
                    .block_on(music::link::follow(&link))
                    .ok_or_else(|| MobileError::Failed(format!("cannot follow {link}")))?;
                self.linked(&followed.address)
                    .filter(|(_, linked)| !matches!(linked, Linked::Short))
                    .unwrap_or((slug, Linked::Short))
            }
            Some(found) => found,
            None => return Err(MobileError::Failed(format!("no service opens {link}"))),
        };
        let api = crate::api_of(&self.shared, &slug)
            .ok_or_else(|| MobileError::Failed(format!("{slug} is not signed in")))?;
        let opened = match linked {
            Linked::Track(id) => {
                let track = self.runtime.block_on(api.track(&id))?;
                let song = songs(vec![track], &slug)
                    .pop()
                    .ok_or_else(|| MobileError::Failed(format!("{slug} has no song {id}")))?;
                Opened::PlaySong { song }
            }
            Linked::Album(id) => {
                let detail = self.runtime.block_on(api.album(&id))?;
                Opened::ShowAlbum {
                    provider: slug,
                    release: release(detail.album),
                }
            }
            Linked::Artist(id) => {
                let name = self
                    .runtime
                    .block_on(api.artist_profile(&id))
                    .map(|profile| profile.name)
                    .unwrap_or_default();
                Opened::ShowArtist {
                    provider: slug,
                    id,
                    name,
                }
            }
            Linked::Playlist(id) => {
                let detail = self.runtime.block_on(api.playlist(&id))?;
                Opened::ShowPlaylist {
                    provider: slug,
                    playlist: list(detail.playlist),
                }
            }
            Linked::User(_) | Linked::Jam(_) | Linked::Short => {
                return Err(MobileError::Failed(format!("DIGIDA cannot open {link}")));
            }
        };
        Ok(opened)
    }
}

impl Digida {
    /// The service `link` belongs to and what it names there, read without fetching anything.
    fn linked(&self, link: &str) -> Option<(String, Linked)> {
        lock(&self.shared.slots).iter().find_map(|(slug, slot)| {
            slot.provider
                .link(link)
                .map(|linked| (slug.clone(), linked))
        })
    }
}
