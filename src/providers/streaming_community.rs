use super::{
    Episode, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, Season, StreamUrl,
    USER_AGENT,
};
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};
use std::time::Duration;

pub struct StreamingCommunityProvider {
    client: Client,
    base_url: RwLock<String>,
}

impl StreamingCommunityProvider {
    const LANG: &str = "it";
    const LANGS: &[&str] = &["it", "en"];
    const IMG_PRIORITY: &[&str] = &["poster", "cover", "cover_mobile", "background"];

    pub fn with_config(timeout: u64) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(timeout))
            .cookie_store(true)
            .build()
            .expect("reqwest client");
        Self {
            client,
            base_url: RwLock::new(String::new()),
        }
    }

    fn base_url(&self) -> String {
        self.base_url
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    async fn ensure_base_url(&self) {
        if self
            .base_url
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
        {
            self.resolve_domain().await;
        }
    }

    async fn resolve_domain(&self) {
        super::resolve_domain_url(&self.client, "streamingcommunity", &self.base_url).await;
    }

    fn parse_data_page(html: &str) -> ProviderResult<serde_json::Value> {
        static APP_SEL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("div#app").unwrap());
        static DATA_PAGE_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"data-page\s*=\s*['"]([^'"]+)['"]"#).unwrap());

        let doc = Html::parse_document(html);
        if let Some(app) = doc.select(&APP_SEL).next() {
            if let Some(data) = app.value().attr("data-page") {
                return serde_json::from_str(data)
                    .map_err(|e| ProviderError::Parse(format!("Invalid JSON: {e}")));
            }
        }
        if let Some(cap) = DATA_PAGE_RE.captures(html) {
            if let Some(m) = cap.get(1) {
                return serde_json::from_str(m.as_str())
                    .map_err(|e| ProviderError::Parse(format!("Invalid JSON: {e}")));
            }
        }
        Err(ProviderError::Parse("No data-page JSON found".into()))
    }

    async fn fetch_inertia_version(&self) -> ProviderResult<String> {
        self.ensure_base_url().await;
        let resp = self.client.get(self.base_url()).send().await?;
        let html = resp.text().await?;
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
        self.ensure_base_url().await;
        let url = format!("{}/{lang}/search", self.base_url());
        let resp = self
            .client
            .get(&url)
            .query(&[("q", query)])
            .header("x-inertia", "true")
            .header("x-inertia-version", version)
            .send()
            .await?;
        let json: serde_json::Value = resp.json().await?;
        let titles = json["props"]["titles"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(titles
            .iter()
            .filter_map(|t| {
                let mut entry = self.parse_result(t)?;
                entry.language = lang.to_string();
                Some(entry)
            })
            .collect())
    }

    fn parse_result(&self, v: &serde_json::Value) -> Option<MediaEntry> {
        let id = v["id"].as_u64()?;
        let name = v["name"].as_str()?.to_string();
        let slug = v["slug"].as_str().unwrap_or("").to_string();
        let media_type = match v["type"].as_str().unwrap_or("") {
            "film" | "movie" | "ova" => MediaType::Movie,
            _ => MediaType::Series,
        };
        let alternative_names = v["original_name"]
            .as_str()
            .filter(|s| !s.is_empty() && *s != name)
            .into_iter()
            .map(String::from)
            .collect();
        Some(MediaEntry {
            id,
            name,
            alternative_names,
            slug,
            media_type,
            year: Self::extract_year(v),
            image_url: self.extract_image_url(v),
            description: Self::extract_description(v),
            score: v["score"].as_str().map(String::from),
            provider: 0,
            provider_name: String::new(),
            language: String::new(),
        })
    }

    fn extract_year(v: &serde_json::Value) -> Option<String> {
        if let Some(translations) = v["translations"].as_array() {
            for t in translations {
                let key = t["key"].as_str().unwrap_or("");
                if key == "first_air_date" || key == "release_date" {
                    if let Some(date) = t["value"].as_str() {
                        if let Some(year) = date.get(..4) {
                            return Some(year.to_string());
                        }
                    }
                }
            }
        }
        for field in &["last_air_date", "release_date"] {
            if let Some(date) = v[field].as_str() {
                if let Some(year) = date.get(..4) {
                    return Some(year.to_string());
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

    fn entry_lang(entry: &MediaEntry) -> &str {
        if entry.language.is_empty() {
            Self::LANG
        } else {
            &entry.language
        }
    }

    async fn fetch_title_page(
        &self,
        entry: &MediaEntry,
    ) -> ProviderResult<(serde_json::Value, String)> {
        self.ensure_base_url().await;
        let lang = Self::entry_lang(entry);
        let base = self.base_url();
        let url = format!("{base}/{lang}/titles/{}-{}", entry.id, entry.slug);
        let resp = self.client.get(&url).send().await?;
        let html = resp.text().await?;
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
        static IFRAME_SEL: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse("iframe").unwrap());
        static IFRAME_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"<iframe[^>]+src=['"]([^'"]+)['"]"#).unwrap());

        self.ensure_base_url().await;
        let base = self.base_url();
        let url = match ep_id {
            Some(eid) => format!("{base}/{lang}/iframe/{media_id}?episode_id={eid}&next_episode=1"),
            None => format!("{base}/{lang}/iframe/{media_id}"),
        };
        let resp = self.client.get(&url).send().await?;
        let html = resp.text().await?;
        let doc = Html::parse_document(&html);
        if let Some(el) = doc.select(&IFRAME_SEL).next() {
            if let Some(src) = el.value().attr("src") {
                return Ok(src.to_string());
            }
            if let Some(data_src) = el.value().attr("data-src") {
                return Ok(data_src.to_string());
            }
        }
        if let Some(cap) = IFRAME_RE.captures(&html) {
            if let Some(m) = cap.get(1) {
                return Ok(m.as_str().to_string());
            }
        }
        Err(ProviderError::Parse("No iframe src found".into()))
    }

    async fn extract_stream_url(&self, iframe_url: &str) -> ProviderResult<StreamUrl> {
        static SCRIPT_SEL: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse("script").unwrap());

        let resp = self.client.get(iframe_url).send().await?;
        let html = resp.text().await?;
        let doc = Html::parse_document(&html);
        let script = doc
            .select(&SCRIPT_SEL)
            .map(|el| el.inner_html())
            .collect::<Vec<_>>()
            .join("\n");

        let url = super::parse_vixcloud_hls(&script)
            .ok_or_else(|| ProviderError::StreamExtraction("No HLS params in script".into()))?;

        Ok(StreamUrl {
            url,
            headers: vec![("User-Agent".into(), USER_AGENT.to_string())],
        })
    }
}

#[async_trait]
impl Provider for StreamingCommunityProvider {
    fn name(&self) -> &'static str {
        "StreamingCommunity"
    }

    async fn init(&self) {
        self.resolve_domain().await;
    }

    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>> {
        let version = self.fetch_inertia_version().await?;
        let mut all = Vec::new();
        let mut seen = HashSet::new();
        for lang in Self::LANGS {
            if let Ok(entries) = self.search_lang(query, lang, &version).await {
                for e in entries {
                    if seen.insert(e.id) {
                        all.push(e);
                    }
                }
            }
        }
        Ok(all)
    }

    async fn get_seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<Season>> {
        let (page, _) = self.fetch_title_page(entry).await?;
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
    }

    async fn get_episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>> {
        let (_, version) = self.fetch_title_page(entry).await?;
        let lang = Self::entry_lang(entry);
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
            .await?;
        let json: serde_json::Value = resp.json().await?;
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
                    image_url: self.extract_image_url(ep),
                })
            })
            .collect();
        eps.sort_unstable_by_key(|e| e.number);
        Ok(eps)
    }

    async fn get_stream_url(
        &self,
        entry: &MediaEntry,
        episode: Option<&Episode>,
        _season: Option<u32>,
    ) -> ProviderResult<StreamUrl> {
        let lang = Self::entry_lang(entry);
        let (page, _) = self.fetch_title_page(entry).await?;
        let media_id = page["props"]["title"]["id"].as_u64().unwrap_or(entry.id);
        let ep_id = episode.map(|e| e.id);
        let iframe = self.fetch_iframe_url(lang, media_id, ep_id).await?;
        self.extract_stream_url(&iframe).await
    }

    async fn get_catalog(&self, limit: usize) -> ProviderResult<Vec<MediaEntry>> {
        self.ensure_base_url().await;
        let resp = self.client.get(self.base_url()).send().await?;
        let html = resp.text().await?;
        let page = Self::parse_data_page(&html)?;
        let mut entries = Vec::new();
        let mut seen = HashSet::new();

        'sliders: for slider in page["props"]["sliders"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if let Some(titles) = slider["titles"].as_array() {
                for t in titles {
                    if let Some(mut e) = self.parse_result(t) {
                        e.language = Self::LANG.to_string();
                        if seen.insert(e.id) {
                            entries.push(e);
                            if entries.len() >= limit {
                                break 'sliders;
                            }
                        }
                    }
                }
            }
        }

        if entries.is_empty() {
            for t in page["props"]["titles"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                if let Some(mut e) = self.parse_result(t) {
                    e.language = Self::LANG.to_string();
                    if seen.insert(e.id) {
                        entries.push(e);
                        if entries.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }

        let genre_configs: Vec<serde_json::Value> = page["props"]["genres"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|g| {
                        let name = g["name"].as_str()?;
                        Some(serde_json::json!({ "name": "genre", "genre": name }))
                    })
                    .collect()
            })
            .unwrap_or_default();

        if entries.len() < limit && !genre_configs.is_empty() {
            let payload = serde_json::json!({ "sliders": genre_configs });
            'outer: for lang in Self::LANGS {
                let url = format!("{}/api/sliders/fetch?lang={}", self.base_url(), lang);
                if let Ok(resp) = self.client.post(&url).json(&payload).send().await {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        let api_sliders = json.as_array().cloned().unwrap_or_default();
                        for slider in &api_sliders {
                            if let Some(titles) = slider["titles"].as_array() {
                                for t in titles {
                                    if let Some(mut e) = self.parse_result(t) {
                                        e.language = lang.to_string();
                                        if seen.insert(e.id) {
                                            entries.push(e);
                                        }
                                    }
                                }
                            }
                            if entries.len() >= limit {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        entries.truncate(limit);
        Ok(entries)
    }
}
