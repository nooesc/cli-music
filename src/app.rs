use crate::bridge::PlayerStatus;
use crate::library::{PlaylistEntry, TrackEntry};
use ratatui::widgets::ListState;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::Instant;

pub struct App {
    pub should_quit: bool,
    pub player: PlayerStatus,
    pub active_panel: Panel,
    // Library browser state
    pub playlists: Vec<PlaylistEntry>,
    pub playlist_state: ListState,
    pub tracks: Vec<TrackEntry>,
    pub track_state: ListState,
    pub view: LibraryView,
    pub search_mode: bool,
    pub search_query: String,
    pub loading: bool,
    pub track_cache: HashMap<String, Vec<TrackEntry>>,
    // Snapshot of full list before search filtering
    pub pre_search_playlists: Vec<PlaylistEntry>,
    pub pre_search_tracks: Vec<TrackEntry>,
    // Artwork
    pub artwork: Option<image::DynamicImage>,
    pub artwork_track: String,
    // Mini-player mode: hide library, show only now playing
    pub mini_player: bool,
    // Temporary notification overlay (message, when it was set)
    pub notification: Option<(String, Instant)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Panel {
    NowPlaying,
    Library,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LibraryView {
    Playlists,
    Tracks,
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
            track_cache: HashMap::new(),
            pre_search_playlists: Vec::new(),
            pre_search_tracks: Vec::new(),
            artwork: None,
            artwork_track: String::new(),
            mini_player: false,
            notification: None,
        }
    }
}

impl App {
    pub fn update_player_status(&mut self, status: PlayerStatus) {
        self.player = status;
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.notification = Some((msg.into(), Instant::now()));
    }

    pub fn clear_expired_notification(&mut self) {
        if let Some((_, when)) = &self.notification {
            if when.elapsed() >= std::time::Duration::from_secs(2) {
                self.notification = None;
            }
        }
    }

    /// Move selection down by `n` in the current list.
    pub fn select_next_by(&mut self, n: usize) {
        match self.view {
            LibraryView::Playlists => {
                let len = self.playlists.len();
                if len == 0 {
                    return;
                }
                let i = self.playlist_state.selected().map_or(0, |i| {
                    if i + n >= len { len - 1 } else { i + n }
                });
                self.playlist_state.select(Some(i));
            }
            LibraryView::Tracks => {
                let len = self.tracks.len();
                if len == 0 {
                    return;
                }
                let i = self.track_state.selected().map_or(0, |i| {
                    if i + n >= len { len - 1 } else { i + n }
                });
                self.track_state.select(Some(i));
            }
        }
    }

    /// Move selection down by 1, wrapping.
    pub fn select_next(&mut self) {
        match self.view {
            LibraryView::Playlists => {
                let len = self.playlists.len();
                if len == 0 {
                    return;
                }
                let i = self.playlist_state.selected().map_or(0, |i| {
                    if i + 1 >= len { 0 } else { i + 1 }
                });
                self.playlist_state.select(Some(i));
            }
            LibraryView::Tracks => {
                let len = self.tracks.len();
                if len == 0 {
                    return;
                }
                let i = self.track_state.selected().map_or(0, |i| {
                    if i + 1 >= len { 0 } else { i + 1 }
                });
                self.track_state.select(Some(i));
            }
        }
    }

    /// Move selection up by `n` in the current list.
    pub fn select_previous_by(&mut self, n: usize) {
        match self.view {
            LibraryView::Playlists => {
                let len = self.playlists.len();
                if len == 0 {
                    return;
                }
                let i = self.playlist_state.selected().map_or(0, |i| {
                    i.saturating_sub(n)
                });
                self.playlist_state.select(Some(i));
            }
            LibraryView::Tracks => {
                let len = self.tracks.len();
                if len == 0 {
                    return;
                }
                let i = self.track_state.selected().map_or(0, |i| {
                    i.saturating_sub(n)
                });
                self.track_state.select(Some(i));
            }
        }
    }

    /// Move selection up by 1, wrapping.
    pub fn select_previous(&mut self) {
        match self.view {
            LibraryView::Playlists => {
                let len = self.playlists.len();
                if len == 0 {
                    return;
                }
                let i = self.playlist_state.selected().map_or(0, |i| {
                    if i == 0 { len - 1 } else { i - 1 }
                });
                self.playlist_state.select(Some(i));
            }
            LibraryView::Tracks => {
                let len = self.tracks.len();
                if len == 0 {
                    return;
                }
                let i = self.track_state.selected().map_or(0, |i| {
                    if i == 0 { len - 1 } else { i - 1 }
                });
                self.track_state.select(Some(i));
            }
        }
    }

    /// Enter search/filter mode: snapshot the current list.
    pub fn enter_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        match self.view {
            LibraryView::Playlists => {
                self.pre_search_playlists = self.playlists.clone();
            }
            LibraryView::Tracks => {
                self.pre_search_tracks = self.tracks.clone();
            }
        }
    }

    /// Apply the current search query as a live filter.
    pub fn apply_search_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        match self.view {
            LibraryView::Playlists => {
                self.playlists = if query.is_empty() {
                    self.pre_search_playlists.clone()
                } else {
                    self.pre_search_playlists
                        .iter()
                        .filter(|p| p.name.to_lowercase().contains(&query))
                        .cloned()
                        .collect()
                };
                self.playlist_state.select(if self.playlists.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
            LibraryView::Tracks => {
                self.tracks = if query.is_empty() {
                    self.pre_search_tracks.clone()
                } else {
                    self.pre_search_tracks
                        .iter()
                        .filter(|t| {
                            t.name.to_lowercase().contains(&query)
                                || t.artist.to_lowercase().contains(&query)
                        })
                        .cloned()
                        .collect()
                };
                self.track_state.select(if self.tracks.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
        }
    }

    /// Exit search, keeping the filtered results.
    pub fn confirm_search(&mut self) {
        self.search_mode = false;
    }

    /// Cancel search, restoring the full list.
    pub fn cancel_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        match self.view {
            LibraryView::Playlists => {
                self.playlists = std::mem::take(&mut self.pre_search_playlists);
                self.playlist_state.select(if self.playlists.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
            LibraryView::Tracks => {
                self.tracks = std::mem::take(&mut self.pre_search_tracks);
                self.track_state.select(if self.tracks.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
        }
    }

    /// Get a reference to the currently selected playlist, if any.
    pub fn selected_playlist(&self) -> Option<&PlaylistEntry> {
        self.playlist_state
            .selected()
            .and_then(|i| self.playlists.get(i))
    }

    /// Get a reference to the currently selected track, if any.
    pub fn selected_track(&self) -> Option<&TrackEntry> {
        self.track_state
            .selected()
            .and_then(|i| self.tracks.get(i))
    }

}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedState {
    pub active_panel: Panel,
    pub mini_player: bool,
    pub library_view: LibraryView,
    pub playlist_index: Option<usize>,
    pub track_index: Option<usize>,
    pub open_playlist_name: Option<String>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            active_panel: Panel::Library,
            mini_player: false,
            library_view: LibraryView::Playlists,
            playlist_index: None,
            track_index: None,
            open_playlist_name: None,
        }
    }
}

impl PersistedState {
    /// Extract persistable state from the current App.
    pub fn from_app(app: &App) -> Self {
        let open_playlist_name = if app.view == LibraryView::Tracks {
            app.playlist_state
                .selected()
                .and_then(|i| app.playlists.get(i))
                .map(|p| p.name.clone())
        } else {
            None
        };

        Self {
            active_panel: app.active_panel.clone(),
            mini_player: app.mini_player,
            library_view: app.view.clone(),
            playlist_index: app.playlist_state.selected(),
            track_index: app.track_state.selected(),
            open_playlist_name,
        }
    }

    /// Apply persisted state onto an App that already has playlists loaded.
    pub fn apply(self, app: &mut App) {
        app.active_panel = self.active_panel;
        app.mini_player = self.mini_player;
        app.view = LibraryView::Playlists; // explicit default; overridden below if tracks restore succeeds

        // Restore playlist selection (clamped to actual count)
        if let Some(idx) = self.playlist_index {
            if idx < app.playlists.len() {
                app.playlist_state.select(Some(idx));
            }
        }

        // If we were in Tracks view and have a playlist name, try to reload tracks
        if self.library_view == LibraryView::Tracks {
            if let Some(ref name) = self.open_playlist_name {
                if let Some(pos) = app.playlists.iter().position(|p| p.name == *name) {
                    app.playlist_state.select(Some(pos));
                    if let Ok(tracks) = crate::library::fetch_playlist_tracks(name) {
                        app.track_cache.insert(name.clone(), tracks.clone());
                        app.tracks = tracks;
                        let track_idx = self.track_index
                            .filter(|&i| i < app.tracks.len())
                            .or(if app.tracks.is_empty() { None } else { Some(0) });
                        app.track_state.select(track_idx);
                        app.view = LibraryView::Tracks;
                        return;
                    }
                }
            }
        }
    }

    /// State file path: ~/.config/cli-music/state.json
    pub fn path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("cli-music").join("state.json"))
    }

    /// Load from disk. Returns None on any failure.
    pub fn load() -> Option<Self> {
        let path = Self::path()?;
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Save to disk. Silently ignores errors.
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
