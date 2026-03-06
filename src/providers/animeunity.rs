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
use url::Url;

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(val);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub struct AnimeUnityProvider {
    client: Client,
    base_url: RwLock<String>,
}

impl AnimeUnityProvider {
    pub fn with_config(timeout: u64) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(timeout))
            .build()
            .expect("reqwest client");
        Self {
            client,
            base_url: RwLock::new(String::new()),
        }
    }

    fn base_url(&self) -> String {
        self.base_url.read().unwrap().clone()
    }

    async fn resolve_domain(&self) {
        super::resolve_domain_url(&self.client, "animeunity", &self.base_url).await;
    }

    fn cookie_header(auth: &(String, String)) -> String {
        format!("XSRF-TOKEN={}; animeunity_session={}", auth.0, auth.1)
    }

    fn collect_records(
        json: &serde_json::Value,
        seen: &mut HashSet<u64>,
        entries: &mut Vec<MediaEntry>,
    ) {
        for record in json["records"].as_array().unwrap_or(&Vec::new()) {
            if let Some(e) = Self::parse_record(record) {
                if seen.insert(e.id) {
                    entries.push(e);
                }
            }
        }
    }

    async fn get_auth(&self) -> ProviderResult<(String, String)> {
        let base = self.base_url();
        let resp = self.client.get(&base).send().await?;
        let mut xsrf = String::new();
        let mut session = String::new();
        for header_val in resp.headers().get_all("set-cookie") {
            let s = header_val.to_str().unwrap_or("");
            if let Some(val) = s.strip_prefix("XSRF-TOKEN=") {
                xsrf = percent_decode(val.split(';').next().unwrap_or(""));
            } else if let Some(val) = s.strip_prefix("animeunity_session=") {
                session = percent_decode(val.split(';').next().unwrap_or(""));
            }
        }
        if xsrf.is_empty() {
            return Err(ProviderError::Network("Failed to get XSRF token".into()));
        }
        Ok((xsrf, session))
    }

    fn parse_record(record: &serde_json::Value) -> Option<MediaEntry> {
        let id = record["id"].as_u64()?;
        let slug = record["slug"].as_str().unwrap_or("").to_string();
        let name = record["title_eng"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| record["title"].as_str().filter(|s| !s.is_empty()))
            .or_else(|| record["title_it"].as_str().filter(|s| !s.is_empty()))?
            .to_string();
        let media_type = match record["type"].as_str() {
            Some("Movie") | Some("Film") => MediaType::Movie,
            _ => MediaType::Series,
        };
        let image_url = record["imageurl"].as_str().map(String::from);
        let episodes_count = record["episodes_count"].as_u64();
        let score = record["score"]
            .as_str()
            .map(String::from)
            .or_else(|| record["score"].as_f64().map(|s| format!("{s:.1}")));
        let description = record["plot"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(String::from);
        let year = record["date"]
            .as_str()
            .and_then(|d| d.split('-').next())
            .map(String::from);

        Some(MediaEntry {
            id,
            name,
            slug: format!("{id}:{slug}:{}", episodes_count.unwrap_or(0)),
            media_type,
            year,
            image_url,
            description,
            score,
            provider: 0,
            language: "ja".to_string(),
        })
    }

    fn parse_slug(slug: &str) -> (u64, u64) {
        let parts: Vec<&str> = slug.split(':').collect();
        let media_id = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let ep_count = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        (media_id, ep_count)
    }

    async fn fetch_episodes_batch(
        &self,
        media_id: u64,
        start: u64,
        end: u64,
        auth: &(String, String),
    ) -> ProviderResult<Vec<serde_json::Value>> {
        let base = self.base_url();
        let url = format!("{base}/info_api/{media_id}/1");
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("start_range", start.to_string()),
                ("end_range", end.to_string()),
            ])
            .header("cookie", Self::cookie_header(&auth))
            .header("x-xsrf-token", &auth.0)
            .header("referer", format!("{base}/"))
            .send()
            .await?;
        let json: serde_json::Value = resp.json().await?;
        Ok(json["episodes"].as_array().cloned().unwrap_or_default())
    }

    async fn get_embed_url(
        &self,
        episode_id: u64,
        auth: &(String, String),
    ) -> ProviderResult<String> {
        let base = self.base_url();
        let resp = self
            .client
            .get(format!("{base}/embed-url/{episode_id}"))
            .header("cookie", Self::cookie_header(&auth))
            .header("x-xsrf-token", &auth.0)
            .header("referer", format!("{base}/"))
            .send()
            .await?;
        let text = resp.text().await?;
        let url = text.trim().to_string();
        if url.is_empty() || !url.starts_with("http") {
            return Err(ProviderError::StreamExtraction("Empty embed URL".into()));
        }
        Ok(url)
    }

    fn extract_mp4(html: &str) -> Option<String> {
        static SEL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("body script").unwrap());
        let doc = Html::parse_document(html);
        let scripts: Vec<_> = doc.select(&SEL).collect();
        if scripts.len() < 2 {
            return None;
        }
        let text: String = scripts[1].text().collect();
        let url = text
            .split(" = ")
            .nth(1)?
            .replace('\'', "")
            .trim()
            .to_string();
        if url.starts_with("http") {
            Some(url)
        } else {
            None
        }
    }

    fn extract_hls(html: &str) -> Option<String> {
        static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"(?:['"]token['"]|token)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
        });
        static EXPIRES_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"(?:['"]expires['"]|expires)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
        });
        static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"(?:['"]url['"]|url)\s*:\s*['"](?P<url>https?://[^'"]+)['"]"#).unwrap()
        });
        static FHD_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"window\.canPlayFHD\s*=\s*(true|false)"#).unwrap());

        static SEL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("body script").unwrap());
        let doc = Html::parse_document(html);
        let script_el = doc.select(&SEL).next()?;
        let script: String = script_el.text().collect();

        let token = TOKEN_RE.captures(&script)?.get(1)?.as_str();
        let expires = EXPIRES_RE.captures(&script)?.get(1)?.as_str();
        let base_url_str = URL_RE.captures(&script)?.get(1)?.as_str();
        let can_fhd = FHD_RE
            .captures(&script)
            .map(|c| c.get(1).map(|m| m.as_str()) == Some("true"))
            .unwrap_or(false);

        let mut parsed = Url::parse(base_url_str).ok()?;
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
}

#[async_trait]
impl Provider for AnimeUnityProvider {
    async fn init(&self) {
        self.resolve_domain().await;
    }

    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>> {
        let base = self.base_url();
        if base.is_empty() {
            return Err(ProviderError::Network(
                "AnimeUnity domain not resolved".into(),
            ));
        }
        let auth = self.get_auth().await?;
        let cookie = Self::cookie_header(&auth);
        let mut seen = HashSet::new();
        let mut entries = Vec::new();

        let form_body = format!(
            "title={}",
            url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>()
        );
        let resp = self
            .client
            .post(format!("{base}/livesearch"))
            .header("cookie", &cookie)
            .header("x-xsrf-token", &auth.0)
            .header("origin", &base)
            .header("referer", format!("{base}/"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await?;
        let json: serde_json::Value = resp.json().await?;
        Self::collect_records(&json, &mut seen, &mut entries);

        let body = serde_json::json!({
            "title": query,
            "type": false,
            "year": false,
            "order": false,
            "status": false,
            "genres": false,
            "offset": 0,
            "dubbed": false,
            "season": false
        });
        let resp = self
            .client
            .post(format!("{base}/archivio/get-animes"))
            .header("cookie", &cookie)
            .header("x-xsrf-token", &auth.0)
            .header("origin", &base)
            .header("referer", format!("{base}/"))
            .json(&body)
            .send()
            .await?;
        let json: serde_json::Value = resp.json().await?;
        Self::collect_records(&json, &mut seen, &mut entries);

        Ok(entries)
    }

    async fn get_seasons(&self, _entry: &MediaEntry) -> ProviderResult<Vec<Season>> {
        Ok(vec![Season {
            id: 1,
            number: 1,
            name: Some("Season 1".into()),
        }])
    }

    async fn get_episodes(&self, entry: &MediaEntry, _season: u32) -> ProviderResult<Vec<Episode>> {
        let (media_id, ep_count) = Self::parse_slug(&entry.slug);
        let auth = self.get_auth().await?;

        let total = if ep_count > 0 {
            ep_count
        } else {
            let base = self.base_url();
            let resp = self
                .client
                .get(format!("{base}/info_api/{media_id}/"))
                .header("cookie", Self::cookie_header(&auth))
                .header("x-xsrf-token", &auth.0)
                .header("referer", format!("{base}/"))
                .send()
                .await?;
            let json: serde_json::Value = resp.json().await?;
            json["episodes_count"]
                .as_u64()
                .ok_or_else(|| ProviderError::Parse("No episodes_count".into()))?
        };

        let mut all_eps = Vec::new();
        let mut start = 1u64;
        while start <= total {
            let end = (start + 119).min(total);
            let batch = self
                .fetch_episodes_batch(media_id, start, end, &auth)
                .await?;
            all_eps.extend(batch);
            start = end + 1;
        }

        let mut episodes: Vec<Episode> = all_eps
            .iter()
            .filter_map(|ep| {
                let id = ep["id"].as_u64()?;
                let number = ep["number"]
                    .as_f64()
                    .map(|n| n as u32)
                    .or_else(|| ep["number"].as_u64().map(|n| n as u32))
                    .unwrap_or(0);
                let name = format!("Episode {number}");
                Some(Episode {
                    id,
                    number,
                    name,
                    duration: None,
                })
            })
            .collect();

        episodes.sort_unstable_by_key(|e| e.number);
        Ok(episodes)
    }

    async fn get_stream_url(
        &self,
        _entry: &MediaEntry,
        episode: Option<&Episode>,
        _season: Option<u32>,
    ) -> ProviderResult<StreamUrl> {
        let ep =
            episode.ok_or_else(|| ProviderError::StreamExtraction("Episode required".into()))?;
        let auth = self.get_auth().await?;
        let embed_url = self.get_embed_url(ep.id, &auth).await?;

        let resp = self.client.get(&embed_url).send().await?;
        let html = resp.text().await?;

        if let Some(mp4) = Self::extract_mp4(&html) {
            return Ok(StreamUrl {
                url: mp4,
                headers: Vec::new(),
            });
        }

        if let Some(hls) = Self::extract_hls(&html) {
            return Ok(StreamUrl {
                url: hls,
                headers: Vec::new(),
            });
        }

        Err(ProviderError::StreamExtraction(
            "Could not extract stream URL from embed page".into(),
        ))
    }

    async fn get_catalog(&self, limit: usize) -> ProviderResult<Vec<MediaEntry>> {
        let base = self.base_url();
        if base.is_empty() {
            return Ok(Vec::new());
        }
        let auth = match self.get_auth().await {
            Ok(a) => a,
            Err(_) => return Ok(Vec::new()),
        };
        let cookie = Self::cookie_header(&auth);

        let body = serde_json::json!({
            "title": "",
            "type": false,
            "year": false,
            "order": "Popolarità",
            "status": false,
            "genres": false,
            "offset": 0,
            "dubbed": false,
            "season": false
        });
        let resp = self
            .client
            .post(format!("{base}/archivio/get-animes"))
            .header("cookie", &cookie)
            .header("x-xsrf-token", &auth.0)
            .header("origin", &base)
            .header("referer", format!("{base}/"))
            .json(&body)
            .send()
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(_) => return Ok(Vec::new()),
        };
        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(_) => return Ok(Vec::new()),
        };

        let mut entries = Vec::new();
        for record in json["records"].as_array().unwrap_or(&Vec::new()) {
            if let Some(e) = Self::parse_record(record) {
                entries.push(e);
                if entries.len() >= limit {
                    break;
                }
            }
        }
        Ok(entries)
    }
}
