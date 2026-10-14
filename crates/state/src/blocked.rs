use music::{ArtistRef, Track};
use serde::{Deserialize, Serialize};

use crate::Session;

/// A song or artist excluded from playback on one provider, kept on this computer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocked {
    pub provider: String,
    pub kind: BlockedKind,
    pub id: Option<String>,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockedKind {
    Song,
    Artist,
}

impl BlockedKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Song => "blocked-song",
            Self::Artist => "blocked-artist",
        }
    }
}

impl Blocked {
    pub fn song(track: &Track, session: &Session) -> Option<Self> {
        let id = track.id.as_deref()?;
        Some(Self {
            provider: session.slug_for(id)?.to_owned(),
            kind: BlockedKind::Song,
            id: Some(id.to_owned()),
            name: track.name.clone(),
        })
    }

    pub fn artist(artist: &ArtistRef, provider: &str) -> Option<Self> {
        if artist.name.trim().is_empty() {
            return None;
        }
        Some(Self {
            provider: provider.to_owned(),
            kind: BlockedKind::Artist,
            id: artist.id.clone(),
            name: artist.name.clone(),
        })
    }

    /// Compares identity without depending on a song or artist's current display name.
    pub fn same(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.kind == other.kind
            && match (&self.id, &other.id) {
                (Some(left), Some(right)) => left == right,
                (None, None) => same_name(&self.name, &other.name),
                _ => false,
            }
    }

    pub fn matches(&self, track: &Track, provider: &str) -> bool {
        if self.provider != provider {
            return false;
        }
        match self.kind {
            BlockedKind::Song => self.id.is_some() && self.id == track.id,
            BlockedKind::Artist => {
                track
                    .artist_refs
                    .iter()
                    .any(|artist| match (&self.id, &artist.id) {
                        (Some(blocked), Some(id)) => blocked == id,
                        _ => same_name(&self.name, &artist.name),
                    })
                    || (track.artist_refs.is_empty() && same_name(&self.name, &track.artists))
            }
        }
    }
}

// shortcut: missing artist IDs match a whole name, use provider metadata if names prove ambiguous.
fn same_name(left: &str, right: &str) -> bool {
    left.trim().to_lowercase() == right.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_respect_provider_ids_and_missing_artist_metadata() {
        let mut track = Track {
            id: Some("song".into()),
            name: "Song".into(),
            playable: true,
            artists: String::new(),
            artist_refs: Vec::new(),
            album: String::new(),
            album_id: None,
            cover: None,
            duration: std::time::Duration::ZERO,
            added_at: None,
            added_by: None,
            playcount: None,
            popularity: 0,
            explicit: false,
            track_number: 0,
            disc_number: 0,
            tags: Vec::new(),
            languages: Vec::new(),
            credits: Vec::new(),
        };
        let song = Blocked {
            provider: "spotify".into(),
            kind: BlockedKind::Song,
            id: track.id.clone(),
            name: track.name.clone(),
        };
        assert!(song.matches(&track, "spotify"));
        assert!(!song.matches(&track, "youtube"));
        let artist = Blocked::artist(
            &ArtistRef {
                name: "Artist".into(),
                id: Some("one".into()),
            },
            "spotify",
        )
        .unwrap();
        track.artist_refs = vec![ArtistRef {
            name: "Renamed".into(),
            id: Some("one".into()),
        }];
        assert!(artist.matches(&track, "spotify"));
        track.artist_refs = vec![ArtistRef {
            name: "Artist".into(),
            id: Some("two".into()),
        }];
        assert!(!artist.matches(&track, "spotify"));
        track.artist_refs.push(ArtistRef {
            name: " ARTIST ".into(),
            id: None,
        });
        assert!(artist.matches(&track, "spotify"));
        track.artist_refs.clear();
        track.artists = "Artist".into();
        assert!(artist.matches(&track, "spotify"));
        track.artists = "Artist, Another".into();
        assert!(!artist.matches(&track, "spotify"));
        let saved: Blocked =
            serde_json::from_str(&serde_json::to_string(&artist).unwrap()).unwrap();
        assert!(saved.same(&artist));
    }
}
