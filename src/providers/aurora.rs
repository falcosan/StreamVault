use super::{
    Episode, Http, MediaEntry, Provider, ProviderError, ProviderResult, StreamUrl, fetch_json,
    items, stable_hash, uint,
};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};

const BASE: &str = "https://public.aurora.enhanced.live";
const SHOW_BLOCKS: [&str; 2] = ["sonicShowBlock", "sonicPlaylistBlock"];
const IMAGE_BUCKETS: &str = "https://s3.eu-west-1.amazonaws.com/";
const IMAGE_SERVICE: &str = "https://d2v9mhsiek5lbq.cloudfront.net/";
const IMAGE_WIDTH: u32 = 400;

pub struct Aurora {
    client: Http,
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

    fn playback(self) -> String {
        let host = match self {
            Self::It => BASE,
            Self::Dplay => "https://eu1-prod.disco-api.com",
        };
        format!("{host}/playback/v3/videoPlaybackInfo")
    }
}

struct Video {
    season: u32,
    episode: Episode,
    id: String,
    realm: Realm,
}

impl Video {
    fn is(&self, season: Option<u32>, wanted: &Episode) -> bool {
        Some(self.season) == season && self.episode.matches(wanted)
    }
}

impl Aurora {
    pub fn new(client: Http, env: &'static str, name: &'static str) -> Self {
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
                    image_url: item["image"]["url"].as_str().map(served_image),
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
            let season = uint(&item["seasonNumber"])?;
            let number = episode_number(item)?;
            seen.insert(id.clone()).then(|| Video {
                season,
                episode: Episode {
                    id: stable_hash(&format!("{}-{season}-{id}", entry.safe_id())),
                    number,
                    part: None,
                    name: item["title"].as_str().unwrap_or_default().to_owned(),
                    duration: item["videoDuration"]
                        .as_u64()
                        .and_then(|ms| u32::try_from(ms / 60_000).ok())
                        .filter(|minutes| *minutes > 0),
                    image_url: ["poster", "image"]
                        .iter()
                        .find_map(|k| item[k]["src"].as_str())
                        .or_else(|| item["image"]["url"].as_str())
                        .map(served_image),
                    description: None,
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

fn served_image(url: &str) -> String {
    let Some((bucket, key)) = url
        .strip_prefix(IMAGE_BUCKETS)
        .and_then(|path| path.split_once('/'))
    else {
        return url.to_owned();
    };
    let request = json!({
        "bucket": bucket,
        "key": key,
        "edits": { "resize": { "fit": "cover", "width": IMAGE_WIDTH } }
    });
    format!("{IMAGE_SERVICE}{}", STANDARD.encode(request.to_string()))
}

fn video_id(item: &Value) -> Option<String> {
    match &item["id"] {
        Value::String(id) => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

fn episode_number(item: &Value) -> Option<u32> {
    uint(&item["episodeNumber"]).or_else(|| {
        item["title"]
            .as_str()?
            .strip_prefix("Episodio ")?
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    })
}

#[async_trait(?Send)]
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
        season: Option<u32>,
        episode: Option<&Episode>,
    ) -> ProviderResult<StreamUrl> {
        let wanted =
            episode.ok_or_else(|| ProviderError::StreamExtraction("Episode required".into()))?;
        let home = self.client.get(format!(
            "{BASE}/site/page/homepage/?include=default&filter[environment]={}&v=2",
            self.env
        ));
        let (videos, home) = tokio::try_join!(self.videos(entry), fetch_json(home))?;
        let video = videos
            .into_iter()
            .find(|v| v.is(season, wanted))
            .ok_or_else(|| ProviderError::StreamExtraction("Episode data not found".into()))?;
        let token = realm_token(&home, video.realm)?;
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

fn realm_token(home: &Value, realm: Realm) -> ProviderResult<&str> {
    home["userMeta"]["realm"][realm.key()]
        .as_str()
        .ok_or_else(|| {
            ProviderError::StreamExtraction(format!("No token for channel {}", realm.key()))
        })
}

fn playback_url(playback: &Value) -> ProviderResult<StreamUrl> {
    let url = playback["data"]["attributes"]["streaming"][0]["url"]
        .as_str()
        .ok_or_else(|| ProviderError::StreamExtraction("No streaming URL in response".into()))?;
    Ok(StreamUrl::new(url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MediaType;

    fn aurora() -> Aurora {
        Aurora::new(Http::new(std::time::Duration::from_secs(1)), "nove", "Nove")
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
        assert_eq!(entries[0].year, None);
        assert_eq!(entries[0].image_url.as_deref(), Some("https://img"));
        assert!(aurora().parse_search(&json!({})).is_err());
    }

    #[test]
    fn images_in_the_private_bucket_come_from_the_image_service() {
        let image = json!({ "url": "https://s3.eu-west-1.amazonaws.com/loma-media-it/nove-it-image-a.jpg" });
        let shows = json!({ "data": [{ "type": "showpage", "title": "Show", "image": image }] });
        let served = aurora().parse_search(&shows).unwrap()[0]
            .image_url
            .clone()
            .unwrap();
        let request = served.strip_prefix(IMAGE_SERVICE).unwrap();
        let request: Value = serde_json::from_slice(&STANDARD.decode(request).unwrap()).unwrap();
        assert_eq!(
            request,
            json!({ "bucket": "loma-media-it", "key": "nove-it-image-a.jpg",
                    "edits": { "resize": { "fit": "cover", "width": IMAGE_WIDTH } } })
        );
        let public = "https://images.aurora.enhanced.live/it/images/video/EHD_1/default.jpg";
        let videos = json!({ "blocks": [{ "type": "sonicShowBlock", "items": [
            { "id": "a", "seasonNumber": 1, "episodeNumber": 1, "image": image },
            { "id": "b", "seasonNumber": 1, "episodeNumber": 2, "poster": { "src": public } }
        ]}]});
        let videos = parse_videos(&MediaEntry::default(), &videos).unwrap();
        assert_eq!(
            videos[0].episode.image_url.as_deref(),
            Some(served.as_str())
        );
        assert_eq!(videos[1].episode.image_url.as_deref(), Some(public));
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
        assert_eq!(videos[1].episode.name, "");
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
    fn a_card_saved_before_ids_were_cut_still_finds_its_video() {
        let json = json!({ "blocks": [{ "type": "sonicShowBlock", "items": [
            { "id": "a", "show": { "id": "s" }, "seasonNumber": 1, "title": "Episodio 4" },
            { "id": "b", "show": { "id": "s" }, "seasonNumber": 2, "title": "Episodio 4" }
        ]}]});
        let url = "https://public.aurora.enhanced.live/site/page/show/";
        let found = MediaEntry {
            id: stable_hash(url),
            slug: url.into(),
            ..Default::default()
        };
        let saved = MediaEntry {
            id: 11_000_000_000_000_000_000,
            ..found.clone()
        };
        let long = Episode {
            id: 14_173_498_472_382_319_551,
            ..Episode::numbered(4, "Episodio 4")
        };
        let videos = parse_videos(&saved, &json).unwrap();
        let picked = |season| videos.iter().find(|v| v.is(season, &long)).map(|v| &v.id);
        assert_eq!(picked(Some(2)).map(String::as_str), Some("b"));
        assert_eq!(picked(Some(3)), None);
        let fresh = parse_videos(&found, &json).unwrap();
        assert_eq!(videos[1].episode.id, fresh[1].episode.id);
        assert!(videos[1].is(Some(2), &fresh[1].episode));
    }

    #[test]
    fn a_clip_under_a_minute_shows_no_duration() {
        let json = json!({ "blocks": [{ "type": "sonicShowBlock", "items": [
            { "id": "a", "show": { "id": "s" }, "seasonNumber": 1, "episodeNumber": 1,
              "videoDuration": 45_000 } ]}]});
        let videos = parse_videos(&MediaEntry::default(), &json).unwrap();
        assert_eq!(videos[0].episode.duration, None);
    }

    #[test]
    fn realm_token_reads_the_homepage_user_meta() {
        let home = json!({ "userMeta": { "realm": {
            "X-REALM-IT": "it-token", "X-REALM-DPLAY": null
        }}});
        assert_eq!(realm_token(&home, Realm::It).unwrap(), "it-token");
        assert!(realm_token(&home, Realm::Dplay).is_err());
        assert!(realm_token(&json!({}), Realm::It).is_err());
    }

    #[test]
    fn realms_map_to_their_playback_endpoints() {
        assert_eq!(Realm::It.key(), "X-REALM-IT");
        assert_eq!(
            Realm::It.playback(),
            "https://public.aurora.enhanced.live/playback/v3/videoPlaybackInfo"
        );
        assert_eq!(Realm::Dplay.key(), "X-REALM-DPLAY");
        assert_eq!(
            Realm::Dplay.playback(),
            "https://eu1-prod.disco-api.com/playback/v3/videoPlaybackInfo"
        );
    }

    #[test]
    fn parse_videos_requires_blocks() {
        assert!(parse_videos(&MediaEntry::default(), &json!({})).is_err());
    }
}
