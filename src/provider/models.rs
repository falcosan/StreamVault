use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MediaType {
    Movie,
    Series,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaEntry {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub media_type: MediaType,
    pub year: Option<String>,
    pub image_url: Option<String>,
    pub tmdb_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Season {
    pub id: u64,
    pub number: u32,
    pub name: Option<String>,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: u64,
    pub number: u32,
    pub name: String,
    pub duration: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct StreamUrl {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl MediaEntry {
    pub fn is_movie(&self) -> bool {
        self.media_type == MediaType::Movie
    }

    pub fn display_title(&self) -> String {
        match &self.year {
            Some(y) => format!("{} ({y})", self.name),
            None => self.name.clone(),
        }
    }

    pub fn year_display(&self) -> &str {
        self.year.as_deref().unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie_entry() -> MediaEntry {
        MediaEntry {
            id: 1,
            name: "Inception".into(),
            slug: "inception".into(),
            media_type: MediaType::Movie,
            year: Some("2010".into()),
            image_url: None,
            tmdb_id: None,
        }
    }

    fn series_entry() -> MediaEntry {
        MediaEntry {
            id: 2,
            name: "Lost".into(),
            slug: "lost".into(),
            media_type: MediaType::Series,
            year: None,
            image_url: None,
            tmdb_id: None,
        }
    }

    #[test]
    fn is_movie_returns_true_for_movies() {
        assert!(movie_entry().is_movie());
    }

    #[test]
    fn is_movie_returns_false_for_series() {
        assert!(!series_entry().is_movie());
    }

    #[test]
    fn display_title_with_year() {
        assert_eq!(movie_entry().display_title(), "Inception (2010)");
    }

    #[test]
    fn display_title_without_year() {
        assert_eq!(series_entry().display_title(), "Lost");
    }

    #[test]
    fn year_display_with_year() {
        assert_eq!(movie_entry().year_display(), "2010");
    }

    #[test]
    fn year_display_without_year() {
        assert_eq!(series_entry().year_display(), "");
    }
}
