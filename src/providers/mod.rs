mod animeunity;
mod aurora;
mod models;
mod raiplay;
mod streaming_community;

pub use models::*;

use crate::time::sleep;
use animeunity::AnimeUnity;
use async_trait::async_trait;
use aurora::Aurora;
use dioxus::logger::tracing::warn;
use futures::future::join_all;
use raiplay::RaiPlay;
use regex::Regex;
use reqwest::{Client, ClientBuilder, IntoUrl, RequestBuilder, Response, Url};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use streaming_community::StreamingCommunity;

const ATTEMPTS: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_millis(400);
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

#[derive(Debug)]
pub enum ProviderError {
    Network(String),
    Parse(String),
    StreamExtraction(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(m) => write!(f, "Network error: {m}"),
            Self::Parse(m) => write!(f, "Parse error: {m}"),
            Self::StreamExtraction(m) => write!(f, "Stream extraction error: {m}"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<reqwest::Error> for ProviderError {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e.to_string())
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

#[derive(Clone)]
pub struct Providers(Arc<[Arc<dyn Provider>]>);

impl Providers {
    pub fn new(timeout: Duration) -> Self {
        let shared = Http::new(Client::builder(), timeout);
        let list: Vec<Arc<dyn Provider>> = vec![
            Arc::new(StreamingCommunity::new(Http::new(
                with_cookies(Client::builder()),
                timeout,
            ))),
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
            Arc::new(AnimeUnity::new(shared)),
        ];
        Self(list.into())
    }

    pub fn get(&self, index: usize) -> ProviderResult<Arc<dyn Provider>> {
        self.0
            .get(index)
            .cloned()
            .ok_or_else(|| ProviderError::Parse(format!("Unknown provider {index}")))
    }

    pub async fn search(&self, query: &str) -> Vec<MediaEntry> {
        self.gather("search", |p| {
            let query = query.to_owned();
            async move { p.search(&query).await }
        })
        .await
    }

    pub async fn catalog(&self) -> Vec<MediaEntry> {
        self.gather("catalog", |p| async move { p.catalog().await })
            .await
    }

    pub async fn catalog_more(
        &self,
        seen: &mut HashSet<(usize, u64)>,
    ) -> ProviderResult<Option<Vec<MediaEntry>>> {
        loop {
            let mut fresh = Vec::new();
            let mut exhausted = true;
            let mut failure = None;
            for (index, provider) in self.0.iter().enumerate() {
                match provider.catalog_more().await {
                    Ok(batch) => {
                        exhausted &= batch.is_empty();
                        fresh.extend(
                            tag(index, provider.name(), batch).filter(|e| seen.insert(e.key())),
                        );
                    }
                    Err(e) => failure = Some(e),
                }
            }
            if !fresh.is_empty() {
                return Ok(Some(fresh));
            }
            if let Some(e) = failure {
                return Err(e);
            }
            if exhausted {
                return Ok(None);
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
    fn new(builder: ClientBuilder, timeout: Duration) -> Self {
        let client = builder
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

#[cfg(not(target_arch = "wasm32"))]
fn with_cookies(builder: ClientBuilder) -> ClientBuilder {
    builder.cookie_store(true)
}

#[cfg(target_arch = "wasm32")]
fn with_cookies(builder: ClientBuilder) -> ClientBuilder {
    builder
}

async fn fetch_json(request: RequestBuilder) -> ProviderResult<Value> {
    Ok(send(request).await?.json().await?)
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

fn domain(key: &str) -> String {
    #[derive(Deserialize)]
    struct Domain {
        full_url: String,
    }
    static DOMAINS: LazyLock<HashMap<String, Domain>> = LazyLock::new(|| {
        serde_json::from_str(include_str!("../../assets/js/domains.json"))
            .expect("assets/js/domains.json must be valid")
    });
    DOMAINS
        .get(key)
        .map(|d| d.full_url.trim_end_matches('/').to_owned())
        .unwrap_or_default()
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn non_blank(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

pub fn stable_hash(text: &str) -> u64 {
    text.bytes()
        .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(u64::from(b)))
}

fn vixcloud_playlist(script: &str) -> Option<String> {
    static TOKEN: LazyLock<Regex> = LazyLock::new(|| field("token", "[^'\"]+"));
    static EXPIRES: LazyLock<Regex> = LazyLock::new(|| field("expires", "[^'\"]+"));
    static URL: LazyLock<Regex> = LazyLock::new(|| field("url", "https?://[^'\"]+"));
    static FHD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"window\.canPlayFHD\s*=\s*(true|false)").unwrap());

    let capture = |re: &Regex| re.captures(script).map(|c| c[1].to_owned());
    let token = capture(&TOKEN)?;
    let expires = capture(&EXPIRES)?;
    let mut url = Url::parse(&capture(&URL)?).ok()?;
    let fhd = capture(&FHD).is_some_and(|v| v == "true");
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
        r#"(?:['"]{name}['"]|{name})\s*:\s*['"]({value})['"]"#
    ))
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    async fn serve(statuses: &'static [u16]) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            for status in statuses {
                let (mut socket, _) = listener.accept().await.unwrap();
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = socket.read(&mut [0; 1024]).await;
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (url, hits)
    }

    #[tokio::test]
    async fn send_retries_transient_server_errors() {
        let (url, hits) = serve(&[500, 200]).await;
        assert_eq!(fetch_text(Client::new().get(url)).await.unwrap(), "ok");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn send_gives_up_after_all_attempts() {
        let (url, hits) = serve(&[503, 500, 502, 504]).await;
        assert!(fetch_text(Client::new().get(url)).await.is_err());
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn send_does_not_retry_client_errors() {
        let (url, hits) = serve(&[404, 200]).await;
        assert!(fetch_text(Client::new().get(url)).await.is_err());
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
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
        let names: Vec<_> = Providers::new(Duration::from_secs(1))
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
        assert!(providers.get(1).is_err());
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
        let fresh = providers.catalog_more(&mut seen).await.unwrap().unwrap();
        assert_eq!(fresh.iter().map(|e| e.id).collect::<Vec<_>>(), [2]);
    }

    #[tokio::test]
    async fn catalog_more_reports_exhaustion_and_failure() {
        let done = Providers(vec![fake("A", vec![])].into());
        assert!(
            done.catalog_more(&mut HashSet::new())
                .await
                .unwrap()
                .is_none()
        );
        let broken = Providers(vec![fake("A", vec![failure()])].into());
        assert!(broken.catalog_more(&mut HashSet::new()).await.is_err());
    }

    #[test]
    fn embedded_domains_resolve_to_https_without_trailing_slash() {
        for key in ["streamingcommunity", "animeunity"] {
            let url = domain(key);
            assert!(url.starts_with("https://"), "{key}: {url}");
            assert!(!url.ends_with('/'), "{key}: {url}");
        }
        assert_eq!(domain("missing"), "");
    }

    #[test]
    fn stable_hash_is_pinned_because_ids_are_persisted() {
        assert_eq!(stable_hash("abc"), 96354);
        assert_eq!(stable_hash(""), 0);
    }

    #[test]
    fn non_blank_trims_and_rejects_empty() {
        assert_eq!(non_blank("  x  ").as_deref(), Some("x"));
        assert_eq!(non_blank("   "), None);
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
        let script = r#"token: "t", expires: "e", url: "https://v.co/p/1""#;
        assert_eq!(
            vixcloud_playlist(script).as_deref(),
            Some("https://v.co/p/1?token=t&expires=e")
        );
    }

    #[test]
    fn vixcloud_playlist_requires_token() {
        assert!(vixcloud_playlist(r#"expires: "e", url: "https://v.co/p/1""#).is_none());
    }
}
