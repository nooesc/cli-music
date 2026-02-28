use crate::bridge::PlayerStatus;
use crate::library::{PlaylistEntry, TrackEntry};
use ratatui::widgets::ListState;
use std::collections::HashMap;

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
        }
    }
}

impl App {
    pub fn update_player_status(&mut self, status: PlayerStatus) {
        self.player = status;
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
