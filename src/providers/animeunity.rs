use super::{
    Episode, Http, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, StreamUrl,
    domain, fetch_json, fetch_text, fetch_vixcloud, items, non_blank, send, uint, year,
};
use async_trait::async_trait;
use percent_encoding::percent_decode_str;
use reqwest::RequestBuilder;
use reqwest::header::{COOKIE, HeaderMap, ORIGIN, REFERER, SET_COOKIE};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::Mutex;
use web_time::Instant;

const AUTH_TTL: Duration = Duration::from_secs(300);
const EPISODE_PAGE: u32 = 120;

#[derive(Clone)]
struct Auth {
    cookie: String,
    xsrf: String,
}

impl Auth {
    fn new(headers: &HeaderMap) -> ProviderResult<Self> {
        let xsrf = cookie(headers, "XSRF-TOKEN")
            .filter(|x| !x.is_empty())
            .ok_or_else(|| ProviderError::Parse("Failed to get XSRF token".into()))?;
        let session = cookie(headers, "animeunity_session").unwrap_or_default();
        Ok(Self {
            cookie: format!("XSRF-TOKEN={xsrf}; animeunity_session={session}"),
            xsrf: percent_decode_str(xsrf).decode_utf8_lossy().into_owned(),
        })
    }
}

pub struct AnimeUnity {
    client: Http,
    base: String,
    auth: Mutex<Option<(Auth, Instant)>>,
}

impl AnimeUnity {
    pub fn new(client: Http) -> Self {
        Self {
            client,
            base: domain("animeunity"),
            auth: Mutex::new(None),
        }
    }

    async fn auth(&self) -> ProviderResult<Auth> {
        let mut cached = self.auth.lock().await;
        if let Some((auth, at)) = cached.as_ref()
            && at.elapsed() < AUTH_TTL
        {
            return Ok(auth.clone());
        }
        let response = send(self.client.get(&self.base)).await?;
        let auth = Auth::new(response.headers())?;
        *cached = Some((auth.clone(), Instant::now()));
        Ok(auth)
    }

    async fn authed(&self, request: RequestBuilder) -> ProviderResult<RequestBuilder> {
        let auth = self.auth().await?;
        Ok(request
            .header(COOKIE, auth.cookie)
            .header("x-xsrf-token", auth.xsrf)
            .header(ORIGIN, &self.base)
            .header(REFERER, format!("{}/", self.base)))
    }
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let value = headers
        .get_all(SET_COOKIE)
        .iter()
        .rev()
        .find_map(|h| h.to_str().ok()?.strip_prefix(name)?.strip_prefix('='))?;
    Some(value.split_once(';').map_or(value, |(value, _)| value))
}

fn parse_record(record: &Value) -> Option<MediaEntry> {
    let id = record["id"].as_u64()?;
    let titles =
        ["title_eng", "title", "title_it"].map(|k| record[k].as_str().filter(|t| !t.is_empty()));
    let name = titles.into_iter().flatten().next()?.to_owned();
    Some(MediaEntry {
        id,
        slug: record["slug"].as_str().unwrap_or_default().to_owned(),
        language: "ja".into(),
        media_type: match record["type"].as_str() {
            Some("Movie" | "Film") => MediaType::Movie,
            _ => MediaType::Series,
        },
        alternative_names: titles
            .into_iter()
            .flatten()
            .filter(|t| *t != name)
            .map(String::from)
            .collect(),
        year: record["date"].as_str().and_then(year),
        score: record["score"]
            .as_str()
            .map(String::from)
            .or_else(|| record["score"].as_f64().map(|s| s.to_string())),
        image_url: record["imageurl"].as_str().map(String::from),
        backdrop_url: record["cover"]
            .as_str()
            .filter(|c| !c.is_empty())
            .map(String::from),
        description: record["plot"].as_str().and_then(non_blank),
        name,
        ..Default::default()
    })
}

fn page_ranges(total: u32) -> impl Iterator<Item = (u32, u32)> {
    (1..=total)
        .step_by(EPISODE_PAGE as usize)
        .map(move |start| (start, start.saturating_add(EPISODE_PAGE - 1).min(total)))
}

fn parse_episode(position: u32, ep: &Value) -> Option<Episode> {
    Some(Episode {
        id: ep["id"].as_u64()?,
        number: number(&ep["number"]).unwrap_or(position),
        name: ["title", "title_it"]
            .iter()
            .find_map(|k| ep[k].as_str().filter(|t| !t.is_empty()))
            .unwrap_or_default()
            .to_owned(),
        duration: None,
        image_url: ep["imageurl"].as_str().map(String::from),
        description: None,
    })
}

fn number(value: &Value) -> Option<u32> {
    let text = match value {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim().to_owned(),
        _ => return None,
    };
    text.split('.').next()?.parse().ok()
}

#[async_trait(?Send)]
impl Provider for AnimeUnity {
    fn name(&self) -> &'static str {
        "AnimeUnity"
    }

    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>> {
        let live = self
            .authed(
                self.client
                    .post(format!("{}/livesearch", self.base))
                    .form(&[("title", query)]),
            )
            .await?;
        let archive = self
            .authed(
                self.client
                    .post(format!("{}/archivio/get-animes", self.base))
                    .json(&json!({
                        "title": query, "type": false, "year": false, "order": false,
                        "status": false, "genres": false, "offset": 0, "dubbed": false,
                        "season": false
                    })),
            )
            .await?;
        let results = tokio::try_join!(fetch_json(live), fetch_json(archive))?;
        let mut seen = HashSet::new();
        Ok([results.0, results.1]
            .iter()
            .flat_map(|r| items(&r["records"]))
            .filter_map(parse_record)
            .filter(|e| seen.insert(e.id))
            .collect())
    }

    async fn seasons(&self, _entry: &MediaEntry) -> ProviderResult<Vec<u32>> {
        Ok(vec![1])
    }

    async fn episodes(&self, entry: &MediaEntry, _season: u32) -> ProviderResult<Vec<Episode>> {
        let id = entry.id;
        let info = self
            .authed(self.client.get(format!("{}/info_api/{id}/", self.base)))
            .await?;
        let total = uint(&fetch_json(info).await?["episodes_count"])
            .ok_or_else(|| ProviderError::Parse("No episodes_count".into()))?;
        let mut pages = Vec::new();
        for (start, end) in page_ranges(total) {
            let request = self
                .authed(
                    self.client
                        .get(format!("{}/info_api/{id}/1", self.base))
                        .query(&[("start_range", start), ("end_range", end)]),
                )
                .await?;
            let page = fetch_json(request).await?;
            if items(&page["episodes"]).next().is_none() {
                break;
            }
            pages.push(page);
        }
        let mut episodes: Vec<Episode> = pages
            .iter()
            .flat_map(|page| items(&page["episodes"]))
            .zip(1u32..)
            .filter_map(|(ep, position)| parse_episode(position, ep))
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
        let id = match episode {
            Some(ep) => ep.id,
            None => self
                .episodes(entry, 1)
                .await?
                .first()
                .map(|ep| ep.id)
                .ok_or_else(|| ProviderError::StreamExtraction("No episodes".into()))?,
        };
        fetch_vixcloud(&self.client, async || {
            let embed = self
                .authed(self.client.get(format!("{}/embed-url/{id}", self.base)))
                .await?;
            let embed = fetch_text(embed).await?.trim().to_owned();
            if !embed.starts_with("http") {
                return Err(ProviderError::StreamExtraction("Empty embed URL".into()));
            }
            Ok(embed)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_cookies(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(SET_COOKIE, value.parse().unwrap());
        }
        headers
    }

    #[test]
    fn cookie_reads_last_matching_set_cookie_raw() {
        let headers = set_cookies(&[
            "XSRF-TOKEN=old; path=/",
            "XSRF-TOKEN-OTHER=x",
            "animeunity_session=s%20id; httponly",
            "XSRF-TOKEN=eyJ%3D%3D; expires=Wed; path=/",
        ]);
        assert_eq!(cookie(&headers, "XSRF-TOKEN"), Some("eyJ%3D%3D"));
        assert_eq!(cookie(&headers, "animeunity_session"), Some("s%20id"));
        assert_eq!(cookie(&headers, "missing"), None);
    }

    #[test]
    fn auth_sends_raw_cookies_and_a_decoded_xsrf_header() {
        let auth = Auth::new(&set_cookies(&[
            "XSRF-TOKEN=eyJ%2Bx%3D%3D; path=/",
            "animeunity_session=s%3D; httponly",
        ]))
        .unwrap();
        assert_eq!(
            auth.cookie,
            "XSRF-TOKEN=eyJ%2Bx%3D%3D; animeunity_session=s%3D"
        );
        assert_eq!(auth.xsrf, "eyJ+x==");
        assert!(matches!(
            Auth::new(&set_cookies(&["XSRF-TOKEN=; path=/"])),
            Err(ProviderError::Parse(_))
        ));
    }

    #[test]
    fn parse_record_prefers_english_title() {
        let record = json!({
            "id": 9, "slug": "naruto", "episodes_count": 220, "type": "TV",
            "title_eng": "Naruto", "title": "NARUTO", "title_it": "",
            "date": "2002-10-03", "score": 8.04, "imageurl": "https://img/n.jpg", "plot": " Ninja ",
            "cover": "https://img/cover.jpg"
        });
        let entry = parse_record(&record).unwrap();
        assert_eq!(entry.name, "Naruto");
        assert_eq!(entry.alternative_names, ["NARUTO"]);
        assert_eq!(entry.slug, "naruto");
        assert_eq!(entry.media_type, MediaType::Series);
        assert_eq!(entry.year.as_deref(), Some("2002"));
        assert_eq!(entry.score.as_deref(), Some("8.04"));
        assert_eq!(entry.language, "ja");
        assert_eq!(entry.image_url.as_deref(), Some("https://img/n.jpg"));
        assert_eq!(entry.description.as_deref(), Some("Ninja"));
        assert_eq!(entry.backdrop_url.as_deref(), Some("https://img/cover.jpg"));
    }

    #[test]
    fn parse_record_detects_movies_and_requires_a_title() {
        let movie =
            parse_record(&json!({ "id": 1, "title": "Akira", "type": "Movie", "score": "7.9" }))
                .unwrap();
        assert_eq!(movie.media_type, MediaType::Movie);
        assert_eq!(movie.score.as_deref(), Some("7.9"));
        assert_eq!(movie.slug, "");
        assert_eq!(movie.description, None);
        assert!(parse_record(&json!({ "id": 1, "title": "" })).is_none());
    }

    #[test]
    fn page_ranges_cover_the_episode_count() {
        assert_eq!(page_ranges(0).count(), 0);
        assert_eq!(page_ranges(5).collect::<Vec<_>>(), [(1, 5)]);
        assert_eq!(
            page_ranges(241).collect::<Vec<_>>(),
            [(1, 120), (121, 240), (241, 241)]
        );
        assert_eq!(page_ranges(240).last(), Some((121, 240)));
        assert_eq!(
            page_ranges(u32::MAX).last(),
            Some((u32::MAX - 14, u32::MAX))
        );
    }

    #[test]
    fn parse_episode_handles_numeric_formats_and_missing_names() {
        let ep = parse_episode(
            1,
            &json!({ "id": 5, "number": "12.5", "title": null, "title_it": "Dodici" }),
        )
        .unwrap();
        assert_eq!((ep.id, ep.number, ep.name.as_str()), (5, 12, "Dodici"));
        let ep = parse_episode(4, &json!({ "id": 6, "number": "OVA" })).unwrap();
        assert_eq!((ep.number, ep.name.as_str()), (4, ""));
        assert!(parse_episode(1, &json!({ "number": 1 })).is_none());
    }

    #[test]
    fn number_rejects_negative_and_missing_values() {
        assert_eq!(number(&json!(3)), Some(3));
        assert_eq!(number(&json!(-1)), None);
        assert_eq!(number(&json!(null)), None);
    }
}
