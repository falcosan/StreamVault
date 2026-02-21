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
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json);
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
