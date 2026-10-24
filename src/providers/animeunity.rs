use super::{
    Episode, Http, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, StreamUrl,
    domain, fetch_json, fetch_text, items, send, vixcloud_playlist,
};
use async_trait::async_trait;
use percent_encoding::percent_decode_str;
use reqwest::RequestBuilder;
use reqwest::header::{COOKIE, HeaderMap, ORIGIN, REFERER, SET_COOKIE};
use scraper::{Html, Selector};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Mutex;
use web_time::Instant;

const AUTH_TTL: Duration = Duration::from_secs(300);
const EPISODE_PAGE: u64 = 120;

#[derive(Clone)]
struct Auth {
    xsrf: String,
    session: String,
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
        let headers = response.headers();
        let auth = Auth {
            xsrf: cookie(headers, "XSRF-TOKEN")
                .filter(|x| !x.is_empty())
                .ok_or_else(|| ProviderError::Network("Failed to get XSRF token".into()))?,
            session: cookie(headers, "animeunity_session").unwrap_or_default(),
        };
        *cached = Some((auth.clone(), Instant::now()));
        Ok(auth)
    }

    async fn authed(&self, request: RequestBuilder) -> ProviderResult<RequestBuilder> {
        let auth = self.auth().await?;
        Ok(request
            .header(
                COOKIE,
                format!(
                    "XSRF-TOKEN={}; animeunity_session={}",
                    auth.xsrf, auth.session
                ),
            )
            .header("x-xsrf-token", auth.xsrf)
            .header(ORIGIN, &self.base)
            .header(REFERER, format!("{}/", self.base)))
    }
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let value = headers
        .get_all(SET_COOKIE)
        .iter()
        .rev()
        .find_map(|h| h.to_str().ok()?.strip_prefix(name)?.strip_prefix('='))?;
    let value = value.split_once(';').map_or(value, |(value, _)| value);
    Some(percent_decode_str(value).decode_utf8_lossy().into_owned())
}

fn parse_record(record: &Value) -> Option<MediaEntry> {
    let id = record["id"].as_u64()?;
    let titles =
        ["title_eng", "title", "title_it"].map(|k| record[k].as_str().filter(|t| !t.is_empty()));
    let name = titles.into_iter().flatten().next()?.to_owned();
    Some(MediaEntry {
        id,
        slug: format!(
            "{id}:{}:{}",
            record["slug"].as_str().unwrap_or_default(),
            record["episodes_count"].as_u64().unwrap_or(0)
        ),
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
        year: record["date"]
            .as_str()
            .and_then(|d| d.split('-').next())
            .map(String::from),
        score: record["score"]
            .as_str()
            .map(String::from)
            .or_else(|| record["score"].as_f64().map(|s| format!("{s:.1}"))),
        image_url: record["imageurl"].as_str().map(String::from),
        backdrop_url: record["cover"]
            .as_str()
            .filter(|c| !c.is_empty())
            .map(String::from),
        description: record["plot"]
            .as_str()
            .filter(|p| !p.is_empty())
            .map(String::from),
        name,
        ..Default::default()
    })
}

fn parse_slug(slug: &str) -> (u64, u64) {
    let mut parts = slug.split(':');
    let id = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let count = parts.nth(1).and_then(|p| p.parse().ok()).unwrap_or(0);
    (id, count)
}

fn parse_episode(index: usize, ep: &Value) -> Option<Episode> {
    let number = number(&ep["number"]).unwrap_or(u32::try_from(index + 1).ok()?);
    Some(Episode {
        id: ep["id"].as_u64()?,
        name: ["title", "title_it"]
            .iter()
            .find_map(|k| ep[k].as_str().filter(|t| !t.is_empty()))
            .map_or_else(|| format!("Episode {number}"), String::from),
        number,
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

fn extract_stream(html: &str) -> Option<String> {
    static SCRIPTS: LazyLock<Selector> = LazyLock::new(|| Selector::parse("body script").unwrap());
    let scripts: Vec<String> = Html::parse_document(html)
        .select(&SCRIPTS)
        .map(|s| s.text().collect())
        .collect();
    scripts
        .get(1)
        .and_then(|s| s.split(" = ").nth(1))
        .map(|url| url.replace('\'', "").trim().to_owned())
        .filter(|url| url.starts_with("http"))
        .or_else(|| vixcloud_playlist(scripts.first()?))
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
        let (id, count) = parse_slug(&entry.slug);
        let total = if count > 0 {
            count
        } else {
            let info = self
                .authed(self.client.get(format!("{}/info_api/{id}/", self.base)))
                .await?;
            fetch_json(info).await?["episodes_count"]
                .as_u64()
                .ok_or_else(|| ProviderError::Parse("No episodes_count".into()))?
        };
        let mut raw = Vec::new();
        let mut start = 1;
        while start <= total {
            let end = (start + EPISODE_PAGE - 1).min(total);
            let page = self
                .authed(
                    self.client
                        .get(format!("{}/info_api/{id}/1", self.base))
                        .query(&[("start_range", start), ("end_range", end)]),
                )
                .await?;
            raw.extend(items(&fetch_json(page).await?["episodes"]).cloned());
            start = end + 1;
        }
        let mut episodes: Vec<Episode> = raw
            .iter()
            .enumerate()
            .filter_map(|(i, ep)| parse_episode(i, ep))
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
        let embed = self
            .authed(self.client.get(format!("{}/embed-url/{id}", self.base)))
            .await?;
        let embed = fetch_text(embed).await?.trim().to_owned();
        if !embed.starts_with("http") {
            return Err(ProviderError::StreamExtraction("Empty embed URL".into()));
        }
        let html = fetch_text(self.client.get(&embed)).await?;
        let url = extract_stream(&html).ok_or_else(|| {
            ProviderError::StreamExtraction("Could not extract stream URL".into())
        })?;
        Ok(StreamUrl::new(url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_reads_last_matching_set_cookie_decoded() {
        let mut headers = HeaderMap::new();
        for value in [
            "XSRF-TOKEN=old; path=/",
            "XSRF-TOKEN-OTHER=x",
            "animeunity_session=s%20id; httponly",
            "XSRF-TOKEN=eyJ%3D%3D; expires=Wed; path=/",
        ] {
            headers.append(SET_COOKIE, value.parse().unwrap());
        }
        assert_eq!(cookie(&headers, "XSRF-TOKEN").as_deref(), Some("eyJ=="));
        assert_eq!(
            cookie(&headers, "animeunity_session").as_deref(),
            Some("s id")
        );
        assert_eq!(cookie(&headers, "missing"), None);
    }

    #[test]
    fn parse_record_prefers_english_title() {
        let record = json!({
            "id": 9, "slug": "naruto", "episodes_count": 220, "type": "TV",
            "title_eng": "Naruto", "title": "NARUTO", "title_it": "",
            "date": "2002-10-03", "score": 8.04, "imageurl": "https://img/n.jpg", "plot": "Ninja",
            "cover": "https://img/cover.jpg"
        });
        let entry = parse_record(&record).unwrap();
        assert_eq!(entry.name, "Naruto");
        assert_eq!(entry.alternative_names, ["NARUTO"]);
        assert_eq!(entry.slug, "9:naruto:220");
        assert_eq!(entry.media_type, MediaType::Series);
        assert_eq!(entry.year.as_deref(), Some("2002"));
        assert_eq!(entry.score.as_deref(), Some("8.0"));
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
        assert_eq!(movie.slug, "1::0");
        assert!(parse_record(&json!({ "id": 1, "title": "" })).is_none());
    }

    #[test]
    fn parse_slug_reads_id_and_count() {
        assert_eq!(parse_slug("9:naruto:220"), (9, 220));
        assert_eq!(parse_slug("garbage"), (0, 0));
    }

    #[test]
    fn parse_episode_handles_numeric_formats_and_missing_names() {
        let ep = parse_episode(
            0,
            &json!({ "id": 5, "number": "12.5", "title": null, "title_it": "Dodici" }),
        )
        .unwrap();
        assert_eq!((ep.id, ep.number, ep.name.as_str()), (5, 12, "Dodici"));
        let ep = parse_episode(3, &json!({ "id": 6, "number": "OVA" })).unwrap();
        assert_eq!((ep.number, ep.name.as_str()), (4, "Episode 4"));
        assert!(parse_episode(0, &json!({ "number": 1 })).is_none());
    }

    #[test]
    fn number_rejects_negative_and_missing_values() {
        assert_eq!(number(&json!(3)), Some(3));
        assert_eq!(number(&json!(-1)), None);
        assert_eq!(number(&json!(null)), None);
    }

    #[test]
    fn extract_stream_prefers_direct_download_url() {
        let html = "<body><script>var a = 1</script><script>window.downloadUrl = 'https://cdn/x.mp4'</script></body>";
        assert_eq!(extract_stream(html).as_deref(), Some("https://cdn/x.mp4"));
    }

    #[test]
    fn extract_stream_falls_back_to_playlist() {
        let html =
            r#"<body><script>token: "t", expires: "e", url: "https://v.co/p/1"</script></body>"#;
        assert_eq!(
            extract_stream(html).as_deref(),
            Some("https://v.co/p/1?token=t&expires=e")
        );
        assert!(extract_stream("<body></body>").is_none());
    }
}
