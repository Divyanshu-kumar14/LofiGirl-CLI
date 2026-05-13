use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, AppState, Screen};

// ─── Colors ─────────────────────────────────────────────

fn fg(c: Color) -> Style {
    Style::default().fg(c)
}


fn magenta_bold() -> Style {
    Style::default()
        .fg(Color::LightMagenta)
        .add_modifier(Modifier::BOLD)
}

fn cyan_bold() -> Style {
    Style::default()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::BOLD)
}

fn green_bold() -> Style {
    Style::default()
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD)
}


// ─── Main Render ──────────────────────────────────────────

pub fn render(frame: &mut Frame, app: &App) {
    if app.show_splash && !app.splash_done() {
        render_splash(frame, app);
        return;
    }

    if app.help_visible {
        render_help_overlay(frame, app);
        return;
    }

    match app.screen {
        Screen::NowPlaying => render_now_playing(frame, app),
        Screen::StationList => render_station_list(frame, app),
        Screen::Help => render_help_overlay(frame, app),
    }

    if app.show_add_station {
        render_add_station_popup(frame, app);
    } else if app.show_timer_prompt {
        render_timer_prompt(frame, app);
    }
}

// ─── Splash Screen ────────────────────────────────────────

fn render_splash(frame: &mut Frame, _app: &App) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled("    ", fg(Color::LightMagenta))),
        Line::from(Span::styled("        ▗▄▄▄▄▄▄▄▄▄▄▖", fg(Color::LightMagenta))),
        Line::from(Span::styled("       ▗█◈◈◈◈◈◈◈◈◈◈█▖", fg(Color::LightMagenta))),
        Line::from(Span::styled("      ▗█◈◈◈◈◈◈◈◈◈◈◈◈█▖", fg(Color::LightMagenta))),
        Line::from(Span::styled("      █◈ L O F I ◈ G I R L ◈█", fg(Color::LightMagenta).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("      ▝█◈◈◈◈◈◈◈◈◈◈◈◈█▘", fg(Color::LightMagenta))),
        Line::from(Span::styled("       ▝█◈◈◈◈◈◈◈◈◈◈█▘", fg(Color::LightMagenta))),
        Line::from(Span::styled("        ▝◈◈◈◈◈◈◈◈◈◘▘", fg(Color::LightMagenta))),
        Line::from(Span::styled("          ▝◈◈◈◈◈◘▘", fg(Color::LightMagenta))),
        Line::from(""),
        Line::from(Span::styled("           beats to relax/study to", fg(Color::DarkGray))),
        Line::from(""),
        Line::from(Span::styled("            v1.0", fg(Color::Gray))),
    ];

    let splash = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(splash, area);
}

// ─── Now Playing Screen ─────────────────────────────────

fn render_now_playing(frame: &mut Frame, app: &App) {
    let size = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(1), // separator
            Constraint::Min(12),   // center info
            Constraint::Length(7), // metadata + bars
            Constraint::Length(3), // volume bar
            Constraint::Length(2), // status / timer
        ])
        .split(size);

    render_np_header(frame, app, chunks[0]);

    // Separator
    let sep = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(fg(Color::DarkGray));
    frame.render_widget(sep, chunks[1]);

    render_center_info(frame, app, chunks[2]);
    render_metadata_and_bars(frame, app, chunks[3]);
    render_volume_bar(frame, app, chunks[4]);
    render_status_and_footer(frame, app, chunks[5]);
}

fn render_np_header(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let title_lines = vec![
        Line::from(Span::styled("Lofi Girl", magenta_bold())),
        Line::from(Span::styled(" ♫ beats to relax/study to", fg(Color::LightYellow))),
        Line::from(Span::styled(&app.play_time_str, fg(Color::Gray))),
    ];
    frame.render_widget(
        Paragraph::new(title_lines).alignment(Alignment::Left),
        chunks[0],
    );

    let state_text = match &app.state {
        AppState::Playing => "Playing",
        AppState::Paused => "Paused",
        AppState::Buffering => &app.status_msg,
        AppState::Error(e) => e.as_str(),
    };
    let state_color = match &app.state {
        AppState::Playing => Color::LightGreen,
        AppState::Paused => Color::Yellow,
        AppState::Buffering => Color::LightBlue,
        AppState::Error(_) => Color::Red,
    };

    let status_lines = vec![
        Line::from(Span::styled("[Live Stream]", fg(Color::LightRed).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(format!("● {}", state_text), fg(state_color))),
    ];
    frame.render_widget(
        Paragraph::new(status_lines).alignment(Alignment::Right),
        chunks[1],
    );
}

fn render_center_info(frame: &mut Frame, app: &App, area: Rect) {
    let station_name = app
        .current_station()
        .map(|s| s.name.as_str())
        .unwrap_or("Select a Station");

    let category = app
        .current_station()
        .map(|s| s.category.to_string());
    let category_str = category.as_deref().unwrap_or("");

    let mut lines = Vec::new();

    // Center title vertically
    let content_lines = 5;
    let top_pad = area.height.saturating_sub(content_lines) / 2;
    for _ in 0..top_pad {
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled("L O F I   G I R L", magenta_bold())));
    lines.push(Line::from(Span::styled(
        station_name,
        cyan_bold(),
    )));
    lines.push(Line::from(Span::styled(category_str, fg(Color::DarkGray))));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("━━━━━━━━━━━━━━━━━━━━━", fg(Color::DarkGray))));

    let para = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(para, area);
}

fn render_metadata_and_bars(frame: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(4)])
        .split(area);

    // Track title
    let title = if let Some(meta) = &app.metadata {
        meta.title.clone().unwrap_or_else(|| "Loading track info...".to_string())
    } else {
        "Waiting for stream...".to_string()
    };

    let title_widget = Paragraph::new(Line::from(vec![
        Span::styled("♪ ", fg(Color::LightYellow)),
        Span::styled(truncate_str(&title, area.width as usize - 3), fg(Color::LightCyan)),
    ]))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(title_widget, layout[0]);

    // Equalizer bars
    render_equalizer(frame, app, layout[1]);
}

fn render_equalizer(frame: &mut Frame, app: &App, area: Rect) {
        let bars = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let num_bars = (area.width as usize).saturating_sub(4) / 2;
    if num_bars == 0 {
        return;
    }

    let is_playing = matches!(&app.state, AppState::Playing);
    let is_buffering = matches!(&app.state, AppState::Buffering);

    let bar_str: String = if !is_playing && !is_buffering {
        vec!["▁"; num_bars].join(" ")
    } else {
        let elapsed = app.splash_start.elapsed().as_secs_f64();
        (0..num_bars)
            .map(|i| {
                let freq = if is_buffering { 1.5 } else { 3.0 };
                let amp = if is_buffering { 1 } else { 7 };
                let h = (((i as f64 + elapsed * freq * 0.5).sin() + 1.0) * (amp as f64 / 2.0)) as usize % bars.len();
                bars[h.min(bars.len() - 1)]
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    let color = match &app.state {
        AppState::Playing => Color::LightGreen,
        AppState::Paused => Color::DarkGray,
        AppState::Buffering => Color::LightCyan,
        AppState::Error(_) => Color::Red,
    };

    let eq = Paragraph::new(Span::styled(bar_str, fg(color))).alignment(Alignment::Center);
    frame.render_widget(eq, area);
}

fn render_volume_bar(frame: &mut Frame, app: &App, area: Rect) {
    let blocks = (app.volume as usize * 40) / 100;
    let full = "████████████████████████████████████████";
    let empty = "░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░";
    let bar_full = &full[..blocks * "█".len()];
    let bar_empty = &empty[..(40 - blocks) * "░".len()];

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("VOL ", fg(Color::LightYellow)),
            Span::styled(bar_full, fg(Color::LightGreen)),
            Span::styled(bar_empty, fg(Color::DarkGray)),
            Span::styled(format!(" {:>3}%", app.volume), fg(Color::Gray)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        area,
    );
}

fn render_status_and_footer(frame: &mut Frame, app: &App, area: Rect) {
    let timer = if let Some(end) = app.sleep_timer_end {
        let rem = end.saturating_duration_since(std::time::Instant::now()).as_secs();
        if rem > 0 {
            format!(" ⏲ Sleep: {:02}:{:02} ", rem / 60, rem % 60)
        } else {
            " ⏲ Timer expired ".to_string()
        }
    } else {
        "".to_string()
    };

    let lines = vec![
        Line::from(Span::styled(&timer, fg(Color::LightCyan))),
        Line::from(vec![
            Span::styled(" [←→] Volume ", fg(Color::DarkGray)),
            Span::styled(" [Spc] ⏯ ", fg(Color::DarkGray)),
            Span::styled(" [Tab] Stations ", fg(Color::DarkGray)),
            Span::styled(" [t] Timer ", fg(Color::DarkGray)),
            Span::styled(" [?] Help ", fg(Color::DarkGray)),
            Span::styled(" [q] Quit ", fg(Color::DarkGray)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        area,
    );
}

// ─── Station List Screen ─────────────────────────────────

fn render_station_list(frame: &mut Frame, app: &App) {
    let size = frame.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(1), // separator
            Constraint::Min(10),   // station list
            Constraint::Length(3), // search bar / status
            Constraint::Length(2), // footer
        ])
        .split(size);

    render_sl_header(frame, app, layout[0]);

    let sep = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(fg(Color::DarkGray));
    frame.render_widget(sep, layout[1]);

    render_station_list_body(frame, app, layout[2]);
    render_sl_footer(frame, app, layout[3]);
    render_sl_keys(frame, app, layout[4]);
}

fn render_sl_header(frame: &mut Frame, _app: &App, area: Rect) {
    let visible = _app.visible_stations();
    let lines = vec![
        Line::from(Span::styled("Stations", magenta_bold())),
        Line::from(Span::styled(
            format!("{} stations visible", visible.len()),
            fg(Color::Gray),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Left),
        area,
    );
}

fn render_station_list_body(frame: &mut Frame, app: &App, area: Rect) {
    let visible = app.visible_stations();
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(list_idx, &station_idx)| {
            let station = &app.stations[station_idx];
            let is_selected = list_idx == app.selected_idx;
            let is_playing = app.current_station_idx == Some(station_idx);
            let is_fav = app.is_favorite(station_idx);

            let fav_icon = if is_fav { "♥ " } else { "  " };
            let play_icon = if is_playing { "▶ " } else { "   " };

            let line = Line::from(vec![
                Span::styled(play_icon, fg(Color::LightGreen)),
                Span::styled(fav_icon, fg(Color::LightRed)),
                Span::styled(&station.name, if is_selected { green_bold() } else { fg(Color::White) }),
                Span::styled(
                    format!("  [{}]", station.category),
                    fg(if is_selected { Color::LightGreen } else { Color::Gray }),
                ),
                Span::styled("  [Live]", fg(Color::LightBlue)),
            ]);

            let style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE));
    frame.render_widget(list, area);
}

fn render_sl_footer(frame: &mut Frame, app: &App, area: Rect) {
    let search_prefix = "Search: ";
    let cursor = if app.is_searching { "█" } else { "" };
    let search_line = Line::from(vec![
        Span::styled(search_prefix, fg(Color::Gray)),
        Span::styled(&app.search_query, fg(Color::White)),
        Span::styled(cursor, fg(Color::White)),
    ]);
    frame.render_widget(Paragraph::new(search_line), area);
}

fn render_sl_keys(frame: &mut Frame, _app: &App, area: Rect) {
    let lines = vec![
        Line::from(vec![
            Span::styled(" [↑↓/j,k] Navigate ", fg(Color::DarkGray)),
            Span::styled(" [Enter] Play ", fg(Color::DarkGray)),
            Span::styled(" [f] Favorite ", fg(Color::DarkGray)),
            Span::styled(" [/] Search ", fg(Color::DarkGray)),
            Span::styled(" [Tab] Back ", fg(Color::DarkGray)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        area,
    );
}

// ─── Help Overlay ─────────────────────────────────────────

fn render_help_overlay(frame: &mut Frame, _app: &App) {
    let size = frame.area();
    let popup = centered_rect(60, 70, size);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(fg(Color::LightCyan))
        .title_style(cyan_bold());

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("Navigation & Playback", cyan_bold())),
        Line::from(Span::styled("─────────────────────", fg(Color::DarkGray))),
        Line::from(Span::styled("  Tab          Switch between screens", fg(Color::White))),
        Line::from(Span::styled("  ↑↓ / jk      Navigate station list", fg(Color::White))),
        Line::from(Span::styled("  Enter        Play selected station", fg(Color::White))),
        Line::from(Span::styled("  Space        Toggle pause/play", fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled("Volume & Controls", cyan_bold())),
        Line::from(Span::styled("─────────────────────", fg(Color::DarkGray))),
        Line::from(Span::styled("  ← → / hl     Adjust volume", fg(Color::White))),
        Line::from(Span::styled("  t            Set sleep timer", fg(Color::White))),
        Line::from(Span::styled("  c            Clear sleep timer", fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled("Station Management", cyan_bold())),
        Line::from(Span::styled("─────────────────────", fg(Color::DarkGray))),
        Line::from(Span::styled("  f            Toggle favorite", fg(Color::White))),
        Line::from(Span::styled("  /            Focus search bar", fg(Color::White))),
        Line::from(Span::styled("  a            Add custom station", fg(Color::White))),
        Line::from(Span::styled("  d            Delete custom station", fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled("Other", cyan_bold())),
        Line::from(Span::styled("─────────────────────", fg(Color::DarkGray))),
        Line::from(Span::styled("  ?            Show this help", fg(Color::White))),
        Line::from(Span::styled("  q / Esc      Quit / go back", fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled("Press any key to close", fg(Color::Gray).add_modifier(Modifier::ITALIC))),
    ];

    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(para, popup);
}

// ─── Utilities ──────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn truncate_str(s: &str, max_len: usize) -> String {
    let width = s.width();
    if width <= max_len {
        s.to_string()
    } else {
        let mut result = String::new();
        let mut current = 0;
        for c in s.chars() {
            if current >= max_len - 3 {
                break;
            }
            current += c.width().unwrap_or(0);
            result.push(c);
        }
        result.push_str("...");
        result
    }
}

// ─── Popups ───────────────────────────────────────────────

fn render_timer_prompt(frame: &mut Frame, app: &App) {
    let size = frame.area();
    let popup = centered_rect(40, 20, size);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Sleep Timer ")
        .borders(Borders::ALL)
        .border_style(fg(Color::LightCyan));

    let lines = vec![
        Line::from("Enter minutes to sleep:"),
        Line::from(vec![
            Span::styled("> ", fg(Color::LightYellow)),
            Span::styled(&app.timer_input, fg(Color::White)),
            Span::styled("█", fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Enter: Confirm | Esc: Cancel", fg(Color::DarkGray))),
    ];

    let para = Paragraph::new(lines).block(block).alignment(Alignment::Left);
    frame.render_widget(para, popup);
}

fn render_add_station_popup(frame: &mut Frame, app: &App) {
    let size = frame.area();
    let popup = centered_rect(50, 30, size);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Add Custom Station ")
        .borders(Borders::ALL)
        .border_style(fg(Color::LightCyan));

    let name_style = if !app.add_station_focus { fg(Color::LightYellow) } else { fg(Color::Gray) };
    let url_style = if app.add_station_focus { fg(Color::LightYellow) } else { fg(Color::Gray) };

    let name_cursor = if !app.add_station_focus { "█" } else { "" };
    let url_cursor = if app.add_station_focus { "█" } else { "" };

    let lines = vec![
        Line::from(Span::styled("Name:", name_style)),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(&app.add_station_name, fg(Color::White)),
            Span::styled(name_cursor, fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled("URL:", url_style)),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(&app.add_station_url, fg(Color::White)),
            Span::styled(url_cursor, fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Tab: Switch | Enter: Save | Esc: Cancel", fg(Color::DarkGray))),
    ];

    let para = Paragraph::new(lines).block(block).alignment(Alignment::Left);
    frame.render_widget(para, popup);
}
