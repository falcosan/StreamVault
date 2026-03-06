use super::{
    Episode, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, Season, StreamUrl,
    USER_AGENT,
};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::{BTreeSet, HashMap, HashSet};
use tokio::sync::Mutex;

const AURORA_BASE: &str = "https://public.aurora.enhanced.live";
const NOVE_ENV: &str = "nove";
const PLAYBACK_URL: &str = "https://public.aurora.enhanced.live/playback/v3/videoPlaybackInfo";
const DPLAY_PLAYBACK_URL: &str = "https://eu1-prod.disco-api.com/playback/v3/videoPlaybackInfo";

fn nove_hash(s: &str) -> u64 {
    s.bytes()
        .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64))
}

fn build_show_url(slug: &str, parent_slug: &str) -> String {
    let normalized = slug.to_lowercase().replace(' ', "-");
    format!(
        "{AURORA_BASE}/site/page/{normalized}/?include=default&filter[environment]={NOVE_ENV}&v=2&parent_slug={parent_slug}"
    )
}

fn video_id_from_json(ep: &serde_json::Value) -> Option<String> {
    ep["id"]
        .as_str()
        .map(String::from)
        .or_else(|| ep["id"].as_u64().map(|n| n.to_string()))
        .or_else(|| ep["id"].as_i64().map(|n| n.to_string()))
}

fn channel_from_json(ep: &serde_json::Value) -> String {
    if ep["channel"].is_null() {
        "X-REALM-IT".to_string()
    } else {
        "X-REALM-DPLAY".to_string()
    }
}

pub struct NoveProvider {
    client: Client,
    show_data: Mutex<HashMap<u64, Vec<serde_json::Value>>>,
    episode_data: Mutex<HashMap<u64, (String, String)>>,
}

impl NoveProvider {
    pub fn with_config(timeout: u64) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(timeout))
            .build()
            .expect("reqwest client");
        Self {
            client,
            show_data: Mutex::new(HashMap::new()),
            episode_data: Mutex::new(HashMap::new()),
        }
    }

    async fn get_bearer_tokens(&self) -> ProviderResult<HashMap<String, (String, String)>> {
        let url = format!(
            "{AURORA_BASE}/site/page/homepage/?include=default&filter[environment]=realtime&v=2"
        );
        let resp = self.client.get(&url).send().await?;
        let json: serde_json::Value = resp.json().await?;
        let realm = &json["userMeta"]["realm"];
        let mut tokens = HashMap::new();
        if let Some(key) = realm["X-REALM-IT"].as_str() {
            tokens.insert(
                "X-REALM-IT".to_string(),
                (PLAYBACK_URL.to_string(), key.to_string()),
            );
        }
        if let Some(key) = realm["X-REALM-DPLAY"].as_str() {
            tokens.insert(
                "X-REALM-DPLAY".to_string(),
                (DPLAY_PLAYBACK_URL.to_string(), key.to_string()),
            );
        }
        if tokens.is_empty() {
            return Err(ProviderError::StreamExtraction(
                "No bearer tokens found".into(),
            ));
        }
        Ok(tokens)
    }
}

#[async_trait]
impl Provider for NoveProvider {
    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>> {
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let url = format!(
            "{AURORA_BASE}/site/search/page/?include=default&filter[environment]={NOVE_ENV}&v=2&q={encoded}&page[number]=1&page[size]=20"
        );
        let resp = self.client.get(&url).send().await?;
        let json: serde_json::Value = resp.json().await?;
        let data = json["data"]
            .as_array()
            .ok_or_else(|| ProviderError::Parse("No data array in search response".into()))?;

        let mut entries = Vec::new();
        for item in data {
            if item["type"].as_str() != Some("showpage") {
                continue;
            }
            let title = match item["title"].as_str() {
                Some(t) if !t.is_empty() => t.to_string(),
                _ => continue,
            };
            let slug = item["slug"].as_str().unwrap_or("");
            let parent_slug = item["parentSlug"].as_str().unwrap_or("");
            let show_url = build_show_url(slug, parent_slug);
            let year = item["dateLastModified"]
                .as_str()
                .and_then(|d| d.split('-').next())
                .map(String::from);
            let image_url = item["image"]["url"].as_str().map(String::from);

            entries.push(MediaEntry {
                id: nove_hash(&show_url),
                name: title,
                slug: show_url,
                media_type: MediaType::Series,
                year,
                image_url,
                description: None,
                score: None,
                provider: 0,
                language: "it".to_string(),
            });
        }
        Ok(entries)
    }

    async fn get_seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<Season>> {
        let resp = self.client.get(&entry.slug).send().await?;
        let json: serde_json::Value = resp.json().await?;
        let blocks = json["blocks"]
            .as_array()
            .ok_or_else(|| ProviderError::Parse("No blocks in response".into()))?;
        let items = blocks
            .get(1)
            .and_then(|b| b["items"].as_array())
            .ok_or_else(|| ProviderError::Parse("No items in blocks[1]".into()))?;

        self.show_data.lock().await.insert(entry.id, items.clone());

        let mut season_nums = BTreeSet::new();
        for ep in items {
            if let Some(n) = ep["seasonNumber"].as_u64() {
                season_nums.insert(n as u32);
            }
        }

        Ok(season_nums
            .into_iter()
            .map(|num| Season {
                id: num as u64,
                number: num,
                name: Some(format!("Season {num}")),
            })
            .collect())
    }

    async fn get_episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>> {
        let lock = self.show_data.lock().await;
        let all_episodes = lock
            .get(&entry.id)
            .ok_or_else(|| ProviderError::Parse("Show data not cached".into()))?;

        let mut episodes = Vec::new();
        let mut ep_map = HashMap::new();

        for ep in all_episodes {
            let ep_season = ep["seasonNumber"].as_u64().unwrap_or(0) as u32;
            if ep_season != season {
                continue;
            }
            let video_id = match video_id_from_json(ep) {
                Some(id) => id,
                None => continue,
            };
            let channel = channel_from_json(ep);
            let ep_num = ep["episodeNumber"].as_u64().unwrap_or(0) as u32;
            let name = ep["title"].as_str().unwrap_or("").to_string();
            let name = if name.is_empty() {
                format!("Episode {ep_num}")
            } else {
                name
            };
            let duration_ms = ep["videoDuration"].as_u64().unwrap_or(0);
            let duration = if duration_ms > 0 {
                Some((duration_ms / 1000 / 60) as u32)
            } else {
                None
            };

            let ep_id = nove_hash(&format!("{}-{}-{}", entry.id, season, video_id));
            ep_map.insert(ep_id, (video_id, channel));

            episodes.push(Episode {
                id: ep_id,
                number: ep_num,
                name,
                duration,
            });
        }
        drop(lock);

        self.episode_data.lock().await.extend(ep_map);
        episodes.sort_unstable_by_key(|e| e.number);
        Ok(episodes)
    }

    async fn get_stream_url(
        &self,
        _entry: &MediaEntry,
        episode: Option<&Episode>,
        _season: Option<u32>,
    ) -> ProviderResult<StreamUrl> {
        let ep = episode
            .ok_or_else(|| ProviderError::StreamExtraction("Episode required for Nove".into()))?;
        let (video_id, channel) = self
            .episode_data
            .lock()
            .await
            .get(&ep.id)
            .cloned()
            .ok_or_else(|| ProviderError::StreamExtraction("Episode data not cached".into()))?;

        let tokens = self.get_bearer_tokens().await?;
        let (endpoint, key) = tokens.get(&channel).ok_or_else(|| {
            ProviderError::StreamExtraction(format!("No token for channel {channel}"))
        })?;

        let body = serde_json::json!({
            "deviceInfo": {
                "adBlocker": false,
                "drmSupported": true
            },
            "videoId": video_id
        });

        let resp = self
            .client
            .post(endpoint)
            .header("authorization", format!("Bearer {key}"))
            .json(&body)
            .send()
            .await?;

        let json: serde_json::Value = resp.json().await?;
        let stream = json["data"]["attributes"]["streaming"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v["url"].as_str())
            .ok_or_else(|| {
                ProviderError::StreamExtraction("No streaming URL in response".into())
            })?;

        Ok(StreamUrl {
            url: stream.to_string(),
            headers: Vec::new(),
        })
    }

    async fn get_catalog(&self, limit: usize) -> ProviderResult<Vec<MediaEntry>> {
        let url = format!(
            "{AURORA_BASE}/site/page/homepage/?include=default&filter[environment]={NOVE_ENV}&v=2"
        );
        let resp = self.client.get(&url).send().await?;
        let json: serde_json::Value = resp.json().await?;
        let blocks = json["blocks"].as_array().unwrap_or(&Vec::new()).clone();

        let mut entries = Vec::new();
        let mut seen = HashSet::new();

        for block in &blocks {
            if block["type"].as_str() != Some("collectionBlock") {
                continue;
            }
            if block["itemsType"].as_str() != Some("showpage") {
                continue;
            }
            let items = match block["items"].as_array() {
                Some(items) => items,
                None => continue,
            };
            for item in items {
                if item["pageType"].as_str() != Some("showpage") {
                    continue;
                }
                let title = match item["title"].as_str().filter(|t| !t.is_empty()) {
                    Some(t) => t.to_string(),
                    None => continue,
                };
                let slug = match item["url"].as_str().filter(|s| !s.is_empty()) {
                    Some(s) => s,
                    None => continue,
                };
                let parent_slug = item["parentUrl"].as_str().unwrap_or("");
                let show_url = build_show_url(slug, parent_slug);
                let id = nove_hash(&show_url);
                if !seen.insert(id) {
                    continue;
                }
                let image_url = item["image"]["url"].as_str().map(String::from);

                entries.push(MediaEntry {
                    id,
                    name: title,
                    slug: show_url,
                    media_type: MediaType::Series,
                    year: None,
                    image_url,
                    description: item["description"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                    score: None,
                    provider: 0,
                    language: "it".to_string(),
                });
                if entries.len() >= limit {
                    return Ok(entries);
                }
            }
        }
        Ok(entries)
    }
}
