mod app;
mod config;
mod player;
mod stations;
mod ui;

use app::{App, AppState, Screen};
use config::Config;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use player::Player;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut config = Config::load();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(&config);
    let player = Player::new();

    // Auto-play last station or first station
    let start_idx = config.last_station_url.as_ref().and_then(|url| {
        app.stations.iter().position(|s| &s.url == url)
    }).unwrap_or(0);
    app.play_station(start_idx);
    if let Some(s) = app.stations.get(start_idx) {
        player.play(&s.url, app.volume);
    }

    let tick_rate = Duration::from_millis(150);
    let mut last_tick = Instant::now();
    let mut buffering_ticks = 0;

    // Setup crossbeam channel for async -> sync communication
    // (player metadata is already received via the callback in player module)

    loop {
        if app.needs_redraw {
            terminal.draw(|f| ui::render(f, &app))?;
            app.needs_redraw = false;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        // Check for player metadata
        if let Some(meta) = player.try_recv_metadata() {
            app.needs_redraw = true;
            app.metadata = Some(app::MpvMetadata {
                title: meta.title,
                time_pos: meta.time_pos,
                duration: meta.duration,
                paused: meta.paused,
            });
            if let Some(err) = meta.error {
                app.state = AppState::Error(err);
            } else if meta.paused && !matches!(app.state, AppState::Paused) {
                app.state = AppState::Paused;
            } else if !meta.paused && matches!(app.state, AppState::Buffering | AppState::Playing) {
                // Transition from Buffering → Playing once mpv actually reports playback
                app.state = AppState::Playing;
            }
        }

        // Handle splash screen auto-advance
        if app.show_splash && app.splash_done() {
            app.show_splash = false;
            app.needs_redraw = true;
        }

        // Handle sleep timer
        if app.timer_expired() {
            app.quit = true;
        }

        if crossterm::event::poll(timeout)?
            && let Event::Key(key) = event::read()? {
                app.needs_redraw = true;
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if app.help_visible {
                    app.help_visible = false;
                    continue;
                }

                match app.screen {
                    Screen::NowPlaying => {
                        if let Some(should_quit) = handle_now_playing_input(&mut app, &player, key.code)
                            && should_quit {
                                break;
                            }
                    }
                    Screen::StationList => {
                        if app.show_add_station {
                            match key.code {
                                KeyCode::Esc => {
                                    app.show_add_station = false;
                                    app.add_station_name.clear();
                                    app.add_station_url.clear();
                                    app.add_station_focus = false;
                                }
                                KeyCode::Enter => {
                                    if !app.add_station_focus {
                                        // On Name field: advance to URL field
                                        app.add_station_focus = true;
                                    } else if !app.add_station_name.is_empty() && !app.add_station_url.is_empty() {
                                        // Both fields filled: add station
                                        let name = app.add_station_name.clone();
                                        let url = app.add_station_url.clone();
                                        app.add_custom_station(&name, &url);
                                        config.custom_stations.push(config::SavedStation {
                                            name,
                                            url,
                                        });
                                        app.show_add_station = false;
                                        app.add_station_name.clear();
                                        app.add_station_url.clear();
                                        app.add_station_focus = false;
                                    }
                                    // If on URL field but URL empty, do nothing (wait for input)
                                }
                                KeyCode::Char(c) => {
                                    if app.add_station_focus {
                                        app.add_station_url.push(c);
                                    } else {
                                        app.add_station_name.push(c);
                                    }
                                }
                                KeyCode::Backspace => {
                                    if app.add_station_focus {
                                        app.add_station_url.pop();
                                    } else {
                                        app.add_station_name.pop();
                                    }
                                }
                                KeyCode::Tab => {
                                    app.add_station_focus = !app.add_station_focus;
                                }
                                _ => {}
                            }
                        } else if app.show_timer_prompt {
                            // Handle timer prompt input
                            match key.code {
                                KeyCode::Char(c) if c.is_ascii_digit() => {
                                    app.timer_input.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.timer_input.pop();
                                }
                                KeyCode::Enter => {
                                    if let Ok(minutes) = app.timer_input.parse::<u64>() {
                                        if minutes > 0 {
                                            app.set_timer(minutes);
                                        }
                                    } else {
                                        app.status_msg = "Invalid timer input".to_string();
                                    }
                                    app.show_timer_prompt = false;
                                    app.timer_input.clear();
                                }
                                KeyCode::Esc => {
                                    app.show_timer_prompt = false;
                                    app.timer_input.clear();
                                }
                                _ => {}
                            }
                        } else if app.is_searching {
                            // Handle search input
                            match key.code {
                                KeyCode::Char(c) => {
                                    app.search_query.push(c);
                                }
                                KeyCode::Backspace => {
                                    app.search_query.pop();
                                }
                                KeyCode::Esc => {
                                    app.is_searching = false;
                                    app.search_query.clear();
                                }
                                KeyCode::Enter => {
                                    // Select current station when pressing Enter during search
                                    let visible = app.visible_stations();
                                    if let Some(&idx) = visible.get(app.selected_idx) {
                                        app.play_station(idx);
                                        if let Some(s) = app.stations.get(idx) {
                                            player.play(&s.url, app.volume);
                                        }
                                    }
                                    app.is_searching = false;
                                    app.search_query.clear();
                                }
                                _ => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    app.quit = true;
                                }
                                KeyCode::Char(' ') => {
                                    app.toggle_pause();
                                    player.toggle_pause();
                                }
                                KeyCode::Char('f') => {
                                    app.toggle_favorite();
                                }
                                KeyCode::Char('a') => {
                                    app.show_add_station = true;
                                    app.add_station_name.clear();
                                    app.add_station_url.clear();
                                    app.add_station_focus = false;
                                }
                                KeyCode::Char('d') => {
                                    app.delete_selected_station();
                                }
                                KeyCode::Char('/') | KeyCode::Char('s') => {
                                    app.is_searching = true;
                                }
                                KeyCode::Char('?') => {
                                    app.help_visible = true;
                                }
                                KeyCode::Char('t') => {
                                    app.show_timer_prompt = true;
                                }
                                KeyCode::Char('c') => {
                                    app.clear_timer();
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    app.select_prev();
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    app.select_next();
                                }
                                KeyCode::Left | KeyCode::Char('h') => {
                                    app.volume_down();
                                    player.set_volume(app.volume);
                                }
                                KeyCode::Right | KeyCode::Char('l') => {
                                    app.volume_up();
                                    player.set_volume(app.volume);
                                }
                                KeyCode::Enter => {
                                    let visible = app.visible_stations();
                                    if let Some(&idx) = visible.get(app.selected_idx) {
                                        app.play_station(idx);
                                        if let Some(s) = app.stations.get(idx) {
                                            player.play(&s.url, app.volume);
                                        }
                                    }
                                }
                                KeyCode::Tab => {
                                    app.screen = Screen::NowPlaying;
                                }
                                _ => {}
                            }
                        }
                    }
                    Screen::Help => {}
                }
            }

            if app.config_changed {
                app.config_changed = false;
                config.volume = app.volume;
                config.last_station_url = app.current_station().map(|s| s.url.clone());
                config.favorites = app.favorite_urls.iter().cloned().collect();
                config.custom_stations = app.stations.iter()
                    .filter(|s| s.is_custom)
                    .map(|s| config::SavedStation {
                        name: s.name.clone(),
                        url: s.url.clone(),
                    })
                    .collect();
                let _ = config.save();
            }

        if last_tick.elapsed() >= tick_rate {
            // Buffering timer — metadata arrival will also trigger Playing
            if let AppState::Buffering = app.state {
                buffering_ticks += 1;
                app.buffering_timer = (buffering_ticks / 6).min(255) as u8;
                app.status_msg = format!("Buffering stream... ({}s)", app.buffering_timer);
                if buffering_ticks > 60 {
                    // Safety fallback: assume playing after ~10s even without metadata
                    app.state = AppState::Playing;
                }
                app.needs_redraw = true;
            }

            last_tick = Instant::now();
        }

        if app.quit {
            break;
        }
    }

    // Save config — rebuild custom_stations from app state so deletions persist
    config.volume = app.volume;
    config.last_station_url = app.current_station().map(|s| s.url.clone());
    config.favorites = app.favorite_urls.into_iter().collect();
    config.custom_stations = app.stations.iter()
        .filter(|s| s.is_custom)
        .map(|s| config::SavedStation {
            name: s.name.clone(),
            url: s.url.clone(),
        })
        .collect();
    let _ = config.save();

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn handle_now_playing_input(app: &mut App, player: &Player, code: KeyCode) -> Option<bool> {
    if app.show_timer_prompt {
        match code {
            KeyCode::Char(c) if c.is_ascii_digit() => {
                app.timer_input.push(c);
                return Some(false);
            }
            KeyCode::Backspace => {
                app.timer_input.pop();
                return Some(false);
            }
            KeyCode::Enter => {
                if let Ok(minutes) = app.timer_input.parse::<u64>() {
                    if minutes > 0 {
                        app.set_timer(minutes);
                    }
                } else {
                    app.status_msg = "Invalid timer input".to_string();
                }
                app.show_timer_prompt = false;
                app.timer_input.clear();
                return Some(false);
            }
            KeyCode::Esc => {
                app.show_timer_prompt = false;
                app.timer_input.clear();
                return Some(false);
            }
            _ => {}
        }
    }

    if app.is_searching() {
        match code {
            KeyCode::Char(c) => {
                app.search_query.push(c);
                return Some(false);
            }
            KeyCode::Backspace => {
                app.search_query.pop();
                return Some(false);
            }
            KeyCode::Esc => {
                app.is_searching = false;
                app.search_query.clear();
                return Some(false);
            }
            _ => {}
        }
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.quit = true;
            return Some(true);
        }
        KeyCode::Char(' ') => {
            app.toggle_pause();
            player.toggle_pause();
        }
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Down | KeyCode::Char('j') => {
            app.volume_down();
            player.set_volume(app.volume);
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Up | KeyCode::Char('k') => {
            app.volume_up();
            player.set_volume(app.volume);
        }
        KeyCode::Tab => {
            app.screen = Screen::StationList;
        }
        KeyCode::Char('?') => {
            app.help_visible = true;
        }
        KeyCode::Char('t') => {
            app.show_timer_prompt = true;
        }
        KeyCode::Char('c') => {
            app.clear_timer();
        }
        KeyCode::Char('/') => {
            app.screen = Screen::StationList;
            app.is_searching = true;
        }
        _ => {}
    }

    Some(false)
}
