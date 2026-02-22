use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{LazyLock, OnceLock, RwLock};
use url::Url;

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
    #[serde(default)]
    pub provider: usize,
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
    fn init(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
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
    base_url: RwLock<String>,
}

const SC_DOMAINS_URL: &str =
    "https://raw.githubusercontent.com/Arrowar/SC_Domains/refs/heads/main/domains.json";

impl StreamingCommunityProvider {
    const LANG: &str = "it";
    const LANGS: &[&str] = &["it", "en"];
    const FALLBACK_URL: &str = "https://streamingcommunityz.name";
    const IMG_PRIORITY: &[&str] = &["poster", "cover", "cover_mobile", "background"];

    pub fn with_config(timeout: u64) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(timeout))
            .cookie_store(true)
            .build()
            .expect("reqwest client");
        Self {
            client,
            base_url: RwLock::new(Self::FALLBACK_URL.to_string()),
        }
    }

    fn base_url(&self) -> String {
        self.base_url.read().unwrap().clone()
    }

    async fn resolve_domain(&self) {
        let resp = match self.client.get(SC_DOMAINS_URL).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[StreamVault] Domain resolve failed: {e}");
                return;
            }
        };
        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => {
                eprintln!("[StreamVault] Domain JSON parse failed: {e}");
                return;
            }
        };
        if let Some(url) = json["streamingcommunity"]["full_url"].as_str() {
            let url = url.trim_end_matches('/').to_string();
            eprintln!("[StreamVault] Resolved SC domain: {url}");
            *self.base_url.write().unwrap() = url;
        }
    }

    fn parse_data_page(html: &str) -> ProviderResult<serde_json::Value> {
        static APP_SEL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("div#app").unwrap());
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
            .get(&self.base_url())
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
        let url = format!("{}/{lang}/search", self.base_url());
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
            provider: 0,
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
        let cdn = self.base_url().replace("stream", "cdn.stream");
        for prio in Self::IMG_PRIORITY {
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
        let lang = Self::LANG;
        let base = self.base_url();
        let url = format!(
            "{base}/{lang}/titles/{}-{}",
            entry.id, entry.slug
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
    async fn fetch_iframe_url(
        &self,
        lang: &str,
        media_id: u64,
        ep_id: Option<u64>,
    ) -> ProviderResult<String> {
        let base = self.base_url();
        let url = match ep_id {
            Some(eid) => format!(
                "{base}/{lang}/iframe/{media_id}?episode_id={eid}&next_episode=1",
            ),
            None => format!("{base}/{lang}/iframe/{media_id}"),
        };
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
        static IFRAME_SEL: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse("iframe").unwrap());
        let doc = Html::parse_document(&html);
        doc.select(&IFRAME_SEL)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(String::from)
            .ok_or_else(|| ProviderError::Parse("No iframe src found".into()))
    }

    async fn extract_stream_url(&self, iframe_url: &str) -> ProviderResult<StreamUrl> {
        static SCRIPT_SEL: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse("body script").unwrap());
        static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
        static EXPIRES_RE: OnceLock<Regex> = OnceLock::new();
        static URL_RE: OnceLock<Regex> = OnceLock::new();
        static FHD_RE: OnceLock<Regex> = OnceLock::new();

        let resp = self
            .client
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
            .select(&SCRIPT_SEL)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();

        let token = TOKEN_RE
            .get_or_init(|| {
                Regex::new(r#"(?:['"]token['"]|token)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
            })
            .captures(&script)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ProviderError::StreamExtraction("No token".into()))?;
        let expires = EXPIRES_RE
            .get_or_init(|| {
                Regex::new(r#"(?:['"]expires['"]|expires)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
            })
            .captures(&script)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ProviderError::StreamExtraction("No expires".into()))?;
        let base = URL_RE
            .get_or_init(|| {
                Regex::new(r#"(?:['"]url['"]|url)\s*:\s*['"](?P<url>https?://[^'"]+)['"]"#).unwrap()
            })
            .captures(&script)
            .and_then(|c| c.name("url"))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ProviderError::StreamExtraction("No URL".into()))?;
        let fhd = FHD_RE
            .get_or_init(|| Regex::new(r"window\.canPlayFHD\s*=\s*(true|false)").unwrap())
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
}

impl Provider for StreamingCommunityProvider {
    fn init(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async { self.resolve_domain().await })
    }

    fn search(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>> {
        let query = query.to_string();
        Box::pin(async move {
            let version = self.fetch_inertia_version().await?;
            let mut all = Vec::new();
            let mut seen = HashSet::new();
            for lang in Self::LANGS {
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
            let lang = Self::LANG;
            let base = self.base_url();
            let url = format!(
                "{base}/{lang}/titles/{}-{}/season-{season}",
                entry.id, entry.slug
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
            let iframe = self.fetch_iframe_url(Self::LANG, media_id, ep_id).await?;
            self.extract_stream_url(&iframe).await
        })
    }

    fn get_catalog(
        &self,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>> {
        Box::pin(async move {
            let resp = self
                .client
                .get(&self.base_url())
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

const RAIPLAY_BASE: &str = "https://www.raiplay.it";

const RAIPLAY_SEARCH: &str =
    "https://www.raiplay.it/atomatic/raiplay-search-service/api/v1/msearch";

const RAIPLAY_RELINKER: &str = "https://mediapolisvod.rai.it/relinker/relinkerServlet.htm";

fn raiplay_hash(s: &str) -> u64 {
    s.bytes()
        .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64))
}

fn raiplay_abs_url(s: &str) -> String {
    if s.starts_with("http") {
        s.to_string()
    } else {
        format!("{RAIPLAY_BASE}{s}")
    }
}

pub struct RaiPlayProvider {
    client: Client,
    season_data: tokio::sync::Mutex<HashMap<u64, Vec<(String, String)>>>,
    episode_data: tokio::sync::Mutex<HashMap<u64, String>>,
}

impl RaiPlayProvider {
    pub fn with_config(timeout: u64) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(timeout))
            .build()
            .expect("reqwest client");
        Self {
            client,
            season_data: tokio::sync::Mutex::new(HashMap::new()),
            episode_data: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    fn parse_search_card(card: &serde_json::Value) -> Option<MediaEntry> {
        let path_id = card["path_id"].as_str()?.to_string();
        let name = card["titolo"].as_str().unwrap_or("").to_string();
        if name.is_empty() {
            return None;
        }
        let image = card["immagine"].as_str().map(raiplay_abs_url);
        let year = image.as_ref().and_then(|img| {
            let parts: Vec<&str> = img.split('/').collect();
            parts.iter().rev().nth(3).and_then(|y| {
                if y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()) {
                    Some(y.to_string())
                } else {
                    None
                }
            })
        });
        Some(MediaEntry {
            id: raiplay_hash(&path_id),
            name,
            slug: path_id,
            media_type: MediaType::Series,
            year,
            image_url: image,
            tmdb_id: None,
            description: None,
            provider: 0,
        })
    }

    async fn resolve_stream(&self, page_url: &str) -> ProviderResult<StreamUrl> {
        let json_url = if page_url.ends_with(".json") {
            page_url.to_string()
        } else if let Some(base) = page_url.strip_suffix(".html") {
            format!("{base}.json")
        } else {
            format!("{page_url}.json")
        };
        let resp = self
            .client
            .get(&json_url)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        let video_json = if let Some(first_item) = json["first_item_path"].as_str() {
            let item_url = format!("{}.json", raiplay_abs_url(first_item));
            let resp = self
                .client
                .get(&item_url)
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            resp.json()
                .await
                .map_err(|e| ProviderError::Parse(e.to_string()))?
        } else {
            json
        };

        let content_url = video_json["video"]["content_url"]
            .as_str()
            .ok_or_else(|| ProviderError::StreamExtraction("No content_url".into()))?;
        let cont = content_url
            .split('=')
            .nth(1)
            .ok_or_else(|| ProviderError::StreamExtraction("No cont parameter".into()))?;

        let relinker_url = format!("{RAIPLAY_RELINKER}?cont={cont}&output=62");
        let resp = self
            .client
            .get(&relinker_url)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let text = String::from_utf8_lossy(&bytes);
        let relinker_json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| ProviderError::Parse(format!("Relinker JSON: {e}")))?;

        let stream = relinker_json["video"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::StreamExtraction("No video in relinker".into()))?;

        static QUALITY_RE: OnceLock<Regex> = OnceLock::new();
        let final_url = if !stream.contains(".mpd") {
            let re = QUALITY_RE.get_or_init(|| Regex::new(r"(_,[\d,]+)(/playlist\.m3u8)").unwrap());
            re.replace(stream, "_1200,1800,2400,3600,5000$2")
                .to_string()
        } else {
            stream.to_string()
        };

        Ok(StreamUrl {
            url: final_url,
            headers: Vec::new(),
        })
    }
}

impl Provider for RaiPlayProvider {
    fn search(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>> {
        let query = query.to_string();
        Box::pin(async move {
            let body = serde_json::json!({
                "templateIn": "6470a982e4e0301afe1f81f1",
                "templateOut": "6516ac5d40da6c377b151642",
                "params": {
                    "param": query,
                    "from": null,
                    "sort": "relevance",
                    "onlyVideoQuery": false
                }
            });
            let resp = self
                .client
                .post(RAIPLAY_SEARCH)
                .json(&body)
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::Parse(e.to_string()))?;
            let cards = json["agg"]["titoli"]["cards"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut seen = HashSet::new();
            Ok(cards
                .iter()
                .take(15)
                .filter_map(Self::parse_search_card)
                .filter(|e| seen.insert(e.id))
                .collect())
        })
    }

    fn get_seasons(
        &self,
        entry: &MediaEntry,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Season>>> + Send + '_>> {
        let entry = entry.clone();
        Box::pin(async move {
            let path = entry.slug.trim_start_matches('/');
            let url = format!("{RAIPLAY_BASE}/{path}");
            let resp = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::Parse(e.to_string()))?;
            let blocks = json["blocks"].as_array().cloned().unwrap_or_default();
            let mut seasons = Vec::new();
            let mut mappings = Vec::new();
            let mut num = 1u32;
            for block in &blocks {
                if block["type"].as_str() != Some("RaiPlay Multimedia Block") {
                    continue;
                }
                let block_name = block["name"].as_str().unwrap_or("").to_lowercase();
                if block_name == "clip" || block_name == "extra" {
                    continue;
                }
                let block_id = block["id"].as_str().unwrap_or("").to_string();
                if let Some(sets) = block["sets"].as_array() {
                    for set in sets {
                        let ep_count = set["episode_size"]["number"].as_u64().unwrap_or(0);
                        if ep_count == 0 {
                            continue;
                        }
                        let set_id = set["id"].as_str().unwrap_or("").to_string();
                        let set_name = set["name"].as_str().map(String::from);
                        seasons.push(Season {
                            id: num as u64,
                            number: num,
                            name: set_name,
                        });
                        mappings.push((block_id.clone(), set_id));
                        num += 1;
                    }
                }
            }
            self.season_data.lock().await.insert(entry.id, mappings);
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
            let idx = (season - 1) as usize;
            let lock = self.season_data.lock().await;
            let (block_id, set_id) = lock
                .get(&entry.id)
                .and_then(|m| m.get(idx))
                .cloned()
                .ok_or_else(|| ProviderError::Parse("Season mapping not found".into()))?;
            drop(lock);

            let base_path = entry.slug.trim_start_matches('/').trim_end_matches(".json");
            let url = format!("{RAIPLAY_BASE}/{base_path}/{block_id}/{set_id}/episodes.json");
            let resp = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::Parse(e.to_string()))?;

            let items: Vec<serde_json::Value> = if let Some(seasons_arr) =
                json["seasons"].as_array()
            {
                seasons_arr
                    .iter()
                    .flat_map(|s| {
                        s["episodes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .flat_map(|ep| ep["cards"].as_array().into_iter().flatten().cloned())
                    })
                    .collect()
            } else {
                json["cards"].as_array().cloned().unwrap_or_default()
            };

            let mut episodes = Vec::new();
            let mut ep_urls = HashMap::new();
            for (i, card) in items.iter().enumerate() {
                let ep_num = card["episode"]
                    .as_str()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or((i + 1) as u32);
                let name = card["episode_title"]
                    .as_str()
                    .or_else(|| card["name"].as_str())
                    .or_else(|| card["toptitle"].as_str())
                    .unwrap_or("")
                    .to_string();
                let duration = card["duration_in_minutes"]
                    .as_str()
                    .and_then(|s| s.parse::<u32>().ok())
                    .or_else(|| {
                        card["duration"]
                            .as_str()
                            .and_then(|s| s.parse::<u32>().ok())
                    });
                let weblink = card["weblink"]
                    .as_str()
                    .or_else(|| card["url"].as_str())
                    .unwrap_or("");
                let ep_id = raiplay_hash(&format!("{}{}", entry.slug, weblink));
                if !weblink.is_empty() {
                    ep_urls.insert(ep_id, raiplay_abs_url(weblink));
                }
                episodes.push(Episode {
                    id: ep_id,
                    number: ep_num,
                    name,
                    duration,
                });
            }
            self.episode_data.lock().await.extend(ep_urls);
            episodes.sort_unstable_by_key(|e| e.number);
            Ok(episodes)
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
            let page_url = if let Some(ref ep) = episode {
                self.episode_data
                    .lock()
                    .await
                    .get(&ep.id)
                    .cloned()
                    .ok_or_else(|| ProviderError::Parse("Episode URL not cached".into()))?
            } else {
                let path = entry.slug.trim_start_matches('/');
                format!("{RAIPLAY_BASE}/{path}")
            };
            self.resolve_stream(&page_url).await
        })
    }

    fn get_catalog(
        &self,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>> {
        Box::pin(async move { Ok(Vec::new()) })
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
