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
    fn default() -> Self {
        Self {
            root_path: "Video".into(),
            movie_folder_name: "Movie".into(),
            serie_folder_name: "Serie".into(),
            map_episode_name: "%(episode_name) S%(season)E%(episode)".into(),
        }
    }
}

impl Default for DownloadConfig {
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
    fn default() -> Self {
        Self {
            use_gpu: false,
            merge_audio: true,
            merge_subtitle: true,
            extension: "mkv".into(),
        }
    }
}

impl Default for RequestsConfig {
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
            let config = Self::default();
            config.save();
            config
        }
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("[StreamVault] Failed to create config directory: {e}");
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    eprintln!("[StreamVault] Failed to write config file: {e}");
                }
            }
            Err(e) => {
                eprintln!("[StreamVault] Failed to serialize config: {e}");
            }
        }
    }

    pub fn download_dir(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(&self.output.root_path)
    }

    pub fn movie_dir(&self) -> PathBuf {
        self.download_dir().join(&self.output.movie_folder_name)
    }

    pub fn serie_dir(&self) -> PathBuf {
        self.download_dir().join(&self.output.serie_folder_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = AppConfig::default();
        assert_eq!(config.output.root_path, "Video");
        assert_eq!(config.output.movie_folder_name, "Movie");
        assert_eq!(config.output.serie_folder_name, "Serie");
        assert_eq!(config.download.thread_count, 8);
        assert_eq!(config.download.retry_count, 30);
        assert!(config.download.concurrent_download);
        assert_eq!(config.process.extension, "mkv");
        assert!(!config.process.use_gpu);
        assert!(config.process.merge_audio);
        assert_eq!(config.requests.timeout, 30);
        assert!(!config.requests.use_proxy);
    }

    #[test]
    fn config_dir_is_inside_streamvault() {
        let dir = AppConfig::config_dir();
        assert!(dir.ends_with("StreamVault"));
    }

    #[test]
    fn config_path_ends_with_json() {
        let path = AppConfig::config_path();
        assert!(path.ends_with("config.json"));
    }

    #[test]
    fn download_dir_uses_root_path() {
        let config = AppConfig::default();
        let dir = config.download_dir();
        assert!(dir.ends_with("Video"));
    }

    #[test]
    fn movie_dir_appends_movie_folder() {
        let config = AppConfig::default();
        let dir = config.movie_dir();
        assert!(dir.ends_with("Video/Movie"));
    }

    #[test]
    fn serie_dir_appends_serie_folder() {
        let config = AppConfig::default();
        let dir = config.serie_dir();
        assert!(dir.ends_with("Video/Serie"));
    }

    #[test]
    fn serde_round_trip() {
        let mut config = AppConfig::default();
        config.output.root_path = "TestPath".into();
        config.download.thread_count = 16;

        let json = serde_json::to_string_pretty(&config).unwrap();
        let loaded: AppConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.output.root_path, "TestPath");
        assert_eq!(loaded.download.thread_count, 16);
    }
}
