use super::{
    Episode, Http, MediaEntry, MediaType, Provider, ProviderError, ProviderResult, StreamUrl,
    domain, fetch_json, fetch_text, items, non_blank, uint, vixcloud_stream, year,
};
use async_trait::async_trait;
use dioxus::logger::tracing::warn;
use futures::future::join_all;
use regex::Regex;
use scraper::{Html, Selector};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex, PoisonError};

const DEFAULT_LANG: &str = "it";
const LANGS: [&str; 2] = ["it", "en"];
const IMAGE_TYPES: [&str; 4] = ["poster", "cover", "cover_mobile", "background"];
const BACKDROP_TYPES: [&str; 2] = ["background", "cover"];
const CATALOG_SIZE: usize = 100;
const SLIDERS_PER_REQUEST: usize = 6;
const BROWSE_PAGE_SIZE: usize = 60;
const BROWSE_MAX_PAGES: usize = 17;

pub struct StreamingCommunity {
    client: Http,
    base: String,
    cdn: String,
    browse: Mutex<Option<Browse>>,
}

struct Browse {
    sections: Vec<(&'static str, Option<String>)>,
    section: usize,
    page: usize,
}

impl Browse {
    fn new(genres: Vec<String>) -> Self {
        let mut sections = vec![("trending", None), ("latest", None)];
        sections.extend(genres.into_iter().map(|g| ("genre", Some(g))));
        Self {
            sections,
            section: 0,
            page: 1,
        }
    }

    fn current(&self) -> Option<(&'static str, Option<String>, usize)> {
        let (path, genre) = self.sections.get(self.section)?;
        Some((path, genre.clone(), self.page))
    }

    fn advance(&mut self, titles: usize) {
        if titles >= BROWSE_PAGE_SIZE && self.page < BROWSE_MAX_PAGES {
            self.page += 1;
        } else {
            self.section += 1;
            self.page = 1;
        }
    }
}

impl StreamingCommunity {
    pub fn new(client: Http) -> Self {
        let base = domain("streamingcommunity");
        Self {
            client,
            cdn: base.replacen("://", "://cdn.", 1),
            base,
            browse: Mutex::new(None),
        }
    }

    async fn page(&self, path: &str, query: &[(&str, &str)]) -> ProviderResult<Value> {
        let request = self.client.get(format!("{}{path}", self.base)).query(query);
        data_page(&fetch_text(request).await?)
    }

    fn title_path(entry: &MediaEntry) -> String {
        format!("/{}/titles/{}-{}", Self::lang(entry), entry.id, entry.slug)
    }

    fn lang(entry: &MediaEntry) -> &str {
        if entry.language.is_empty() {
            DEFAULT_LANG
        } else {
            &entry.language
        }
    }

    fn titles<'a>(
        &'a self,
        titles: impl Iterator<Item = &'a Value> + 'a,
        lang: &'a str,
    ) -> impl Iterator<Item = MediaEntry> + 'a {
        titles.filter_map(move |t| self.parse_title(t, lang))
    }

    fn parse_title(&self, v: &Value, lang: &str) -> Option<MediaEntry> {
        let name = v["name"].as_str().filter(|n| !n.is_empty())?.to_owned();
        Some(MediaEntry {
            id: v["id"].as_u64()?,
            slug: v["slug"].as_str().unwrap_or_default().to_owned(),
            language: lang.to_owned(),
            media_type: match v["type"].as_str() {
                Some("film" | "movie" | "ova") => MediaType::Movie,
                _ => MediaType::Series,
            },
            alternative_names: v["original_name"]
                .as_str()
                .filter(|o| !o.is_empty() && *o != name)
                .map(String::from)
                .into_iter()
                .collect(),
            year: release_year(v),
            score: v["score"].as_str().map(String::from),
            image_url: self.image(v),
            backdrop_url: self.find_image(v, &BACKDROP_TYPES),
            description: description(v),
            name,
            ..Default::default()
        })
    }

    fn image(&self, v: &Value) -> Option<String> {
        self.find_image(v, &IMAGE_TYPES).or_else(|| {
            let first = v["images"].as_array()?.first()?["filename"].as_str()?;
            Some(self.cdn_image(first))
        })
    }

    fn find_image(&self, v: &Value, kinds: &[&str]) -> Option<String> {
        let images = v["images"].as_array()?;
        kinds
            .iter()
            .find_map(|kind| {
                images
                    .iter()
                    .filter(|i| i["type"] == *kind)
                    .find_map(|i| i["filename"].as_str())
            })
            .map(|f| self.cdn_image(f))
    }

    fn cdn_image(&self, filename: &str) -> String {
        format!("{}/images/{filename}", self.cdn)
    }

    fn home(&self, page: &Value, seen: &mut HashSet<u64>) -> Vec<MediaEntry> {
        let props = &page["props"];
        let mut entries: Vec<MediaEntry> = self
            .titles(
                items(&props["sliders"]).flat_map(|s| items(&s["titles"])),
                DEFAULT_LANG,
            )
            .filter(|e| seen.insert(e.id))
            .collect();
        if entries.is_empty() {
            entries = self
                .titles(items(&props["titles"]), DEFAULT_LANG)
                .filter(|e| seen.insert(e.id))
                .collect();
        }
        entries
    }

    fn parse_episodes(&self, page: &Value) -> Vec<Episode> {
        let mut episodes: Vec<Episode> = items(&page["props"]["loadedSeason"]["episodes"])
            .filter_map(|ep| {
                Some(Episode {
                    id: ep["id"].as_u64()?,
                    number: uint(&ep["number"])?,
                    name: ep["name"].as_str().unwrap_or_default().to_owned(),
                    duration: uint(&ep["duration"]),
                    image_url: self.image(ep),
                    description: ep["plot"].as_str().and_then(non_blank),
                })
            })
            .collect();
        episodes.sort_by_key(|e| e.number);
        episodes
    }

    fn browse<T>(&self, f: impl FnOnce(&mut Option<Browse>) -> T) -> T {
        f(&mut self.browse.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

fn release_year(v: &Value) -> Option<String> {
    translation(v, &["first_air_date", "release_date"], year).or_else(|| {
        ["last_air_date", "release_date"]
            .iter()
            .find_map(|k| v[k].as_str().and_then(year))
    })
}

fn description(v: &Value) -> Option<String> {
    translation(v, &["description", "overview", "plot"], non_blank).or_else(|| {
        ["plot", "description", "overview"]
            .iter()
            .find_map(|k| v[k].as_str().and_then(non_blank))
    })
}

fn translation(v: &Value, keys: &[&str], parse: impl Fn(&str) -> Option<String>) -> Option<String> {
    items(&v["translations"])
        .filter(|t| t["key"].as_str().is_some_and(|k| keys.contains(&k)))
        .find_map(|t| t["value"].as_str().and_then(&parse))
}

fn parse_seasons(page: &Value) -> Vec<u32> {
    let mut seasons: Vec<u32> = items(&page["props"]["title"]["seasons"])
        .filter_map(|s| uint(&s["number"]))
        .collect();
    seasons.sort_unstable();
    seasons
}

fn genres(page: &Value) -> Vec<String> {
    items(&page["props"]["genres"])
        .filter_map(|g| g["name"].as_str().map(String::from))
        .collect()
}

fn slider_requests(genres: &[String]) -> impl Iterator<Item = Value> + '_ {
    genres.chunks(SLIDERS_PER_REQUEST).map(|chunk| {
        let sliders: Vec<Value> = chunk
            .iter()
            .map(|g| json!({ "name": "genre", "genre": g }))
            .collect();
        json!({ "sliders": sliders })
    })
}

fn data_page(html: &str) -> ProviderResult<Value> {
    static APP: LazyLock<Selector> =
        LazyLock::new(|| Selector::parse("div#app[data-page]").unwrap());
    let doc = Html::parse_document(html);
    let data = doc
        .select(&APP)
        .next()
        .and_then(|app| app.attr("data-page"))
        .ok_or_else(|| ProviderError::Parse("No data-page JSON found".into()))?;
    Ok(serde_json::from_str(data)?)
}

fn iframe_src(html: &str) -> Option<String> {
    static IFRAME: LazyLock<Selector> = LazyLock::new(|| Selector::parse("iframe").unwrap());
    static IFRAME_TAG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"<iframe[^>]+src=['"]([^'"]+)['"]"#).unwrap());
    let doc = Html::parse_document(html);
    doc.select(&IFRAME)
        .next()
        .and_then(|iframe| iframe.attr("src").or(iframe.attr("data-src")))
        .map(String::from)
        .or_else(|| IFRAME_TAG.captures(html).map(|c| c[1].to_owned()))
}

#[async_trait(?Send)]
impl Provider for StreamingCommunity {
    fn name(&self) -> &'static str {
        "StreamingCommunity"
    }

    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>> {
        let pages = join_all(LANGS.map(|lang| async move {
            self.page(&format!("/{lang}/search"), &[("q", query)]).await
        }))
        .await;
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        for (lang, page) in LANGS.into_iter().zip(pages) {
            match page {
                Ok(page) => entries.extend(
                    self.titles(items(&page["props"]["titles"]), lang)
                        .filter(|e| seen.insert(e.id)),
                ),
                Err(e) => warn!("StreamingCommunity {lang} search failed: {e}"),
            }
        }
        Ok(entries)
    }

    async fn seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<u32>> {
        let page = self.page(&Self::title_path(entry), &[]).await?;
        Ok(parse_seasons(&page))
    }

    async fn episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>> {
        let path = format!("{}/season-{season}", Self::title_path(entry));
        let page = self.page(&path, &[]).await?;
        Ok(self.parse_episodes(&page))
    }

    async fn stream(
        &self,
        entry: &MediaEntry,
        _season: Option<u32>,
        episode: Option<&Episode>,
    ) -> ProviderResult<StreamUrl> {
        let lang = Self::lang(entry);
        let mut iframe = self
            .client
            .get(format!("{}/{lang}/iframe/{}", self.base, entry.id));
        if let Some(ep) = episode {
            iframe = iframe.query(&[
                ("episode_id", ep.id.to_string()),
                ("next_episode", "1".into()),
            ]);
        }
        let src = iframe_src(&fetch_text(iframe).await?)
            .ok_or_else(|| ProviderError::Parse("No iframe src found".into()))?;
        vixcloud_stream(&fetch_text(self.client.get(src)).await?)
    }

    async fn catalog(&self) -> ProviderResult<Vec<MediaEntry>> {
        let page = self.page("", &[]).await?;
        let mut seen = HashSet::new();
        let mut entries = self.home(&page, &mut seen);
        let genres = genres(&page);
        let batches = LANGS
            .into_iter()
            .flat_map(|lang| slider_requests(&genres).map(move |sliders| (lang, sliders)));
        for (lang, sliders) in batches {
            if entries.len() >= CATALOG_SIZE {
                break;
            }
            let request = self
                .client
                .post(format!("{}/api/sliders/fetch", self.base))
                .query(&[("lang", lang)])
                .json(&sliders);
            match fetch_json(request).await {
                Ok(json) => entries.extend(
                    self.titles(items(&json).flat_map(|s| items(&s["titles"])), lang)
                        .filter(|e| seen.insert(e.id)),
                ),
                Err(e) => warn!("StreamingCommunity {lang} sliders failed: {e}"),
            }
        }
        self.browse(|b| *b = Some(Browse::new(genres)));
        entries.truncate(CATALOG_SIZE);
        Ok(entries)
    }

    async fn catalog_more(&self) -> ProviderResult<Vec<MediaEntry>> {
        loop {
            let Some((path, genre, page)) = self.browse(|b| b.as_ref().and_then(Browse::current))
            else {
                return Ok(Vec::new());
            };
            let page = page.to_string();
            let mut query = vec![("lang", DEFAULT_LANG), ("page", page.as_str())];
            if let Some(genre) = &genre {
                query.push(("g", genre.as_str()));
            }
            let json = fetch_json(
                self.client
                    .get(format!("{}/{DEFAULT_LANG}/browse/{path}", self.base))
                    .query(&query)
                    .header("accept", "application/json")
                    .header("x-requested-with", "XMLHttpRequest"),
            )
            .await;
            let count = json
                .as_ref()
                .map_or(0, |json| items(&json["titles"]).count());
            self.browse(|b| {
                if let Some(b) = b {
                    b.advance(count);
                }
            });
            let json = json?;
            let entries: Vec<MediaEntry> =
                self.titles(items(&json["titles"]), DEFAULT_LANG).collect();
            if !entries.is_empty() {
                return Ok(entries);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::serve;

    fn provider() -> StreamingCommunity {
        StreamingCommunity {
            client: Http::new(std::time::Duration::from_secs(1)),
            base: "https://streamingcommunity.test".into(),
            cdn: "https://cdn.streamingcommunity.test".into(),
            browse: Mutex::new(None),
        }
    }

    fn ids(entries: &[MediaEntry]) -> Vec<u64> {
        entries.iter().map(|e| e.id).collect()
    }

    #[test]
    fn cdn_prefixes_the_site_host() {
        let provider = StreamingCommunity::new(Http::new(std::time::Duration::from_secs(1)));
        let host = provider.base.split_once("://").unwrap().1;
        assert_eq!(provider.cdn, format!("https://cdn.{host}"));
    }

    #[test]
    fn parse_title_maps_fields() {
        let title = json!({
            "id": 42, "name": "Dune", "slug": "dune", "type": "movie",
            "original_name": "Dune: Part One", "score": "8.1",
            "translations": [
                { "key": "release_date", "value": null },
                { "key": "release_date", "value": "2021-09-15" },
                { "key": "plot", "value": "  Spice.  " }
            ],
            "images": [
                { "type": "background", "filename": "bg.webp" },
                { "type": "poster" },
                { "type": "poster", "filename": "poster.webp" }
            ]
        });
        let entry = provider().parse_title(&title, "en").unwrap();
        assert_eq!(entry.id, 42);
        assert_eq!(entry.name, "Dune");
        assert_eq!(entry.slug, "dune");
        assert_eq!(entry.language, "en");
        assert_eq!(entry.media_type, MediaType::Movie);
        assert_eq!(entry.alternative_names, ["Dune: Part One"]);
        assert_eq!(entry.year.as_deref(), Some("2021"));
        assert_eq!(entry.score.as_deref(), Some("8.1"));
        assert_eq!(entry.description.as_deref(), Some("Spice."));
        assert_eq!(
            entry.image_url.as_deref(),
            Some("https://cdn.streamingcommunity.test/images/poster.webp")
        );
        assert_eq!(
            entry.backdrop_url.as_deref(),
            Some("https://cdn.streamingcommunity.test/images/bg.webp")
        );
    }

    #[test]
    fn parse_title_falls_back_to_top_level_fields() {
        let title = json!({
            "id": 1, "name": "Show", "type": "tv", "original_name": "Show",
            "last_air_date": "2019-01-01", "plot": " ", "overview": "Story",
            "images": [{ "type": "logo", "filename": "logo.png" }]
        });
        let entry = provider().parse_title(&title, "it").unwrap();
        assert_eq!(entry.media_type, MediaType::Series);
        assert_eq!(entry.alternative_names, Vec::<String>::new());
        assert_eq!(entry.year.as_deref(), Some("2019"));
        assert_eq!(entry.description.as_deref(), Some("Story"));
        assert_eq!(
            entry.image_url.as_deref(),
            Some("https://cdn.streamingcommunity.test/images/logo.png")
        );
        assert_eq!(entry.backdrop_url, None);
    }

    #[test]
    fn parse_title_requires_id_and_name() {
        assert!(
            provider()
                .parse_title(&json!({ "name": "x" }), "it")
                .is_none()
        );
        assert!(provider().parse_title(&json!({ "id": 1 }), "it").is_none());
        assert!(
            provider()
                .parse_title(&json!({ "id": 1, "name": "" }), "it")
                .is_none()
        );
    }

    #[test]
    fn parse_seasons_sorts_numbers() {
        let page = json!({ "props": { "title": { "seasons": [
            { "number": 2 }, { "number": 1 }, { "number": "3" }, { "id": 4 }
        ]}}});
        assert_eq!(parse_seasons(&page), [1, 2]);
        assert_eq!(parse_seasons(&json!({})), Vec::<u32>::new());
    }

    #[test]
    fn parse_episodes_reads_the_loaded_season() {
        let page = json!({ "props": { "loadedSeason": { "episodes": [
            { "id": 12, "number": 2, "name": "Two", "duration": 45, "plot": " Plot ",
              "images": [{ "type": "cover", "filename": "e2.webp" }] },
            { "id": 11, "number": 1, "name": null },
            { "id": 13, "name": "No number" }
        ]}}});
        let episodes = provider().parse_episodes(&page);
        assert_eq!(episodes.len(), 2);
        assert_eq!((episodes[0].id, episodes[0].number), (11, 1));
        assert_eq!(episodes[0].name, "");
        assert_eq!(
            (episodes[1].name.as_str(), episodes[1].duration),
            ("Two", Some(45))
        );
        assert_eq!(episodes[1].description.as_deref(), Some("Plot"));
        assert_eq!(
            episodes[1].image_url.as_deref(),
            Some("https://cdn.streamingcommunity.test/images/e2.webp")
        );
    }

    #[test]
    fn home_reads_unique_slider_titles() {
        let page = json!({ "props": {
            "sliders": [
                { "titles": [{ "id": 1, "name": "A" }, { "id": 2, "name": "B" }] },
                { "titles": [{ "id": 2, "name": "B" }, { "id": 3, "name": "C" }] }
            ],
            "titles": [{ "id": 9, "name": "Unused" }]
        }});
        let mut seen = HashSet::new();
        let entries = provider().home(&page, &mut seen);
        assert_eq!(ids(&entries), [1, 2, 3]);
        assert!(entries.iter().all(|e| e.language == DEFAULT_LANG));
        assert_eq!(seen, HashSet::from([1, 2, 3]));
    }

    #[test]
    fn home_falls_back_to_page_titles() {
        let page = json!({ "props": {
            "sliders": [{ "titles": [] }],
            "titles": [{ "id": 4, "name": "D" }, { "id": 4, "name": "D" }]
        }});
        assert_eq!(ids(&provider().home(&page, &mut HashSet::new())), [4]);
    }

    #[tokio::test]
    async fn catalog_more_reuses_catalog_genres_and_skips_failed_pages() {
        let (base, paths) = serve(&[
            (
                200,
                r#"<div id="app" data-page='{"props":{"titles":[{"id":1,"name":"A"}]}}'></div>"#,
            ),
            (404, ""),
            (200, r#"{"titles":[{"id":2,"name":"B"}]}"#),
        ])
        .await;
        let provider = StreamingCommunity { base, ..provider() };
        assert_eq!(ids(&provider.catalog().await.unwrap()), [1]);
        assert!(provider.catalog_more().await.is_err());
        assert_eq!(ids(&provider.catalog_more().await.unwrap()), [2]);
        assert!(provider.catalog_more().await.unwrap().is_empty());
        assert_eq!(
            *paths.lock().unwrap(),
            [
                "/",
                "/it/browse/trending?lang=it&page=1",
                "/it/browse/latest?lang=it&page=1"
            ]
        );
    }

    #[test]
    fn title_path_defaults_language() {
        let entry = MediaEntry {
            id: 5,
            slug: "x".into(),
            ..Default::default()
        };
        assert_eq!(StreamingCommunity::title_path(&entry), "/it/titles/5-x");
    }

    #[test]
    fn data_page_reads_app_attribute() {
        let html = r#"<div id="app" data-page="{&quot;version&quot;:&quot;v1&quot;}"></div>"#;
        assert_eq!(data_page(html).unwrap()["version"], "v1");
        assert!(data_page("<div id=\"app\"></div>").is_err());
    }

    #[test]
    fn slider_requests_respect_the_server_limit() {
        let genres: Vec<String> = (0..13).map(|i| format!("g{i}")).collect();
        let requests: Vec<Value> = slider_requests(&genres).collect();
        assert_eq!(requests.len(), 3);
        assert!(
            requests
                .iter()
                .all(|r| items(&r["sliders"]).count() <= SLIDERS_PER_REQUEST)
        );
        assert_eq!(
            requests[2],
            json!({ "sliders": [{ "name": "genre", "genre": "g12" }] })
        );
        assert_eq!(slider_requests(&[]).count(), 0);
    }

    #[test]
    fn iframe_src_prefers_src_then_data_src() {
        assert_eq!(
            iframe_src(r#"<iframe src="https://a"></iframe>"#).as_deref(),
            Some("https://a")
        );
        assert_eq!(
            iframe_src(r#"<iframe data-src="https://b"></iframe>"#).as_deref(),
            Some("https://b")
        );
        assert_eq!(
            iframe_src(r#"<script>w('<iframe width="1" src="https://c">')</script>"#).as_deref(),
            Some("https://c")
        );
        assert!(iframe_src("<p></p>").is_none());
    }

    #[test]
    fn browse_pages_through_sections() {
        let mut browse = Browse::new(vec!["Drama".into()]);
        assert_eq!(browse.current(), Some(("trending", None, 1)));
        browse.advance(BROWSE_PAGE_SIZE);
        assert_eq!(browse.current(), Some(("trending", None, 2)));
        browse.advance(BROWSE_PAGE_SIZE - 1);
        assert_eq!(browse.current(), Some(("latest", None, 1)));
        browse.advance(0);
        assert_eq!(browse.current(), Some(("genre", Some("Drama".into()), 1)));
        browse.advance(0);
        assert_eq!(browse.current(), None);
    }

    #[test]
    fn browse_stops_at_page_limit() {
        let mut browse = Browse::new(Vec::new());
        for _ in 1..BROWSE_MAX_PAGES {
            browse.advance(BROWSE_PAGE_SIZE);
        }
        assert_eq!(browse.current(), Some(("trending", None, BROWSE_MAX_PAGES)));
        browse.advance(BROWSE_PAGE_SIZE);
        assert_eq!(browse.current(), Some(("latest", None, 1)));
    }
}
