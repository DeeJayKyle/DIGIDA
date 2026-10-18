use std::collections::HashMap;
use std::time::Duration;

use gpui::{App, Global, Hsla, Rgba};
use music::{Lyrics as Sheet, LyricsLine, MediaKind, Track};
use serde::Serialize;
use state::{LyricsState, Origin, PlaybackState, Repeat, Shelf, Sonora};
use ui::ActiveTheme as _;

const SCHEME: &str = "sonora-api://";
const DEFAULT_QUEUE_LIMIT: usize = 10;
const MAX_QUEUE_LIMIT: usize = 50;
const DEFAULT_UNMUTE_VOLUME: f32 = 0.5;
const MAX_SEEK: Duration = Duration::from_secs(u32::MAX as u64);

/// Remembers the volume level from before the last API mute so unmuting restores it.
struct MuteMemory(f32);

impl Global for MuteMemory {}

#[derive(Serialize)]
struct Envelope<T> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct StatusPayload {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    wants_playing: bool,
    position_ms: u64,
    duration_ms: u64,
    progress: f32,
    volume: f32,
    volume_percent: u8,
    muted: bool,
    shuffle: bool,
    repeat: Repeat,
    radio: bool,
    saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<&'static str>,
    theme: ThemePayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<OriginPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    track: Option<TrackPayload>,
}

#[derive(Serialize)]
struct ThemePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    tint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tint_secondary: Option<String>,
}

#[derive(Serialize)]
struct OriginPayload {
    whence: state::Whence,
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize)]
struct TrackPayload {
    id: Option<String>,
    name: String,
    artists: String,
    album: String,
    album_id: Option<String>,
    cover: Option<String>,
    duration_ms: u64,
    explicit: bool,
    is_local: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    share_url: Option<String>,
}

#[derive(Serialize)]
struct LyricsPayload {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'static str>,
    synced: bool,
    instrumental: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_line: Option<LyricsLinePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_line: Option<LyricsLinePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plain: Option<String>,
    lines: Vec<LyricsLinePayload>,
}

#[derive(Clone, Serialize)]
struct LyricsLinePayload {
    start_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_ms: Option<u64>,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    romanized: Option<String>,
}

#[derive(Serialize)]
struct QueuePayload {
    shuffle: bool,
    repeat: Repeat,
    manual_count: usize,
    total_upcoming: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<OriginPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<TrackPayload>,
    upcoming: Vec<TrackPayload>,
}

/// Whether `value` is a `sonora-api://` request URI.
pub fn is_uri(value: &str) -> bool {
    value.trim().starts_with(SCHEME)
}

/// Whether `response` is a JSON envelope with `"ok":true`.
pub fn is_ok(response: &str) -> bool {
    response.trim().starts_with(r#"{"ok":true"#)
}

/// Returns the first `sonora-api://` URI found in `args`.
pub fn extract(args: &[String]) -> Option<&str> {
    args.iter()
        .map(String::as_str)
        .map(str::trim)
        .find(|arg| is_uri(arg))
}

/// The JSON error returned when an API command is invoked while no Sonora instance is running.
pub fn not_running() -> String {
    error_json("sonora is not running")
}

/// The JSON error returned when an API command times out waiting for the main thread.
pub fn timed_out() -> String {
    error_json("request timed out")
}

/// The JSON error returned when an API command exceeds the maximum URI length.
pub fn uri_too_long() -> String {
    error_json("request uri too long")
}

/// Executes a `sonora-api://` request against the live app state and returns a single-line JSON
/// response. Never activates or focuses the window.
pub fn handle(uri: &str, cx: &mut App) -> String {
    let Some(rest) = uri.trim().strip_prefix(SCHEME) else {
        return error_json("invalid scheme: expected sonora-api://");
    };
    let (raw_path, raw_query) = match rest.split_once('?') {
        Some((path, query)) => (path, query),
        None => (rest, ""),
    };
    let path = normalize_path(raw_path);
    let params = parse_query(raw_query);

    match path.as_str() {
        "" | "current" | "status" | "player" | "player/current" | "player/status" => {
            ok_json(status_snapshot(cx))
        }
        "play" | "resume" | "player/play" | "player/resume" => {
            let playback = Sonora::global(cx).playback.clone();
            playback.update(cx, |playback, cx| playback.resume(cx));
            ok_json(status_snapshot(cx))
        }
        "pause" | "player/pause" => {
            let playback = Sonora::global(cx).playback.clone();
            playback.update(cx, |playback, cx| playback.pause(cx));
            ok_json(status_snapshot(cx))
        }
        "toggle" | "play-pause" | "player/toggle" | "player/play-pause" => {
            let playback = Sonora::global(cx).playback.clone();
            playback.update(cx, |playback, cx| playback.toggle_play(cx));
            ok_json(status_snapshot(cx))
        }
        "next" | "skip" | "player/next" | "player/skip" => {
            let playback = Sonora::global(cx).playback.clone();
            playback.update(cx, |playback, cx| playback.next(cx));
            ok_json(status_snapshot(cx))
        }
        "previous" | "prev" | "player/previous" | "player/prev" => {
            let force = params
                .get("force")
                .is_some_and(|val| matches!(val.as_str(), "true" | "1" | "yes"));
            let playback = Sonora::global(cx).playback.clone();
            playback.update(cx, |playback, cx| match force {
                true => playback.previous_track(cx),
                false => playback.previous(cx),
            });
            ok_json(status_snapshot(cx))
        }
        "volume" | "player/volume" => handle_volume(&params, cx),
        "mute" | "player/mute" => handle_mute(&params, cx),
        "seek" | "player/seek" => handle_seek(&params, cx),
        "shuffle" | "player/shuffle" => handle_shuffle(&params, cx),
        "repeat" | "player/repeat" => handle_repeat(&params, cx),
        "favorite" | "like" | "player/favorite" | "player/like" => handle_favorite(&params, cx),
        "lyrics" | "player/lyrics" => ok_json(lyrics_snapshot(cx)),
        "queue" | "player/queue" => ok_json(queue_snapshot(&params, cx)),
        unknown => error_json(format!("unknown endpoint: {unknown}")),
    }
}

fn handle_volume(params: &HashMap<String, String>, cx: &mut App) -> String {
    let set_arg = params
        .get("set")
        .or_else(|| params.get("value"))
        .or_else(|| params.get("level"));
    let delta_arg = params
        .get("delta")
        .or_else(|| params.get("step"))
        .or_else(|| params.get("change"))
        .or_else(|| params.get("inc"));

    let playback = Sonora::global(cx).playback.clone();
    let current = playback.read(cx).volume();

    let target = match (set_arg, delta_arg) {
        (Some(raw), _) => match parse_volume(raw) {
            Some(level) => Some(level.clamp(0., 1.)),
            None => return error_json(format!("invalid volume value: {raw}")),
        },
        (None, Some(raw)) => match parse_volume(raw) {
            Some(delta) => Some((current + delta).clamp(0., 1.)),
            None => return error_json(format!("invalid volume delta: {raw}")),
        },
        (None, None) => None,
    };

    if let Some(level) = target {
        if level > 0. {
            cx.set_global(MuteMemory(level));
        }
        playback.update(cx, |playback, cx| playback.set_volume(level, cx));
    }

    ok_json(status_snapshot(cx))
}

fn handle_mute(params: &HashMap<String, String>, cx: &mut App) -> String {
    let mode = params.get("mode").map(String::as_str).unwrap_or("toggle");
    let playback = Sonora::global(cx).playback.clone();
    let current = playback.read(cx).volume();
    let muted = current <= f32::EPSILON;

    let should_mute = match mode {
        "toggle" => !muted,
        "on" | "true" | "1" => true,
        "off" | "false" | "0" => false,
        other => return error_json(format!("invalid mute mode: {other}")),
    };

    match should_mute {
        true if !muted => {
            cx.set_global(MuteMemory(current));
            playback.update(cx, |playback, cx| playback.set_volume(0., cx));
        }
        false if muted => {
            let restored = cx
                .try_global::<MuteMemory>()
                .map(|memory| memory.0)
                .filter(|&level| level > f32::EPSILON)
                .unwrap_or(DEFAULT_UNMUTE_VOLUME);
            playback.update(cx, |playback, cx| playback.set_volume(restored, cx));
        }
        _ => {}
    }

    ok_json(status_snapshot(cx))
}

fn handle_seek(params: &HashMap<String, String>, cx: &mut App) -> String {
    let playback = Sonora::global(cx).playback.clone();
    if let Some(raw) = params.get("fraction").or_else(|| params.get("progress")) {
        let Some(fraction) = raw
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
        else {
            return error_json(format!("invalid seek fraction: {raw}"));
        };
        playback.update(cx, |playback, cx| playback.seek_fraction(fraction, cx));
        return ok_json(status_snapshot(cx));
    }

    let current = playback.read(cx).live_position();
    let duration = playback
        .read(cx)
        .track()
        .map(|track| track.duration)
        .unwrap_or(Duration::ZERO);

    match resolve_seek_target(params, current, duration) {
        Ok(target) => {
            playback.update(cx, |playback, cx| playback.seek(target, cx));
            ok_json(status_snapshot(cx))
        }
        Err(reason) => error_json(reason),
    }
}

fn resolve_seek_target(
    params: &HashMap<String, String>,
    current: Duration,
    duration: Duration,
) -> Result<Duration, String> {
    let target = if let Some(raw) = params.get("ms") {
        let Some(target) = raw
            .trim()
            .parse::<u64>()
            .ok()
            .map(Duration::from_millis)
            .filter(|&target| target <= MAX_SEEK)
        else {
            return Err(format!("invalid seek ms: {raw}"));
        };
        target
    } else if let Some(raw) = params
        .get("set")
        .or_else(|| params.get("position"))
        .or_else(|| params.get("seconds"))
    {
        let Some(target) = parse_signed_f64(raw)
            .and_then(|secs| Duration::try_from_secs_f64(secs).ok())
            .filter(|&target| target <= MAX_SEEK)
        else {
            return Err(format!("invalid seek position: {raw}"));
        };
        target
    } else if let Some(raw) = params.get("delta").or_else(|| params.get("step")) {
        let Some(delta) = parse_signed_f64(raw) else {
            return Err(format!("invalid seek delta: {raw}"));
        };
        let Some(step) = Duration::try_from_secs_f64(delta.abs())
            .ok()
            .filter(|&step| step <= MAX_SEEK)
        else {
            return Err(format!("invalid seek delta: {raw}"));
        };
        match delta >= 0. {
            true => current.saturating_add(step).min(MAX_SEEK),
            false => current.saturating_sub(step),
        }
    } else {
        return Err(
            "seek requires ?set=<seconds>, ?ms=<ms>, ?delta=<seconds>, or ?fraction=<0..1>"
                .to_owned(),
        );
    };

    let clamped = match duration.is_zero() {
        true => target,
        false => target.min(duration),
    };
    Ok(clamped)
}

fn handle_shuffle(params: &HashMap<String, String>, cx: &mut App) -> String {
    let mode = params.get("mode").map(String::as_str).unwrap_or("toggle");
    let queue = Sonora::global(cx).queue.clone();
    match mode {
        "toggle" => queue.update(cx, |queue, cx| queue.toggle_shuffle(cx)),
        "on" | "true" | "1" => queue.update(cx, |queue, cx| queue.set_shuffle(true, cx)),
        "off" | "false" | "0" => queue.update(cx, |queue, cx| queue.set_shuffle(false, cx)),
        other => return error_json(format!("invalid shuffle mode: {other}")),
    }
    ok_json(status_snapshot(cx))
}

fn handle_repeat(params: &HashMap<String, String>, cx: &mut App) -> String {
    let mode = params.get("mode").map(String::as_str).unwrap_or("cycle");
    let playback = Sonora::global(cx).playback.clone();
    match mode {
        "cycle" => playback.update(cx, |playback, cx| playback.cycle_repeat(cx)),
        "toggle" => playback.update(cx, |playback, cx| playback.toggle_repeat(cx)),
        "off" | "none" => playback.update(cx, |playback, cx| playback.set_repeat(Repeat::Off, cx)),
        "all" | "queue" => playback.update(cx, |playback, cx| playback.set_repeat(Repeat::All, cx)),
        "one" | "track" => playback.update(cx, |playback, cx| playback.set_repeat(Repeat::One, cx)),
        other => return error_json(format!("invalid repeat mode: {other}")),
    }
    ok_json(status_snapshot(cx))
}

fn handle_favorite(params: &HashMap<String, String>, cx: &mut App) -> String {
    let mode = params.get("mode").map(String::as_str).unwrap_or("toggle");
    let sonora = Sonora::global(cx);
    let Some(track) = sonora.playback.read(cx).track().cloned() else {
        return error_json("no track is currently loaded");
    };
    let Some(id) = track.id.clone() else {
        return error_json("current track has no id");
    };
    let library = sonora.library.clone();
    let saved = library.read(cx).saved(&id);
    let target = match mode {
        "toggle" => !saved,
        "on" | "true" | "1" => true,
        "off" | "false" | "0" => false,
        other => return error_json(format!("invalid favorite mode: {other}")),
    };
    if target != saved {
        library.update(cx, |library, cx| library.toggle(track, cx));
    }
    ok_json(status_snapshot(cx))
}

fn status_snapshot(cx: &App) -> StatusPayload {
    let sonora = Sonora::global(cx);
    let playback = sonora.playback.read(cx);
    let queue = sonora.queue.read(cx);
    let library = sonora.library.read(cx);
    let session = sonora.session.read(cx);
    let cover = sonora.cover.read(cx);

    let (state, error) = match playback.state() {
        PlaybackState::Idle => ("idle", None),
        PlaybackState::Playing => ("playing", None),
        PlaybackState::Paused => ("paused", None),
        PlaybackState::Loading => ("loading", None),
        PlaybackState::Failed(reason) => ("failed", Some(reason.clone())),
    };

    let position = playback.live_position();
    let track = playback.track();
    let duration = track.map(|item| item.duration).unwrap_or(Duration::ZERO);
    let progress = match duration.is_zero() {
        true => 0.,
        false => (position.as_secs_f32() / duration.as_secs_f32()).clamp(0., 1.),
    };

    let volume = playback.volume();
    let volume_percent = (volume * 100.).round().clamp(0., 100.) as u8;
    let saved = track
        .and_then(|item| item.id.as_deref())
        .is_some_and(|id| library.saved(id));
    let provider = track
        .and_then(|item| item.id.as_deref())
        .and_then(|id| session.slug_for(id))
        .or_else(|| session.provider_slug());

    let theme = ThemePayload {
        tint: cx.theme().tint.map(hex_color),
        tint_secondary: cx.theme().tint_secondary.map(hex_color),
    };

    let origin = queue.origin().map(origin_payload);
    let track = track.map(|item| track_payload(item, cover, session));

    StatusPayload {
        state,
        error,
        wants_playing: playback.wants_playing(),
        position_ms: position.as_millis() as u64,
        duration_ms: duration.as_millis() as u64,
        progress,
        volume,
        volume_percent,
        muted: volume <= f32::EPSILON,
        shuffle: queue.shuffle(),
        repeat: playback.repeat(),
        radio: playback.radio(),
        saved,
        provider,
        theme,
        origin,
        track,
    }
}

fn lyrics_snapshot(cx: &App) -> LyricsPayload {
    let sonora = Sonora::global(cx);
    let lyrics = sonora.lyrics.read(cx);
    let position = sonora.playback.read(cx).live_position();

    let state = match lyrics.state() {
        LyricsState::Idle => "idle",
        LyricsState::Loading => "loading",
        LyricsState::Ready => "ready",
        LyricsState::Instrumental => "instrumental",
        LyricsState::Missing => "missing",
        LyricsState::Failed(_) => "failed",
    };

    let Some(hit) = lyrics.current() else {
        return LyricsPayload {
            state,
            source: None,
            synced: false,
            instrumental: matches!(lyrics.state(), LyricsState::Instrumental),
            active_index: None,
            active_line: None,
            next_line: None,
            plain: None,
            lines: Vec::new(),
        };
    };

    match &hit.lyrics {
        Sheet::Plain { text, romanized } => {
            let plain = match romanized {
                Some(rom) if !rom.text.is_empty() => Some(format!("{text}\n{}", rom.text)),
                _ => Some(text.clone()),
            };
            LyricsPayload {
                state,
                source: Some(hit.source),
                synced: false,
                instrumental: hit.instrumental,
                active_index: None,
                active_line: None,
                next_line: None,
                plain,
                lines: Vec::new(),
            }
        }
        Sheet::Synced { lines } => {
            let active_index = music::lyrics::active(lines, position);
            let mapped: Vec<LyricsLinePayload> = lines.iter().map(lyrics_line_payload).collect();
            let active_line = active_index.and_then(|idx| mapped.get(idx).cloned());
            let next_line = match active_index {
                Some(idx) => mapped.get(idx + 1).cloned(),
                None => mapped.first().cloned(),
            };
            LyricsPayload {
                state,
                source: Some(hit.source),
                synced: true,
                instrumental: hit.instrumental,
                active_index,
                active_line,
                next_line,
                plain: None,
                lines: mapped,
            }
        }
    }
}

fn queue_snapshot(params: &HashMap<String, String>, cx: &App) -> QueuePayload {
    let limit = params
        .get("limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(DEFAULT_QUEUE_LIMIT)
        .min(MAX_QUEUE_LIMIT);

    let sonora = Sonora::global(cx);
    let playback = sonora.playback.read(cx);
    let queue = sonora.queue.read(cx);
    let cover = sonora.cover.read(cx);
    let session = sonora.session.read(cx);

    let upcoming_iter = queue.upcoming();
    let total_upcoming = upcoming_iter.len();
    let upcoming = upcoming_iter
        .take(limit)
        .map(|track| track_payload(track, cover, session))
        .collect();

    QueuePayload {
        shuffle: queue.shuffle(),
        repeat: playback.repeat(),
        manual_count: queue.manual().len(),
        total_upcoming,
        origin: queue.origin().map(origin_payload),
        current: playback
            .track()
            .map(|track| track_payload(track, cover, session)),
        upcoming,
    }
}

fn origin_payload(origin: &Origin) -> OriginPayload {
    OriginPayload {
        whence: origin.whence,
        id: origin.id.clone(),
        name: origin.name.as_ref().map(ToString::to_string),
    }
}

fn track_payload(track: &Track, cover: &state::Cover, session: &state::Session) -> TrackPayload {
    let is_local = track.id.as_deref().is_some_and(music::is_local_id);
    let artwork = track
        .album_id
        .as_deref()
        .and_then(|album| cover.large_for(album))
        .or(track.cover.as_deref())
        .map(str::to_owned);
    let share_url = track.id.as_deref().and_then(|id| {
        let client = session.client_of(Shelf::of(id))?;
        client.share_url(MediaKind::Track, id)
    });

    TrackPayload {
        id: track.id.clone(),
        name: track.name.clone(),
        artists: track.artists.clone(),
        album: track.album.clone(),
        album_id: track.album_id.clone(),
        cover: artwork,
        duration_ms: track.duration.as_millis() as u64,
        explicit: track.explicit,
        is_local,
        share_url,
    }
}

fn lyrics_line_payload(line: &LyricsLine) -> LyricsLinePayload {
    LyricsLinePayload {
        start_ms: line.start.as_millis() as u64,
        end_ms: line.sung_end().map(|end| end.as_millis() as u64),
        text: line.text.clone(),
        romanized: line.romanized.as_ref().map(|rom| rom.text.clone()),
    }
}

fn hex_color(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    let r = (rgba.r.clamp(0., 1.) * 255.).round() as u8;
    let g = (rgba.g.clamp(0., 1.) * 255.).round() as u8;
    let b = (rgba.b.clamp(0., 1.) * 255.).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn normalize_path(raw: &str) -> String {
    raw.trim().trim_matches('/').to_ascii_lowercase()
}

fn parse_query(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&').filter(|part| !part.is_empty()) {
        let (key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, "true"),
        };
        let key = decode_component(key).trim().to_ascii_lowercase();
        let value = decode_component(value).trim().to_owned();
        if !key.is_empty() {
            map.insert(key, value);
        }
    }
    map
}

fn decode_component(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses a volume target or delta into the `0.0..=1.0` scale. Whole numbers (`50`, `+5`, `-1`)
/// and values ending with `%` are read as percentages (`0..=100`); decimal numbers in `-1.0..=1.0`
/// (`0.5`, `+0.05`) are read directly as fractions.
fn parse_volume(raw: &str) -> Option<f32> {
    let trimmed = raw.trim();
    let (body, explicit_percent) = match trimmed.strip_suffix('%') {
        Some(prefix) => (prefix.trim(), true),
        None => (trimmed, false),
    };
    let has_decimal = body.contains('.');
    let normalized = body.strip_prefix('+').unwrap_or(body).trim();
    let value = normalized.parse::<f32>().ok().filter(|v| v.is_finite())?;

    match explicit_percent || value.abs() > 1. || (!has_decimal && value != 0.) {
        true => Some(value / 100.),
        false => Some(value),
    }
}

fn parse_signed_f64(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    let normalized = trimmed.strip_prefix('+').unwrap_or(trimmed).trim();
    normalized.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn ok_json<T: Serialize>(data: T) -> String {
    serde_json::to_string(&Envelope {
        ok: true,
        data: Some(data),
        error: None,
    })
    .unwrap_or_else(|_| error_json("cannot serialize response"))
}

fn error_json(message: impl Into<String>) -> String {
    serde_json::to_string(&Envelope::<()> {
        ok: false,
        data: None,
        error: Some(message.into()),
    })
    .unwrap_or_else(|_| r#"{"ok":false,"error":"internal serialization error"}"#.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_api_uris() {
        assert!(is_uri("sonora-api://player/current"));
        assert!(is_uri("  sonora-api://volume?set=50 "));
        assert!(!is_uri("spotify:track:123"));
        assert!(!is_uri("https://open.spotify.com/track/123"));
    }

    #[test]
    fn parses_percent_and_fraction_volumes() {
        assert_eq!(parse_volume("50"), Some(0.5));
        assert_eq!(parse_volume("50%"), Some(0.5));
        assert_eq!(parse_volume("100"), Some(1.0));
        assert_eq!(parse_volume("1"), Some(0.01));
        assert_eq!(parse_volume("+5"), Some(0.05));
        assert_eq!(parse_volume("-5"), Some(-0.05));
        assert_eq!(parse_volume("-1"), Some(-0.01));
        assert_eq!(parse_volume("0"), Some(0.0));
        assert_eq!(parse_volume("0.5"), Some(0.5));
        assert_eq!(parse_volume("1.0"), Some(1.0));
        assert_eq!(parse_volume("+0.05"), Some(0.05));
        assert_eq!(parse_volume("-0.05"), Some(-0.05));
    }

    #[test]
    fn parses_query_parameters() {
        let params = parse_query("delta=%2B5&force=true&mode=toggle");
        assert_eq!(params.get("delta").map(String::as_str), Some("+5"));
        assert_eq!(params.get("force").map(String::as_str), Some("true"));
        assert_eq!(params.get("mode").map(String::as_str), Some("toggle"));
    }

    #[test]
    fn resolves_seek_and_rejects_extreme_values() {
        let current = Duration::from_secs(30);
        let duration = Duration::from_secs(180);

        assert_eq!(
            resolve_seek_target(&parse_query("set=45.5"), current, duration),
            Ok(Duration::from_millis(45_500))
        );
        assert_eq!(
            resolve_seek_target(&parse_query("delta=%2B15"), current, duration),
            Ok(Duration::from_secs(45))
        );
        assert_eq!(
            resolve_seek_target(&parse_query("delta=-45"), current, duration),
            Ok(Duration::ZERO)
        );
        assert_eq!(
            resolve_seek_target(&parse_query("ms=999999"), current, duration),
            Ok(duration)
        );

        assert!(resolve_seek_target(&parse_query("set=1e300"), current, duration).is_err());
        assert!(resolve_seek_target(&parse_query("set=-10"), current, duration).is_err());
        assert!(resolve_seek_target(&parse_query("delta=1e300"), current, duration).is_err());
        assert!(resolve_seek_target(&parse_query("delta=-1e300"), current, duration).is_err());
        assert!(resolve_seek_target(&parse_query("set=inf"), current, duration).is_err());
        assert!(resolve_seek_target(&parse_query("set=NaN"), current, duration).is_err());
    }

    #[test]
    fn detects_ok_and_error_envelopes() {
        assert!(is_ok(&ok_json(42)));
        assert!(!is_ok(&not_running()));
        assert!(!is_ok(&timed_out()));
        assert!(!is_ok(&uri_too_long()));
    }
}
