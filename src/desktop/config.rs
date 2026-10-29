use crate::folders::{config_dir, icloud_dir, local_dir};
use dioxus::logger::tracing::warn;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

pub const WATCH_FILE: &str = "continue_watching.json";
pub const PREFS_FILE: &str = "player_prefs.json";
pub const REPLICA_FILE: &str = "sync_replica.json";
pub const DOWNLOADS_FILE: &str = "downloads.json";
pub const LANGUAGE_FILE: &str = "language.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub output: OutputConfig,
    pub download: DownloadConfig,
    pub process: ProcessConfig,
    pub requests: RequestsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub root_path: String,
    pub movie_folder_name: String,
    pub serie_folder_name: String,
    pub map_episode_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[serde(default)]
pub struct ProcessConfig {
    pub merge_audio: bool,
    pub merge_subtitle: bool,
    pub extension: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestsConfig {
    pub timeout: u64,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            root_path: "Medias".into(),
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
            select_video: "best".into(),
            select_audio: "all".into(),
            select_subtitle: "all".into(),
        }
    }
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            merge_audio: true,
            merge_subtitle: true,
            extension: "mp4".into(),
        }
    }
}

impl Default for RequestsConfig {
    fn default() -> Self {
        Self { timeout: 30 }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_dir().join("config.json");
        match read_json(&path) {
            Ok(config) => config,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                if let Err(e) = write_json(&path, &config) {
                    warn!("config write error: {e}");
                }
                config
            }
            Err(e) => {
                warn!("config read error, using defaults: {e}");
                Self::default()
            }
        }
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.requests.timeout)
    }

    pub fn movie_dir(&self) -> PathBuf {
        self.download_dir().join(&self.output.movie_folder_name)
    }

    pub fn serie_dir(&self) -> PathBuf {
        self.download_dir().join(&self.output.serie_folder_name)
    }

    fn download_dir(&self) -> PathBuf {
        icloud_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
            .join(&self.output.root_path)
    }
}

pub fn state_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = local_dir().join("state");
        migrate_legacy(&config_dir(), &dir);
        dir
    })
}

fn migrate_legacy(legacy: &Path, dir: &Path) {
    if dir.join(REPLICA_FILE).exists() {
        return;
    }
    for name in [WATCH_FILE, PREFS_FILE] {
        migrate_file(&legacy.join(name), &dir.join(name));
    }
}

fn migrate_file(from: &Path, to: &Path) {
    if to.exists() || !from.is_file() {
        return;
    }
    if let Err(e) = replace_file(to, |tmp| fs::copy(from, tmp).map(drop)) {
        warn!("state migration error: {e}");
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub fn missing_ok(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

pub fn write_json(path: &Path, value: &(impl Serialize + ?Sized)) -> io::Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    replace_file(path, |tmp| fs::write(tmp, json))
}

fn replace_file(path: &Path, fill: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fill(&tmp)?;
    fs::File::open(&tmp)?.sync_all()?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;

    #[test]
    fn defaults_match_documented_values() {
        let c = AppConfig::default();
        assert_eq!(c.output.root_path, "Medias");
        assert_eq!(c.output.movie_folder_name, "Movie");
        assert_eq!(c.output.serie_folder_name, "Serie");
        assert_eq!(c.download.thread_count, 8);
        assert_eq!(c.download.retry_count, 30);
        assert!(c.download.concurrent_download);
        assert_eq!(
            (
                c.download.select_video.as_str(),
                c.download.select_audio.as_str()
            ),
            ("best", "all")
        );
        assert!(c.process.merge_audio && c.process.merge_subtitle);
        assert_eq!(c.process.extension, "mp4");
        assert_eq!(c.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn partial_config_keeps_defaults_and_ignores_retired_keys() {
        let c: AppConfig = serde_json::from_str(
            r#"{"download": {"thread_count": 16}, "process": {"use_gpu": true},
                "requests": {"timeout": 5, "use_proxy": false, "proxy_url": "", "max_retry": 8}}"#,
        )
        .unwrap();
        assert_eq!(c.download.thread_count, 16);
        assert_eq!(c.download.retry_count, 30);
        assert_eq!(c.process.extension, "mp4");
        assert_eq!(c.timeout(), Duration::from_secs(5));
        assert_eq!(c.output.root_path, "Medias");
    }

    #[test]
    fn media_dirs_nest_under_root_path() {
        let c = AppConfig::default();
        assert!(c.movie_dir().ends_with("Medias/Movie"));
        assert!(c.serie_dir().ends_with("Medias/Serie"));
    }

    #[test]
    fn legacy_state_is_copied_only_until_a_replica_exists() {
        let base = scratch_dir("migrate_legacy_once");
        let (legacy, dir) = (base.join("legacy"), base.join("state"));
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join(WATCH_FILE), "[]").unwrap();
        migrate_legacy(&legacy, &dir);
        assert!(dir.join(WATCH_FILE).is_file());
        fs::remove_file(dir.join(WATCH_FILE)).unwrap();
        fs::write(dir.join(REPLICA_FILE), "{}").unwrap();
        migrate_legacy(&legacy, &dir);
        assert!(!dir.join(WATCH_FILE).exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn json_round_trips_through_missing_parent_dirs() {
        let base = scratch_dir("json_round_trip");
        let path = base.join("a/b/config.json");
        write_json(&path, &AppConfig::default()).unwrap();
        let loaded: AppConfig = read_json(&path).unwrap();
        assert_eq!(loaded.download.thread_count, 8);
        assert!(!path.with_extension("tmp").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn read_json_distinguishes_missing_from_corrupt() {
        let base = scratch_dir("json_errors");
        let missing = read_json::<AppConfig>(&base.join("none.json")).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        fs::write(base.join("bad.json"), "{").unwrap();
        let corrupt = read_json::<AppConfig>(&base.join("bad.json")).unwrap_err();
        assert_ne!(corrupt.kind(), io::ErrorKind::NotFound);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_ok_tolerates_only_missing_files() {
        assert!(missing_ok(Ok(())).is_ok());
        assert!(missing_ok(Err(io::ErrorKind::NotFound.into())).is_ok());
        let denied = missing_ok(Err(io::ErrorKind::PermissionDenied.into()));
        assert_eq!(denied.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn migrate_file_copies_content_and_mtime() {
        let base = scratch_dir("migrate_copy");
        let from = base.join("cloud/continue_watching.json");
        let to = base.join("local/state/continue_watching.json");
        fs::create_dir_all(from.parent().unwrap()).unwrap();
        fs::write(&from, "[1]").unwrap();
        migrate_file(&from, &to);
        assert_eq!(fs::read_to_string(&to).unwrap(), "[1]");
        assert_eq!(
            fs::metadata(&to).unwrap().modified().unwrap(),
            fs::metadata(&from).unwrap().modified().unwrap()
        );
        assert!(!to.with_extension("tmp").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn migrate_file_keeps_existing_target() {
        let base = scratch_dir("migrate_keep");
        let (from, to) = (base.join("old.json"), base.join("new.json"));
        fs::write(&from, "old").unwrap();
        fs::write(&to, "new").unwrap();
        migrate_file(&from, &to);
        assert_eq!(fs::read_to_string(&to).unwrap(), "new");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn migrate_file_ignores_missing_source() {
        let base = scratch_dir("migrate_missing");
        let to = base.join("state/new.json");
        migrate_file(&base.join("absent.json"), &to);
        assert!(!to.exists());
        let _ = fs::remove_dir_all(&base);
    }
}
