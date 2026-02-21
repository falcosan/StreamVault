use super::models::StreamUrl;
use super::traits::{ProviderError, ProviderResult};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::{LazyLock, OnceLock};
use url::Url;

static IFRAME_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("iframe").expect("valid selector"));
static BODY_SCRIPT_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("body script").expect("valid selector"));

static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
static EXPIRES_RE: OnceLock<Regex> = OnceLock::new();
static URL_RE: OnceLock<Regex> = OnceLock::new();
static FHD_RE: OnceLock<Regex> = OnceLock::new();

fn token_re() -> &'static Regex {
    TOKEN_RE
        .get_or_init(|| Regex::new(r#"(?:['"]token['"]|token)\s*:\s*['"]([^'"]+)['"]"#).unwrap())
}

fn expires_re() -> &'static Regex {
    EXPIRES_RE.get_or_init(|| {
        Regex::new(r#"(?:['"]expires['"]|expires)\s*:\s*['"]([^'"]+)['"]"#).unwrap()
    })
}

fn url_re() -> &'static Regex {
    URL_RE.get_or_init(|| {
        Regex::new(r#"(?:['"]url['"]|url)\s*:\s*['"](?P<url>https?://[^'"]+)['"]"#).unwrap()
    })
}

fn fhd_re() -> &'static Regex {
    FHD_RE.get_or_init(|| Regex::new(r"window\.canPlayFHD\s*=\s*(true|false)").unwrap())
}

pub async fn fetch_iframe_url(
    client: &Client,
    base_url: &str,
    lang: &str,
    media_id: u64,
    episode_id: Option<u64>,
) -> ProviderResult<String> {
    let url = if let Some(ep_id) = episode_id {
        format!("{base_url}/{lang}/iframe/{media_id}?episode_id={ep_id}&next_episode=1")
    } else {
        format!("{base_url}/{lang}/iframe/{media_id}")
    };

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let html = resp
        .text()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let document = Html::parse_document(&html);

    document
        .select(&IFRAME_SELECTOR)
        .next()
        .and_then(|el| el.value().attr("src"))
        .map(String::from)
        .ok_or_else(|| ProviderError::Parse("No iframe src found".into()))
}

pub async fn extract_stream_url(client: &Client, iframe_url: &str) -> ProviderResult<StreamUrl> {
    let resp = client
        .get(iframe_url)
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let html = resp
        .text()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let document = Html::parse_document(&html);

    let script_text = document
        .select(&BODY_SCRIPT_SELECTOR)
        .next()
        .map(|el| el.inner_html())
        .unwrap_or_default();

    let token = token_re()
        .captures(&script_text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ProviderError::StreamExtraction("No token found".into()))?;

    let expires = expires_re()
        .captures(&script_text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ProviderError::StreamExtraction("No expires found".into()))?;

    let base_url = url_re()
        .captures(&script_text)
        .and_then(|c| c.name("url"))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ProviderError::StreamExtraction("No URL found".into()))?;

    let can_play_fhd = fhd_re()
        .captures(&script_text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str() == "true")
        .unwrap_or(false);

    let mut parsed = Url::parse(&base_url)
        .map_err(|e| ProviderError::Parse(format!("Invalid stream URL: {e}")))?;

    let has_b_param = parsed.query_pairs().any(|(k, v)| k == "b" && v == "1");

    parsed.set_query(None);

    let mut params = Vec::new();
    if can_play_fhd {
        params.push(("h", "1".to_string()));
    }
    if has_b_param {
        params.push(("b", "1".to_string()));
    }
    params.push(("token", token));
    params.push(("expires", expires));

    let query_string: String = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    parsed.set_query(Some(&query_string));

    Ok(StreamUrl {
        url: parsed.to_string(),
        headers: Vec::new(),
    })
}
