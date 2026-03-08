use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MediaType {
    Movie,
    Series,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaEntry {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub provider: usize,
    pub language: String,
    pub media_type: MediaType,
    pub year: Option<String>,
    pub score: Option<String>,
    pub image_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Season {
    pub id: u64,
    pub number: u32,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[inline]
    pub fn is_movie(&self) -> bool {
        self.media_type == MediaType::Movie
    }

    pub fn display_title(&self) -> String {
        match &self.year {
            Some(y) => format!("{} ({y})", self.name),
            None => self.name.clone(),
        }
    }

    #[inline]
    pub fn year_display(&self) -> &str {
        self.year.as_deref().unwrap_or("")
    }
}
