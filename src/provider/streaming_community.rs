use super::models::{Episode, MediaEntry, MediaType, Season, StreamUrl};
use super::traits::{Provider, ProviderError, ProviderResult};
use crate::util::UNKNOWN_YEAR;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{LazyLock, OnceLock};
use url::Url;

const DEFAULT_BASE_URL: &str = "https://streamingcommunityz.name";
const LANGUAGES: &[&str] = &["it", "en"];
const IMAGE_PRIORITIES: &[&str] = &["poster", "cover", "cover_mobile", "background"];

static APP_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("div#app").expect("valid selector"));
static IFRAME_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("iframe").expect("valid selector"));
static BODY_SCRIPT_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("body script").expect("valid selector"));

static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
static EXPIRES_RE: OnceLock<Regex> = OnceLock::new();
static URL_RE: OnceLock<Regex> = OnceLock::new();
static FHD_RE: OnceLock<Regex> = OnceLock::new();

fn token_re() -> &'static Regex {
    TOKEN_RE.get_or_init(|| {
        Regex::new(r#"(?:['"]token['"]|token)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
    })
}

fn expires_re() -> &'static Regex {
    EXPIRES_RE.get_or_init(|| {
        Regex::new(r#"(?:['"]expires['"]|expires)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
    })
}

fn url_re() -> &'static Regex {
    URL_RE.get_or_init(|| {
        Regex::new(r#"(?:['"]url['"]|url)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
    })
}

fn fhd_re() -> &'static Regex {
    FHD_RE.get_or_init(|| Regex::new(r"window\.canPlayFHD\s*=\s*(true|false)").unwrap())
}

pub struct StreamingCommunityProvider {
    client: Client,
    base_url: String,
}

impl StreamingCommunityProvider {
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL.to_string())
    }

    pub fn with_base_url(base_url: String) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .cookie_store(true)
            .build()
            .expect("reqwest client build");

        Self { client, base_url }
    }

    fn parse_data_page(html: &str) -> ProviderResult<serde_json::Value> {
        let document = Html::parse_document(html);

        let app_div = document
            .select(&APP_SELECTOR)
            .next()
            .ok_or_else(|| ProviderError::Parse("No #app div found".into()))?;

        let data_page = app_div
            .value()
            .attr("data-page")
            .ok_or_else(|| ProviderError::Parse("No data-page attribute".into()))?;

        serde_json::from_str(data_page)
            .map_err(|e| ProviderError::Parse(format!("Invalid data-page JSON: {e}")))
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

        let page_data = Self::parse_data_page(&html)?;

        page_data["version"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| ProviderError::Parse("No version in data-page".into()))
    }

    async fn search_language(
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

        let entries = titles
            .iter()
            .filter_map(|t| self.parse_search_result(t))
            .collect();

        Ok(entries)
    }

    fn parse_search_result(&self, value: &serde_json::Value) -> Option<MediaEntry> {
        let id = value["id"].as_u64()?;
        let name = value["name"].as_str()?.to_string();
        let slug = value["slug"].as_str().unwrap_or("").to_string();
        let type_str = value["type"].as_str().unwrap_or("");

        let media_type = match type_str {
            "film" | "movie" | "ova" => MediaType::Movie,
            _ => MediaType::Series,
        };

        let year = Self::extract_year(value);
        let image_url = self.extract_image_url(value);

        Some(MediaEntry {
            id,
            name,
            slug,
            media_type,
            year,
            image_url,
            tmdb_id: value["tmdb_id"].as_u64().map(|v| v.to_string()),
        })
    }

    fn extract_year(value: &serde_json::Value) -> Option<String> {
        if let Some(translations) = value["translations"].as_array() {
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
            if let Some(date) = value[field].as_str() {
                if date.len() >= 4 {
                    return Some(date[..4].to_string());
                }
            }
        }

        None
    }

    fn extract_image_url(&self, value: &serde_json::Value) -> Option<String> {
        let images = value["images"].as_array()?;
        let cdn_base = self.base_url.replace("stream", "cdn.stream");

        for priority in IMAGE_PRIORITIES {
            for img in images {
                if img["type"].as_str() == Some(priority) {
                    if let Some(filename) = img["filename"].as_str() {
                        return Some(format!("{cdn_base}/images/{filename}"));
                    }
                }
            }
        }

        images.first().and_then(|img| {
            img["filename"]
                .as_str()
                .map(|f| format!("{cdn_base}/images/{f}"))
        })
    }

    async fn fetch_title_page(
        &self,
        entry: &MediaEntry,
    ) -> ProviderResult<(serde_json::Value, String)> {
        let url = format!("{}/titles/{}-{}", self.base_url, entry.id, entry.slug);

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

        let page_data = Self::parse_data_page(&html)?;
        let version = page_data["version"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok((page_data, version))
    }

    async fn fetch_iframe_url(
        &self,
        media_id: u64,
        episode_id: Option<u64>,
    ) -> ProviderResult<String> {
        let url = if let Some(ep_id) = episode_id {
            format!(
                "{}/iframe/{media_id}?episode_id={ep_id}&next_episode=1",
                self.base_url
            )
        } else {
            format!("{}/iframe/{media_id}", self.base_url)
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

        let document = Html::parse_document(&html);

        document
            .select(&IFRAME_SELECTOR)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(String::from)
            .ok_or_else(|| ProviderError::Parse("No iframe src found".into()))
    }

    async fn extract_stream_from_vixcloud(&self, iframe_url: &str) -> ProviderResult<StreamUrl> {
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

        let document = Html::parse_document(&html);

        let script_text = document
            .select(&BODY_SCRIPT_SELECTOR)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();

        let token = token_re()
            .captures(&script_text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ProviderError::StreamExtraction("No token found".into()))?;

        let expires = expires_re()
            .captures(&script_text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ProviderError::StreamExtraction("No expires found".into()))?;

        let base_url = url_re()
            .captures(&script_text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ProviderError::StreamExtraction("No URL found".into()))?;

        let can_play_fhd = fhd_re()
            .captures(&script_text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str() == "true")
            .unwrap_or(false);

        let mut parsed = Url::parse(&base_url)
            .map_err(|e| ProviderError::Parse(format!("Invalid stream URL: {e}")))?;

        let has_b_param = parsed.query_pairs().any(|(k, v)| k == "b" && v == "1");

        parsed.set_query(None);

        let mut params = Vec::new();
        if can_play_fhd {
            params.push(("h", "1".to_string()));
        }
        if has_b_param {
            params.push(("b", "1".to_string()));
        }
        params.push(("token", token));
        params.push(("expires", expires));

        let query_string: String = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");

        parsed.set_query(Some(&query_string));

        Ok(StreamUrl {
            url: parsed.to_string(),
            headers: Vec::new(),
        })
    }
}

impl Provider for StreamingCommunityProvider {
    fn name(&self) -> &str {
        "StreamingCommunity"
    }

    fn search(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>> {
        let query = query.to_string();
        Box::pin(async move {
            let version = self.fetch_inertia_version().await?;
            let mut all_entries = Vec::new();
            let mut seen_ids = HashSet::new();

            for lang in LANGUAGES {
                if let Ok(entries) = self.search_language(&query, lang, &version).await {
                    for entry in entries {
                        if seen_ids.insert(entry.id) {
                            all_entries.push(entry);
                        }
                    }
                }
            }

            Ok(all_entries)
        })
    }

    fn get_seasons(
        &self,
        entry: &MediaEntry,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Season>>> + Send + '_>> {
        let entry = entry.clone();
        Box::pin(async move {
            let (page_data, _version) = self.fetch_title_page(&entry).await?;

            let seasons_json = page_data["props"]["title"]["seasons"]
                .as_array()
                .cloned()
                .unwrap_or_default();

            let mut seasons: Vec<Season> = seasons_json
                .iter()
                .filter_map(|s| {
                    let id = s["id"].as_u64()?;
                    let number = s["number"].as_u64()? as u32;
                    Some(Season {
                        id,
                        number,
                        name: s["name"].as_str().map(String::from),
                        episodes: Vec::new(),
                    })
                })
                .collect();

            seasons.sort_by_key(|s| s.number);
            Ok(seasons)
        })
    }

    fn get_episodes(
        &self,
        entry: &MediaEntry,
        season_number: u32,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Episode>>> + Send + '_>> {
        let entry = entry.clone();
        Box::pin(async move {
            let (_page_data, version) = self.fetch_title_page(&entry).await?;

            let url = format!(
                "{}/titles/{}-{}/season-{season_number}",
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

            let episodes_json = json["props"]["loadedSeason"]["episodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();

            let mut episodes: Vec<Episode> = episodes_json
                .iter()
                .filter_map(|ep| {
                    let id = ep["id"].as_u64()?;
                    let number = ep["number"].as_u64()? as u32;
                    Some(Episode {
                        id,
                        number,
                        name: ep["name"].as_str().unwrap_or("").to_string(),
                        duration: ep["duration"].as_u64().map(|d| d as u32),
                    })
                })
                .collect();

            episodes.sort_by_key(|e| e.number);
            Ok(episodes)
        })
    }

    fn get_stream_url(
        &self,
        entry: &MediaEntry,
        episode: Option<&Episode>,
        _season_number: Option<u32>,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<StreamUrl>> + Send + '_>> {
        let entry = entry.clone();
        let episode = episode.cloned();
        Box::pin(async move {
            let (page_data, _version) = self.fetch_title_page(&entry).await?;

            let media_id = page_data["props"]["title"]["id"]
                .as_u64()
                .unwrap_or(entry.id);

            let episode_id = episode.as_ref().map(|ep| ep.id);
            let iframe_url = self.fetch_iframe_url(media_id, episode_id).await?;
            self.extract_stream_from_vixcloud(&iframe_url).await
        })
    }
}
