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
            fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            let cfg = Self::default();
            cfg.save();
            cfg
        }
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("[StreamVault] config dir error: {e}");
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    eprintln!("[StreamVault] config write error: {e}");
                }
            }
            Err(e) => eprintln!("[StreamVault] config serialize error: {e}"),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(AppConfig::default()
            .movie_dir()
            .ends_with("Movies/Movie"));
    }

    #[test]
    fn serie_dir_appends_serie_folder() {
        assert!(AppConfig::default()
            .serie_dir()
            .ends_with("Movies/Serie"));
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
}
