use super::{
    Episode, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, Season, StreamUrl,
    USER_AGENT,
};
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

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
            .user_agent(USER_AGENT)
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
            provider_language: String::new(),
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
        let resp = self.client.get(&json_url).send().await?;
        let json: serde_json::Value = resp.json().await?;

        let video_json = if let Some(first_item) = json["first_item_path"].as_str() {
            let item_url = format!("{}.json", raiplay_abs_url(first_item));
            let resp = self.client.get(&item_url).send().await?;
            resp.json().await?
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
        let resp = self.client.get(&relinker_url).send().await?;
        let bytes = resp.bytes().await?;
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

#[async_trait]
impl Provider for RaiPlayProvider {
    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>> {
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
        let resp = self.client.post(RAIPLAY_SEARCH).json(&body).send().await?;
        let json: serde_json::Value = resp.json().await?;
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
    }

    async fn get_seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<Season>> {
        let path = entry.slug.trim_start_matches('/');
        let url = format!("{RAIPLAY_BASE}/{path}");
        let resp = self.client.get(&url).send().await?;
        let json: serde_json::Value = resp.json().await?;
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
    }

    async fn get_episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>> {
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
        let resp = self.client.get(&url).send().await?;
        let json: serde_json::Value = resp.json().await?;

        let items: Vec<serde_json::Value> = if let Some(seasons_arr) = json["seasons"].as_array() {
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
    }

    async fn get_stream_url(
        &self,
        entry: &MediaEntry,
        episode: Option<&Episode>,
        _season: Option<u32>,
    ) -> ProviderResult<StreamUrl> {
        let page_url = if let Some(ep) = episode {
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
    }

    async fn get_catalog(&self) -> ProviderResult<Vec<MediaEntry>> {
        Ok(Vec::new())
    }
}
