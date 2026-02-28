use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style, Stylize},
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
        Line::from(Span::from(app.player.track_name.clone()).bold().white()),
        Line::from(vec![
            Span::from(app.player.artist.clone()).cyan(),
            Span::from(" - ").dark_gray(),
            Span::from(app.player.album.clone()).dark_gray(),
        ]),
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
        PlayState::Playing => "\u{25b6}",
        PlayState::Paused => "\u{23f8}",
        PlayState::Stopped => "\u{23f9}",
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
