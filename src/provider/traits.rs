use super::models::{Episode, MediaEntry, Season, StreamUrl};
use std::future::Future;
use std::pin::Pin;

pub type ProviderResult<T> = Result<T, ProviderError>;

#[derive(Debug, Clone)]
pub enum ProviderError {
    Network(String),
    Parse(String),
    StreamExtraction(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::Parse(msg) => write!(f, "Parse error: {msg}"),
            Self::StreamExtraction(msg) => write!(f, "Stream extraction error: {msg}"),
        }
    }
}

impl std::error::Error for ProviderError {}

pub trait Provider: Send + Sync {
    fn search(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<MediaEntry>>> + Send + '_>>;

    fn get_seasons(
        &self,
        entry: &MediaEntry,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Season>>> + Send + '_>>;

    fn get_episodes(
        &self,
        entry: &MediaEntry,
        season_number: u32,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Vec<Episode>>> + Send + '_>>;

    fn get_stream_url(
        &self,
        entry: &MediaEntry,
        episode: Option<&Episode>,
        season_number: Option<u32>,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<StreamUrl>> + Send + '_>>;
}
