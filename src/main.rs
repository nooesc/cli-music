mod app;
mod artwork;
mod bridge;
mod library;
mod ui;

use app::{App, LibraryView, Panel};
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
    TracksLoaded(LibraryView, String, Vec<library::TrackEntry>),
    ArtworkLoaded(String, Option<image::DynamicImage>),
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

    // Load playlists on startup
    app.playlists = library::fetch_playlists().unwrap_or_default();
    if !app.playlists.is_empty() {
        app.playlist_state.select(Some(0));
    }

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
    thread::spawn(move || loop {
        let status = bridge::poll_player_status();
        let _ = tx_player.send(AppEvent::PlayerUpdate(status));
        thread::sleep(Duration::from_millis(500));
    });

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        match rx.recv()? {
            AppEvent::Key(key) => handle_key(&mut app, key, &tx),
            AppEvent::Tick => {}
            AppEvent::PlayerUpdate(status) => {
                let track_changed =
                    status.track_name != app.artwork_track && !status.track_name.is_empty();

                if track_changed {
                    app.artwork_track = status.track_name.clone();
                    app.artwork = None;

                    let track_name = status.track_name.clone();
                    let artist = status.artist.clone();
                    let tx_art = tx.clone();
                    thread::spawn(move || {
                        let img = artwork::fetch_artwork_url(&track_name, &artist)
                            .and_then(|url| artwork::download_image(&url));
                        let _ = tx_art.send(AppEvent::ArtworkLoaded(track_name, img));
                    });
                }

                app.update_player_status(status);
            }
            AppEvent::TracksLoaded(view, cache_key, tracks) => {
                app.loading = false;
                if !cache_key.is_empty() {
                    app.track_cache.insert(cache_key, tracks.clone());
                }
                app.tracks = tracks;
                app.track_state.select(if app.tracks.is_empty() {
                    None
                } else {
                    Some(0)
                });
                app.view = view;
            }
            AppEvent::ArtworkLoaded(track, img) => {
                if track == app.artwork_track {
                    app.artwork = img;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent, tx: &mpsc::Sender<AppEvent>) {
    // Search mode intercepts all keys
    if app.search_mode {
        match key.code {
            KeyCode::Enter => {
                app.search_mode = false;
                let query = app.search_query.clone();
                if !query.is_empty() {
                    app.loading = true;
                    let tx_bg = tx.clone();
                    std::thread::spawn(move || {
                        let tracks = library::search_library(&query).unwrap_or_default();
                        let _ = tx_bg.send(AppEvent::TracksLoaded(LibraryView::SearchResults, String::new(), tracks));
                    });
                }
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

    // Library navigation keys (only when Library panel is active)
    if app.active_panel == Panel::Library {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                app.select_next();
                return;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.select_previous();
                return;
            }
            // Right arrow / Enter / l: drill into playlist or play track
            KeyCode::Right | KeyCode::Enter | KeyCode::Char('l') => {
                match app.view {
                    LibraryView::Playlists => {
                        if let Some(playlist) = app.selected_playlist() {
                            let name = playlist.name.clone();
                            if let Some(cached) = app.track_cache.get(&name) {
                                app.tracks = cached.clone();
                                app.track_state.select(if app.tracks.is_empty() {
                                    None
                                } else {
                                    Some(0)
                                });
                                app.view = LibraryView::Tracks;
                            } else {
                                app.loading = true;
                                let tx_bg = tx.clone();
                                std::thread::spawn(move || {
                                    let tracks = library::fetch_playlist_tracks(&name).unwrap_or_default();
                                    let _ = tx_bg.send(AppEvent::TracksLoaded(LibraryView::Tracks, name, tracks));
                                });
                            }
                        }
                    }
                    LibraryView::Tracks | LibraryView::SearchResults => {
                        if let Some(track) = app.selected_track() {
                            library::play_track_by_id(track.id);
                        }
                    }
                }
                return;
            }
            // Left arrow / h / Esc: go back to playlists
            KeyCode::Left | KeyCode::Esc | KeyCode::Char('h') => {
                match app.view {
                    LibraryView::Tracks | LibraryView::SearchResults => {
                        app.view = LibraryView::Playlists;
                        app.tracks.clear();
                        app.track_state.select(None);
                    }
                    LibraryView::Playlists => {}
                }
                return;
            }
            KeyCode::Char('/') => {
                app.search_mode = true;
                app.search_query.clear();
                return;
            }
            _ => {}
        }
    }

    // Global keys
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char(' ') => {
            let _ = bridge::toggle_playback();
        }
        KeyCode::Char('n') => {
            let _ = bridge::next_track();
        }
        KeyCode::Char('p') => {
            let _ = bridge::previous_track();
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let _ = bridge::set_volume(app.player.volume.saturating_add(5).min(100));
        }
        KeyCode::Char('-') => {
            let _ = bridge::set_volume(app.player.volume.saturating_sub(5).max(0));
        }
        KeyCode::Char('s') => {
            let _ = bridge::toggle_shuffle();
        }
        KeyCode::Char('r') => {
            let _ = bridge::cycle_repeat();
        }
        KeyCode::Left | KeyCode::Char('<') | KeyCode::Char(',') => {
            let new_pos = (app.player.position - 5.0).max(0.0);
            bridge::seek_to(new_pos);
        }
        KeyCode::Right | KeyCode::Char('>') | KeyCode::Char('.') => {
            let new_pos = (app.player.position + 5.0).min(app.player.duration);
            bridge::seek_to(new_pos);
        }
        KeyCode::Char('1') => app.active_panel = Panel::NowPlaying,
        KeyCode::Char('2') => app.active_panel = Panel::Library,
        KeyCode::Tab => {
            app.active_panel = match app.active_panel {
                Panel::NowPlaying => Panel::Library,
                Panel::Library => Panel::NowPlaying,
            };
        }
        _ => {}
    }
}
