# cli-music Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build an interactive terminal UI for Apple Music on macOS, with a now-playing panel (ASCII album art), library/playlist browser, and playback controls.

**Architecture:** Hybrid bridge using the `apple-music` crate for playback/track info + custom JXA scripts for gaps (search, seek, lightweight polling). Ratatui TUI with background polling thread via `std::thread` + `mpsc`.

**Tech Stack:** Rust, ratatui 0.30, crossterm 0.29, apple-music 0.11.5, image crate, serde/serde_json, color-eyre

---

### Task 1: Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

**Step 1: Initialize the Rust project**

Run: `cargo init --name cli-music`

**Step 2: Set up dependencies in Cargo.toml**

Replace the generated `Cargo.toml` with:

```toml
[package]
name = "cli-music"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = "0.30"
crossterm = "0.29"
apple-music = "0.11"
image = "0.25"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
color-eyre = "0.6"
```

**Step 3: Write a minimal main.rs that compiles**

```rust
use color_eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;
    println!("cli-music");
    Ok(())
}
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully (will download deps)

**Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: initialize project with dependencies"
```

---

### Task 2: Music Bridge - Core Types and Polling

**Files:**
- Create: `src/bridge.rs`
- Modify: `src/main.rs`

**Step 1: Create the bridge module with core types**

Create `src/bridge.rs`:

```rust
use apple_music::{AppleMusic, PlayerState};
use color_eyre::Result;
use std::process::Command;

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

/// Lightweight poll using custom JXA - only fetches what we need.
/// Much cheaper than apple_music::AppleMusic::get_application_data().
pub fn poll_player_status() -> Result<PlayerStatus> {
    let script = r#"
        var Music = Application("Music");
        var state = Music.playerState();
        var result = {
            state: state,
            position: 0,
            volume: Music.soundVolume(),
            shuffle: Music.shuffleEnabled(),
            repeat: String(Music.songRepeat()),
            track_name: "",
            artist: "",
            album: "",
            duration: 0
        };
        if (state !== "stopped") {
            result.position = Music.playerPosition();
            try {
                var t = Music.currentTrack;
                result.track_name = t.name();
                result.artist = t.artist();
                result.album = t.album();
                result.duration = t.duration();
            } catch(e) {}
        }
        JSON.stringify(result);
    "#;

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", script])
        .output()?;

    if !output.status.success() {
        return Ok(PlayerStatus::default());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim())?;

    Ok(PlayerStatus {
        track_name: v["track_name"].as_str().unwrap_or("").to_string(),
        artist: v["artist"].as_str().unwrap_or("").to_string(),
        album: v["album"].as_str().unwrap_or("").to_string(),
        duration: v["duration"].as_f64().unwrap_or(0.0),
        position: v["position"].as_f64().unwrap_or(0.0),
        state: match v["state"].as_str().unwrap_or("stopped") {
            "playing" => PlayState::Playing,
            "paused" => PlayState::Paused,
            _ => PlayState::Stopped,
        },
        volume: v["volume"].as_i64().unwrap_or(50) as i8,
        shuffle: v["shuffle"].as_bool().unwrap_or(false),
        repeat: match v["repeat"].as_str().unwrap_or("off") {
            "one" => RepeatMode::One,
            "all" => RepeatMode::All,
            _ => RepeatMode::Off,
        },
    })
}

pub fn toggle_playback() {
    let _ = AppleMusic::playpause();
}

pub fn next_track() {
    let _ = AppleMusic::next_track();
}

pub fn previous_track() {
    let _ = AppleMusic::previous_track();
}

pub fn set_volume(vol: i8) {
    let _ = AppleMusic::set_sound_volume(vol.clamp(0, 100));
}

pub fn toggle_shuffle() {
    if let Ok(data) = AppleMusic::get_application_data() {
        let _ = AppleMusic::set_shuffle(!data.shuffle_enabled);
    }
}

pub fn cycle_repeat() {
    if let Ok(data) = AppleMusic::get_application_data() {
        use apple_music::SongRepeatMode;
        let next = match data.song_repeat {
            Some(apple_music::SongRepeat::Off) | None => SongRepeatMode::ALL,
            Some(apple_music::SongRepeat::All) => SongRepeatMode::ONE,
            Some(apple_music::SongRepeat::One) => SongRepeatMode::OFF,
        };
        let _ = AppleMusic::set_song_repeat_mode(next);
    }
}
```

**Step 2: Register the module in main.rs**

```rust
mod bridge;

use color_eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;

    let status = bridge::poll_player_status()?;
    println!("{} - {} ({:?})", status.track_name, status.artist, status.state);

    Ok(())
}
```

**Step 3: Test with Music.app running**

Run: `cargo run`
Expected: Prints current track info or empty strings if nothing playing

**Step 4: Commit**

```bash
git add src/bridge.rs src/main.rs
git commit -m "feat: add music bridge with lightweight JXA polling"
```

---

### Task 3: TUI Skeleton - Event Loop and Layout

**Files:**
- Create: `src/app.rs`
- Create: `src/ui.rs`
- Modify: `src/main.rs`

**Step 1: Create the app state module**

Create `src/app.rs`:

```rust
use crate::bridge::{PlayerStatus, PlayState};

pub struct App {
    pub should_quit: bool,
    pub player: PlayerStatus,
    pub active_panel: Panel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    NowPlaying,
    Library,
}

impl Default for App {
    fn default() -> Self {
        Self {
            should_quit: false,
            player: PlayerStatus::default(),
            active_panel: Panel::Library,
        }
    }
}

impl App {
    pub fn update_player(&mut self, status: PlayerStatus) {
        self.player = status;
    }
}
```

**Step 2: Create the UI rendering module**

Create `src/ui.rs`:

```rust
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

use crate::app::{App, Panel};
use crate::bridge::PlayState;

pub fn draw(frame: &mut Frame, app: &App) {
    let [main_area, bottom_bar] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    let [left_panel, right_panel] = Layout::horizontal([
        Constraint::Percentage(40),
        Constraint::Percentage(60),
    ])
    .areas(main_area);

    draw_now_playing(frame, left_panel, app);
    draw_library(frame, right_panel, app);
    draw_controls(frame, bottom_bar, app);
}

fn draw_now_playing(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let border_style = if app.active_panel == Panel::NowPlaying {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Now Playing ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.player.track_name.is_empty() {
        frame.render_widget(
            Paragraph::new("Nothing playing").dark_gray(),
            inner,
        );
        return;
    }

    let text = vec![
        Line::from(""),
        Line::from(app.player.track_name.clone().bold().white()),
        Line::from(
            vec![
                Span::from(app.player.artist.clone()).cyan(),
                Span::from(" - ").dark_gray(),
                Span::from(app.player.album.clone()).dark_gray(),
            ]
        ),
    ];

    frame.render_widget(Paragraph::new(text), inner);
}

fn draw_library(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let border_style = if app.active_panel == Panel::Library {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Library ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    frame.render_widget(
        Paragraph::new("Playlists will go here").dark_gray(),
        inner,
    );
}

fn draw_controls(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let state_icon = match app.player.state {
        PlayState::Playing => "▶",
        PlayState::Paused => "⏸",
        PlayState::Stopped => "⏹",
    };

    let elapsed = format_time(app.player.position);
    let total = format_time(app.player.duration);
    let ratio = if app.player.duration > 0.0 {
        (app.player.position / app.player.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let label = format!("{state_icon}  {elapsed} / {total}");

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .ratio(ratio)
        .label(label)
        .use_unicode(true);

    frame.render_widget(gauge, area);
}

fn format_time(seconds: f64) -> String {
    let s = seconds as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
```

**Step 3: Wire up main.rs with the event loop**

```rust
mod app;
mod bridge;
mod ui;

use app::App;
use bridge::PlayerStatus;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Tick,
    PlayerUpdate(PlayerStatus),
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

fn run(mut terminal: ratatui::DefaultTerminal) -> Result<()> {
    let mut app = App::default();
    let (tx, rx) = mpsc::channel();

    // Input thread
    let tx_input = tx.clone();
    thread::spawn(move || {
        loop {
            if event::poll(Duration::from_millis(200)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    if key.kind == KeyEventKind::Press {
                        let _ = tx_input.send(AppEvent::Key(key));
                    }
                }
            } else {
                let _ = tx_input.send(AppEvent::Tick);
            }
        }
    });

    // Player polling thread
    let tx_player = tx.clone();
    thread::spawn(move || {
        loop {
            if let Ok(status) = bridge::poll_player_status() {
                let _ = tx_player.send(AppEvent::PlayerUpdate(status));
            }
            thread::sleep(Duration::from_millis(500));
        }
    });

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        match rx.recv()? {
            AppEvent::Key(key) => handle_key(&mut app, key),
            AppEvent::Tick => {}
            AppEvent::PlayerUpdate(status) => app.update_player(status),
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char(' ') => bridge::toggle_playback(),
        KeyCode::Char('n') => bridge::next_track(),
        KeyCode::Char('p') => bridge::previous_track(),
        KeyCode::Char('+') | KeyCode::Char('=') => {
            bridge::set_volume((app.player.volume + 5).min(100));
        }
        KeyCode::Char('-') => {
            bridge::set_volume((app.player.volume - 5).max(0));
        }
        KeyCode::Char('s') => bridge::toggle_shuffle(),
        KeyCode::Char('r') => bridge::cycle_repeat(),
        KeyCode::Char('1') => app.active_panel = app::Panel::NowPlaying,
        KeyCode::Char('2') => app.active_panel = app::Panel::Library,
        KeyCode::Tab => {
            app.active_panel = match app.active_panel {
                app::Panel::NowPlaying => app::Panel::Library,
                app::Panel::Library => app::Panel::NowPlaying,
            };
        }
        _ => {}
    }
}
```

**Step 4: Run and verify the TUI renders**

Run: `cargo run`
Expected: Two-panel TUI with now-playing and library placeholders. Press `q` to quit. Space toggles playback. Progress bar updates every ~500ms.

**Step 5: Commit**

```bash
git add src/app.rs src/ui.rs src/main.rs
git commit -m "feat: add TUI skeleton with event loop and two-panel layout"
```

---

### Task 4: Library/Playlist Browser

**Files:**
- Create: `src/library.rs`
- Modify: `src/app.rs`
- Modify: `src/ui.rs`
- Modify: `src/main.rs`

**Step 1: Create the library module for fetching playlists and tracks**

Create `src/library.rs`:

```rust
use color_eyre::Result;
use serde::Deserialize;
use std::process::Command;

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

pub fn fetch_playlists() -> Result<Vec<PlaylistEntry>> {
    let script = r#"
        var Music = Application("Music");
        var playlists = Music.playlists();
        var result = [];
        for (var i = 0; i < playlists.length; i++) {
            try {
                result.push({
                    id: playlists[i].id(),
                    name: playlists[i].name()
                });
            } catch(e) {}
        }
        JSON.stringify(result);
    "#;

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", script])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let items: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).unwrap_or_default();

    Ok(items
        .iter()
        .filter_map(|v| {
            Some(PlaylistEntry {
                id: v["id"].as_i64()? as i32,
                name: v["name"].as_str()?.to_string(),
            })
        })
        .collect())
}

pub fn fetch_playlist_tracks(playlist_name: &str) -> Result<Vec<TrackEntry>> {
    let script = format!(
        r#"
        var Music = Application("Music");
        var pl = Music.playlists.byName("{}");
        var tracks = pl.tracks();
        var count = Math.min(tracks.length, 500);
        var result = [];
        for (var i = 0; i < count; i++) {{
            try {{
                var t = tracks[i];
                result.push({{
                    id: t.id(),
                    name: t.name(),
                    artist: t.artist(),
                    album: t.album(),
                    duration: t.duration()
                }});
            }} catch(e) {{}}
        }}
        JSON.stringify(result);
        "#,
        playlist_name.replace('\\', "\\\\").replace('"', "\\\"")
    );

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let items: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).unwrap_or_default();

    Ok(items
        .iter()
        .filter_map(|v| {
            Some(TrackEntry {
                id: v["id"].as_i64()? as i32,
                name: v["name"].as_str()?.to_string(),
                artist: v["artist"].as_str()?.to_string(),
                album: v["album"].as_str()?.to_string(),
                duration: v["duration"].as_f64()?,
            })
        })
        .collect())
}

pub fn play_track_by_id(track_id: i32) {
    let script = format!(
        r#"
        var Music = Application("Music");
        var tracks = Music.tracks.whose({{id: {}}});
        if (tracks.length > 0) {{ tracks[0].play(); }}
        "#,
        track_id
    );
    let _ = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output();
}

pub fn search_library(query: &str) -> Result<Vec<TrackEntry>> {
    let script = format!(
        r#"
        var Music = Application("Music");
        var library = Music.playlists.whose({{specialKind: {{_equals: "Library"}}}});
        if (library.length === 0) {{ JSON.stringify([]); }}
        var results = library[0].search({{for: "{}"}});
        var count = Math.min(results.length, 200);
        var tracks = [];
        for (var i = 0; i < count; i++) {{
            try {{
                var t = results[i];
                tracks.push({{
                    id: t.id(),
                    name: t.name(),
                    artist: t.artist(),
                    album: t.album(),
                    duration: t.duration()
                }});
            }} catch(e) {{}}
        }}
        JSON.stringify(tracks);
        "#,
        query.replace('\\', "\\\\").replace('"', "\\\"")
    );

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let items: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).unwrap_or_default();

    Ok(items
        .iter()
        .filter_map(|v| {
            Some(TrackEntry {
                id: v["id"].as_i64()? as i32,
                name: v["name"].as_str()?.to_string(),
                artist: v["artist"].as_str()?.to_string(),
                album: v["album"].as_str()?.to_string(),
                duration: v["duration"].as_f64()?,
            })
        })
        .collect())
}
```

**Step 2: Add library state to App**

Update `src/app.rs` to add playlist/track list state:

```rust
use crate::bridge::{PlayerStatus, PlayState};
use crate::library::{PlaylistEntry, TrackEntry};
use ratatui::widgets::ListState;

pub struct App {
    pub should_quit: bool,
    pub player: PlayerStatus,
    pub active_panel: Panel,
    pub playlists: Vec<PlaylistEntry>,
    pub playlist_state: ListState,
    pub tracks: Vec<TrackEntry>,
    pub track_state: ListState,
    pub view: LibraryView,
    pub search_mode: bool,
    pub search_query: String,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    NowPlaying,
    Library,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LibraryView {
    Playlists,
    Tracks,
    SearchResults,
}

impl Default for App {
    fn default() -> Self {
        Self {
            should_quit: false,
            player: PlayerStatus::default(),
            active_panel: Panel::Library,
            playlists: Vec::new(),
            playlist_state: ListState::default(),
            tracks: Vec::new(),
            track_state: ListState::default(),
            view: LibraryView::Playlists,
            search_mode: false,
            search_query: String::new(),
            loading: false,
        }
    }
}

impl App {
    pub fn update_player(&mut self, status: PlayerStatus) {
        self.player = status;
    }

    pub fn select_next(&mut self) {
        match self.view {
            LibraryView::Playlists => self.playlist_state.select_next(),
            LibraryView::Tracks | LibraryView::SearchResults => self.track_state.select_next(),
        }
    }

    pub fn select_previous(&mut self) {
        match self.view {
            LibraryView::Playlists => self.playlist_state.select_previous(),
            LibraryView::Tracks | LibraryView::SearchResults => self.track_state.select_previous(),
        }
    }

    pub fn selected_playlist(&self) -> Option<&PlaylistEntry> {
        self.playlist_state.selected().and_then(|i| self.playlists.get(i))
    }

    pub fn selected_track(&self) -> Option<&TrackEntry> {
        self.track_state.selected().and_then(|i| self.tracks.get(i))
    }
}
```

**Step 3: Update ui.rs to render the library list**

Replace `draw_library` in `src/ui.rs`:

```rust
use ratatui::widgets::{List, ListItem, HighlightSpacing};
use crate::app::LibraryView;

fn draw_library(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    let border_style = if app.active_panel == Panel::Library {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = match app.view {
        LibraryView::Playlists => " Playlists ",
        LibraryView::Tracks => " Tracks ",
        LibraryView::SearchResults => " Search Results ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    match app.view {
        LibraryView::Playlists => {
            let items: Vec<ListItem> = app
                .playlists
                .iter()
                .map(|p| ListItem::new(p.name.clone()))
                .collect();

            let list = List::new(items)
                .block(block)
                .highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ")
                .highlight_spacing(HighlightSpacing::Always);

            frame.render_stateful_widget(list, area, &mut app.playlist_state);
        }
        LibraryView::Tracks | LibraryView::SearchResults => {
            let items: Vec<ListItem> = app
                .tracks
                .iter()
                .map(|t| {
                    ListItem::new(Line::from(vec![
                        Span::from(t.name.clone()).white(),
                        Span::from(" - ").dark_gray(),
                        Span::from(t.artist.clone()).cyan(),
                    ]))
                })
                .collect();

            let list = List::new(items)
                .block(block)
                .highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ")
                .highlight_spacing(HighlightSpacing::Always);

            frame.render_stateful_widget(list, area, &mut app.track_state);
        }
    }
}
```

Note: The `draw` function signature and `draw_library` call need to change so `app` is `&mut App` (for `render_stateful_widget`). Update `pub fn draw(frame: &mut Frame, app: &mut App)` and the call in main.rs to `terminal.draw(|frame| ui::draw(frame, &mut app))?;`.

**Step 4: Update main.rs key handling for list navigation and library loading**

Add to the `handle_key` function and load playlists on startup:

```rust
// In run(), after creating app:
app.playlists = library::fetch_playlists().unwrap_or_default();
if !app.playlists.is_empty() {
    app.playlist_state.select(Some(0));
}

// Add to handle_key:
KeyCode::Char('j') | KeyCode::Down => {
    if app.active_panel == app::Panel::Library {
        app.select_next();
    }
}
KeyCode::Char('k') | KeyCode::Up => {
    if app.active_panel == app::Panel::Library {
        app.select_previous();
    }
}
KeyCode::Enter => {
    if app.active_panel == app::Panel::Library {
        match app.view {
            app::LibraryView::Playlists => {
                if let Some(playlist) = app.selected_playlist().cloned() {
                    app.tracks = library::fetch_playlist_tracks(&playlist.name)
                        .unwrap_or_default();
                    app.track_state = ratatui::widgets::ListState::default();
                    if !app.tracks.is_empty() {
                        app.track_state.select(Some(0));
                    }
                    app.view = app::LibraryView::Tracks;
                }
            }
            app::LibraryView::Tracks | app::LibraryView::SearchResults => {
                if let Some(track) = app.selected_track() {
                    library::play_track_by_id(track.id);
                }
            }
        }
    }
}
KeyCode::Esc => {
    if app.search_mode {
        app.search_mode = false;
        app.search_query.clear();
    } else if app.view != app::LibraryView::Playlists {
        app.view = app::LibraryView::Playlists;
        app.tracks.clear();
    }
}
KeyCode::Char('/') => {
    if app.active_panel == app::Panel::Library {
        app.search_mode = true;
        app.search_query.clear();
    }
}
```

Also add `mod library;` to main.rs.

**Step 5: Add search input handling**

When `app.search_mode` is true, capture characters into `search_query` and execute search on Enter:

```rust
// At the top of handle_key, before the main match:
if app.search_mode {
    match key.code {
        KeyCode::Enter => {
            if !app.search_query.is_empty() {
                app.tracks = library::search_library(&app.search_query)
                    .unwrap_or_default();
                app.track_state = ratatui::widgets::ListState::default();
                if !app.tracks.is_empty() {
                    app.track_state.select(Some(0));
                }
                app.view = app::LibraryView::SearchResults;
            }
            app.search_mode = false;
        }
        KeyCode::Esc => {
            app.search_mode = false;
            app.search_query.clear();
        }
        KeyCode::Backspace => {
            app.search_query.pop();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
        }
        _ => {}
    }
    return;
}
```

**Step 6: Add a search bar to the UI**

In `draw_library`, if `app.search_mode`, render a search input at the top of the library panel:

```rust
// At the bottom of draw_library, if in search mode, overlay a search bar:
if app.search_mode {
    let search_area = ratatui::layout::Rect {
        x: area.x + 1,
        y: area.y + area.height.saturating_sub(2),
        width: area.width.saturating_sub(2),
        height: 1,
    };
    let search_text = format!("/{}", app.search_query);
    frame.render_widget(
        Paragraph::new(search_text).style(Style::default().fg(Color::Yellow)),
        search_area,
    );
}
```

**Step 7: Test**

Run: `cargo run`
Expected: Library panel shows playlists. j/k or arrows to navigate. Enter to open a playlist and see tracks. Enter on a track to play it. `/` to search. Esc to go back.

**Step 8: Commit**

```bash
git add src/library.rs src/app.rs src/ui.rs src/main.rs
git commit -m "feat: add library browser with playlist/track navigation and search"
```

---

### Task 5: Album Art (Half-Block Renderer)

**Files:**
- Create: `src/artwork.rs`
- Modify: `src/app.rs`
- Modify: `src/ui.rs`
- Modify: `src/bridge.rs`

**Step 1: Create the artwork module**

Create `src/artwork.rs`:

```rust
use color_eyre::Result;
use image::{DynamicImage, imageops::FilterType};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use std::process::Command;

const UPPER_HALF: char = '\u{2580}'; // ▀

/// Convert a DynamicImage to ratatui Lines using half-block characters.
/// Each terminal row represents 2 pixel rows.
pub fn image_to_halfblocks(img: &DynamicImage, width: u16, height: u16) -> Vec<Line<'static>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let resized = img.resize_exact(width as u32, (height * 2) as u32, FilterType::Triangle);
    let rgb = resized.to_rgb8();
    let mut lines = Vec::with_capacity(height as usize);

    for row in 0..height {
        let mut spans = Vec::with_capacity(width as usize);
        let upper_y = (row * 2) as u32;
        let lower_y = upper_y + 1;

        for col in 0..width {
            let up = rgb.get_pixel(col as u32, upper_y);
            let lo = rgb.get_pixel(col as u32, lower_y);

            let fg = Color::Rgb(up[0], up[1], up[2]);
            let bg = Color::Rgb(lo[0], lo[1], lo[2]);

            spans.push(Span::styled(
                UPPER_HALF.to_string(),
                Style::default().fg(fg).bg(bg),
            ));
        }

        lines.push(Line::from(spans));
    }

    lines
}

/// Fetch artwork URL for a track from iTunes Search API.
/// Returns a URL string that can be fetched for the image data.
pub fn fetch_artwork_url(track_name: &str, artist: &str) -> Option<String> {
    let query = format!("{} {}", track_name, artist);
    let encoded = urlencoding::encode(&query);
    let url = format!(
        "https://itunes.apple.com/search?term={}&entity=song&limit=10",
        encoded
    );

    let resp = reqwest::blocking::get(&url).ok()?;
    let json: serde_json::Value = resp.json().ok()?;

    let results = json["results"].as_array()?;
    for result in results {
        if let Some(art_url) = result["artworkUrl100"].as_str() {
            // Upgrade to 300x300
            let high_res = art_url.replace("100x100bb", "300x300bb");
            return Some(high_res);
        }
    }
    None
}

/// Download image from URL and decode it.
pub fn download_image(url: &str) -> Option<DynamicImage> {
    let bytes = reqwest::blocking::get(url).ok()?.bytes().ok()?;
    image::load_from_memory(&bytes).ok()
}
```

Note: This adds `reqwest` (blocking) and `urlencoding` as dependencies. Add to Cargo.toml:

```toml
reqwest = { version = "0.12", features = ["blocking", "json"] }
urlencoding = "2"
```

**Step 2: Add artwork state to App**

Add to `src/app.rs`:

```rust
use image::DynamicImage;

// Add to App struct:
pub artwork: Option<DynamicImage>,
pub artwork_track: String,  // track name that the current artwork is for
```

Initialize both as `None` and `String::new()` in Default impl.

**Step 3: Fetch artwork when track changes**

In `App::update_player`, detect track changes and trigger artwork fetch:

```rust
pub fn update_player(&mut self, status: PlayerStatus) {
    let track_changed = status.track_name != self.artwork_track;
    self.player = status;

    if track_changed && !self.player.track_name.is_empty() {
        self.artwork_track = self.player.track_name.clone();
        // Fetch artwork (blocking for now, move to bg thread later if slow)
        if let Some(url) = crate::artwork::fetch_artwork_url(
            &self.player.track_name,
            &self.player.artist,
        ) {
            self.artwork = crate::artwork::download_image(&url);
        } else {
            self.artwork = None;
        }
    }
}
```

**Step 4: Render artwork in the now-playing panel**

Update `draw_now_playing` in `src/ui.rs` to show the album art above the track info:

```rust
fn draw_now_playing(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let border_style = if app.active_panel == Panel::NowPlaying {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Now Playing ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.player.track_name.is_empty() {
        frame.render_widget(
            Paragraph::new("Nothing playing").dark_gray(),
            inner,
        );
        return;
    }

    // Split inner area: artwork on top, track info below
    let [art_area, info_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
    ]).areas(inner);

    // Render album art
    if let Some(ref img) = app.artwork {
        let lines = crate::artwork::image_to_halfblocks(img, art_area.width, art_area.height);
        frame.render_widget(Paragraph::new(lines), art_area);
    } else {
        frame.render_widget(
            Paragraph::new("No artwork").dark_gray().alignment(ratatui::layout::Alignment::Center),
            art_area,
        );
    }

    // Track info
    let info = vec![
        Line::from(app.player.track_name.clone().bold().white()),
        Line::from(vec![
            Span::from(app.player.artist.clone()).cyan(),
            Span::from(" - ").dark_gray(),
            Span::from(app.player.album.clone()).dark_gray(),
        ]),
    ];
    frame.render_widget(Paragraph::new(info), info_area);
}
```

**Step 5: Test**

Run: `cargo run`
Expected: When a track is playing, album art renders as colored blocks in the left panel. Track info shows below. Falls back to "No artwork" if not found.

**Step 6: Commit**

```bash
git add src/artwork.rs src/app.rs src/ui.rs Cargo.toml
git commit -m "feat: add half-block album art rendering"
```

---

### Task 6: Enhanced Controls Bar

**Files:**
- Modify: `src/ui.rs`

**Step 1: Improve the bottom bar with shuffle/repeat/volume indicators**

Replace `draw_controls` in `src/ui.rs`:

```rust
fn draw_controls(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [progress_row, status_row] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
    ]).areas(inner);

    // Progress bar
    let state_icon = match app.player.state {
        PlayState::Playing => "▶ ",
        PlayState::Paused => "⏸ ",
        PlayState::Stopped => "⏹ ",
    };
    let elapsed = format_time(app.player.position);
    let total = format_time(app.player.duration);
    let ratio = if app.player.duration > 0.0 {
        (app.player.position / app.player.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .ratio(ratio)
        .label(format!("{state_icon}{elapsed} / {total}"))
        .use_unicode(true);

    frame.render_widget(gauge, progress_row);

    // Status line: shuffle, repeat, volume
    use crate::bridge::RepeatMode;
    let shuffle_str = if app.player.shuffle { "shuffle on" } else { "shuffle off" };
    let repeat_str = match app.player.repeat {
        RepeatMode::Off => "repeat off",
        RepeatMode::One => "repeat one",
        RepeatMode::All => "repeat all",
    };
    let vol_bars = (app.player.volume as usize) / 10;
    let vol_str = format!(
        "vol {}{}",
        "#".repeat(vol_bars),
        "-".repeat(10 - vol_bars)
    );

    let status_line = Line::from(vec![
        Span::from("  ").dark_gray(),
        Span::from(shuffle_str).style(if app.player.shuffle {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        }),
        Span::from("  |  ").dark_gray(),
        Span::from(repeat_str).style(if app.player.repeat != RepeatMode::Off {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        }),
        Span::from("  |  ").dark_gray(),
        Span::from(vol_str).cyan(),
    ]);

    frame.render_widget(Paragraph::new(status_line), status_row);
}
```

Update the bottom bar height from 3 to 4 in `draw()`:

```rust
let [main_area, bottom_bar] = Layout::vertical([
    Constraint::Fill(1),
    Constraint::Length(4),
])
.areas(frame.area());
```

**Step 2: Test**

Run: `cargo run`
Expected: Bottom bar shows progress with time, plus shuffle/repeat/volume indicators. `s` toggles shuffle (turns green), `r` cycles repeat, `+`/`-` adjusts volume.

**Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "feat: enhance controls bar with shuffle, repeat, and volume display"
```

---

### Task 7: Seek Support

**Files:**
- Modify: `src/bridge.rs`
- Modify: `src/main.rs`

**Step 1: Add seek function to bridge**

Add to `src/bridge.rs`:

```rust
pub fn seek_to(position: f64) {
    let script = format!(
        r#"
        var Music = Application("Music");
        if (Music.playerState() === "playing") {{
            Music.playerPosition = {};
        }}
        "#,
        position
    );
    let _ = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", &script])
        .output();
}
```

**Step 2: Add Left/Right arrow key handling for seek**

Add to `handle_key` in `src/main.rs`:

```rust
KeyCode::Left => {
    let new_pos = (app.player.position - 5.0).max(0.0);
    bridge::seek_to(new_pos);
}
KeyCode::Right => {
    let new_pos = (app.player.position + 5.0).min(app.player.duration);
    bridge::seek_to(new_pos);
}
```

**Step 3: Test**

Run: `cargo run`
Expected: Left/Right arrows seek backward/forward 5 seconds. Only works while playing (Apple limitation).

**Step 4: Commit**

```bash
git add src/bridge.rs src/main.rs
git commit -m "feat: add seek support with left/right arrow keys"
```

---

### Task 8: Move Artwork Fetching to Background Thread

The artwork fetch (HTTP request to iTunes + image download) blocks the main thread. Move it to the player polling thread.

**Files:**
- Modify: `src/main.rs`
- Modify: `src/app.rs`

**Step 1: Add artwork variant to AppEvent**

In `src/main.rs`:

```rust
enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Tick,
    PlayerUpdate(PlayerStatus),
    ArtworkLoaded(Option<DynamicImage>),
}
```

**Step 2: Fetch artwork in a spawned thread on track change**

Move artwork fetching out of `App::update_player` and into the event loop. When a track change is detected, spawn a thread:

```rust
AppEvent::PlayerUpdate(status) => {
    let track_changed = status.track_name != app.artwork_track
        && !status.track_name.is_empty();
    app.update_player_status(status);

    if track_changed {
        app.artwork_track = app.player.track_name.clone();
        app.artwork = None; // clear while loading
        let name = app.player.track_name.clone();
        let artist = app.player.artist.clone();
        let tx_art = tx.clone();
        thread::spawn(move || {
            let img = artwork::fetch_artwork_url(&name, &artist)
                .and_then(|url| artwork::download_image(&url));
            let _ = tx_art.send(AppEvent::ArtworkLoaded(img));
        });
    }
}
AppEvent::ArtworkLoaded(img) => {
    app.artwork = img;
}
```

Rename `App::update_player` to `update_player_status` and remove the artwork fetching logic from it -- it should only update the player fields.

**Step 3: Test**

Run: `cargo run`
Expected: Artwork loads without freezing the UI. Brief "No artwork" flash while loading, then artwork appears.

**Step 4: Commit**

```bash
git add src/main.rs src/app.rs
git commit -m "feat: move artwork fetching to background thread"
```

---

### Task 9: Help Bar and Polish

**Files:**
- Modify: `src/ui.rs`

**Step 1: Add a header bar with app title and help hint**

Update `draw()` in `src/ui.rs` to add a top bar:

```rust
pub fn draw(frame: &mut Frame, app: &mut App) {
    let [header, main_area, bottom_bar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(4),
    ])
    .areas(frame.area());

    // Header
    let header_line = Line::from(vec![
        Span::from(" cli-music ").bold().cyan(),
        Span::from("  q:quit  space:play/pause  n/p:next/prev  s:shuffle  r:repeat  /:search").dark_gray(),
    ]);
    frame.render_widget(Paragraph::new(header_line), header);

    let [left_panel, right_panel] = Layout::horizontal([
        Constraint::Percentage(40),
        Constraint::Percentage(60),
    ])
    .areas(main_area);

    draw_now_playing(frame, left_panel, app);
    draw_library(frame, right_panel, app);
    draw_controls(frame, bottom_bar, app);
}
```

**Step 2: Test**

Run: `cargo run`
Expected: Top bar shows key hints. Everything works together.

**Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add header bar with keybinding hints"
```

---

## Summary

| Task | Description | Key files |
|------|-------------|-----------|
| 1 | Project scaffolding | `Cargo.toml`, `src/main.rs` |
| 2 | Music bridge (JXA polling, playback controls) | `src/bridge.rs` |
| 3 | TUI skeleton (event loop, layout, keys) | `src/app.rs`, `src/ui.rs`, `src/main.rs` |
| 4 | Library browser (playlists, tracks, search) | `src/library.rs` |
| 5 | Album art (half-block renderer) | `src/artwork.rs` |
| 6 | Enhanced controls bar | `src/ui.rs` |
| 7 | Seek support | `src/bridge.rs` |
| 8 | Background artwork loading | `src/main.rs` |
| 9 | Header bar and polish | `src/ui.rs` |
