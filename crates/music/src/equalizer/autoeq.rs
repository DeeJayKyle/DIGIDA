//! AutoEQ's published headphone corrections: the list of every measured headphone, and the
//! parametric correction for one, in Equalizer APO's text that [`super::apo`] reads.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result};

/// Where AutoEQ's results are read from.
const RESULTS: &str = "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results";
/// How long a kept list is used before it is fetched again.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// One headphone AutoEQ has a correction for, and who measured it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Headphone {
    pub name: String,
    pub source: String,
    /// Where its results live under AutoEQ's `results` folder, already percent-encoded.
    pub path: String,
}

/// Every headphone AutoEQ lists, from `kept` when it was fetched within the week and from
/// AutoEQ otherwise, keeping the fresh list there. A list that cannot be kept is still used.
pub async fn index(kept: &Path) -> Result<Vec<Headphone>> {
    let fresh = std::fs::metadata(kept)
        .and_then(|meta| meta.modified())
        .is_ok_and(|at| at.elapsed().is_ok_and(|age| age < KEEP));
    let text = match fresh {
        true => std::fs::read_to_string(kept).context("cannot read the kept AutoEQ list")?,
        false => {
            let text = fetch(&format!("{RESULTS}/INDEX.md")).await?;
            if let Err(error) = std::fs::write(kept, &text) {
                log::warn!("autoeq: cannot keep the list: {error}");
            }
            text
        }
    };
    Ok(text.lines().filter_map(headphone).collect())
}

/// The headphones whose name holds every word of `query`, at most `limit` of them.
pub fn search(headphones: &[Headphone], query: &str, limit: usize) -> Vec<Headphone> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    headphones
        .iter()
        .filter(|headphone| {
            let name = headphone.name.to_lowercase();
            words.iter().all(|word| name.contains(word.as_str()))
        })
        .take(limit)
        .cloned()
        .collect()
}

/// AutoEQ's parametric correction for `headphone`, as Equalizer APO's text.
pub async fn profile(headphone: &Headphone) -> Result<String> {
    let folder = headphone
        .path
        .trim_start_matches("./")
        .trim_end_matches('/');
    let file = folder.rsplit('/').next().unwrap_or(folder);
    fetch(&format!("{RESULTS}/{folder}/{file}%20ParametricEQ.txt")).await
}

async fn fetch(url: &str) -> Result<String> {
    reqwest::get(url)
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("cannot fetch {url}"))?
        .text()
        .await
        .with_context(|| format!("cannot read {url}"))
}

/// One headphone out of a line of AutoEQ's index, which reads
/// `- [Name](./source/rig/Name) by source on rig`. The name may hold brackets of its own.
fn headphone(line: &str) -> Option<Headphone> {
    let rest = line.strip_prefix("- [")?;
    let (name, rest) = rest.split_once("](")?;
    let (path, source) = match rest.rsplit_once(") by ") {
        Some((path, by)) => (path, by.split(" on ").next().unwrap_or(by)),
        None => (rest.strip_suffix(')')?, ""),
    };
    Some(Headphone {
        name: name.to_owned(),
        source: source.trim().to_owned(),
        path: path.to_owned(),
    })
}
