//! The platform folders everything is kept under. A desktop asks the system; the Android app
//! has no answer from `dirs` and names its own private folders once at startup instead.

use std::path::PathBuf;
use std::sync::OnceLock;

/// The data and cache roots named by [`set`], when an app has named them.
static FOLDERS: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();

/// Names the data root, for what has to last, and the cache root, for what can be fetched
/// again. Call it once, before anything opens a database or reads a sign-in; a later call is
/// ignored, so every part of the app agrees on one place.
pub fn set(data: PathBuf, cache: PathBuf) {
    if FOLDERS.set((data, cache)).is_err() {
        log::warn!("storage: the folders were already named, so the new ones are ignored");
    }
}

/// The folder lasting, private data goes under: the one named by [`set`], or the system's.
pub fn data_dir() -> Option<PathBuf> {
    match FOLDERS.get() {
        Some((data, _)) => Some(data.clone()),
        None => dirs::data_local_dir(),
    }
}

/// The folder data that can be fetched again goes under: the one named by [`set`], or the
/// system's.
pub fn cache_dir() -> Option<PathBuf> {
    match FOLDERS.get() {
        Some((_, cache)) => Some(cache.clone()),
        None => dirs::cache_dir(),
    }
}
