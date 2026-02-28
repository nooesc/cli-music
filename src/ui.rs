use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Padding, Paragraph},
    Frame,
};

use crate::app::{App, LibraryView, Panel};
use crate::bridge::{PlayState, RepeatMode};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let width = frame.area().width;
    // Compact mode: hide now-playing panel when too narrow
    let show_now_playing = width >= 60;
    let controls_height = 1;

    let [header, main_area, bottom_bar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(controls_height),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);

    if show_now_playing {
        // Responsive split: narrower left panel on smaller terminals
        let left_pct = if width >= 120 { 35 } else if width >= 80 { 40 } else { 45 };
        let [left_panel, right_panel] = Layout::horizontal([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(100 - left_pct),
        ])
        .areas(main_area);

        draw_now_playing(frame, left_panel, app);
        draw_library(frame, right_panel, app);
    } else {
        // Too narrow: full-width library, no artwork
        draw_library(frame, main_area, app);
    }

    draw_controls(frame, bottom_bar, app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;

    let mut spans = vec![
        Span::from(" \u{266b} cli-music ").bold().cyan(),
    ];

    // Only show keybindings if there's room
    let play_hint = match app.player.state {
        PlayState::Playing => "space:pause",
        _ => "space:play",
    };
    let hints = format!("  {play_hint}  S-\u{2190}/\u{2192}:track  m:mode  s:search  f:save");
    if width > 50 {
        spans.push(Span::from(hints).dark_gray());
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_now_playing(frame: &mut Frame, area: Rect, app: &App) {
    let border_style = if app.active_panel == Panel::NowPlaying {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Now Playing ")
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.player.track_name.is_empty() {
        let center_y = inner.y + inner.height / 2;
        let msg_area = Rect { y: center_y, height: 1, ..inner };
        frame.render_widget(
            Paragraph::new("Nothing playing")
                .dark_gray()
                .alignment(Alignment::Center),
            msg_area,
        );
        return;
    }

    // Decide layout based on available height
    let show_artwork = inner.height >= 10;
    let info_height = 3u16;

    if show_artwork {
        let [art_area, info_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(info_height),
        ])
        .areas(inner);

        // Render artwork (centered if narrower than area)
        if let Some(ref img) = app.artwork {
            // Keep artwork square-ish: width = height * 2 (half-blocks are ~2:1)
            let art_w = art_area.width.min(art_area.height * 2);
            let art_x = art_area.x + (art_area.width.saturating_sub(art_w)) / 2;
            let centered_art = Rect {
                x: art_x,
                width: art_w,
                ..art_area
            };
            let lines = crate::artwork::image_to_halfblocks(
                img,
                centered_art.width,
                centered_art.height,
            );
            frame.render_widget(Paragraph::new(lines), centered_art);
        } else {
            let center_y = art_area.y + art_area.height / 2;
            let msg_area = Rect { y: center_y, height: 1, ..art_area };
            frame.render_widget(
                Paragraph::new("\u{1f3b5}")
                    .dark_gray()
                    .alignment(Alignment::Center),
                msg_area,
            );
        }

        render_track_info(frame, info_area, app);
    } else {
        // No room for artwork, just show track info
        render_track_info(frame, inner, app);
    }
}

fn render_track_info(frame: &mut Frame, area: Rect, app: &App) {
    let elapsed = format_time(app.player.position);
    let total = format_time(app.player.duration);

    let info_text = vec![
        Line::from(Span::from(app.player.track_name.clone()).bold().white()),
        Line::from(vec![
            Span::from(app.player.artist.clone()).cyan(),
        ]),
        Line::from(vec![
            Span::from(app.player.album.clone()).dark_gray(),
            Span::from(format!("  {elapsed} / {total}")).dark_gray(),
        ]),
    ];

    frame.render_widget(Paragraph::new(info_text), area);
}

fn draw_library(frame: &mut Frame, area: Rect, app: &mut App) {
    let border_style = if app.active_panel == Panel::Library {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Build title with track count
    let title = match app.view {
        LibraryView::Playlists => format!(
            " Playlists ({}) ",
            app.playlists.len()
        ),
        LibraryView::Tracks => format!(
            " {} \u{2014} {} tracks ",
            app.tracks.first().map(|t| t.album.as_str()).unwrap_or("Tracks"),
            app.tracks.len()
        ),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::from(title));

    if app.loading {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let center_y = inner.y + inner.height / 2;
        let msg_area = Rect { y: center_y, height: 1, ..inner };
        frame.render_widget(
            Paragraph::new("Loading...")
                .style(Style::default().fg(Color::Yellow))
                .alignment(Alignment::Center),
            msg_area,
        );
        return;
    }

    if app.search_mode {
        let [list_area, search_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(block.inner(area));

        frame.render_widget(block, area);
        render_library_list(frame, list_area, app);

        let search_line = Line::from(vec![
            Span::from(" / ").yellow().bold(),
            Span::from(app.search_query.clone()).white(),
            Span::from("\u{2588}").yellow(), // blinking cursor
        ]);
        frame.render_widget(Paragraph::new(search_line), search_area);
    } else {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        render_library_list(frame, inner, app);
    }
}

fn render_library_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let highlight_style = Style::default()
        .bg(Color::Cyan)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    match app.view {
        LibraryView::Playlists => {
            let items: Vec<ListItem> = app
                .playlists
                .iter()
                .map(|p| {
                    ListItem::new(Line::from(vec![
                        Span::from(p.name.clone()),
                        Span::from(" \u{203a}").dark_gray(), // › arrow hint
                    ]))
                })
                .collect();

            let list = List::new(items)
                .highlight_style(highlight_style)
                .highlight_symbol(" \u{25b6} ");

            frame.render_stateful_widget(list, area, &mut app.playlist_state);
        }
        LibraryView::Tracks => {
            let items: Vec<ListItem> = app
                .tracks
                .iter()
                .map(|t| {
                    let is_playing = !app.player.track_name.is_empty()
                        && t.name == app.player.track_name
                        && t.artist == app.player.artist;

                    let prefix = if is_playing {
                        Span::styled("\u{266b} ", Style::default().fg(Color::Green))
                    } else {
                        Span::from("  ")
                    };

                    let name_style = if is_playing {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let spans = vec![
                        prefix,
                        Span::styled(t.name.clone(), name_style),
                        Span::styled("  ", Style::default()),
                        Span::styled(t.artist.clone(), Style::default().fg(Color::Cyan)),
                    ];

                    ListItem::new(Line::from(spans))
                })
                .collect();

            let list = List::new(items)
                .highlight_style(highlight_style)
                .highlight_symbol(" \u{25b6} ");

            frame.render_stateful_widget(list, area, &mut app.track_state);
        }
    }
}

fn draw_controls(frame: &mut Frame, area: Rect, app: &App) {
    let inner = area;

    let state_icon = match app.player.state {
        PlayState::Playing => "\u{25b6}",
        PlayState::Paused => "\u{2016}",
        PlayState::Stopped => "\u{25a0}",
    };

    let elapsed = format_time(app.player.position);
    let total = format_time(app.player.duration);

    let mode = if app.player.shuffle {
        "\u{2921} shuffle"
    } else {
        match app.player.repeat {
            RepeatMode::All => "\u{21bb} repeat all",
            RepeatMode::One => "\u{21bb} repeat one",
            RepeatMode::Off => "normal",
        }
    };

    let vol = app.player.volume.clamp(0, 100);

    let left = format!(" {state_icon}  {mode}  \u{2502}  vol {vol}%");
    let right = format!("{elapsed} / {total} ");
    let w = inner.width as usize;
    let pad = w.saturating_sub(left.chars().count() + right.chars().count());
    let full_text = format!("{left}{:pad$}{right}", "");

    let ratio = if app.player.duration > 0.0 {
        (app.player.position / app.player.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = ((w as f64) * ratio).round() as usize;

    // Split text into filled (progress color) and unfilled portions
    let chars: Vec<char> = full_text.chars().collect();
    let filled_str: String = chars[..filled.min(chars.len())].iter().collect();
    let unfilled_str: String = chars[filled.min(chars.len())..].iter().collect();

    let line = Line::from(vec![
        Span::styled(filled_str, Style::default().bg(Color::Cyan).fg(Color::White).bold()),
        Span::styled(unfilled_str, Style::default().fg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(line), inner);
}

fn format_time(seconds: f64) -> String {
    let s = seconds as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
