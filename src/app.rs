use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::stations::{get_preset_stations, Station};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum Screen {
    NowPlaying,
    StationList,
    Help,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AppState {
    Playing,
    Paused,
    Buffering,
    Error(String),
}

#[allow(dead_code)]
pub struct MpvMetadata {
    pub title: Option<String>,
    pub time_pos: Option<f64>,
    pub duration: Option<f64>,
    pub paused: bool,
}

pub struct App {
    pub screen: Screen,
    pub state: AppState,
    pub stations: Vec<Station>,
    pub selected_idx: usize,
    pub current_station_idx: Option<usize>,
    pub favorite_urls: HashSet<String>,
    pub metadata: Option<MpvMetadata>,
    pub volume: u8,
    pub quit: bool,
    pub status_msg: String,
    pub play_time_str: String,
    pub buffering_timer: u8,
    pub help_visible: bool,

    pub search_query: String,
    pub is_searching: bool,

    pub show_timer_prompt: bool,
    pub timer_input: String,
    pub sleep_timer_end: Option<Instant>,

    pub show_add_station: bool,
    pub add_station_name: String,
    pub add_station_url: String,
    pub add_station_focus: bool,

    pub history: Vec<String>,
    pub filter_category: Option<crate::stations::Category>,

    pub show_splash: bool,
    pub splash_start: Instant,

    pub config_changed: bool,
    pub needs_redraw: bool,
}

impl App {
    pub fn new(config: &Config) -> Self {
        let mut preset = get_preset_stations();

        // Add custom stations from config
        for saved in &config.custom_stations {
            preset.push(Station::new_custom(&saved.name, &saved.url));
        }
        let stations = preset;

        let favorite_urls: HashSet<String> = config.favorites.iter().cloned().collect();
        let selected_idx = stations.iter().position(|s| {
            config.last_station_url.as_ref().map(|u| u == &s.url).unwrap_or(false)
        }).unwrap_or(0);

        Self {
            screen: Screen::NowPlaying,
            state: AppState::Buffering,
            stations,
            selected_idx,
            current_station_idx: None,
            favorite_urls,
            metadata: None,
            volume: config.volume,
            quit: false,
            status_msg: "Buffering ...".to_string(),
            play_time_str: "00:00 / 00:00".to_string(),
            buffering_timer: 0,
            help_visible: false,
            search_query: String::new(),
            is_searching: false,
            show_timer_prompt: false,
            timer_input: String::new(),
            sleep_timer_end: None,
            show_add_station: false,
            add_station_name: String::new(),
            add_station_url: String::new(),
            add_station_focus: false,
            history: Vec::new(),
            filter_category: None,
            show_splash: true,
            splash_start: Instant::now(),
            config_changed: false,
            needs_redraw: true,
        }
    }

    pub fn current_station(&self) -> Option<&Station> {
        self.current_station_idx.map(|idx| &self.stations[idx])
    }

    pub fn volume_up(&mut self) {
        if self.volume < 100 {
            self.volume = (self.volume + 5).min(100);
            self.config_changed = true;
        }
    }

    pub fn volume_down(&mut self) {
        if self.volume > 0 {
            self.volume = self.volume.saturating_sub(5);
            self.config_changed = true;
        }
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            AppState::Playing => self.state = AppState::Paused,
            AppState::Paused => self.state = AppState::Playing,
            _ => {}
        }
    }

    pub fn select_next(&mut self) {
        let visible = self.visible_stations();
        if !visible.is_empty() {
            // Clamp first in case visible list shrank since last navigation
            self.selected_idx = self.selected_idx.min(visible.len() - 1);
            self.selected_idx = (self.selected_idx + 1).min(visible.len() - 1);
        } else {
            self.selected_idx = 0;
        }
    }

    pub fn select_prev(&mut self) {
        let visible = self.visible_stations();
        if !visible.is_empty() {
            self.selected_idx = self.selected_idx.min(visible.len() - 1);
        }
        self.selected_idx = self.selected_idx.saturating_sub(1);
    }

    pub fn visible_stations(&self) -> Vec<usize> {
        let filter = self.search_query.to_lowercase();
        self.stations.iter().enumerate().filter(|(_, s)| {
            let matches_filter = if filter.is_empty() {
                true
            } else {
                s.name.to_lowercase().contains(&filter) ||
                s.category.to_string().to_lowercase().contains(&filter)
            };
            let matches_cat = match &self.filter_category {
                Some(cat) => s.category == *cat,
                None => true,
            };
            matches_filter && matches_cat
        }).map(|(i, _)| i).collect()
    }

    pub fn toggle_favorite(&mut self) {
        let visible = self.visible_stations();
        if let Some(&idx) = visible.get(self.selected_idx) {
            let url = self.stations[idx].url.clone();
            if self.favorite_urls.contains(&url) {
                self.favorite_urls.remove(&url);
            } else {
                self.favorite_urls.insert(url);
            }
            self.config_changed = true;
        }
    }

    pub fn is_favorite(&self, idx: usize) -> bool {
        self.favorite_urls.contains(&self.stations[idx].url)
    }

    pub fn play_station(&mut self, station_idx: usize) {
        self.current_station_idx = Some(station_idx);
        self.state = AppState::Buffering;
        self.buffering_timer = 0;
        self.status_msg = "Buffering ...".to_string();
        self.play_time_str = "00:00 / 00:00".to_string();
        self.metadata = None;
        self.config_changed = true;

        let url = self.stations[station_idx].url.clone();
        if !self.history.contains(&url) {
            self.history.push(url);
        }
    }

    pub fn add_custom_station(&mut self, name: &str, url: &str) {
        let station = Station::new_custom(name, url);
        self.stations.push(station);
        self.config_changed = true;
    }

    pub fn delete_selected_station(&mut self) {
        let visible = self.visible_stations();
        if let Some(&idx) = visible.get(self.selected_idx)
            && idx < self.stations.len() && self.stations[idx].is_custom {
                self.stations.remove(idx);
                self.selected_idx = self.selected_idx.saturating_sub(1);
                self.config_changed = true;
            }
    }

    pub fn set_timer(&mut self, minutes: u64) {
        self.sleep_timer_end = Some(Instant::now() + Duration::from_secs(minutes * 60));
    }

    #[allow(dead_code)]
    pub fn remaining_sleep_time(&self) -> Option<Duration> {
        self.sleep_timer_end.map(|end| {
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                Duration::from_secs(0)
            } else {
                remaining
            }
        })
    }

    pub fn clear_timer(&mut self) {
        self.sleep_timer_end = None;
    }

    pub fn timer_expired(&self) -> bool {
        self.sleep_timer_end
            .map(|end| Instant::now() >= end)
            .unwrap_or(false)
    }

    pub fn splash_done(&self) -> bool {
        self.splash_start.elapsed() > Duration::from_millis(1200)
    }

    pub fn is_searching(&self) -> bool {
        self.is_searching
    }
}