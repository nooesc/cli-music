use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, LibraryView, Panel};
use crate::bridge::{PlayState, RepeatMode};

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

    // Split inner area: artwork on top, track info (3 rows) at bottom
    let info_height = 3u16;
    let [art_area, info_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(info_height),
    ])
    .areas(inner);

    // Render artwork
    if let Some(ref img) = app.artwork {
        let lines = crate::artwork::image_to_halfblocks(img, art_area.width, art_area.height);
        frame.render_widget(Paragraph::new(lines), art_area);
    } else {
        let no_art = Paragraph::new("No artwork")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(no_art, art_area);
    }

    // Render track info
    let info_text = vec![
        Line::from(Span::from(app.player.track_name.clone()).bold().white()),
        Line::from(vec![
            Span::from(app.player.artist.clone()).cyan(),
            Span::from(" - ").dark_gray(),
            Span::from(app.player.album.clone()).dark_gray(),
        ]),
    ];

    frame.render_widget(Paragraph::new(info_text), info_area);
}

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

    if app.loading {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("Loading...")
                .style(Style::default().fg(Color::Yellow))
                .alignment(ratatui::layout::Alignment::Center),
            inner,
        );
        return;
    }

    // If search mode is active, split the library area to show a search bar at the bottom.
    if app.search_mode {
        let [list_area, search_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(block.inner(area));

        frame.render_widget(block, area);

        render_library_list(frame, list_area, app);

        let search_line = Line::from(Span::styled(
            format!("/{}", app.search_query),
            Style::default().fg(Color::Yellow),
        ));
        frame.render_widget(Paragraph::new(search_line), search_area);
    } else {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        render_library_list(frame, inner, app);
    }
}

fn render_library_list(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    let highlight_style = Style::default()
        .bg(Color::Cyan)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    match app.view {
        LibraryView::Playlists => {
            let items: Vec<ListItem> = app
                .playlists
                .iter()
                .map(|p| ListItem::new(Line::from(p.name.clone())))
                .collect();

            let list = List::new(items)
                .highlight_style(highlight_style)
                .highlight_symbol("\u{25b6} ");

            frame.render_stateful_widget(list, area, &mut app.playlist_state);
        }
        LibraryView::Tracks | LibraryView::SearchResults => {
            let items: Vec<ListItem> = app
                .tracks
                .iter()
                .map(|t| {
                    ListItem::new(Line::from(vec![
                        Span::styled(t.name.clone(), Style::default().fg(Color::White)),
                        Span::styled(" - ", Style::default().fg(Color::DarkGray)),
                        Span::styled(t.artist.clone(), Style::default().fg(Color::Cyan)),
                    ]))
                })
                .collect();

            let list = List::new(items)
                .highlight_style(highlight_style)
                .highlight_symbol("\u{25b6} ");

            frame.render_stateful_widget(list, area, &mut app.track_state);
        }
    }
}

fn draw_controls(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [progress_area, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    // Row 1: Progress bar (no block border)
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
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .ratio(ratio)
        .label(label)
        .use_unicode(true);

    frame.render_widget(gauge, progress_area);

    // Row 2: Status line (shuffle | repeat | volume)
    let separator = Span::styled("  |  ", Style::default().fg(Color::DarkGray));

    let shuffle_span = if app.player.shuffle {
        Span::styled("shuffle on", Style::default().fg(Color::Green))
    } else {
        Span::styled("shuffle off", Style::default().fg(Color::DarkGray))
    };

    let repeat_span = match app.player.repeat {
        RepeatMode::Off => Span::styled("repeat off", Style::default().fg(Color::DarkGray)),
        RepeatMode::One => Span::styled("repeat one", Style::default().fg(Color::Green)),
        RepeatMode::All => Span::styled("repeat all", Style::default().fg(Color::Green)),
    };

    let vol_level = ((app.player.volume.clamp(0, 100) as f64 / 100.0) * 10.0).round() as usize;
    let vol_filled = "#".repeat(vol_level);
    let vol_empty = "-".repeat(10 - vol_level);
    let vol_span = Span::styled(
        format!("vol {vol_filled}{vol_empty}"),
        Style::default().fg(Color::Cyan),
    );

    let status_line = Line::from(vec![
        shuffle_span,
        separator.clone(),
        repeat_span,
        separator,
        vol_span,
    ]);

    frame.render_widget(Paragraph::new(status_line), status_area);
}

fn format_time(seconds: f64) -> String {
    let s = seconds as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
