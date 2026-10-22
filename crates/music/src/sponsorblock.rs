use std::time::Duration;

use anyhow::{Context as _, Result};
use serde::Deserialize;

const ENDPOINT: &str = "https://sponsor.ajay.app/api/skipSegments";
const TIMEOUT: Duration = Duration::from_secs(5);
const CATEGORIES: [&str; 3] = ["music_offtopic", "sponsor", "selfpromo"];

/// A community-marked section to skip, with an exclusive end position.
#[derive(Clone, Copy, Debug)]
pub struct SkipSegment {
    pub start: Duration,
    pub end: Duration,
}

/// Reads sponsors, self-promotion and non-music sections without changing the audio stream.
#[derive(Clone, Default)]
pub struct SponsorBlock {
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct Found {
    segment: [f64; 2],
    category: String,
    #[serde(rename = "actionType")]
    action: String,
}

impl SponsorBlock {
    pub async fn segments(&self, video: &str) -> Result<Vec<SkipSegment>> {
        let categories = serde_json::to_string(&CATEGORIES)?;
        let response = self
            .http
            .get(ENDPOINT)
            .query(&[
                ("videoID", video),
                ("categories", categories.as_str()),
                ("actionTypes", "[\"skip\"]"),
            ])
            .timeout(TIMEOUT)
            .send()
            .await
            .context("cannot reach sponsorblock")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        let found: Vec<Found> = response
            .error_for_status()
            .context("sponsorblock refused the lookup")?
            .json()
            .await
            .context("cannot read the sponsorblock response")?;
        Ok(normalize(found))
    }
}

/// Rejects invalid ranges and combines overlapping or adjacent sections in enabled categories.
fn normalize(found: Vec<Found>) -> Vec<SkipSegment> {
    let mut segments: Vec<SkipSegment> = found
        .into_iter()
        .filter_map(|found| {
            if !CATEGORIES.contains(&found.category.as_str()) || found.action != "skip" {
                return None;
            }
            let start = Duration::try_from_secs_f64(found.segment[0]).ok()?;
            let end = Duration::try_from_secs_f64(found.segment[1]).ok()?;
            (end > start).then_some(SkipSegment { start, end })
        })
        .collect();
    segments.sort_by_key(|segment| segment.start);
    // Overlapping submissions form one skip, so seeking never lands inside another one.
    let mut merged: Vec<SkipSegment> = Vec::new();
    for segment in segments {
        if let Some(last) = merged.last_mut()
            && segment.start <= last.end
        {
            last.end = last.end.max(segment.end);
        } else {
            merged.push(segment);
        }
    }
    merged
}
