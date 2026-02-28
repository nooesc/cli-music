use apple_music::AppleMusic;
use color_eyre::Result;
use serde::Deserialize;
use std::process::Command;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PlayerStatus {
    pub track_name: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub position: f64,
    pub state: PlayState,
    pub volume: i8,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RepeatMode {
    Off,
    One,
    All,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        Self {
            track_name: String::new(),
            artist: String::new(),
            album: String::new(),
            duration: 0.0,
            position: 0.0,
            state: PlayState::Stopped,
            volume: 50,
            shuffle: false,
            repeat: RepeatMode::Off,
        }
    }
}

// ---------------------------------------------------------------------------
// Lightweight JXA polling
// ---------------------------------------------------------------------------

/// Raw shape returned by the JXA script.
#[derive(Deserialize)]
struct JxaStatus {
    state: String,
    position: f64,
    volume: i8,
    shuffle: bool,
    repeat: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: String,
    #[serde(default)]
    duration: f64,
}

const JXA_POLL_SCRIPT: &str = r#"
(function() {
    var app = Application('Music');
    var state = app.playerState();
    var result = {
        state:    state,
        position: 0,
        volume:   app.soundVolume(),
        shuffle:  app.shuffleEnabled(),
        repeat:   app.songRepeat(),
        name:     '',
        artist:   '',
        album:    '',
        duration: 0
    };
    if (state !== 'stopped') {
        result.position = app.playerPosition();
        var t = app.currentTrack;
        result.name     = t.name();
        result.artist   = t.artist();
        result.album    = t.album();
        result.duration = t.duration();
    }
    return JSON.stringify(result);
})()
"#;

/// Poll Apple Music player status via a single lightweight JXA invocation.
///
/// This is intentionally cheaper than `AppleMusic::get_application_data()`, which
/// fetches playlists, airplay devices, and much more. We only grab the fields
/// the TUI status bar needs.
pub fn poll_player_status() -> PlayerStatus {
    let output = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(JXA_POLL_SCRIPT)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return PlayerStatus::default(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: JxaStatus = match serde_json::from_str(stdout.trim()) {
        Ok(v) => v,
        Err(_) => return PlayerStatus::default(),
    };

    let state = match raw.state.as_str() {
        "playing" => PlayState::Playing,
        "paused" => PlayState::Paused,
        _ => PlayState::Stopped,
    };

    let repeat = match raw.repeat.as_str() {
        "one" => RepeatMode::One,
        "all" => RepeatMode::All,
        _ => RepeatMode::Off,
    };

    PlayerStatus {
        track_name: raw.name,
        artist: raw.artist,
        album: raw.album,
        duration: raw.duration,
        position: raw.position,
        state,
        volume: raw.volume,
        shuffle: raw.shuffle,
        repeat,
    }
}

// ---------------------------------------------------------------------------
// Playback controls
// ---------------------------------------------------------------------------

/// Toggle play/pause.
pub fn toggle_playback() -> Result<()> {
    AppleMusic::playpause().map_err(|e| color_eyre::eyre::eyre!("{e:?}"))?;
    Ok(())
}

/// Skip to the next track.
pub fn next_track() -> Result<()> {
    AppleMusic::next_track().map_err(|e| color_eyre::eyre::eyre!("{e:?}"))?;
    Ok(())
}

/// Go back to the previous track.
pub fn previous_track() -> Result<()> {
    AppleMusic::previous_track().map_err(|e| color_eyre::eyre::eyre!("{e:?}"))?;
    Ok(())
}

/// Set the player volume, clamped to 0..=100.
pub fn set_volume(vol: i8) -> Result<()> {
    let clamped = vol.clamp(0, 100);
    AppleMusic::set_sound_volume(clamped).map_err(|e| color_eyre::eyre::eyre!("{e:?}"))?;
    Ok(())
}

/// Cycle play mode: normal → shuffle → repeat all → repeat one → normal.
/// Uses the already-polled player state to decide what to set next.
pub fn cycle_play_mode(player: &PlayerStatus) {
    let script = if player.shuffle {
        // shuffle on → turn off shuffle, turn on repeat all
        r#"
            var app = Application('Music');
            app.shuffleEnabled = false;
            app.songRepeat = 'all';
        "#.to_string()
    } else {
        match player.repeat {
            RepeatMode::All => {
                // repeat all → repeat one
                r#"
                    var app = Application('Music');
                    app.songRepeat = 'one';
                "#.to_string()
            }
            RepeatMode::One => {
                // repeat one → normal (everything off)
                r#"
                    var app = Application('Music');
                    app.songRepeat = 'off';
                "#.to_string()
            }
            RepeatMode::Off => {
                // normal → shuffle
                r#"
                    var app = Application('Music');
                    app.shuffleEnabled = true;
                "#.to_string()
            }
        }
    };
    let _ = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output();
}

/// Add the currently playing track to the user's library.
pub fn add_to_library() {
    let script = r#"
        var app = Application('Music');
        if (app.playerState() !== 'stopped') {
            var t = app.currentTrack;
            t.favorited = true;
        }
    "#;
    let _ = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", script])
        .output();
}


/// Seek to a specific position (in seconds) in the current track.
pub fn seek_to(position: f64) {
    let script = format!(
        r#"
        var Music = Application("Music");
        if (Music.playerState() !== "stopped") {{
            Music.playerPosition = {};
        }}
        "#,
        position
    );
    let _ = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output();
}
