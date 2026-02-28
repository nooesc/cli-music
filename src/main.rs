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
    thread::spawn(move || loop {
        let status = bridge::poll_player_status();
        let _ = tx_player.send(AppEvent::PlayerUpdate(status));
        thread::sleep(Duration::from_millis(500));
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
            let _ = bridge::set_volume((app.player.volume + 5).min(100));
        }
        KeyCode::Char('-') => {
            let _ = bridge::set_volume((app.player.volume - 5).max(0));
        }
        KeyCode::Char('s') => {
            let _ = bridge::toggle_shuffle();
        }
        KeyCode::Char('r') => {
            let _ = bridge::cycle_repeat();
        }
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
