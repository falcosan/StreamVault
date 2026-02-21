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
    pub year: String,
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

    pub fn poster_url(&self) -> Option<&str> {
        self.image_url.as_deref()
    }

    pub fn display_title(&self) -> String {
        if self.year != "9999" {
            format!("{} ({})", self.name, self.year)
        } else {
            self.name.clone()
        }
    }
}
