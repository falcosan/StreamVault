mod animeunity;
mod models;
mod nove;
mod raiplay;
mod streaming_community;

pub use animeunity::AnimeUnityProvider;
pub use models::*;
pub use nove::NoveProvider;
pub use raiplay::RaiPlayProvider;
pub use streaming_community::StreamingCommunityProvider;

pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

pub(crate) const DOMAINS_URL: &str =
    "https://raw.githubusercontent.com/Arrowar/SC_Domains/refs/heads/main/domains.json";

pub(crate) fn provider_hash(s: &str) -> u64 {
    s.bytes()
        .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64))
}

pub(crate) async fn resolve_domain_url(
    client: &reqwest::Client,
    key: &str,
    base_url: &std::sync::RwLock<String>,
) {
    for attempt in 0u64..3 {
        match client.get(DOMAINS_URL).send().await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(json) => {
                    if let Some(url) = json[key]["full_url"].as_str() {
                        let url = url.trim_end_matches('/').to_string();
                        if url.starts_with("http") {
                            eprintln!("[StreamVault] Resolved {key} domain: {url}");
                            *base_url.write().unwrap() = url;
                            return;
                        }
                    }
                }
                Err(e) => eprintln!("[StreamVault] {key} domain parse error: {e}"),
            },
            Err(e) => eprintln!("[StreamVault] {key} domain fetch error: {e}"),
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt + 1))).await;
        }
    }
    eprintln!("[StreamVault] Failed to resolve {key} domain after 3 attempts");
}

pub(crate) fn parse_vixcloud_hls(script: &str) -> Option<String> {
    use regex::Regex;
    use std::sync::LazyLock;

    static TOKEN_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?:['"]token['"]|token)\s*:\s*['"]([^'"]+)['"]"#).unwrap());
    static EXPIRES_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?:['"]expires['"]|expires)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
    });
    static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?:['"]url['"]|url)\s*:\s*['"](?P<url>https?://[^'"]+)['"]"#).unwrap()
    });
    static FHD_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"window\.canPlayFHD\s*=\s*(true|false)").unwrap());

    let token = TOKEN_RE.captures(script)?.get(1)?.as_str();
    let expires = EXPIRES_RE.captures(script)?.get(1)?.as_str();
    let base_url = URL_RE.captures(script)?.get(1)?.as_str();
    let can_fhd = FHD_RE
        .captures(script)
        .and_then(|c| c.get(1))
        .is_some_and(|m| m.as_str() == "true");

    let mut parsed = url::Url::parse(base_url).ok()?;
    let has_b = parsed.query_pairs().any(|(k, v)| k == "b" && v == "1");
    {
        let mut q = parsed.query_pairs_mut();
        q.clear();
        if can_fhd {
            q.append_pair("h", "1");
        }
        if has_b {
            q.append_pair("b", "1");
        }
        q.append_pair("token", token);
        q.append_pair("expires", expires);
    }
    Some(parsed.to_string())
}

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
    async fn get_catalog(&self, limit: usize) -> ProviderResult<Vec<MediaEntry>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie() -> MediaEntry {
        MediaEntry {
            id: 1,
            name: "Inception".into(),
            alternative_names: Vec::new(),
            slug: "inception".into(),
            media_type: MediaType::Movie,
            year: Some("2010".into()),
            image_url: None,
            description: None,
            score: None,
            provider: 0,
            language: String::new(),
        }
    }

    fn series() -> MediaEntry {
        MediaEntry {
            id: 2,
            name: "Lost".into(),
            alternative_names: Vec::new(),
            slug: "lost".into(),
            media_type: MediaType::Series,
            year: None,
            image_url: None,
            description: None,
            score: None,
            provider: 0,
            language: String::new(),
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
            image_url: None,
        };
        let json = serde_json::to_string(&ep).unwrap();
        let loaded: Episode = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.number, 5);
        assert_eq!(loaded.duration, Some(42));
    }
}
