use super::{
    Episode, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, Season, StreamUrl,
    USER_AGENT,
};
use async_trait::async_trait;
use reqwest::{Client, RequestBuilder};
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const AUTH_TTL: Duration = Duration::from_secs(300);

static SCRIPT_SEL: LazyLock<Selector> = LazyLock::new(|| Selector::parse("body script").unwrap());

struct Auth {
    xsrf: String,
    session: String,
    at: Instant,
}

pub struct AnimeUnityProvider {
    client: Client,
    base_url: RwLock<String>,
    auth: Mutex<Option<Auth>>,
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
            auth: Mutex::new(None),
        }
    }

    fn base_url(&self) -> String {
        self.base_url.read().unwrap().clone()
    }

    async fn resolve_domain(&self) {
        super::resolve_domain_url(&self.client, "animeunity", &self.base_url).await;
    }

    async fn ensure_auth(&self) -> ProviderResult<(String, String)> {
        let mut guard = self.auth.lock().await;
        if let Some(ref a) = *guard {
            if a.at.elapsed() < AUTH_TTL {
                return Ok((a.xsrf.clone(), a.session.clone()));
            }
        }
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
        *guard = Some(Auth {
            xsrf: xsrf.clone(),
            session: session.clone(),
            at: Instant::now(),
        });
        Ok((xsrf, session))
    }

    fn auth_request(&self, builder: RequestBuilder, auth: &(String, String)) -> RequestBuilder {
        let base = self.base_url();
        builder
            .header(
                "cookie",
                format!("XSRF-TOKEN={}; animeunity_session={}", auth.0, auth.1),
            )
            .header("x-xsrf-token", &auth.0)
            .header("origin", &base)
            .header("referer", format!("{base}/"))
    }

    fn parse_record(record: &serde_json::Value) -> Option<MediaEntry> {
        let id = record["id"].as_u64()?;
        let slug = record["slug"].as_str().unwrap_or("").to_string();
        let title_eng = record["title_eng"].as_str().filter(|s| !s.is_empty());
        let title = record["title"].as_str().filter(|s| !s.is_empty());
        let title_it = record["title_it"].as_str().filter(|s| !s.is_empty());
        let name = title_eng.or(title).or(title_it)?.to_string();
        let media_type = match record["type"].as_str() {
            Some("Movie") | Some("Film") => MediaType::Movie,
            _ => MediaType::Series,
        };
        let alternative_names = [title_eng, title, title_it]
            .into_iter()
            .flatten()
            .filter(|t| *t != name)
            .map(|t| t.to_string())
            .collect();
        Some(MediaEntry {
            id,
            name,
            alternative_names,
            slug: format!(
                "{id}:{slug}:{}",
                record["episodes_count"].as_u64().unwrap_or(0)
            ),
            media_type,
            year: record["date"]
                .as_str()
                .and_then(|d| d.split('-').next())
                .map(String::from),
            image_url: record["imageurl"].as_str().map(String::from),
            description: record["plot"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(String::from),
            score: record["score"]
                .as_str()
                .map(String::from)
                .or_else(|| record["score"].as_f64().map(|s| format!("{s:.1}"))),
            provider: 0,
            language: "ja".to_string(),
        })
    }

    fn parse_slug(slug: &str) -> (u64, u64) {
        let mut parts = slug.split(':');
        let media_id = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let ep_count = parts.nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        (media_id, ep_count)
    }

    fn extract_stream(html: &str) -> Option<String> {
        let doc = Html::parse_document(html);
        let scripts: Vec<String> = doc
            .select(&SCRIPT_SEL)
            .map(|el| el.text().collect())
            .collect();

        if scripts.len() >= 2 {
            if let Some(url) = scripts[1].split(" = ").nth(1) {
                let mp4 = url.replace('\'', "").trim().to_string();
                if mp4.starts_with("http") {
                    return Some(mp4);
                }
            }
        }

        let script = scripts.first()?;
        Self::build_hls_url(script)
    }

    fn build_hls_url(script: &str) -> Option<String> {
        super::parse_vixcloud_hls(script)
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
        let auth = self.ensure_auth().await?;
        let mut seen = HashSet::new();
        let mut entries = Vec::new();

        let form_body = format!(
            "title={}",
            url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>()
        );
        let req = self
            .client
            .post(format!("{base}/livesearch"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form_body);
        let resp = self.auth_request(req, &auth).send().await?;
        let json: serde_json::Value = resp.json().await?;
        for record in json["records"].as_array().unwrap_or(&Vec::new()) {
            if let Some(e) = Self::parse_record(record) {
                if seen.insert(e.id) {
                    entries.push(e);
                }
            }
        }

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
        let req = self
            .client
            .post(format!("{base}/archivio/get-animes"))
            .json(&body);
        let resp = self.auth_request(req, &auth).send().await?;
        let json: serde_json::Value = resp.json().await?;
        for record in json["records"].as_array().unwrap_or(&Vec::new()) {
            if let Some(e) = Self::parse_record(record) {
                if seen.insert(e.id) {
                    entries.push(e);
                }
            }
        }

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
        let auth = self.ensure_auth().await?;
        let base = self.base_url();

        let total = if ep_count > 0 {
            ep_count
        } else {
            let req = self.client.get(format!("{base}/info_api/{media_id}/"));
            let resp = self.auth_request(req, &auth).send().await?;
            let json: serde_json::Value = resp.json().await?;
            json["episodes_count"]
                .as_u64()
                .ok_or_else(|| ProviderError::Parse("No episodes_count".into()))?
        };

        let mut all_eps = Vec::new();
        let mut start = 1u64;
        while start <= total {
            let end = (start + 119).min(total);
            let req = self
                .client
                .get(format!("{base}/info_api/{media_id}/1"))
                .query(&[
                    ("start_range", start.to_string()),
                    ("end_range", end.to_string()),
                ]);
            let resp = self.auth_request(req, &auth).send().await?;
            let json: serde_json::Value = resp.json().await?;
            all_eps.extend(json["episodes"].as_array().cloned().unwrap_or_default());
            start = end + 1;
        }

        let mut episodes: Vec<Episode> = all_eps
            .iter()
            .enumerate()
            .filter_map(|(idx, ep)| {
                let id = ep["id"].as_u64()?;
                let number = parse_number(&ep["number"]).unwrap_or((idx + 1) as u32);
                let name = ep["title"]
                    .as_str()
                    .or_else(|| ep["title_it"].as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| format!("Episode {number}"));
                Some(Episode {
                    id,
                    number,
                    name,
                    duration: None,
                    image_url: ep["imageurl"].as_str().map(String::from),
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
        let auth = self.ensure_auth().await?;
        let base = self.base_url();

        let req = self.client.get(format!("{base}/embed-url/{}", ep.id));
        let resp = self.auth_request(req, &auth).send().await?;
        let embed_url = resp.text().await?.trim().to_string();
        if !embed_url.starts_with("http") {
            return Err(ProviderError::StreamExtraction("Empty embed URL".into()));
        }

        let html = self.client.get(&embed_url).send().await?.text().await?;
        Self::extract_stream(&html)
            .map(|url| StreamUrl {
                url,
                headers: Vec::new(),
            })
            .ok_or_else(|| ProviderError::StreamExtraction("Could not extract stream URL".into()))
    }

    async fn get_catalog(&self, limit: usize) -> ProviderResult<Vec<MediaEntry>> {
        let base = self.base_url();
        if base.is_empty() {
            return Ok(Vec::new());
        }
        let auth = match self.ensure_auth().await {
            Ok(a) => a,
            Err(_) => return Ok(Vec::new()),
        };

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
        let req = self
            .client
            .post(format!("{base}/archivio/get-animes"))
            .json(&body);
        let resp = match self.auth_request(req, &auth).send().await {
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

fn parse_number(val: &serde_json::Value) -> Option<u32> {
    val.as_u64()
        .map(|n| n as u32)
        .or_else(|| val.as_f64().map(|n| n as u32))
        .or_else(|| {
            val.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|n| n as u32)
        })
}
