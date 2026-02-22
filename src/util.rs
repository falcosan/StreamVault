use crate::config::AppConfig;
use regex::Regex;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

// Searches bundled Resources/bin, then dev bin/, then common system paths
#[inline]
fn bundled_bin_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let resources_bin = exe_dir.parent().map(|c| c.join("Resources").join("bin"));
    if let Some(ref p) = resources_bin {
        if p.is_dir() { return Some(p.clone()); }
    }
    let dev_bin = exe_dir.join("bin");
    if dev_bin.is_dir() { return Some(dev_bin); }
    None
}

// Resolves binary path: bundled → homebrew → /usr/local → /usr → ~/.local → fallback to name
pub fn find_binary(name: &str) -> PathBuf {
    if let Some(bin_dir) = bundled_bin_dir() {
        let bundled = bin_dir.join(name);
        if bundled.exists() { return bundled; }
    }
    let candidates = [
        PathBuf::from(format!("/opt/homebrew/bin/{name}")),
        PathBuf::from(format!("/usr/local/bin/{name}")),
        PathBuf::from(format!("/usr/bin/{name}")),
        dirs::home_dir().unwrap_or_default().join(".local/bin").join(name),
    ];
    candidates.into_iter().find(|p| p.exists()).unwrap_or_else(|| PathBuf::from(name))
}

#[derive(Debug, Clone)]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Muxing,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub id: uuid::Uuid,
    pub title: String,
    pub status: DownloadStatus,
    pub percent: f64,
    pub speed: String,
    pub downloaded: String,
    pub total: String,
}

impl DownloadProgress {
    #[inline]
    pub fn new(id: uuid::Uuid, title: String) -> Self {
        Self { id, title, status: DownloadStatus::Queued, percent: 0.0,
               speed: String::new(), downloaded: String::new(), total: String::new() }
    }

    // Parses N_m3u8DL-RE stdout lines for progress percentage, speed, and size info
    pub fn parse_line(&mut self, line: &str) {
        static PERCENT_RE: OnceLock<Regex> = OnceLock::new();
        static SPEED_RE: OnceLock<Regex> = OnceLock::new();
        static SIZE_RE: OnceLock<Regex> = OnceLock::new();
        let pct = PERCENT_RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?)%").unwrap());
        let spd = SPEED_RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?(?:MB|KB|GB|B)ps)").unwrap());
        let sz = SIZE_RE.get_or_init(|| Regex::new(r"(\d+(?:\.\d+)?(?:MB|GB|KB|B))/(\d+(?:\.\d+)?(?:MB|GB|KB|B))").unwrap());
        if let Some(c) = pct.captures(line) { if let Ok(p) = c[1].parse::<f64>() { self.percent = p; } }
        if let Some(c) = spd.captures(line) { self.speed = c[1].to_string(); }
        if let Some(c) = sz.captures(line) { self.downloaded = c[1].to_string(); self.total = c[2].to_string(); }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub id: uuid::Uuid,
    pub title: String,
    pub stream_url: String,
    pub output_dir: PathBuf,
    pub filename: String,
    pub headers: Vec<(String, String)>,
}

pub struct DownloadEngine {
    config: AppConfig,
}

impl DownloadEngine {
    #[inline]
    pub fn new(config: AppConfig) -> Self { Self { config } }

    // Spawns N_m3u8DL-RE subprocess with all config flags, streams progress via channel
    pub async fn download(&self, req: DownloadRequest, tx: mpsc::UnboundedSender<DownloadProgress>) {
        let mut progress = DownloadProgress::new(req.id, req.title.clone());
        progress.status = DownloadStatus::Downloading;
        let _ = tx.send(progress.clone());

        let n_m3u8dl = find_binary("N_m3u8DL-RE");
        let ffmpeg = find_binary("ffmpeg");

        let mut cmd = Command::new(&n_m3u8dl);
        cmd.arg(&req.stream_url)
            .arg("--save-name").arg(&req.filename)
            .arg("--save-dir").arg(&req.output_dir)
            .arg("--tmp-dir").arg(req.output_dir.join("tmp"))
            .arg("--ffmpeg-binary-path").arg(&ffmpeg)
            .arg("--no-log").arg("--binary-merge").arg("--del-after-done")
            .arg("--mux-after-done").arg(format!("format={}", self.config.process.extension))
            .arg("--auto-subtitle-fix").arg("false")
            .arg("--check-segments-count").arg(if self.config.download.concurrent_download { "true" } else { "false" });

        if self.config.download.concurrent_download { cmd.arg("--concurrent-download"); }
        cmd.arg("--thread-count").arg(self.config.download.thread_count.to_string());
        cmd.arg("--download-retry-count").arg(self.config.download.retry_count.to_string());
        if !self.config.download.max_speed.is_empty() { cmd.arg("--max-speed").arg(&self.config.download.max_speed); }
        cmd.arg("--select-video").arg(&self.config.download.select_video);
        cmd.arg("--select-audio").arg(&self.config.download.select_audio);
        cmd.arg("--select-subtitle").arg(&self.config.download.select_subtitle);
        if self.config.process.merge_audio { cmd.arg("--mux-import").arg("audio"); }
        if self.config.process.merge_subtitle { cmd.arg("--mux-import").arg("subtitle"); }
        for (k, v) in &req.headers { cmd.arg("--header").arg(format!("{k}: {v}")); }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                progress.status = DownloadStatus::Failed(format!("Failed to start N_m3u8DL-RE: {e}"));
                let _ = tx.send(progress);
                return;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                progress.parse_line(&line);
                progress.status = if line.contains("Muxing") || line.contains("muxing") || line.contains("MUX") {
                    DownloadStatus::Muxing
                } else {
                    DownloadStatus::Downloading
                };
                let _ = tx.send(progress.clone());
            }
        }

        match child.wait().await {
            Ok(exit) if exit.success() => { progress.status = DownloadStatus::Completed; progress.percent = 100.0; }
            Ok(exit) => { progress.status = DownloadStatus::Failed(format!("N_m3u8DL-RE exited: {exit}")); }
            Err(e) => { progress.status = DownloadStatus::Failed(format!("Process error: {e}")); }
        }
        let _ = tx.send(progress);
    }

    #[inline]
    pub fn build_output_path(&self, title: &str, is_movie: bool) -> PathBuf {
        let base = if is_movie { self.config.movie_dir() } else { self.config.serie_dir() };
        base.join(sanitize_filename(title))
    }

    #[inline]
    pub fn format_episode_name(&self, show: &str, season: u32, ep: u32, ep_name: &str) -> String {
        self.config.output.map_episode_name
            .replace("%(episode_name)", ep_name)
            .replace("%(season)", &format!("{season:02}"))
            .replace("%(episode)", &format!("{ep:02}"))
            .replace("%(show_name)", show)
    }
}

#[inline]
fn sanitize_filename(name: &str) -> String {
    name.chars().map(|c| match c {
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
        _ => c,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_binary_returns_name_for_missing() {
        assert_eq!(find_binary("not_a_real_binary_xyz"), PathBuf::from("not_a_real_binary_xyz"));
    }

    #[test]
    fn bundled_bin_dir_returns_option() { let _ = bundled_bin_dir(); }

    #[test]
    fn sanitize_replaces_illegal_chars() {
        assert_eq!(sanitize_filename("hello/world"), "hello_world");
        assert_eq!(sanitize_filename("a:b*c?d"), "a_b_c_d");
        assert_eq!(sanitize_filename("clean"), "clean");
    }

    #[test]
    fn sanitize_preserves_unicode() {
        assert_eq!(sanitize_filename("café"), "café");
        assert_eq!(sanitize_filename("日本語"), "日本語");
    }

    #[test]
    fn format_episode_name_replaces_placeholders() {
        let engine = DownloadEngine::new(AppConfig::default());
        assert_eq!(engine.format_episode_name("Breaking Bad", 2, 5, "Mandala"), "Mandala S02E05");
    }

    #[test]
    fn format_episode_name_with_show_name() {
        let mut cfg = AppConfig::default();
        cfg.output.map_episode_name = "%(show_name) - %(episode_name) S%(season)E%(episode)".into();
        let engine = DownloadEngine::new(cfg);
        assert_eq!(engine.format_episode_name("Lost", 1, 3, "Tabula Rasa"), "Lost - Tabula Rasa S01E03");
    }

    #[test]
    fn build_output_path_movie() {
        let path = DownloadEngine::new(AppConfig::default()).build_output_path("Inception", true);
        assert!(path.ends_with("Movie/Inception"));
    }

    #[test]
    fn build_output_path_series() {
        let path = DownloadEngine::new(AppConfig::default()).build_output_path("Breaking Bad", false);
        assert!(path.ends_with("Serie/Breaking Bad"));
    }

    fn make_progress() -> DownloadProgress {
        DownloadProgress::new(uuid::Uuid::nil(), "test".into())
    }

    #[test]
    fn new_progress_starts_queued() {
        let p = make_progress();
        assert!(matches!(p.status, DownloadStatus::Queued));
        assert_eq!(p.percent, 0.0);
        assert!(p.speed.is_empty());
    }

    #[test]
    fn parse_percent() {
        let mut p = make_progress();
        p.parse_line("Downloading 45.3% done");
        assert!((p.percent - 45.3).abs() < 0.01);
    }

    #[test]
    fn parse_speed() {
        let mut p = make_progress();
        p.parse_line("Speed: 12.5MBps");
        assert_eq!(p.speed, "12.5MBps");
    }

    #[test]
    fn parse_size() {
        let mut p = make_progress();
        p.parse_line("Downloaded 150MB/500MB");
        assert_eq!(p.downloaded, "150MB");
        assert_eq!(p.total, "500MB");
    }

    #[test]
    fn parse_combined_line() {
        let mut p = make_progress();
        p.parse_line("50.0% 200MB/400MB 10MBps");
        assert!((p.percent - 50.0).abs() < 0.01);
        assert_eq!(p.speed, "10MBps");
        assert_eq!(p.downloaded, "200MB");
        assert_eq!(p.total, "400MB");
    }

    #[test]
    fn parse_no_match_leaves_defaults() {
        let mut p = make_progress();
        p.parse_line("Some random log line");
        assert_eq!(p.percent, 0.0);
        assert!(p.speed.is_empty());
    }
}
