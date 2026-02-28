use crate::bridge::PlayerStatus;
use crate::library::{PlaylistEntry, TrackEntry};
use ratatui::widgets::ListState;

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
            artwork: None,
            artwork_track: String::new(),
        }
    }
}

impl App {
    pub fn update_player(&mut self, status: PlayerStatus) {
        let track_changed = status.track_name != self.artwork_track;
        self.player = status;

        if track_changed && !self.player.track_name.is_empty() {
            self.artwork_track = self.player.track_name.clone();
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

    /// Move selection down in the current list.
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
            LibraryView::Tracks | LibraryView::SearchResults => {
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

    /// Move selection up in the current list.
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
            LibraryView::Tracks | LibraryView::SearchResults => {
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
