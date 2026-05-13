use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Lofi,
    Jazz,
    Classical,
    Ambient,
    Synthwave,
    Nature,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Category::Lofi => write!(f, "Lofi"),
            Category::Jazz => write!(f, "Jazz"),
            Category::Classical => write!(f, "Classical"),
            Category::Ambient => write!(f, "Ambient"),
            Category::Synthwave => write!(f, "Synthwave"),
            Category::Nature => write!(f, "Nature"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Station {
    pub name: String,
    pub url: String,
    pub category: Category,
    pub is_custom: bool,
}

impl Station {
    pub fn new(name: &str, url: &str, category: Category) -> Self {
        Self {
            name: name.to_string(),
            url: url.to_string(),
            category,
            is_custom: false,
        }
    }

    pub fn new_custom(name: &str, url: &str) -> Self {
        Self {
            name: name.to_string(),
            url: url.to_string(),
            category: Category::Lofi,
            is_custom: true,
        }
    }
}

pub fn get_preset_stations() -> Vec<Station> {
    vec![
        Station::new(
            "Lofi Girl",
            "https://www.youtube.com/@LofiGirl/live",
            Category::Lofi,
        ),
        Station::new(
            "Chillhop Music",
            "https://www.youtube.com/@ChillhopMusic/live",
            Category::Lofi,
        ),
        Station::new(
            "Anime Lofi",
            "https://www.youtube.com/@lofigeek/live",
            Category::Lofi,
        ),
        Station::new(
            "Coffee Shop Ambience",
            "https://www.youtube.com/@CafeMusicBGMchannel/live",
            Category::Ambient,
        ),
        Station::new(
            "Jazz Vibes",
            "https://www.youtube.com/@TheJazzHopCafe/live",
            Category::Jazz,
        ),
        Station::new(
            "Classical Focus",
            "https://www.youtube.com/channel/UCFd5q5UWWV0RpSpPkTFOtRg/live",
            Category::Classical,
        ),
        Station::new(
            "Synthwave City",
            "https://www.youtube.com/@NightrideFM/live",
            Category::Synthwave,
        ),
        Station::new(
            "Ambient Study",
            "https://www.youtube.com/@TheBootlegBoy/live",
            Category::Nature,
        ),
    ]
}
