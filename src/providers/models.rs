use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    Movie,
    #[default]
    Series,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaEntry {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub provider: usize,
    pub provider_name: String,
    pub language: String,
    pub media_type: MediaType,
    pub alternative_names: Vec<String>,
    pub year: Option<String>,
    pub score: Option<String>,
    pub image_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub id: u64,
    pub number: u32,
    pub name: String,
    pub duration: Option<u32>,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StreamUrl {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl MediaEntry {
    pub fn key(&self) -> (usize, u64) {
        (self.provider, self.id)
    }

    pub fn is_movie(&self) -> bool {
        self.media_type == MediaType::Movie
    }

    pub fn year_display(&self) -> &str {
        self.year.as_deref().unwrap_or_default()
    }

    pub fn display_title(&self) -> String {
        match &self.year {
            Some(year) => format!("{} ({year})", self.name),
            None => self.name.clone(),
        }
    }

    pub fn episode_title(&self, season: u32, episode: &Episode) -> String {
        let code = format!("{} S{season:02}E{:02}", self.name, episode.number);
        if episode.name.is_empty() {
            code
        } else {
            format!("{code} - {}", episode.name)
        }
    }
}

pub fn next_episode(episodes: &[Episode], after: u32) -> Option<&Episode> {
    episodes
        .iter()
        .filter(|e| e.number > after)
        .min_by_key(|e| e.number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie() -> MediaEntry {
        MediaEntry {
            name: "Inception".into(),
            media_type: MediaType::Movie,
            year: Some("2010".into()),
            ..Default::default()
        }
    }

    fn episode(number: u32, name: &str) -> Episode {
        Episode {
            id: u64::from(number),
            number,
            name: name.into(),
            duration: None,
            image_url: None,
        }
    }

    #[test]
    fn display_title_appends_year_when_known() {
        assert_eq!(movie().display_title(), "Inception (2010)");
        let untitled = MediaEntry {
            year: None,
            ..movie()
        };
        assert_eq!(untitled.display_title(), "Inception");
    }

    #[test]
    fn year_display_is_empty_without_year() {
        assert_eq!(movie().year_display(), "2010");
        assert_eq!(MediaEntry::default().year_display(), "");
    }

    #[test]
    fn is_movie_follows_media_type() {
        assert!(movie().is_movie());
        assert!(!MediaEntry::default().is_movie());
    }

    #[test]
    fn episode_title_includes_name_only_when_present() {
        let show = MediaEntry {
            name: "Lost".into(),
            ..Default::default()
        };
        assert_eq!(show.episode_title(1, &episode(3, "")), "Lost S01E03");
        assert_eq!(
            show.episode_title(2, &episode(5, "Pilot")),
            "Lost S02E05 - Pilot"
        );
    }

    #[test]
    fn next_episode_picks_lowest_higher_number() {
        let episodes = [episode(3, ""), episode(1, ""), episode(2, "")];
        assert_eq!(next_episode(&episodes, 1).map(|e| e.number), Some(2));
        assert!(next_episode(&episodes, 3).is_none());
    }

    #[test]
    fn media_entry_json_shape_is_stable() {
        let json = serde_json::to_value(movie()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": 0, "name": "Inception", "slug": "", "provider": 0,
                "provider_name": "", "language": "", "media_type": "Movie",
                "alternative_names": [], "year": "2010", "score": null,
                "image_url": null, "description": null
            })
        );
    }
}
