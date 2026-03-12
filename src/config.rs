use crate::providers::{Episode, MediaEntry};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub output: OutputConfig,
    pub download: DownloadConfig,
    pub process: ProcessConfig,
    pub requests: RequestsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub root_path: String,
    pub movie_folder_name: String,
    pub serie_folder_name: String,
    pub map_episode_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
    pub thread_count: u32,
    pub retry_count: u32,
    pub concurrent_download: bool,
    pub max_speed: String,
    pub select_video: String,
    pub select_audio: String,
    pub select_subtitle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub use_gpu: bool,
    pub merge_audio: bool,
    pub merge_subtitle: bool,
    pub extension: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestsConfig {
    pub timeout: u64,
    pub max_retry: u32,
    pub use_proxy: bool,
    pub proxy_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchItem {
    pub entry: MediaEntry,
    pub current_time: f64,
    pub duration: f64,
    pub season: Option<u32>,
    pub episode: Option<Episode>,
}

impl Default for OutputConfig {
    #[inline]
    fn default() -> Self {
        Self {
            root_path: "Movies".into(),
            movie_folder_name: "Movie".into(),
            serie_folder_name: "Serie".into(),
            map_episode_name: "%(episode_name) S%(season)E%(episode)".into(),
        }
    }
}

impl Default for DownloadConfig {
    #[inline]
    fn default() -> Self {
        Self {
            thread_count: 8,
            retry_count: 30,
            concurrent_download: true,
            max_speed: String::new(),
            select_video: "all".into(),
            select_audio: "all".into(),
            select_subtitle: "all".into(),
        }
    }
}

impl Default for ProcessConfig {
    #[inline]
    fn default() -> Self {
        Self {
            use_gpu: false,
            merge_audio: true,
            merge_subtitle: true,
            extension: "mp4".into(),
        }
    }
}

impl Default for RequestsConfig {
    #[inline]
    fn default() -> Self {
        Self {
            timeout: 30,
            max_retry: 8,
            use_proxy: false,
            proxy_url: String::new(),
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("StreamVault")
    }

    #[inline]
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            read_json(&path).unwrap_or_default()
        } else {
            let cfg = Self::default();
            cfg.save();
            cfg
        }
    }

    pub fn save(&self) {
        write_json(&Self::config_path(), self, "config");
    }

    pub fn download_dir(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(&self.output.root_path)
    }

    #[inline]
    pub fn movie_dir(&self) -> PathBuf {
        self.download_dir().join(&self.output.movie_folder_name)
    }

    #[inline]
    pub fn serie_dir(&self) -> PathBuf {
        self.download_dir().join(&self.output.serie_folder_name)
    }
}

impl WatchItem {
    pub fn progress_pct(&self) -> f64 {
        if self.duration > 0.0 {
            (self.current_time / self.duration * 100.0).min(100.0)
        } else {
            0.0
        }
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn write_json(path: &std::path::Path, value: &(impl Serialize + ?Sized), label: &str) {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("[StreamVault] {label} dir error: {e}");
            return;
        }
    }
    match serde_json::to_string_pretty(value) {
        Ok(json) => {
            if let Err(e) = fs::write(path, json) {
                eprintln!("[StreamVault] {label} write error: {e}");
            }
        }
        Err(e) => eprintln!("[StreamVault] {label} serialize error: {e}"),
    }
}

fn watch_items_path() -> PathBuf {
    AppConfig::config_dir().join("continue_watching.json")
}

pub fn load_watch_items() -> Vec<WatchItem> {
    read_json(&watch_items_path()).unwrap_or_default()
}

pub fn save_watch_items(items: &[WatchItem]) {
    write_json(&watch_items_path(), items, "watch progress");
}

pub fn upsert_watch_item(items: &mut Vec<WatchItem>, item: WatchItem) {
    if let Some(pos) = items
        .iter()
        .position(|i| i.entry.provider == item.entry.provider && i.entry.id == item.entry.id)
    {
        items.remove(pos);
    }
    items.insert(0, item);
}

pub fn advance_watch_item(
    items: &mut Vec<WatchItem>,
    entry: MediaEntry,
    episodes: &[Episode],
    cur_ep: u32,
    season: u32,
) -> bool {
    let next = episodes
        .iter()
        .filter(|e| e.number > cur_ep)
        .min_by_key(|e| e.number);
    if let Some(next) = next {
        let item = WatchItem {
            entry,
            current_time: 0.0,
            duration: next.duration.map(|d| d as f64).unwrap_or(0.0),
            season: Some(season),
            episode: Some(next.clone()),
        };
        upsert_watch_item(items, item);
        return true;
    }
    false
}

pub fn remove_watch_item(items: &mut Vec<WatchItem>, provider: usize, id: u64) {
    items.retain(|i| i.entry.provider != provider || i.entry.id != id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MediaType;

    #[test]
    fn default_config_has_expected_values() {
        let c = AppConfig::default();
        assert_eq!(c.output.root_path, "Movies");
        assert_eq!(c.output.movie_folder_name, "Movie");
        assert_eq!(c.output.serie_folder_name, "Serie");
        assert_eq!(c.download.thread_count, 8);
        assert_eq!(c.download.retry_count, 30);
        assert!(c.download.concurrent_download);
        assert_eq!(c.process.extension, "mp4");
        assert!(!c.process.use_gpu);
        assert!(c.process.merge_audio);
        assert_eq!(c.requests.timeout, 30);
        assert!(!c.requests.use_proxy);
    }

    #[test]
    fn config_dir_ends_with_streamvault() {
        assert!(AppConfig::config_dir().ends_with("StreamVault"));
    }

    #[test]
    fn config_path_ends_with_json() {
        assert!(AppConfig::config_path().ends_with("config.json"));
    }

    #[test]
    fn download_dir_uses_root_path() {
        assert!(AppConfig::default().download_dir().ends_with("Movies"));
    }

    #[test]
    fn movie_dir_appends_movie_folder() {
        assert!(AppConfig::default().movie_dir().ends_with("Movies/Movie"));
    }

    #[test]
    fn serie_dir_appends_serie_folder() {
        assert!(AppConfig::default().serie_dir().ends_with("Movies/Serie"));
    }

    #[test]
    fn serde_round_trip() {
        let mut c = AppConfig::default();
        c.output.root_path = "TestPath".into();
        c.download.thread_count = 16;
        let json = serde_json::to_string_pretty(&c).unwrap();
        let loaded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.output.root_path, "TestPath");
        assert_eq!(loaded.download.thread_count, 16);
    }

    #[test]
    fn default_process_merge_flags() {
        let p = ProcessConfig::default();
        assert!(p.merge_audio);
        assert!(p.merge_subtitle);
        assert!(!p.use_gpu);
    }

    #[test]
    fn default_request_no_proxy() {
        let r = RequestsConfig::default();
        assert!(!r.use_proxy);
        assert!(r.proxy_url.is_empty());
        assert_eq!(r.max_retry, 8);
    }

    fn make_entry(provider: usize, id: u64) -> MediaEntry {
        MediaEntry {
            id,
            name: format!("Title {id}"),
            slug: String::new(),
            provider,
            language: String::new(),
            media_type: MediaType::Movie,
            alternative_names: Vec::new(),
            year: None,
            score: None,
            image_url: None,
            description: None,
        }
    }

    fn make_watch(provider: usize, id: u64, time: f64, dur: f64) -> WatchItem {
        WatchItem {
            entry: make_entry(provider, id),
            current_time: time,
            duration: dur,
            season: None,
            episode: None,
        }
    }

    #[test]
    fn progress_pct_normal() {
        let w = make_watch(0, 1, 30.0, 60.0);
        assert!((w.progress_pct() - 50.0).abs() < 0.01);
    }

    #[test]
    fn progress_pct_zero_duration() {
        assert_eq!(make_watch(0, 1, 10.0, 0.0).progress_pct(), 0.0);
    }

    #[test]
    fn progress_pct_clamped_at_100() {
        assert_eq!(make_watch(0, 1, 200.0, 100.0).progress_pct(), 100.0);
    }

    #[test]
    fn upsert_inserts_new_at_front() {
        let mut items = vec![make_watch(0, 1, 10.0, 60.0)];
        upsert_watch_item(&mut items, make_watch(0, 2, 20.0, 90.0));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].entry.id, 2);
    }

    #[test]
    fn upsert_updates_existing() {
        let mut items = vec![make_watch(0, 1, 10.0, 60.0)];
        upsert_watch_item(&mut items, make_watch(0, 1, 45.0, 60.0));
        assert_eq!(items.len(), 1);
        assert!((items[0].current_time - 45.0).abs() < 0.01);
    }

    #[test]
    fn upsert_moves_existing_to_front() {
        let mut items = vec![
            make_watch(0, 1, 10.0, 60.0),
            make_watch(0, 2, 20.0, 90.0),
            make_watch(0, 3, 30.0, 120.0),
        ];
        upsert_watch_item(&mut items, make_watch(0, 3, 55.0, 120.0));
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].entry.id, 3);
        assert!((items[0].current_time - 55.0).abs() < 0.01);
        assert_eq!(items[1].entry.id, 1);
        assert_eq!(items[2].entry.id, 2);
    }

    #[test]
    fn upsert_matches_on_provider_and_id() {
        let mut items = vec![make_watch(0, 1, 10.0, 60.0)];
        upsert_watch_item(&mut items, make_watch(1, 1, 20.0, 60.0));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn remove_watch_item_removes_match() {
        let mut items = vec![make_watch(0, 1, 10.0, 60.0), make_watch(0, 2, 20.0, 90.0)];
        remove_watch_item(&mut items, 0, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].entry.id, 2);
    }

    #[test]
    fn remove_watch_item_no_match_unchanged() {
        let mut items = vec![make_watch(0, 1, 10.0, 60.0)];
        remove_watch_item(&mut items, 1, 99);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn advance_watch_item_to_next_episode() {
        let mut items = vec![make_watch(0, 1, 30.0, 60.0)];
        let eps: Vec<Episode> = (1..=3)
            .map(|n| Episode {
                id: n as u64,
                number: n,
                name: format!("Ep {n}"),
                duration: Some(45),
                image_url: None,
            })
            .collect();
        let advanced = advance_watch_item(&mut items, make_entry(0, 1), &eps, 1, 1);
        assert!(advanced);
        assert_eq!(items.len(), 1);
        assert!((items[0].current_time).abs() < 0.01);
        assert_eq!(items[0].episode.as_ref().unwrap().number, 2);
        assert_eq!(items[0].season, Some(1));
    }

    #[test]
    fn advance_watch_item_no_next_episode() {
        let mut items = vec![make_watch(0, 1, 30.0, 60.0)];
        let eps = vec![Episode {
            id: 1,
            number: 1,
            name: "Ep 1".into(),
            duration: Some(45),
            image_url: None,
        }];
        let advanced = advance_watch_item(&mut items, make_entry(0, 1), &eps, 1, 1);
        assert!(!advanced);
        assert!((items[0].current_time - 30.0).abs() < 0.01);
    }

    #[test]
    fn watch_item_serde_round_trip() {
        let w = WatchItem {
            entry: make_entry(2, 42),
            current_time: 123.5,
            duration: 3600.0,
            season: Some(3),
            episode: Some(Episode {
                id: 7,
                number: 5,
                name: "Test Ep".into(),
                duration: Some(45),
                image_url: None,
            }),
        };
        let json = serde_json::to_string(&w).unwrap();
        let loaded: WatchItem = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.entry.id, 42);
        assert_eq!(loaded.entry.provider, 2);
        assert!((loaded.current_time - 123.5).abs() < 0.01);
        assert_eq!(loaded.season, Some(3));
        assert_eq!(loaded.episode.unwrap().number, 5);
    }
}
