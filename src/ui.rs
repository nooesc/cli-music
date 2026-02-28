use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Padding, Paragraph},
    Frame,
};

use crate::app::{App, LibraryView, Panel};
use crate::bridge::{PlayState, RepeatMode};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let width = frame.area().width;
    let height = frame.area().height;

    // Compact mode: hide now-playing panel when too narrow
    let show_now_playing = width >= 60;
    // Tiny mode: simplify controls when very short
    let show_status_row = height >= 10;

    let controls_height = if show_status_row { 4 } else { 3 };

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

    draw_controls(frame, bottom_bar, app, show_status_row);
}

fn draw_header(frame: &mut Frame, area: Rect, _app: &App) {
    let width = area.width as usize;

    let mut spans = vec![
        Span::from(" \u{266b} cli-music ").bold().cyan(),
    ];

    // Only show keybindings if there's room
    let hints = "  q:quit  space:play  n/p:track  ,/.:seek  s:shuf  r:rep  /:search";
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
        LibraryView::SearchResults => format!(
            " Search \u{2014} {} results ",
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
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let available_width = area.width as usize;

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
        LibraryView::Tracks | LibraryView::SearchResults => {
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

                    let duration = format_time(t.duration);

                    // Calculate space for album: total - name - artist - decorators
                    let name_artist_len = t.name.len() + t.artist.len() + 8; // " - " + dur + spaces
                    let show_album = available_width > name_artist_len + 15;

                    let name_style = if is_playing {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let mut spans = vec![
                        prefix,
                        Span::styled(t.name.clone(), name_style),
                        Span::styled("  ", Style::default()),
                        Span::styled(t.artist.clone(), Style::default().fg(Color::Cyan)),
                    ];

                    if show_album {
                        // Truncate album if needed (char-safe)
                        let max_album = 20;
                        let album_display: String = if t.album.chars().count() > max_album {
                            let truncated: String = t.album.chars().take(max_album - 3).collect();
                            format!("{truncated}...")
                        } else {
                            t.album.clone()
                        };
                        spans.push(Span::styled(
                            format!("  {}", album_display),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }

                    spans.push(Span::styled(
                        format!("  {}", duration),
                        Style::default().fg(Color::DarkGray),
                    ));

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

fn draw_controls(frame: &mut Frame, area: Rect, app: &App, show_status_row: bool) {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if !show_status_row {
        // Compact: just the progress bar
        draw_progress(frame, inner, app);
        return;
    }

    let [progress_area, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    draw_progress(frame, progress_area, app);

    // Status line
    let separator = Span::styled(" \u{2502} ", Style::default().fg(Color::DarkGray));

    let shuffle_icon = if app.player.shuffle { "\u{2921} " } else { "" };
    let shuffle_span = if app.player.shuffle {
        Span::styled(format!("{shuffle_icon}shuffle"), Style::default().fg(Color::Green))
    } else {
        Span::styled("shuffle", Style::default().fg(Color::DarkGray))
    };

    let repeat_span = match app.player.repeat {
        RepeatMode::Off => Span::styled("repeat", Style::default().fg(Color::DarkGray)),
        RepeatMode::One => Span::styled("\u{21bb} one", Style::default().fg(Color::Green)),
        RepeatMode::All => Span::styled("\u{21bb} all", Style::default().fg(Color::Green)),
    };

    let vol = app.player.volume.clamp(0, 100);
    let vol_level = ((vol as f64 / 100.0) * 10.0).round() as usize;
    let vol_bar: String = (0..10)
        .map(|i| if i < vol_level { '\u{2501}' } else { '\u{2500}' })
        .collect();
    let vol_span = Span::styled(
        format!("\u{1f50a} {vol_bar} {vol}%"),
        Style::default().fg(Color::Cyan),
    );

    let status_line = Line::from(vec![
        Span::from(" "),
        shuffle_span,
        separator.clone(),
        repeat_span,
        separator,
        vol_span,
    ]);

    frame.render_widget(Paragraph::new(status_line), status_area);
}

fn draw_progress(frame: &mut Frame, area: Rect, app: &App) {
    let state_icon = match app.player.state {
        PlayState::Playing => "\u{25b6}",
        PlayState::Paused => "\u{2016}",
        PlayState::Stopped => "\u{25a0}",
    };

    let elapsed = format_time(app.player.position);
    let total = format_time(app.player.duration);
    let ratio = if app.player.duration > 0.0 {
        (app.player.position / app.player.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let label = format!(" {state_icon}  {elapsed} / {total}");

    let gauge = Gauge::default()
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
