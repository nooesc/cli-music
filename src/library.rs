use color_eyre::Result;
use serde::Deserialize;
use std::process::Command;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PlaylistEntry {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct TrackEntry {
    pub id: i32,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
}

// Serde helpers for JSON parsing
#[derive(Deserialize)]
struct RawPlaylist {
    id: i32,
    name: String,
}

#[derive(Deserialize)]
struct RawTrack {
    id: i32,
    name: String,
    artist: String,
    album: String,
    duration: f64,
}

// ---------------------------------------------------------------------------
// JS string escaping
// ---------------------------------------------------------------------------

fn escape_js(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\0', "")
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch all playlists (id + name) from Apple Music.
pub fn fetch_playlists() -> Result<Vec<PlaylistEntry>> {
    let script = r#"
(function() {
    var app = Application('Music');
    var pls = app.playlists();
    var result = [];
    for (var i = 0; i < pls.length; i++) {
        result.push({ id: pls[i].id(), name: pls[i].name() });
    }
    return JSON.stringify(result);
})()
"#;

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", script])
        .output()?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: Vec<RawPlaylist> = serde_json::from_str(stdout.trim()).unwrap_or_default();

    Ok(raw
        .into_iter()
        .map(|p| PlaylistEntry {
            id: p.id,
            name: p.name,
        })
        .collect())
}

/// Fetch tracks from a named playlist (capped at 500).
pub fn fetch_playlist_tracks(playlist_name: &str) -> Result<Vec<TrackEntry>> {
    let escaped = escape_js(playlist_name);
    let script = format!(
        r#"
(function() {{
    var app = Application('Music');
    var pl = app.playlists.byName("{}");
    var tracks = pl.tracks();
    var cap = Math.min(tracks.length, 500);
    var result = [];
    for (var i = 0; i < cap; i++) {{
        var t = tracks[i];
        result.push({{
            id:       t.id(),
            name:     t.name(),
            artist:   t.artist(),
            album:    t.album(),
            duration: t.duration()
        }});
    }}
    return JSON.stringify(result);
}})()"#,
        escaped
    );

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output()?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: Vec<RawTrack> = serde_json::from_str(stdout.trim()).unwrap_or_default();

    Ok(raw
        .into_iter()
        .map(|t| TrackEntry {
            id: t.id,
            name: t.name,
            artist: t.artist,
            album: t.album,
            duration: t.duration,
        })
        .collect())
}

/// Play a track by its persistent ID.
pub fn play_track_by_id(track_id: i32) {
    let script = format!(
        r#"
(function() {{
    var app = Application('Music');
    var matches = app.tracks.whose({{id: {}}});
    if (matches.length > 0) {{
        matches[0].play();
    }}
}})()"#,
        track_id
    );

    let _ = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output();
}

/// Search the main Library playlist (capped at 200 results).
pub fn search_library(query: &str) -> Result<Vec<TrackEntry>> {
    let escaped = escape_js(query);
    let script = format!(
        r#"
(function() {{
    var app = Application('Music');
    var library = app.playlists.whose({{name: "Library"}});
    var results = library[0].search({{for: "{}"}});
    if (!results) return JSON.stringify([]);
    var cap = Math.min(results.length, 200);
    var out = [];
    for (var i = 0; i < cap; i++) {{
        var t = results[i];
        out.push({{
            id:       t.id(),
            name:     t.name(),
            artist:   t.artist(),
            album:    t.album(),
            duration: t.duration()
        }});
    }}
    return JSON.stringify(out);
}})()"#,
        escaped
    );

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output()?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: Vec<RawTrack> = serde_json::from_str(stdout.trim()).unwrap_or_default();

    Ok(raw
        .into_iter()
        .map(|t| TrackEntry {
            id: t.id,
            name: t.name,
            artist: t.artist,
            album: t.album,
            duration: t.duration,
        })
        .collect())
}
