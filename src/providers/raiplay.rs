use super::{
    Episode, Http, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, StreamUrl,
    fetch_json, items, non_blank, stable_hash, year,
};
use async_trait::async_trait;
use dioxus::logger::tracing::warn;
use futures::future::join_all;
use reqwest::Url;
use serde_json::{Value, json};
use std::collections::HashSet;

const BASE: &str = "https://www.raiplay.it";
const SEARCH: &str = "/atomatic/raiplay-search-service/api/v1/msearch";
const RELINKER: &str = "https://mediapolisvod.rai.it/relinker/relinkerServlet.htm";
const SEARCH_LIMIT: usize = 15;
const IMAGE_KEYS: [&str; 4] = ["landscape", "portrait_logo", "portrait", "square"];

pub struct RaiPlay {
    client: Http,
}

impl RaiPlay {
    pub fn new(client: Http) -> Self {
        Self { client }
    }

    async fn sets(&self, entry: &MediaEntry) -> ProviderResult<Vec<(String, String)>> {
        Ok(parse_sets(
            &fetch_json(self.client.get(program_url(&entry.slug))).await?,
        ))
    }

    async fn cards(
        &self,
        entry: &MediaEntry,
        season: u32,
    ) -> ProviderResult<Vec<(Episode, String)>> {
        let sets = self.sets(entry).await?;
        let (block, set) = season
            .checked_sub(1)
            .and_then(|i| sets.get(i as usize))
            .ok_or_else(|| ProviderError::Parse("Season mapping not found".into()))?;
        let path = entry.slug.trim_start_matches('/').trim_end_matches(".json");
        let url = format!("{BASE}/{path}/{block}/{set}/episodes.json");
        Ok(parse_cards(
            &entry.slug,
            &fetch_json(self.client.get(url)).await?,
        ))
    }

    async fn resolve(&self, page: &str) -> ProviderResult<StreamUrl> {
        let json = fetch_json(self.client.get(json_url(page))).await?;
        let video = match json["first_item_path"].as_str() {
            Some(item) => fetch_json(self.client.get(with_json(&absolute(item)))).await?,
            None => json,
        };
        let relinker = self
            .client
            .get(RELINKER)
            .query(&[("cont", cont(&video)?.as_str()), ("output", "62")]);
        relinked(&fetch_json(relinker).await?)
    }

    async fn enrich(&self, entry: MediaEntry) -> MediaEntry {
        match fetch_json(self.client.get(program_url(&entry.slug))).await {
            Ok(json) => enriched(entry, &json["program_info"]),
            Err(e) => {
                warn!("RaiPlay details for {} failed: {e}", entry.name);
                entry
            }
        }
    }
}

fn cont(video: &Value) -> ProviderResult<String> {
    let url = video["video"]["content_url"]
        .as_str()
        .ok_or_else(|| ProviderError::StreamExtraction("No content_url".into()))?;
    Url::parse(RELINKER)
        .and_then(|relinker| relinker.join(url))
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find_map(|(key, value)| (key == "cont").then(|| value.into_owned()))
        })
        .ok_or_else(|| ProviderError::StreamExtraction("No cont parameter".into()))
}

fn relinked(relinker: &Value) -> ProviderResult<StreamUrl> {
    let url = relinker["video"][0]
        .as_str()
        .ok_or_else(|| ProviderError::StreamExtraction("No video in relinker".into()))?;
    Ok(StreamUrl::new(url))
}

fn enriched(entry: MediaEntry, info: &Value) -> MediaEntry {
    MediaEntry {
        description: info["description"].as_str().and_then(non_blank),
        backdrop_url: info["images"]["landscape"]
            .as_str()
            .filter(|p| !p.is_empty())
            .map(absolute),
        year: info["year"].as_str().and_then(year).or(entry.year),
        media_type: match info["layout"].as_str() {
            Some("single") => MediaType::Movie,
            Some("multi") => MediaType::Series,
            _ => entry.media_type,
        },
        ..entry
    }
}

fn parse_card(v: &Value) -> Option<MediaEntry> {
    let slug = v["path_id"].as_str()?.to_owned();
    let name = v["titolo"].as_str().filter(|t| !t.is_empty())?.to_owned();
    let image_url = v["immagine"].as_str().map(absolute);
    Some(MediaEntry {
        id: stable_hash(&slug),
        year: image_url
            .as_deref()
            .and_then(|img| img.rsplit('/').nth(3))
            .filter(|segment| segment.len() == 4)
            .and_then(year),
        image_url,
        language: "it".into(),
        name,
        slug,
        ..Default::default()
    })
}

fn parse_sets(json: &Value) -> Vec<(String, String)> {
    items(&json["blocks"])
        .filter(|b| b["type"] == "RaiPlay Multimedia Block")
        .filter(|b| {
            let name = b["name"].as_str().unwrap_or_default();
            !["clip", "extra"]
                .iter()
                .any(|skip| name.eq_ignore_ascii_case(skip))
        })
        .filter_map(|b| Some((b["id"].as_str()?, items(&b["sets"]))))
        .flat_map(|(block, sets)| {
            sets.filter(|s| s["episode_size"]["number"].as_u64().unwrap_or(0) > 0)
                .filter_map(move |s| Some((block.to_owned(), s["id"].as_str()?.to_owned())))
        })
        .collect()
}

fn parse_cards(slug: &str, json: &Value) -> Vec<(Episode, String)> {
    let cards: Vec<&Value> = if json["seasons"].is_array() {
        items(&json["seasons"])
            .flat_map(|s| items(&s["episodes"]))
            .flat_map(|e| items(&e["cards"]))
            .collect()
    } else {
        items(&json["cards"]).collect()
    };
    cards
        .into_iter()
        .zip(1u32..)
        .map(|(card, position)| {
            let text = |keys: &[&str]| keys.iter().find_map(|k| card[k].as_str());
            let link = text(&["weblink", "url"]).unwrap_or_default();
            let episode = Episode {
                id: stable_hash(&format!("{slug}{link}")),
                number: text(&["episode"])
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(position),
                name: text(&["episode_title", "name", "toptitle"])
                    .unwrap_or_default()
                    .to_owned(),
                duration: ["duration_in_minutes", "duration"]
                    .iter()
                    .find_map(|k| card[k].as_str()?.split_whitespace().next()?.parse().ok()),
                image_url: IMAGE_KEYS
                    .iter()
                    .find_map(|k| card["images"][k].as_str())
                    .or_else(|| text(&["immagine", "image"]))
                    .filter(|i| !i.is_empty())
                    .map(absolute),
                description: text(&["description"]).and_then(non_blank),
            };
            (episode, link.to_owned())
        })
        .collect()
}

fn program_url(slug: &str) -> String {
    format!("{BASE}/{}", slug.trim_start_matches('/'))
}

fn absolute(path: &str) -> String {
    if path.starts_with("http") {
        path.to_owned()
    } else {
        format!("{BASE}{path}")
    }
}

fn with_json(url: &str) -> String {
    format!("{}.json", url.strip_suffix(".json").unwrap_or(url))
}

fn json_url(page: &str) -> String {
    with_json(page.strip_suffix(".html").unwrap_or(page))
}

#[async_trait(?Send)]
impl Provider for RaiPlay {
    fn name(&self) -> &'static str {
        "RaiPlay"
    }

    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>> {
        let body = json!({
            "templateIn": "6470a982e4e0301afe1f81f1",
            "templateOut": "6516ac5d40da6c377b151642",
            "params": { "param": query, "from": null, "sort": "relevance", "onlyVideoQuery": false }
        });
        let json = fetch_json(self.client.post(format!("{BASE}{SEARCH}")).json(&body)).await?;
        let mut seen = HashSet::new();
        let entries = items(&json["agg"]["titoli"]["cards"])
            .take(SEARCH_LIMIT)
            .filter_map(parse_card)
            .filter(|e| seen.insert(e.id))
            .map(|entry| self.enrich(entry));
        Ok(join_all(entries).await)
    }

    async fn seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<u32>> {
        let sets = self.sets(entry).await?;
        Ok((1..).take(sets.len()).collect())
    }

    async fn episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>> {
        let mut episodes: Vec<Episode> = self
            .cards(entry, season)
            .await?
            .into_iter()
            .map(|(episode, _)| episode)
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
        let page = match episode {
            None => program_url(&entry.slug),
            Some(wanted) => {
                let season = season.ok_or_else(|| {
                    ProviderError::StreamExtraction("Season required to fetch episode data".into())
                })?;
                self.cards(entry, season)
                    .await?
                    .into_iter()
                    .find(|(episode, link)| episode.id == wanted.id && !link.is_empty())
                    .map(|(_, link)| absolute(&link))
                    .ok_or_else(|| {
                        ProviderError::StreamExtraction("Episode URL not found".into())
                    })?
            }
        };
        self.resolve(&page).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_card_reads_year_from_image_path() {
        let card = json!({
            "path_id": "/programmi/montalbano.json", "titolo": "Montalbano",
            "immagine": "/dl/img/2021/03/15/1615000000_2048x1152.jpg"
        });
        let entry = parse_card(&card).unwrap();
        assert_eq!(entry.id, stable_hash("/programmi/montalbano.json"));
        assert_eq!(entry.year.as_deref(), Some("2021"));
        assert_eq!(
            entry.image_url.as_deref(),
            Some("https://www.raiplay.it/dl/img/2021/03/15/1615000000_2048x1152.jpg")
        );
        assert_eq!(entry.media_type, MediaType::Series);
        assert!(parse_card(&json!({ "path_id": "/p", "titolo": "" })).is_none());
        let sized = json!({
            "path_id": "/p", "titolo": "P", "immagine": "/dl/img/20211/1200x675/03/a.jpg"
        });
        assert_eq!(parse_card(&sized).unwrap().year, None);
    }

    #[test]
    fn enriched_fills_details_and_prefers_the_programme_year() {
        let entry = MediaEntry {
            year: Some("2020".into()),
            ..Default::default()
        };
        let info = json!({ "description": " Plot ", "year": "1999", "layout": "single",
                           "images": { "landscape": "/dl/img/l.jpg" } });
        let entry = enriched(entry, &info);
        assert_eq!(entry.description.as_deref(), Some("Plot"));
        assert_eq!(
            entry.backdrop_url.as_deref(),
            Some("https://www.raiplay.it/dl/img/l.jpg")
        );
        assert_eq!(entry.year.as_deref(), Some("1999"));
        let undated = MediaEntry {
            year: Some("2020".into()),
            ..Default::default()
        };
        assert_eq!(enriched(undated, &json!({})).year.as_deref(), Some("2020"));
        assert_eq!(entry.media_type, MediaType::Movie);
        assert_eq!(
            enriched(MediaEntry::default(), &json!({})).media_type,
            MediaType::Series
        );
    }

    #[test]
    fn parse_sets_skips_extras_and_empty_sets() {
        let json = json!({ "blocks": [
            { "type": "RaiPlay Multimedia Block", "name": "Episodi", "id": "b1", "sets": [
                { "id": "s1", "episode_size": { "number": 10 } },
                { "id": "s0", "episode_size": { "number": 0 } },
                { "episode_size": { "number": 4 } }
            ]},
            { "type": "RaiPlay Multimedia Block", "name": "Stagioni",
              "sets": [{ "id": "n", "episode_size": { "number": 3 } }] },
            { "type": "RaiPlay Multimedia Block", "name": "EXTRA", "id": "b4",
              "sets": [{ "id": "e", "episode_size": { "number": 3 } }] },
            { "type": "RaiPlay Multimedia Block", "name": "Clip", "id": "b2",
              "sets": [{ "id": "c", "episode_size": { "number": 3 } }] },
            { "type": "Other", "id": "b3", "sets": [{ "id": "o", "episode_size": { "number": 3 } }] }
        ]});
        assert_eq!(parse_sets(&json), [("b1".to_owned(), "s1".to_owned())]);
    }

    #[test]
    fn parse_cards_reads_nested_seasons() {
        let json = json!({ "seasons": [{ "episodes": [{ "cards": [
            { "episode": "4", "episode_title": "Quattro", "duration_in_minutes": "50 min",
              "weblink": "/video/a.html", "images": { "portrait": "/img/p.jpg" },
              "description": " Montalbano indaga. " },
            { "name": "Senza numero", "duration": "12", "url": "https://x/b.html" }
        ]}]}]});
        let cards = parse_cards("/programmi/x.json", &json);
        assert_eq!(cards.len(), 2);
        let (first, link) = &cards[0];
        assert_eq!(
            (first.number, first.name.as_str(), first.duration),
            (4, "Quattro", Some(50))
        );
        assert_eq!(first.id, stable_hash("/programmi/x.json/video/a.html"));
        assert_eq!(
            first.image_url.as_deref(),
            Some("https://www.raiplay.it/img/p.jpg")
        );
        assert_eq!(first.description.as_deref(), Some("Montalbano indaga."));
        assert_eq!(link, "/video/a.html");
        assert_eq!((cards[1].0.number, cards[1].0.duration), (2, Some(12)));
        assert_eq!(cards[1].1, "https://x/b.html");
    }

    #[test]
    fn parse_cards_reads_flat_cards() {
        let cards = parse_cards("/p", &json!({ "cards": [{ "toptitle": "Top" }] }));
        assert_eq!(cards[0].0.name, "Top");
        assert_eq!(cards[0].0.number, 1);
        assert_eq!(cards[0].1, "");
    }

    #[test]
    fn cont_reads_the_relinker_parameter() {
        let video = json!({ "video": { "content_url":
            "https://mediapolisvod.rai.it/relinker/relinkerServlet.htm?cont=a%2Bb%3D%3D&p=1" } });
        assert_eq!(cont(&video).unwrap(), "a+b==");
        let relative = json!({ "video": { "content_url": "//mediapolisvod.rai.it/r.htm?cont=x" } });
        assert_eq!(cont(&relative).unwrap(), "x");
        let other = json!({ "video": { "content_url": "https://x/r.htm?id=1" } });
        assert!(cont(&other).is_err());
        assert!(cont(&json!({})).is_err());
    }

    #[test]
    fn relinked_reads_the_first_video() {
        let relinker = json!({ "video": ["https://cdn/master.m3u8"] });
        assert_eq!(relinked(&relinker).unwrap().url, "https://cdn/master.m3u8");
        assert!(relinked(&json!({ "video": [] })).is_err());
    }

    #[test]
    fn json_url_normalizes_page_urls() {
        assert_eq!(json_url("https://r/v.html"), "https://r/v.json");
        assert_eq!(json_url("https://r/v.json"), "https://r/v.json");
        assert_eq!(json_url("https://r/v"), "https://r/v.json");
        assert_eq!(with_json("https://r/v.html"), "https://r/v.html.json");
    }

    #[test]
    fn absolute_prefixes_relative_paths() {
        assert_eq!(absolute("/x"), "https://www.raiplay.it/x");
        assert_eq!(absolute("https://y/z"), "https://y/z");
    }
}
