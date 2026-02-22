mod models;
mod raiplay;
mod streaming_community;

pub use models::*;
pub use raiplay::RaiPlayProvider;
pub use streaming_community::StreamingCommunityProvider;

pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Debug, Clone)]
pub enum ProviderError {
    Network(String),
    Parse(String),
    StreamExtraction(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(m) => write!(f, "Network error: {m}"),
            Self::Parse(m) => write!(f, "Parse error: {m}"),
            Self::StreamExtraction(m) => write!(f, "Stream extraction error: {m}"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e.to_string())
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(e: serde_json::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

pub type ProviderResult<T> = Result<T, ProviderError>;

#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    async fn init(&self) {}
    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>>;
    async fn get_seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<Season>>;
    async fn get_episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>>;
    async fn get_stream_url(
        &self,
        entry: &MediaEntry,
        episode: Option<&Episode>,
        season: Option<u32>,
    ) -> ProviderResult<StreamUrl>;
    async fn get_catalog(&self) -> ProviderResult<Vec<MediaEntry>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie() -> MediaEntry {
        MediaEntry {
            id: 1,
            name: "Inception".into(),
            slug: "inception".into(),
            media_type: MediaType::Movie,
            year: Some("2010".into()),
            image_url: None,
            tmdb_id: None,
            description: None,
            provider: 0,
        }
    }

    fn series() -> MediaEntry {
        MediaEntry {
            id: 2,
            name: "Lost".into(),
            slug: "lost".into(),
            media_type: MediaType::Series,
            year: None,
            image_url: None,
            tmdb_id: None,
            description: None,
            provider: 0,
        }
    }

    #[test]
    fn is_movie_true() {
        assert!(movie().is_movie());
    }
    #[test]
    fn is_movie_false() {
        assert!(!series().is_movie());
    }
    #[test]
    fn display_title_with_year() {
        assert_eq!(movie().display_title(), "Inception (2010)");
    }
    #[test]
    fn display_title_without_year() {
        assert_eq!(series().display_title(), "Lost");
    }
    #[test]
    fn year_display_with() {
        assert_eq!(movie().year_display(), "2010");
    }
    #[test]
    fn year_display_without() {
        assert_eq!(series().year_display(), "");
    }

    #[test]
    fn provider_error_display() {
        assert_eq!(
            ProviderError::Network("timeout".into()).to_string(),
            "Network error: timeout"
        );
        assert_eq!(
            ProviderError::Parse("bad json".into()).to_string(),
            "Parse error: bad json"
        );
        assert_eq!(
            ProviderError::StreamExtraction("no token".into()).to_string(),
            "Stream extraction error: no token"
        );
    }

    #[test]
    fn media_entry_serde_roundtrip() {
        let m = movie();
        let json = serde_json::to_string(&m).unwrap();
        let loaded: MediaEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.id, 1);
        assert_eq!(loaded.name, "Inception");
        assert!(loaded.is_movie());
    }

    #[test]
    fn season_serde_roundtrip() {
        let s = Season {
            id: 10,
            number: 3,
            name: Some("Season 3".into()),
        };
        let json = serde_json::to_string(&s).unwrap();
        let loaded: Season = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.number, 3);
        assert_eq!(loaded.name.as_deref(), Some("Season 3"));
    }

    #[test]
    fn episode_serde_roundtrip() {
        let ep = Episode {
            id: 100,
            number: 5,
            name: "Pilot".into(),
            duration: Some(42),
        };
        let json = serde_json::to_string(&ep).unwrap();
        let loaded: Episode = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.number, 5);
        assert_eq!(loaded.duration, Some(42));
    }

    #[test]
    fn fallback_url_is_https() {
        assert!(StreamingCommunityProvider::FALLBACK_URL.starts_with("https://"));
    }

}
