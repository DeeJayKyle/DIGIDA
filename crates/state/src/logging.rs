use std::path::PathBuf;

const FILE: &str = "digida.log";

/// The file the running app appends its log to, `digida.log` under the platform's
/// state folder, or the cache folder where the platform has none. `None` when neither is known.
pub fn log_file() -> Option<PathBuf> {
    let root = dirs::state_dir().or_else(dirs::cache_dir)?;

    Some(root.join("digida").join(FILE))
}
