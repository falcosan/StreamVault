use super::{
    Episode, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, Season, StreamUrl,
    DOMAINS_URL, USER_AGENT,
};
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::sync::{LazyLock, OnceLock, RwLock};
use url::Url;

pub struct StreamingCommunityProvider {
    client: Client,
    base_url: RwLock<String>,
}

impl StreamingCommunityProvider {
    const LANG: &str = "it";
    const LANGS: &[&str] = &["it", "en"];
    pub(crate) const FALLBACK_URL: &str = "https://streamingcommunityz.name";
    const IMG_PRIORITY: &[&str] = &["poster", "cover", "cover_mobile", "background"];

    pub fn with_config(timeout: u64) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
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
        let resp = match self.client.get(DOMAINS_URL).send().await {
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
                entry.provider_language = lang.to_string();
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
            provider_language: String::new(),
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

    fn entry_lang(entry: &MediaEntry) -> &str {
        if entry.provider_language.is_empty() {
            Self::LANG
        } else {
            &entry.provider_language
        }
    }

    async fn fetch_title_page(
        &self,
        entry: &MediaEntry,
    ) -> ProviderResult<(serde_json::Value, String)> {
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
        let base = self.base_url();
        let url = match ep_id {
            Some(eid) => {
                format!("{base}/{lang}/iframe/{media_id}?episode_id={eid}&next_episode=1")
            }
            None => format!("{base}/{lang}/iframe/{media_id}"),
        };
        let resp = self.client.get(&url).send().await?;
        let html = resp.text().await?;
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

        let resp = self.client.get(iframe_url).send().await?;
        let html = resp.text().await?;
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

#[async_trait]
impl Provider for StreamingCommunityProvider {
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

    async fn get_catalog(&self) -> ProviderResult<Vec<MediaEntry>> {
        let resp = self.client.get(self.base_url()).send().await?;
        let html = resp.text().await?;
        let page = Self::parse_data_page(&html)?;
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        if let Some(sliders) = page["props"]["sliders"].as_array() {
            for slider in sliders {
                if let Some(titles) = slider["titles"].as_array() {
                    for t in titles {
                        if let Some(mut e) = self.parse_result(t) {
                            e.provider_language = Self::LANG.to_string();
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
                    if let Some(mut e) = self.parse_result(t) {
                        e.provider_language = Self::LANG.to_string();
                        if seen.insert(e.id) {
                            entries.push(e);
                        }
                    }
                }
            }
        }
        Ok(entries)
    }
}
