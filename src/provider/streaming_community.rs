use super::models::{Episode, MediaEntry, MediaType, Season, StreamUrl};
use super::traits::{Provider, ProviderError, ProviderResult};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use url::Url;

const BASE_URL: &str = "https://streamingcommunityz.name";
const LANGUAGES: &[&str] = &["it", "en"];
const IMAGE_PRIORITIES: &[&str] = &["poster", "cover", "cover_mobile", "background"];

pub struct StreamingCommunityProvider {
    client: Client,
}

impl StreamingCommunityProvider {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .cookie_store(true)
            .build()
            .expect("Failed to build HTTP client");

        Self { client }
    }

    async fn fetch_inertia_version(&self) -> ProviderResult<String> {
        let resp = self
            .client
            .get(BASE_URL)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let html = resp
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        Self::extract_version_from_html(&html)
    }

    fn extract_version_from_html(html: &str) -> ProviderResult<String> {
        let document = Html::parse_document(html);
        let selector = Selector::parse("div#app").unwrap();

        let app_div = document
            .select(&selector)
            .next()
            .ok_or_else(|| ProviderError::Parse("No #app div found".into()))?;

        let data_page = app_div
            .value()
            .attr("data-page")
            .ok_or_else(|| ProviderError::Parse("No data-page attribute".into()))?;

        let page_data: serde_json::Value = serde_json::from_str(data_page)
            .map_err(|e| ProviderError::Parse(format!("Invalid data-page JSON: {e}")))?;

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
        let url = format!("{BASE_URL}/{lang}/search?q={query}");

        let resp = self
            .client
            .get(&url)
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
            .unwrap_or(&Vec::new())
            .clone();

        let mut entries = Vec::new();
        for title in &titles {
            if let Some(entry) = Self::parse_search_result(title) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    fn parse_search_result(value: &serde_json::Value) -> Option<MediaEntry> {
        let id = value["id"].as_u64()?;
        let name = value["name"].as_str()?.to_string();
        let slug = value["slug"].as_str().unwrap_or("").to_string();
        let type_str = value["type"].as_str().unwrap_or("");

        let media_type = match type_str {
            "film" | "movie" | "ova" => MediaType::Movie,
            _ => MediaType::Series,
        };

        let year = Self::extract_year(value);
        let image_url = Self::extract_image_url(value);

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

    fn extract_year(value: &serde_json::Value) -> String {
        if let Some(translations) = value["translations"].as_array() {
            for t in translations {
                let key = t["key"].as_str().unwrap_or("");
                if key == "first_air_date" || key == "release_date" {
                    if let Some(date) = t["value"].as_str() {
                        if date.len() >= 4 {
                            return date[..4].to_string();
                        }
                    }
                }
            }
        }

        for field in &["last_air_date", "release_date"] {
            if let Some(date) = value[field].as_str() {
                if date.len() >= 4 {
                    return date[..4].to_string();
                }
            }
        }

        "9999".into()
    }

    fn extract_image_url(value: &serde_json::Value) -> Option<String> {
        let images = value["images"].as_array()?;
        let cdn_base = BASE_URL.replace("stream", "cdn.stream");

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
        let url = format!("{BASE_URL}/titles/{}-{}", entry.id, entry.slug);

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
        let selector = Selector::parse("div#app").unwrap();

        let app_div = document
            .select(&selector)
            .next()
            .ok_or_else(|| ProviderError::Parse("No #app div on title page".into()))?;

        let data_page = app_div
            .value()
            .attr("data-page")
            .ok_or_else(|| ProviderError::Parse("No data-page on title page".into()))?;

        let page_data: serde_json::Value = serde_json::from_str(data_page)
            .map_err(|e| ProviderError::Parse(format!("Invalid title page JSON: {e}")))?;

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
            format!("{BASE_URL}/iframe/{media_id}?episode_id={ep_id}&next_episode=1")
        } else {
            format!("{BASE_URL}/iframe/{media_id}")
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
        let selector = Selector::parse("iframe").unwrap();

        document
            .select(&selector)
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
        let selector = Selector::parse("body script").unwrap();

        let script_text = document
            .select(&selector)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();

        let token = Self::extract_regex(&script_text, r#"(?:['"]token['"]|token)\s*:\s*['"]([^'"]+)['"]"#)
            .ok_or_else(|| ProviderError::StreamExtraction("No token found".into()))?;

        let expires = Self::extract_regex(&script_text, r#"(?:['"]expires['"]|expires)\s*:\s*['"]([^'"]+)['"]"#)
            .ok_or_else(|| ProviderError::StreamExtraction("No expires found".into()))?;

        let base_url = Self::extract_regex(&script_text, r#"(?:['"]url['"]|url)\s*:\s*['"]([^'"]+)['"]"#)
            .ok_or_else(|| ProviderError::StreamExtraction("No URL found".into()))?;

        let can_play_fhd = Self::extract_regex(&script_text, r"window\.canPlayFHD\s*=\s*(true|false)")
            .map(|v| v == "true")
            .unwrap_or(false);

        let mut parsed = Url::parse(&base_url)
            .map_err(|e| ProviderError::Parse(format!("Invalid stream URL: {e}")))?;

        let has_b_param = parsed
            .query_pairs()
            .any(|(k, v)| k == "b" && v == "1");

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

    fn extract_regex(text: &str, pattern: &str) -> Option<String> {
        Regex::new(pattern)
            .ok()?
            .captures(text)?
            .get(1)
            .map(|m| m.as_str().to_string())
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

            let mut seasons = Vec::new();
            for s in &seasons_json {
                if let (Some(id), Some(number)) = (s["id"].as_u64(), s["number"].as_u64()) {
                    seasons.push(Season {
                        id,
                        number: number as u32,
                        name: s["name"].as_str().map(String::from),
                        episodes: Vec::new(),
                    });
                }
            }

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
                "{BASE_URL}/titles/{}-{}/season-{season_number}",
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

            let episodes_json = json["props"]["loadedSeason"]["episodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();

            let mut episodes = Vec::new();
            for ep in &episodes_json {
                if let (Some(id), Some(number)) = (ep["id"].as_u64(), ep["number"].as_u64()) {
                    episodes.push(Episode {
                        id,
                        number: number as u32,
                        name: ep["name"].as_str().unwrap_or("").to_string(),
                        duration: ep["duration"].as_u64().map(|d| d as u32),
                    });
                }
            }

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
