use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{LazyLock, OnceLock};
use url::Url;

const LANG: &str = "it";
const LANGS: &[&str] = &["it", "en"];
const BASE_URL: &str = "https://streamingcommunityz.name";
const IMG_PRIORITY: &[&str] = &["poster", "cover", "cover_mobile", "background"];

static APP_SEL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("div#app").unwrap());
static IFRAME_SEL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("iframe").unwrap());
static BODY_SCRIPT_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("body script").unwrap());

static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
static EXPIRES_RE: OnceLock<Regex> = OnceLock::new();
static URL_RE: OnceLock<Regex> = OnceLock::new();
static FHD_RE: OnceLock<Regex> = OnceLock::new();

#[inline]
fn token_re() -> &'static Regex {
    TOKEN_RE
        .get_or_init(|| Regex::new(r#"(?:['"]token['"]|token)\s*:\s*['"]([^'"]+)['"]"#).unwrap())
}
#[inline]
fn expires_re() -> &'static Regex {
    EXPIRES_RE.get_or_init(|| {
        Regex::new(r#"(?:['"]expires['"]|expires)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
    })
}
#[inline]
fn url_re() -> &'static Regex {
    URL_RE.get_or_init(|| {
        Regex::new(r#"(?:['"]url['"]|url)\s*:\s*['"](?P<url>https?://[^'"]+)['"]"#).unwrap()
    })
}
#[inline]
fn fhd_re() -> &'static Regex {
    FHD_RE.get_or_init(|| Regex::new(r"window\.canPlayFHD\s*=\s*(true|false)").unwrap())
}

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
    pub media_type: MediaType,
    pub year: Option<String>,
    pub image_url: Option<String>,
    pub tmdb_id: Option<String>,
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

    #[inline]
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
pub type ProviderResult<T> = Result<T, ProviderError>;

pub trait Provider: Send + Sync {
    fn search(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>>;
    fn get_seasons(
        &self,
        entry: &MediaEntry,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Season>>> + Send + '_>>;
    fn get_episodes(
        &self,
        entry: &MediaEntry,
        season: u32,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Episode>>> + Send + '_>>;
    fn get_stream_url(
        &self,
        entry: &MediaEntry,
        episode: Option<&Episode>,
        season: Option<u32>,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<StreamUrl>> + Send + '_>>;
    fn get_catalog(
        &self,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>>;
}

pub struct StreamingCommunityProvider {
    client: Client,
    base_url: String,
}

impl StreamingCommunityProvider {
    #[inline]
    pub fn default_base_url() -> &'static str {
        BASE_URL
    }

    pub fn with_config(base_url: String, timeout: u64) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(timeout))
            .cookie_store(true)
            .build()
            .expect("reqwest client");
        Self { client, base_url }
    }

    fn parse_data_page(html: &str) -> ProviderResult<serde_json::Value> {
        let doc = Html::parse_document(html);
        let app = doc
            .select(&APP_SEL)
            .next()
            .ok_or_else(|| ProviderError::Parse("No #app div".into()))?;
        let data = app
            .value()
            .attr("data-page")
            .ok_or_else(|| ProviderError::Parse("No data-page attr".into()))?;
        serde_json::from_str(data).map_err(|e| ProviderError::Parse(format!("Invalid JSON: {e}")))
    }

    async fn fetch_inertia_version(&self) -> ProviderResult<String> {
        let resp = self
            .client
            .get(&self.base_url)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let html = resp
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let page = Self::parse_data_page(&html)?;
        page["version"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| ProviderError::Parse("No version in data-page".into()))
    }

    async fn search_lang(
        &self,
        query: &str,
        lang: &str,
        version: &str,
    ) -> ProviderResult<Vec<MediaEntry>> {
        let url = format!("{}/{lang}/search", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("q", query)])
            .header("x-inertia", "true")
            .header("x-inertia-version", version)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;
        let titles = json["props"]["titles"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(titles.iter().filter_map(|t| self.parse_result(t)).collect())
    }

    fn parse_result(&self, v: &serde_json::Value) -> Option<MediaEntry> {
        let id = v["id"].as_u64()?;
        let name = v["name"].as_str()?.to_string();
        let slug = v["slug"].as_str().unwrap_or("").to_string();
        let media_type = match v["type"].as_str().unwrap_or("") {
            "film" | "movie" | "ova" => MediaType::Movie,
            _ => MediaType::Series,
        };
        Some(MediaEntry {
            id,
            name,
            slug,
            media_type,
            year: Self::extract_year(v),
            image_url: self.extract_image_url(v),
            tmdb_id: v["tmdb_id"].as_u64().map(|n| n.to_string()),
            description: Self::extract_description(v),
        })
    }

    fn extract_year(v: &serde_json::Value) -> Option<String> {
        if let Some(translations) = v["translations"].as_array() {
            for t in translations {
                let key = t["key"].as_str().unwrap_or("");
                if key == "first_air_date" || key == "release_date" {
                    if let Some(date) = t["value"].as_str() {
                        if date.len() >= 4 {
                            return Some(date[..4].to_string());
                        }
                    }
                }
            }
        }
        for field in &["last_air_date", "release_date"] {
            if let Some(date) = v[field].as_str() {
                if date.len() >= 4 {
                    return Some(date[..4].to_string());
                }
            }
        }
        None
    }

    fn extract_description(v: &serde_json::Value) -> Option<String> {
        if let Some(translations) = v["translations"].as_array() {
            for t in translations {
                let key = t["key"].as_str().unwrap_or("");
                if key == "description" || key == "overview" || key == "plot" {
                    if let Some(desc) = t["value"].as_str() {
                        let trimmed = desc.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
        }
        v["plot"]
            .as_str()
            .or_else(|| v["description"].as_str())
            .or_else(|| v["overview"].as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
    }

    fn extract_image_url(&self, v: &serde_json::Value) -> Option<String> {
        let images = v["images"].as_array()?;
        let cdn = self.base_url.replace("stream", "cdn.stream");
        for prio in IMG_PRIORITY {
            for img in images {
                if img["type"].as_str() == Some(prio) {
                    if let Some(f) = img["filename"].as_str() {
                        return Some(format!("{cdn}/images/{f}"));
                    }
                }
            }
        }
        images.first().and_then(|img| {
            img["filename"]
                .as_str()
                .map(|f| format!("{cdn}/images/{f}"))
        })
    }

    async fn fetch_title_page(
        &self,
        entry: &MediaEntry,
    ) -> ProviderResult<(serde_json::Value, String)> {
        let url = format!(
            "{}/{LANG}/titles/{}-{}",
            self.base_url, entry.id, entry.slug
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let html = resp
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let page = Self::parse_data_page(&html)?;
        let version = page["version"].as_str().unwrap_or("").to_string();
        Ok((page, version))
    }
}

async fn fetch_iframe_url(
    client: &Client,
    base_url: &str,
    lang: &str,
    media_id: u64,
    ep_id: Option<u64>,
) -> ProviderResult<String> {
    let url = match ep_id {
        Some(eid) => format!("{base_url}/{lang}/iframe/{media_id}?episode_id={eid}&next_episode=1"),
        None => format!("{base_url}/{lang}/iframe/{media_id}"),
    };
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    let html = resp
        .text()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    let doc = Html::parse_document(&html);
    doc.select(&IFRAME_SEL)
        .next()
        .and_then(|el| el.value().attr("src"))
        .map(String::from)
        .ok_or_else(|| ProviderError::Parse("No iframe src found".into()))
}

async fn extract_stream_url(client: &Client, iframe_url: &str) -> ProviderResult<StreamUrl> {
    let resp = client
        .get(iframe_url)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    let html = resp
        .text()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    let doc = Html::parse_document(&html);
    let script = doc
        .select(&BODY_SCRIPT_SEL)
        .next()
        .map(|el| el.inner_html())
        .unwrap_or_default();

    let token = token_re()
        .captures(&script)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ProviderError::StreamExtraction("No token".into()))?;
    let expires = expires_re()
        .captures(&script)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ProviderError::StreamExtraction("No expires".into()))?;
    let base = url_re()
        .captures(&script)
        .and_then(|c| c.name("url"))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ProviderError::StreamExtraction("No URL".into()))?;
    let fhd = fhd_re()
        .captures(&script)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str() == "true")
        .unwrap_or(false);

    let mut parsed =
        Url::parse(&base).map_err(|e| ProviderError::Parse(format!("Invalid URL: {e}")))?;
    let has_b = parsed.query_pairs().any(|(k, v)| k == "b" && v == "1");
    parsed.set_query(None);

    let mut params: Vec<(&str, String)> = Vec::with_capacity(4);
    if fhd {
        params.push(("h", "1".into()));
    }
    if has_b {
        params.push(("b", "1".into()));
    }
    params.push(("token", token));
    params.push(("expires", expires));
    let qs: String = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    parsed.set_query(Some(&qs));

    Ok(StreamUrl {
        url: parsed.to_string(),
        headers: Vec::new(),
    })
}

impl Provider for StreamingCommunityProvider {
    fn search(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>> {
        let query = query.to_string();
        Box::pin(async move {
            let version = self.fetch_inertia_version().await?;
            let mut all = Vec::new();
            let mut seen = HashSet::new();
            for lang in LANGS {
                if let Ok(entries) = self.search_lang(&query, lang, &version).await {
                    for e in entries {
                        if seen.insert(e.id) {
                            all.push(e);
                        }
                    }
                }
            }
            Ok(all)
        })
    }

    fn get_seasons(
        &self,
        entry: &MediaEntry,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Season>>> + Send + '_>> {
        let entry = entry.clone();
        Box::pin(async move {
            let (page, _) = self.fetch_title_page(&entry).await?;
            let arr = page["props"]["title"]["seasons"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut seasons: Vec<Season> = arr
                .iter()
                .filter_map(|s| {
                    Some(Season {
                        id: s["id"].as_u64()?,
                        number: s["number"].as_u64()? as u32,
                        name: s["name"].as_str().map(String::from),
                    })
                })
                .collect();
            seasons.sort_unstable_by_key(|s| s.number);
            Ok(seasons)
        })
    }

    fn get_episodes(
        &self,
        entry: &MediaEntry,
        season: u32,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Episode>>> + Send + '_>> {
        let entry = entry.clone();
        Box::pin(async move {
            let (_, version) = self.fetch_title_page(&entry).await?;
            let url = format!(
                "{}/{LANG}/titles/{}-{}/season-{season}",
                self.base_url, entry.id, entry.slug
            );
            let resp = self
                .client
                .get(&url)
                .header("x-inertia", "true")
                .header("x-inertia-version", &version)
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::Parse(e.to_string()))?;
            let arr = json["props"]["loadedSeason"]["episodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut eps: Vec<Episode> = arr
                .iter()
                .filter_map(|ep| {
                    Some(Episode {
                        id: ep["id"].as_u64()?,
                        number: ep["number"].as_u64()? as u32,
                        name: ep["name"].as_str().unwrap_or("").to_string(),
                        duration: ep["duration"].as_u64().map(|d| d as u32),
                    })
                })
                .collect();
            eps.sort_unstable_by_key(|e| e.number);
            Ok(eps)
        })
    }

    fn get_stream_url(
        &self,
        entry: &MediaEntry,
        episode: Option<&Episode>,
        _season: Option<u32>,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<StreamUrl>> + Send + '_>> {
        let entry = entry.clone();
        let episode = episode.cloned();
        Box::pin(async move {
            let (page, _) = self.fetch_title_page(&entry).await?;
            let media_id = page["props"]["title"]["id"].as_u64().unwrap_or(entry.id);
            let ep_id = episode.as_ref().map(|e| e.id);
            let iframe =
                fetch_iframe_url(&self.client, &self.base_url, LANG, media_id, ep_id).await?;
            extract_stream_url(&self.client, &iframe).await
        })
    }

    fn get_catalog(
        &self,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>> {
        Box::pin(async move {
            let resp = self
                .client
                .get(&self.base_url)
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            let html = resp
                .text()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            let page = Self::parse_data_page(&html)?;
            let mut entries = Vec::new();
            let mut seen = HashSet::new();
            if let Some(sliders) = page["props"]["sliders"].as_array() {
                for slider in sliders {
                    if let Some(titles) = slider["titles"].as_array() {
                        for t in titles {
                            if let Some(e) = self.parse_result(t) {
                                if seen.insert(e.id) {
                                    entries.push(e);
                                }
                            }
                        }
                    }
                }
            }
            if entries.is_empty() {
                if let Some(titles) = page["props"]["titles"].as_array() {
                    for t in titles {
                        if let Some(e) = self.parse_result(t) {
                            if seen.insert(e.id) {
                                entries.push(e);
                            }
                        }
                    }
                }
            }
            Ok(entries)
        })
    }
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
    fn default_base_url_is_https() {
        assert!(StreamingCommunityProvider::default_base_url().starts_with("https://"));
    }
}
