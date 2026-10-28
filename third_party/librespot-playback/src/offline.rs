//! Tracks an app downloaded so they play with no network, the way Spotify's own clients keep
//! them. The app fetches everything once with [`download`] while the session is connected and
//! keeps it however it likes. A player whose [`PlayerConfig`](crate::config::PlayerConfig)
//! names an [`Offline`] source asks it first, and a track it answers for plays without
//! metadata, key or file ever being requested.

use std::io::Read;

pub use crate::metadata::audio::AudioFileFormat;

use crate::{
    audio::{AudioFile, ProvidedFile},
    config::Bitrate,
    core::{Error, FileId, Session, SpotifyId, SpotifyUri, audio_key::AudioKey},
    metadata::audio::AudioItem,
    player::{data_rate, select_file},
};

/// What a downloaded track plays from.
pub struct OfflineTrack {
    /// The track's `TRACK_V4` metadata, as [`Download::metadata`] carried it.
    pub metadata: Vec<u8>,
    pub format: AudioFileFormat,
    pub file_id: FileId,
    /// The key the file is encrypted under, or `None` for one Spotify serves in the clear.
    pub key: Option<AudioKey>,
    /// The encrypted file as the CDN sent it.
    pub file: Box<dyn ProvidedFile>,
    pub len: u64,
}

/// The tracks an app has downloaded, which a player plays before reaching for the network.
pub trait Offline: Send + Sync {
    /// The downloaded copy of `track`, or `None` to load it over the network as usual.
    fn track(&self, track: SpotifyId) -> Option<OfflineTrack>;
}

/// Everything needed to play a track with no network, fetched while connected.
pub struct Download {
    pub metadata: Vec<u8>,
    pub format: AudioFileFormat,
    pub file_id: FileId,
    pub key: Option<AudioKey>,
    /// The whole encrypted file.
    pub file: Vec<u8>,
}

/// Fetches all of a track for offline play at `bitrate`: its metadata, the file the player would
/// pick, still encrypted, and the key to it. A track not available to this account is refused,
/// as it would be when played.
pub async fn download(
    session: &Session,
    uri: SpotifyUri,
    bitrate: Bitrate,
) -> Result<Download, Error> {
    let track: SpotifyId = (&uri)
        .try_into()
        .map_err(|_| Error::invalid_argument("not a track"))?;
    let metadata = session.spclient().get_track_metadata(&uri).await?.to_vec();
    let item = AudioItem::get_file(session, uri).await?;
    if let Err(reason) = item.availability {
        return Err(Error::unavailable(format!("{reason:?}")));
    }
    let (format, file_id) = select_file(&item.files, bitrate)
        .ok_or_else(|| Error::unavailable("no file in a supported format"))?;
    let audio = AudioFile::open(session, file_id, data_rate(format)).await?;
    audio.get_stream_loader_controller()?.set_stream_mode();
    let file = tokio::task::spawn_blocking(move || {
        let mut audio = audio;
        let mut file = Vec::new();
        audio.read_to_end(&mut file).map(|_| file)
    })
    .await
    .map_err(|error| Error::internal(error.to_string()))??;
    let key = session.audio_key().request(track, file_id).await?;
    Ok(Download {
        metadata,
        format,
        file_id,
        key: Some(key),
        file,
    })
}
