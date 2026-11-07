mod animeunity;
mod aurora;
mod models;
mod raiplay;
mod streaming_community;

pub use models::*;

use crate::i18n::Lang;
use crate::time::sleep;
use animeunity::AnimeUnity;
use async_trait::async_trait;
use aurora::Aurora;
use dioxus::logger::tracing::{info, warn};
use futures::future::join_all;
use raiplay::RaiPlay;
use regex::Regex;
use reqwest::header::REFERER;
use reqwest::{Client, IntoUrl, RequestBuilder, Response, Url};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use streaming_community::StreamingCommunity;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const DOMAINS_URL: &str =
    "https://raw.githubusercontent.com/falcosan/StreamVault/main/assets/js/domains.json";
const DOMAINS_TIMEOUT: Duration = Duration::from_secs(5);
const ATTEMPTS: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_millis(400);
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

#[derive(Debug)]
pub enum ProviderError {
    Network(String),
    Parse(String),
    StreamExtraction(String),
    UnknownProvider(usize),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(m) => write!(f, "Network error: {m}"),
            Self::Parse(m) => write!(f, "Parse error: {m}"),
            Self::StreamExtraction(m) => write!(f, "Stream extraction error: {m}"),
            Self::UnknownProvider(index) => write!(f, "Unknown provider {index}"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e.without_url().to_string())
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(e: serde_json::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

pub type ProviderResult<T> = Result<T, ProviderError>;

#[async_trait(?Send)]
pub trait Provider {
    fn name(&self) -> &'static str;
    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>>;
    async fn seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<u32>>;
    async fn details(&self, entry: &MediaEntry) -> ProviderResult<Details> {
        let seasons = if entry.is_movie() {
            Vec::new()
        } else {
            self.seasons(entry).await?
        };
        Ok(Details {
            entry: entry.clone(),
            seasons,
        })
    }
    async fn episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>>;
    async fn stream(
        &self,
        entry: &MediaEntry,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) -> ProviderResult<StreamUrl>;
    async fn catalog(&self) -> ProviderResult<Vec<MediaEntry>> {
        Ok(Vec::new())
    }
    async fn catalog_more(&self) -> ProviderResult<Vec<MediaEntry>> {
        Ok(Vec::new())
    }
}

#[derive(Deserialize)]
struct Domain {
    full_url: String,
}

type Domains = HashMap<String, Domain>;

#[derive(Clone)]
pub struct Providers(Arc<[Arc<dyn Provider>]>);

impl Providers {
    pub async fn load(timeout: Duration, lang: Lang) -> Self {
        let http = Http::new(timeout);
        let mut domains = domains(&http, DOMAINS_URL).await;
        follow_moves(&http, &mut domains).await;
        Self::with(http, &domains, lang)
    }

    fn with(shared: Http, domains: &Domains, lang: Lang) -> Self {
        let list: Vec<Arc<dyn Provider>> = vec![
            Arc::new(StreamingCommunity::new(
                shared.clone(),
                domain(domains, "streamingcommunity"),
                lang,
            )),
            Arc::new(RaiPlay::new(shared.clone())),
            Arc::new(Aurora::new(shared.clone(), "nove", "Nove")),
            Arc::new(Aurora::new(shared.clone(), "realtime", "RealTime")),
            Arc::new(Aurora::new(shared.clone(), "dmaxit", "DMAX")),
            Arc::new(Aurora::new(shared.clone(), "foodnetwork", "FoodNetwork")),
            Arc::new(Aurora::new(
                shared.clone(),
                "discoverychannelit",
                "Discovery",
            )),
            Arc::new(AnimeUnity::new(shared, domain(domains, "animeunity"))),
        ];
        Self(list.into())
    }

    pub fn get(&self, index: usize) -> ProviderResult<Arc<dyn Provider>> {
        self.0
            .get(index)
            .cloned()
            .ok_or(ProviderError::UnknownProvider(index))
    }

    pub async fn search(&self, query: &str) -> Vec<MediaEntry> {
        self.gather("search", |p| async move { p.search(query).await })
            .await
    }

    pub async fn catalog(&self) -> Vec<MediaEntry> {
        self.gather("catalog", |p| async move { p.catalog().await })
            .await
    }

    pub async fn catalog_more(&self, seen: &mut HashSet<(usize, u64)>) -> Option<Vec<MediaEntry>> {
        loop {
            let mut fresh = Vec::new();
            let mut exhausted = true;
            let mut failed = false;
            for (index, provider) in self.0.iter().enumerate() {
                match provider.catalog_more().await {
                    Ok(batch) => {
                        exhausted &= batch.is_empty();
                        fresh.extend(
                            tag(index, provider.name(), batch).filter(|e| seen.insert(e.key())),
                        );
                    }
                    Err(e) => {
                        warn!("{} catalog page failed: {e}", provider.name());
                        failed = true;
                    }
                }
            }
            if failed || !fresh.is_empty() {
                return Some(fresh);
            }
            if exhausted {
                return None;
            }
        }
    }

    async fn gather<F, Fut>(&self, action: &'static str, call: F) -> Vec<MediaEntry>
    where
        F: Fn(Arc<dyn Provider>) -> Fut,
        Fut: Future<Output = ProviderResult<Vec<MediaEntry>>>,
    {
        let requests = self.0.iter().enumerate().map(|(index, provider)| {
            let name = provider.name();
            let request = call(provider.clone());
            async move {
                match request.await {
                    Ok(entries) => tag(index, name, entries).collect(),
                    Err(e) => {
                        warn!("{name} {action} failed: {e}");
                        Vec::new()
                    }
                }
            }
        });
        join_all(requests).await.into_iter().flatten().collect()
    }
}

fn tag(index: usize, name: &str, entries: Vec<MediaEntry>) -> impl Iterator<Item = MediaEntry> {
    entries.into_iter().map(move |entry| MediaEntry {
        provider: index,
        provider_name: name.to_owned(),
        ..entry
    })
}

#[derive(Clone)]
struct Http {
    client: Client,
    timeout: Duration,
}

impl Http {
    fn new(timeout: Duration) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("HTTP client must initialize");
        Self { client, timeout }
    }

    fn get(&self, url: impl IntoUrl) -> RequestBuilder {
        self.client.get(url).timeout(self.timeout)
    }

    fn post(&self, url: impl IntoUrl) -> RequestBuilder {
        self.client.post(url).timeout(self.timeout)
    }
}

async fn fetch_json(request: RequestBuilder) -> ProviderResult<Value> {
    let bytes = send(request).await?.bytes().await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn fetch_text(request: RequestBuilder) -> ProviderResult<String> {
    Ok(send(request).await?.text().await?)
}

async fn send(request: RequestBuilder) -> ProviderResult<Response> {
    let mut delay = RETRY_DELAY;
    for _ in 1..ATTEMPTS {
        let Some(attempt) = request.try_clone() else {
            break;
        };
        let response = attempt.send().await?;
        if !response.status().is_server_error() {
            return Ok(response.error_for_status()?);
        }
        sleep(delay).await;
        delay *= 2;
    }
    Ok(request.send().await?.error_for_status()?)
}

async fn domains(http: &Http, url: &str) -> Domains {
    let mut domains = built_in_domains();
    let fetched = fetch_json(http.client.get(url).timeout(DOMAINS_TIMEOUT))
        .await
        .and_then(|value| Ok(serde_json::from_value::<Domains>(value)?));
    match fetched {
        Ok(fetched) => domains.extend(fetched),
        Err(e) => warn!("domains fetch failed, using the built-in ones: {e}"),
    }
    domains
}

async fn follow_moves(http: &Http, domains: &mut Domains) {
    join_all(domains.iter_mut().map(|(site, domain)| async move {
        let request = http.client.get(&domain.full_url).timeout(DOMAINS_TIMEOUT);
        match request.send().await {
            Ok(response) => {
                if let Some(origin) = moved_origin(&domain.full_url, response.url()) {
                    info!("{site} moved to {origin}");
                    domain.full_url = origin;
                }
            }
            Err(e) => warn!(
                "{site} domain check failed, keeping {}: {e}",
                domain.full_url
            ),
        }
    }))
    .await;
}

fn moved_origin(from: &str, to: &Url) -> Option<String> {
    let from = Url::parse(from).ok()?;
    (to.scheme() == from.scheme() && to.origin() != from.origin())
        .then(|| to.origin().ascii_serialization())
}

fn built_in_domains() -> Domains {
    serde_json::from_str(include_str!("../../assets/js/domains.json"))
        .expect("assets/js/domains.json must be valid")
}

fn domain(domains: &Domains, key: &str) -> String {
    domains
        .get(key)
        .map(|d| d.full_url.trim_end_matches('/').to_owned())
        .unwrap_or_default()
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn uint(value: &Value) -> Option<u32> {
    u32::try_from(value.as_u64()?).ok()
}

fn year(date: &str) -> Option<String> {
    date.get(..4)
        .filter(|y| y.bytes().all(|b| b.is_ascii_digit()))
        .map(String::from)
}

fn non_blank(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

pub fn stable_hash(text: &str) -> u64 {
    text.bytes()
        .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(u64::from(b)))
        & MAX_SAFE_INTEGER
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(module = "/assets/js/referer.js")]
extern "C" {
    #[wasm_bindgen(js_name = refererMissing)]
    fn referer_missing() -> bool;

    #[wasm_bindgen(catch, js_name = fetchFromSite)]
    async fn fetch_from_site(url: &str, seconds: f64) -> Result<JsValue, JsValue>;
}

async fn fetch_vixcloud(
    http: &Http,
    site: &str,
    embed: impl AsyncFn() -> ProviderResult<String>,
) -> ProviderResult<StreamUrl> {
    let referer = format!("{site}/");
    let fetch = async |url: String| {
        #[cfg(target_arch = "wasm32")]
        if referer_missing() {
            return fetch_from_site(&url, http.timeout.as_secs_f64())
                .await
                .map(|page| page.as_string().unwrap_or_default())
                .map_err(|e| ProviderError::Network(e.as_string().unwrap_or_default()));
        }
        fetch_text(http.get(url).header(REFERER, &referer)).await
    };
    let player = match fetch(embed().await?).await {
        Ok(player) => player,
        Err(e) => {
            warn!("vixcloud embed failed, retrying with a new link: {e}");
            fetch(embed().await?).await?
        }
    };
    vixcloud_stream(&player)
}

fn vixcloud_stream(player: &str) -> ProviderResult<StreamUrl> {
    let url = vixcloud_playlist(player)
        .ok_or_else(|| ProviderError::StreamExtraction("No HLS params in script".into()))?;
    Ok(StreamUrl {
        url,
        #[cfg(feature = "desktop")]
        headers: vec![("User-Agent".into(), USER_AGENT.into())],
    })
}

fn vixcloud_playlist(player: &str) -> Option<String> {
    static TOKEN: LazyLock<Regex> = LazyLock::new(|| field("token", "[^'\"]+"));
    static EXPIRES: LazyLock<Regex> = LazyLock::new(|| field("expires", "[^'\"]+"));
    static URL: LazyLock<Regex> = LazyLock::new(|| field("url", "https?://[^'\"]+"));
    static FHD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"window\.canPlayFHD\s*=\s*true\b").unwrap());

    let capture = |re: &Regex| re.captures(player).map(|c| c[1].to_owned());
    let token = capture(&TOKEN)?;
    let expires = capture(&EXPIRES)?;
    let mut url = Url::parse(&capture(&URL)?).ok()?;
    let fhd = FHD.is_match(player);
    let bandwidth = url.query_pairs().any(|(k, v)| k == "b" && v == "1");
    {
        let mut query = url.query_pairs_mut();
        query.clear();
        if fhd {
            query.append_pair("h", "1");
        }
        if bandwidth {
            query.append_pair("b", "1");
        }
        query
            .append_pair("token", &token)
            .append_pair("expires", &expires);
    }
    Some(url.into())
}

fn field(name: &str, value: &str) -> Regex {
    Regex::new(&format!(
        r#"(?:['"]{name}['"]|\b{name})\s*:\s*['"]({value})['"]"#
    ))
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::serve;

    struct Fake {
        name: &'static str,
        pages: std::sync::Mutex<Vec<ProviderResult<Vec<MediaEntry>>>>,
    }

    fn fake(name: &'static str, pages: Vec<ProviderResult<Vec<MediaEntry>>>) -> Arc<dyn Provider> {
        Arc::new(Fake {
            name,
            pages: std::sync::Mutex::new(pages),
        })
    }

    impl Fake {
        fn next(&self) -> ProviderResult<Vec<MediaEntry>> {
            let mut pages = self.pages.lock().unwrap();
            if pages.is_empty() {
                Ok(Vec::new())
            } else {
                pages.remove(0)
            }
        }
    }

    #[async_trait(?Send)]
    impl Provider for Fake {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn search(&self, _: &str) -> ProviderResult<Vec<MediaEntry>> {
            self.next()
        }
        async fn seasons(&self, _: &MediaEntry) -> ProviderResult<Vec<u32>> {
            Ok(Vec::new())
        }
        async fn episodes(&self, _: &MediaEntry, _: u32) -> ProviderResult<Vec<Episode>> {
            Ok(Vec::new())
        }
        async fn stream(
            &self,
            _: &MediaEntry,
            _: Option<u32>,
            _: Option<&Episode>,
        ) -> ProviderResult<StreamUrl> {
            Err(ProviderError::StreamExtraction("fake".into()))
        }
        async fn catalog_more(&self) -> ProviderResult<Vec<MediaEntry>> {
            self.next()
        }
    }

    fn entry(id: u64) -> MediaEntry {
        MediaEntry {
            id,
            ..Default::default()
        }
    }

    fn failure() -> ProviderResult<Vec<MediaEntry>> {
        Err(ProviderError::Network("down".into()))
    }

    #[tokio::test]
    async fn send_retries_transient_server_errors() {
        let (url, requests) = serve(&[(500, "ok"), (200, "ok")]).await;
        assert_eq!(fetch_text(Client::new().get(url)).await.unwrap(), "ok");
        assert_eq!(requests.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn send_gives_up_after_all_attempts() {
        let (url, requests) = serve(&[(503, ""), (500, ""), (502, ""), (504, "")]).await;
        assert!(fetch_text(Client::new().get(url)).await.is_err());
        assert_eq!(requests.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn send_does_not_retry_client_errors() {
        let (url, requests) = serve(&[(404, ""), (200, "ok")]).await;
        assert!(fetch_text(Client::new().get(url)).await.is_err());
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn fetch_json_reports_invalid_json_as_parse_error() {
        let (url, _) = serve(&[(200, "ok"), (200, r#"{"a":1}"#)]).await;
        let client = Client::new();
        assert!(matches!(
            fetch_json(client.get(&url)).await,
            Err(ProviderError::Parse(_))
        ));
        assert_eq!(fetch_json(client.get(&url)).await.unwrap()["a"], 1);
    }

    #[tokio::test]
    async fn send_does_not_retry_connection_failures() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        drop(listener);
        let started = std::time::Instant::now();
        assert!(fetch_text(Client::new().get(url)).await.is_err());
        assert!(started.elapsed() < RETRY_DELAY);
    }

    #[test]
    fn provider_order_is_stable_because_indices_are_persisted() {
        let http = Http::new(Duration::from_secs(1));
        let names: Vec<_> = Providers::with(http, &built_in_domains(), Lang::En)
            .0
            .iter()
            .map(|p| p.name())
            .collect();
        assert_eq!(
            names,
            [
                "StreamingCommunity",
                "RaiPlay",
                "Nove",
                "RealTime",
                "DMAX",
                "FoodNetwork",
                "Discovery",
                "AnimeUnity"
            ]
        );
    }

    #[test]
    fn get_rejects_unknown_index() {
        let providers = Providers(vec![fake("A", vec![])].into());
        assert_eq!(providers.get(0).unwrap().name(), "A");
        assert!(matches!(
            providers.get(1),
            Err(ProviderError::UnknownProvider(1))
        ));
    }

    #[tokio::test]
    async fn search_tags_entries_and_skips_failed_providers() {
        let providers = Providers(
            vec![
                fake("A", vec![failure()]),
                fake("B", vec![Ok(vec![entry(7)])]),
            ]
            .into(),
        );
        let found = providers.search("q").await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key(), (1, 7));
        assert_eq!(found[0].provider_name, "B");
    }

    #[tokio::test]
    async fn catalog_more_skips_duplicate_pages() {
        let providers = Providers(
            vec![fake(
                "A",
                vec![Ok(vec![entry(1)]), Ok(vec![entry(1), entry(2)])],
            )]
            .into(),
        );
        let mut seen = HashSet::from([(0, 1)]);
        let fresh = providers.catalog_more(&mut seen).await.unwrap();
        assert_eq!(fresh.iter().map(|e| e.id).collect::<Vec<_>>(), [2]);
    }

    #[tokio::test]
    async fn catalog_more_ends_only_when_every_provider_is_exhausted() {
        let done = Providers(vec![fake("A", vec![])].into());
        assert!(done.catalog_more(&mut HashSet::new()).await.is_none());
        let broken = Providers(vec![fake("A", vec![failure()])].into());
        assert_eq!(
            broken.catalog_more(&mut HashSet::new()).await,
            Some(Vec::new())
        );
        assert!(broken.catalog_more(&mut HashSet::new()).await.is_none());
    }

    #[test]
    fn embedded_domains_resolve_to_https_without_trailing_slash() {
        let domains = built_in_domains();
        for key in ["streamingcommunity", "animeunity"] {
            let url = domain(&domains, key);
            assert!(url.starts_with("https://"), "{key}: {url}");
            assert!(!url.ends_with('/'), "{key}: {url}");
        }
        assert_eq!(domain(&domains, "missing"), "");
    }

    #[tokio::test]
    async fn domains_from_the_repo_win_and_the_built_in_ones_fill_the_gaps() {
        let (url, _) = serve(&[
            (
                200,
                r#"{"streamingcommunity":{"full_url":"https://sc.new/"}}"#,
            ),
            (404, ""),
        ])
        .await;
        let (http, built_in) = (Http::new(Duration::from_secs(1)), built_in_domains());
        let fetched = domains(&http, &url).await;
        assert_eq!(domain(&fetched, "streamingcommunity"), "https://sc.new");
        assert_eq!(
            domain(&fetched, "animeunity"),
            domain(&built_in, "animeunity")
        );
        let unavailable = domains(&http, &url).await;
        assert_eq!(
            domain(&unavailable, "streamingcommunity"),
            domain(&built_in, "streamingcommunity")
        );
    }

    #[tokio::test]
    async fn a_moved_site_is_followed_and_an_unreachable_one_is_kept() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (new, _) = serve(&[(200, "")]).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let old = format!("http://{}/", listener.local_addr().unwrap());
        let redirect = format!(
            "HTTP/1.1 301 X\r\nlocation: {new}/it\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        );
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let _ = socket.read(&mut [0; 4096]).await;
            socket.write_all(redirect.as_bytes()).await.unwrap();
        });
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let down = format!("http://{}/", closed.local_addr().unwrap());
        drop(closed);
        let mut domains = Domains::from([
            ("moved".into(), Domain { full_url: old }),
            (
                "down".into(),
                Domain {
                    full_url: down.clone(),
                },
            ),
        ]);
        follow_moves(&Http::new(Duration::from_secs(1)), &mut domains).await;
        assert_eq!(domain(&domains, "moved"), new);
        assert_eq!(domains["down"].full_url, down);
    }

    #[test]
    fn moved_origin_needs_another_host_on_the_same_scheme() {
        let to = |url| Url::parse(url).unwrap();
        assert_eq!(
            moved_origin("https://sc.old/", &to("https://www.sc.new/it?x=1")).as_deref(),
            Some("https://www.sc.new")
        );
        assert_eq!(
            moved_origin("https://sc.old", &to("https://sc.old/it")),
            None
        );
        assert_eq!(moved_origin("https://sc.old", &to("http://sc.new/")), None);
        assert_eq!(moved_origin("", &to("https://sc.new/")), None);
    }

    #[test]
    fn stable_hash_is_pinned_because_ids_are_persisted() {
        assert_eq!(stable_hash("abc"), 96354);
        assert_eq!(stable_hash(""), 0);
        assert_eq!(
            stable_hash("/programmi/montalbano.json"),
            2_640_782_405_220_837
        );
    }

    #[test]
    fn stable_hash_stays_exact_as_a_json_number() {
        let id = stable_hash("/programmi/montalbano.json");
        assert!(id > u64::from(u32::MAX) && id <= MAX_SAFE_INTEGER);
    }

    #[tokio::test]
    async fn network_errors_never_repeat_the_signed_address() {
        let (url, _) = serve(&[(403, "")]).await;
        let error = fetch_text(Client::new().get(format!("{url}/embed?token=secret")))
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("secret"), "{error}");
        assert!(error.to_string().contains("403"), "{error}");
    }

    #[test]
    fn non_blank_trims_and_rejects_empty() {
        assert_eq!(non_blank("  x  ").as_deref(), Some("x"));
        assert_eq!(non_blank("   "), None);
    }

    #[test]
    fn uint_rejects_values_outside_u32() {
        assert_eq!(uint(&serde_json::json!(7)), Some(7));
        assert_eq!(uint(&serde_json::json!(u64::from(u32::MAX) + 1)), None);
        assert_eq!(uint(&serde_json::json!(-1)), None);
        assert_eq!(uint(&serde_json::json!("7")), None);
    }

    #[test]
    fn year_reads_four_leading_digits() {
        assert_eq!(year("2021-09-15").as_deref(), Some("2021"));
        assert_eq!(year("1999").as_deref(), Some("1999"));
        assert_eq!(year("Prima-2001"), None);
        assert_eq!(year("202"), None);
        assert_eq!(year(""), None);
    }

    #[tokio::test]
    async fn an_expired_vixcloud_link_is_fetched_again_once() {
        const PLAYER: &str = "<script>window.masterPlaylist = { params: { 'token': 't', 'expires': 'e' }, url: 'https://v.co/p/1' }</script>";
        let (url, requests) = serve(&[(410, ""), (200, PLAYER), (410, ""), (410, "")]).await;
        let http = Http::new(Duration::from_secs(1));
        let links = std::cell::Cell::new(0);
        let embed = async || {
            links.set(links.get() + 1);
            Ok::<_, ProviderError>(url.clone())
        };
        assert_eq!(
            fetch_vixcloud(&http, "https://site", &embed)
                .await
                .unwrap()
                .url,
            "https://v.co/p/1?token=t&expires=e"
        );
        assert!(fetch_vixcloud(&http, "https://site", &embed).await.is_err());
        assert_eq!((links.get(), requests.lock().unwrap().len()), (4, 4));
    }

    #[tokio::test]
    async fn every_vixcloud_embed_request_names_the_site_as_referer() {
        let (url, requests) = serve(&[(403, ""), (403, "")]).await;
        let http = Http::new(Duration::from_secs(1));
        let embed = async || Ok::<_, ProviderError>(url.clone());
        assert!(fetch_vixcloud(&http, "https://site", &embed).await.is_err());
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|r| r.contains("referer: https://site/"))
        );
    }

    #[test]
    fn vixcloud_stream_reads_the_embed_page() {
        let player = r#"<html><head><script src="/app.js"></script></head><body>
            <script>
                window.video = { id: '262696', filename: '', timestamps: [], };
                window.streams = [{"name":"Server1","active":true,"url":"https:\/\/vixcloud.cc\/playlist\/262696?ub=1"}];
                window.masterPlaylist = {
                    params: { 'token': 'tok', 'expires': '1798073132', 'asn': '', },
                    url: 'https://vixcloud.cc/playlist/262696',
                }
                window.canPlayFHD = true
            </script>
            <script>window.downloadUrl = 'https://animeunity.so/video/262696'</script>
        </body></html>"#;
        let stream = vixcloud_stream(player).unwrap();
        assert_eq!(
            stream.url,
            "https://vixcloud.cc/playlist/262696?h=1&token=tok&expires=1798073132"
        );
        #[cfg(feature = "desktop")]
        assert_eq!(
            stream.headers,
            [("User-Agent".to_owned(), USER_AGENT.to_owned())]
        );
        assert!(vixcloud_stream("<body><script>var a = 1</script></body>").is_err());
    }

    #[test]
    fn vixcloud_playlist_builds_signed_url() {
        let script = "window.masterPlaylist = { params: { 'token': 'abc', 'expires': '123', }, url: 'https://vixcloud.co/playlist/9?b=1&x=2', }; window.canPlayFHD = true";
        assert_eq!(
            vixcloud_playlist(script).as_deref(),
            Some("https://vixcloud.co/playlist/9?h=1&b=1&token=abc&expires=123")
        );
    }

    #[test]
    fn vixcloud_playlist_omits_optional_flags() {
        let script =
            r#"token: "t", expires: "e", url: "https://v.co/p/1"; window.canPlayFHD = false"#;
        assert_eq!(
            vixcloud_playlist(script).as_deref(),
            Some("https://v.co/p/1?token=t&expires=e")
        );
    }

    #[test]
    fn vixcloud_stream_keeps_the_address_of_the_page_as_written() {
        let player = "<script>window.masterPlaylist = { params: { 'token': 't', 'expires': 'e' }, url: 'https://v.co/p/1?ub=1&b=1' }</script>";
        assert_eq!(
            vixcloud_stream(player).unwrap().url,
            "https://v.co/p/1?b=1&token=t&expires=e"
        );
    }

    #[test]
    fn vixcloud_playlist_ignores_longer_keys() {
        let script = "a = { csrf_token: 'no', poster_url: 'https://img/p.jpg' }; b = { 'token': 't', 'expires': 'e', url: 'https://v.co/p/1' }";
        assert_eq!(
            vixcloud_playlist(script).as_deref(),
            Some("https://v.co/p/1?token=t&expires=e")
        );
    }

    #[test]
    fn vixcloud_playlist_requires_token() {
        assert!(vixcloud_playlist(r#"expires: "e", url: "https://v.co/p/1""#).is_none());
    }

    #[cfg(target_os = "macos")]
    mod referer {
        use crate::check::run_js;
        use serde_json::{Value, json};

        const SCRIPT: &str = include_str!("../../assets/js/referer.js");

        fn fetch_from_site(status: u16, loads: bool) -> Value {
            run_js(&format!(
                r#"
                var location = {{ protocol: "file:" }};
                var AbortSignal = {{
                  timeout: () => {{
                    const signal = {{ reason: new Error("signal timed out") }};
                    if (!{loads}) setTimeout(() => signal.onabort(), 0);
                    return signal;
                  }},
                }};
                var URL = class {{
                  constructor(path, base) {{
                    this.href = base.match(/^https:\/\/[^/]+/)[0] + path;
                  }}
                }};
                var frames = [];
                var document = {{
                  body: {{
                    append: (frame) => {{
                      frames.push(frame);
                      if ({loads}) setTimeout(frame.onload, 0);
                    }},
                  }},
                  createElement: () => {{
                    const frame = {{
                      removed: false,
                      remove: () => (frame.removed = true),
                      contentWindow: {{
                        fetch: async (url) => ({{
                          ok: {ok},
                          status: {status},
                          text: async () => `${{url}} from ${{frame.src}}`,
                        }}),
                      }},
                    }};
                    return frame;
                  }},
                }};
                {script}
                var left = () => frames.map(({{ sandbox, removed }}) => ({{ sandbox, removed }}));
                fetchFromSite("https://vixcloud.cc/embed/1?token=t", 30).then(
                  (page) => JSON.stringify({{ page, missing: refererMissing(), frames: left() }}),
                  (error) => JSON.stringify({{ error, frames: left() }}),
                )
                "#,
                ok = status == 200,
                script = SCRIPT.replace("export ", ""),
            ))
        }

        #[test]
        fn a_page_that_sends_no_referer_fetches_inside_a_sandboxed_frame_of_the_site() {
            assert_eq!(
                fetch_from_site(200, true),
                json!({
                    "page": "https://vixcloud.cc/embed/1?token=t from https://vixcloud.cc/robots.txt",
                    "missing": true,
                    "frames": [{ "sandbox": "allow-same-origin", "removed": true }],
                })
            );
        }

        #[test]
        fn a_refused_page_fails_and_leaves_no_frame() {
            assert_eq!(
                fetch_from_site(403, true),
                json!({
                    "error": "HTTP 403",
                    "frames": [{ "sandbox": "allow-same-origin", "removed": true }],
                })
            );
        }

        #[test]
        fn a_frame_that_never_loads_times_out_and_leaves_no_frame() {
            assert_eq!(
                fetch_from_site(200, false),
                json!({
                    "error": "signal timed out",
                    "frames": [{ "sandbox": "allow-same-origin", "removed": true }],
                })
            );
        }
    }
}
