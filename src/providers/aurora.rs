use super::{
    Episode, MediaEntry, Provider, ProviderError, ProviderResult, StreamUrl, fetch_json, items,
    stable_hash,
};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};

const BASE: &str = "https://public.aurora.enhanced.live";
const SHOW_BLOCKS: [&str; 2] = ["sonicShowBlock", "sonicPlaylistBlock"];

pub struct Aurora {
    client: Client,
    env: &'static str,
    name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Realm {
    It,
    Dplay,
}

impl Realm {
    fn key(self) -> &'static str {
        match self {
            Self::It => "X-REALM-IT",
            Self::Dplay => "X-REALM-DPLAY",
        }
    }

    fn playback(self) -> &'static str {
        match self {
            Self::It => "https://public.aurora.enhanced.live/playback/v3/videoPlaybackInfo",
            Self::Dplay => "https://eu1-prod.disco-api.com/playback/v3/videoPlaybackInfo",
        }
    }
}

#[derive(Debug)]
struct Video {
    season: u32,
    episode: Episode,
    id: String,
    realm: Realm,
}

impl Aurora {
    pub fn new(client: Client, env: &'static str, name: &'static str) -> Self {
        Self { client, env, name }
    }

    fn show(&self, title: &str, slug: &str, parent: &str) -> MediaEntry {
        let url = format!(
            "{BASE}/site/page/{}/?include=default&filter[environment]={}&v=2&parent_slug={parent}",
            slug.to_lowercase().replace(' ', "-"),
            self.env
        );
        MediaEntry {
            id: stable_hash(&url),
            name: title.to_owned(),
            slug: url,
            language: "it".into(),
            ..Default::default()
        }
    }

    fn parse_search(&self, json: &Value) -> ProviderResult<Vec<MediaEntry>> {
        let data = json["data"]
            .as_array()
            .ok_or_else(|| ProviderError::Parse("No data array in search response".into()))?;
        Ok(data
            .iter()
            .filter(|item| item["type"] == "showpage")
            .filter_map(|item| {
                let title = item["title"].as_str().filter(|t| !t.is_empty())?;
                Some(MediaEntry {
                    year: item["dateLastModified"]
                        .as_str()
                        .and_then(|d| d.split('-').next())
                        .map(String::from),
                    image_url: item["image"]["url"].as_str().map(String::from),
                    ..self.show(
                        title,
                        item["slug"].as_str().unwrap_or_default(),
                        item["parentSlug"].as_str().unwrap_or_default(),
                    )
                })
            })
            .collect())
    }

    async fn videos(&self, entry: &MediaEntry) -> ProviderResult<Vec<Video>> {
        parse_videos(entry, &fetch_json(self.client.get(&entry.slug)).await?)
    }
}

fn parse_videos(entry: &MediaEntry, json: &Value) -> ProviderResult<Vec<Video>> {
    let blocks = json["blocks"]
        .as_array()
        .ok_or_else(|| ProviderError::Parse("No blocks in response".into()))?;
    let show_id = SHOW_BLOCKS.iter().find_map(|kind| {
        blocks
            .iter()
            .filter(|b| b["type"] == *kind)
            .find_map(|b| b["items"][0]["show"]["id"].as_str())
    });
    let mut seen = HashSet::new();
    Ok(blocks
        .iter()
        .filter(|b| b["type"].as_str().is_some_and(|t| SHOW_BLOCKS.contains(&t)))
        .flat_map(|b| items(&b["items"]))
        .filter(|item| show_id.is_none_or(|id| item["show"]["id"] == id))
        .filter_map(|item| {
            let id = video_id(item)?;
            let season = u32::try_from(item["seasonNumber"].as_u64()?).ok()?;
            let number = episode_number(item)?;
            seen.insert(id.clone()).then(|| Video {
                season,
                episode: Episode {
                    id: stable_hash(&format!("{}-{season}-{id}", entry.id)),
                    number,
                    name: item["title"]
                        .as_str()
                        .filter(|t| !t.is_empty())
                        .map_or_else(|| format!("Episode {number}"), String::from),
                    duration: item["videoDuration"]
                        .as_u64()
                        .filter(|ms| *ms > 0)
                        .and_then(|ms| u32::try_from(ms / 60_000).ok()),
                    image_url: ["poster", "image"]
                        .iter()
                        .find_map(|k| item[k]["src"].as_str())
                        .or_else(|| item["image"]["url"].as_str())
                        .map(String::from),
                },
                realm: if item["channel"].is_null() {
                    Realm::It
                } else {
                    Realm::Dplay
                },
                id,
            })
        })
        .collect())
}

fn video_id(item: &Value) -> Option<String> {
    match &item["id"] {
        Value::String(id) => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

fn episode_number(item: &Value) -> Option<u32> {
    if let Some(n) = item["episodeNumber"].as_u64() {
        return u32::try_from(n).ok();
    }
    item["title"]
        .as_str()?
        .strip_prefix("Episodio ")?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

#[async_trait]
impl Provider for Aurora {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>> {
        let url = format!(
            "{BASE}/site/search/page/?include=default&filter[environment]={}&v=2&page[number]=1&page[size]=20",
            self.env
        );
        self.parse_search(&fetch_json(self.client.get(url).query(&[("q", query)])).await?)
    }

    async fn seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<u32>> {
        let seasons: BTreeSet<u32> = self.videos(entry).await?.iter().map(|v| v.season).collect();
        Ok(seasons.into_iter().collect())
    }

    async fn episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>> {
        let mut episodes: Vec<Episode> = self
            .videos(entry)
            .await?
            .into_iter()
            .filter(|v| v.season == season)
            .map(|v| v.episode)
            .collect();
        episodes.sort_by_key(|e| e.number);
        Ok(episodes)
    }

    async fn stream(
        &self,
        entry: &MediaEntry,
        _season: Option<u32>,
        episode: Option<&Episode>,
    ) -> ProviderResult<StreamUrl> {
        let wanted =
            episode.ok_or_else(|| ProviderError::StreamExtraction("Episode required".into()))?;
        let home = self.client.get(format!(
            "{BASE}/site/page/homepage/?include=default&filter[environment]=realtime&v=2"
        ));
        let (videos, home) = tokio::try_join!(self.videos(entry), fetch_json(home))?;
        let video = videos
            .into_iter()
            .find(|v| v.episode.id == wanted.id)
            .ok_or_else(|| ProviderError::StreamExtraction("Episode data not found".into()))?;
        let token = home["userMeta"]["realm"][video.realm.key()]
            .as_str()
            .ok_or_else(|| {
                ProviderError::StreamExtraction(format!(
                    "No token for channel {}",
                    video.realm.key()
                ))
            })?;
        let playback = fetch_json(
            self.client
                .post(video.realm.playback())
                .bearer_auth(token)
                .json(&json!({
                    "deviceInfo": { "adBlocker": false, "drmSupported": true },
                    "videoId": video.id
                })),
        )
        .await?;
        playback_url(&playback)
    }
}

fn playback_url(playback: &Value) -> ProviderResult<StreamUrl> {
    let url = playback["data"]["attributes"]["streaming"][0]["url"]
        .as_str()
        .ok_or_else(|| ProviderError::StreamExtraction("No streaming URL in response".into()))?;
    Ok(StreamUrl {
        url: url.to_owned(),
        headers: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MediaType;

    fn aurora() -> Aurora {
        Aurora::new(Client::new(), "nove", "Nove")
    }

    #[test]
    fn show_url_and_id_are_stable() {
        let entry = aurora().show("Cash or Trash", "Cash Or Trash", "programmi");
        assert_eq!(
            entry.slug,
            "https://public.aurora.enhanced.live/site/page/cash-or-trash/?include=default&filter[environment]=nove&v=2&parent_slug=programmi"
        );
        assert_eq!(entry.id, stable_hash(&entry.slug));
        assert_eq!(entry.media_type, MediaType::Series);
    }

    #[test]
    fn parse_search_keeps_titled_show_pages() {
        let json = json!({ "data": [
            { "type": "showpage", "title": "Show", "slug": "show", "parentSlug": "p",
              "dateLastModified": "2024-05-01T00:00:00Z", "image": { "url": "https://img" } },
            { "type": "showpage", "title": "" },
            { "type": "video", "title": "Clip" }
        ]});
        let entries = aurora().parse_search(&json).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Show");
        assert_eq!(entries[0].year.as_deref(), Some("2024"));
        assert_eq!(entries[0].image_url.as_deref(), Some("https://img"));
        assert!(aurora().parse_search(&json!({})).is_err());
    }

    #[test]
    fn parse_videos_filters_to_show_and_dedupes() {
        let entry = MediaEntry {
            id: 7,
            ..Default::default()
        };
        let json = json!({ "blocks": [
            { "type": "otherBlock", "items": [{ "id": "x", "seasonNumber": 1, "episodeNumber": 1 }] },
            { "type": "sonicPlaylistBlock", "items": [
                { "id": "b", "show": { "id": "s" }, "seasonNumber": 2, "title": "Episodio 3 - Finale",
                  "channel": { "id": 1 }, "poster": { "src": "https://poster" } }
            ]},
            { "type": "sonicShowBlock", "items": [
                { "id": 11, "show": { "id": "s" }, "seasonNumber": 1, "episodeNumber": 1,
                  "videoDuration": 1_800_000, "image": { "url": "https://image" } },
                { "id": 11, "show": { "id": "s" }, "seasonNumber": 1, "episodeNumber": 1 },
                { "id": "c", "show": { "id": "other" }, "seasonNumber": 1, "episodeNumber": 2 },
                { "id": "d", "show": { "id": "s" }, "seasonNumber": 1 }
            ]}
        ]});
        let videos = parse_videos(&entry, &json).unwrap();
        assert_eq!(videos.len(), 2);
        assert_eq!((videos[0].season, videos[0].episode.number), (2, 3));
        assert_eq!(videos[0].realm, Realm::Dplay);
        assert_eq!(videos[0].episode.name, "Episodio 3 - Finale");
        assert_eq!(
            videos[0].episode.image_url.as_deref(),
            Some("https://poster")
        );
        assert_eq!(videos[1].id, "11");
        assert_eq!(videos[1].realm, Realm::It);
        assert_eq!(videos[1].episode.id, stable_hash("7-1-11"));
        assert_eq!(videos[1].episode.name, "Episode 1");
        assert_eq!(videos[1].episode.duration, Some(30));
        assert_eq!(
            videos[1].episode.image_url.as_deref(),
            Some("https://image")
        );
    }

    #[test]
    fn playback_url_reads_first_stream() {
        let response = json!({ "data": { "attributes": { "streaming": [
            { "type": "hls", "url": "https://cdn/master.m3u8" },
            { "type": "dash", "url": "https://cdn/manifest.mpd" }
        ]}}});
        assert_eq!(
            playback_url(&response).unwrap().url,
            "https://cdn/master.m3u8"
        );
        assert!(
            playback_url(&json!({ "errors": [{ "code": "access.denied.geoblocked" }] })).is_err()
        );
    }

    #[test]
    fn realms_map_to_their_playback_endpoints() {
        assert_eq!(Realm::It.key(), "X-REALM-IT");
        assert!(
            Realm::It
                .playback()
                .starts_with("https://public.aurora.enhanced.live/")
        );
        assert_eq!(Realm::Dplay.key(), "X-REALM-DPLAY");
        assert!(
            Realm::Dplay
                .playback()
                .starts_with("https://eu1-prod.disco-api.com/")
        );
    }

    #[test]
    fn parse_videos_requires_blocks() {
        assert!(parse_videos(&MediaEntry::default(), &json!({})).is_err());
    }
}
