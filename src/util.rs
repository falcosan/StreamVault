use crate::config::AppConfig;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

fn bundled_bin_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let resources_bin = exe_dir.parent().map(|c| c.join("Resources").join("bin"));
    if let Some(ref p) = resources_bin {
        if p.is_dir() {
            return Some(p.clone());
        }
    }
    let dev_bin = exe_dir.join("bin");
    if dev_bin.is_dir() {
        return Some(dev_bin);
    }
    None
}

pub fn find_binary(name: &str) -> PathBuf {
    if let Some(bin_dir) = bundled_bin_dir() {
        let bundled = bin_dir.join(name);
        if bundled.exists() {
            return bundled;
        }
    }
    let candidates = [
        PathBuf::from(format!("/opt/homebrew/bin/{name}")),
        PathBuf::from(format!("/usr/local/bin/{name}")),
        PathBuf::from(format!("/usr/bin/{name}")),
        dirs::home_dir()
            .unwrap_or_default()
            .join(".local/bin")
            .join(name),
    ];
    candidates
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(name))
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Muxing,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadProgress {
    pub id: uuid::Uuid,
    pub title: String,
    pub status: DownloadStatus,
}

impl DownloadProgress {
    #[inline]
    pub fn new(id: uuid::Uuid, title: String) -> Self {
        Self {
            id,
            title,
            status: DownloadStatus::Queued,
        }
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
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn download(
        &self,
        req: DownloadRequest,
        tx: mpsc::UnboundedSender<DownloadProgress>,
    ) {
        let mut progress = DownloadProgress::new(req.id, req.title.clone());
        progress.status = DownloadStatus::Downloading;
        let _ = tx.send(progress.clone());

        if let Err(e) = std::fs::create_dir_all(&req.output_dir) {
            progress.status = DownloadStatus::Failed(format!("Cannot create output dir: {e}"));
            let _ = tx.send(progress);
            return;
        }

        let n_m3u8dl = find_binary("N_m3u8DL-RE");
        let ffmpeg = find_binary("ffmpeg");
        let save_name = sanitize_filename(&req.filename);

        let mut cmd = Command::new(&n_m3u8dl);
        cmd.arg(&req.stream_url)
            .arg("--save-name")
            .arg(&save_name)
            .arg("--save-dir")
            .arg(&req.output_dir)
            .arg("--tmp-dir")
            .arg(&req.output_dir)
            .arg("--ffmpeg-binary-path")
            .arg(&ffmpeg)
            .arg("--no-log")
            .arg("--write-meta-json")
            .arg("false")
            .arg("--binary-merge")
            .arg("--del-after-done")
            .arg("--auto-subtitle-fix")
            .arg("false")
            .arg("--check-segments-count")
            .arg("true")
            .arg("--force-ansi-console")
            .arg("--no-ansi-color");

        if self.config.download.concurrent_download {
            cmd.arg("--concurrent-download");
        }
        cmd.arg("--thread-count")
            .arg(self.config.download.thread_count.to_string());
        cmd.arg("--download-retry-count")
            .arg(self.config.download.retry_count.to_string());
        if !self.config.download.max_speed.is_empty() {
            cmd.arg("--max-speed").arg(&self.config.download.max_speed);
        }
        cmd.arg("--select-video")
            .arg(&self.config.download.select_video);
        cmd.arg("--select-audio")
            .arg(&self.config.download.select_audio);
        cmd.arg("--select-subtitle")
            .arg(&self.config.download.select_subtitle);
        if !self.config.process.merge_audio {
            cmd.arg("--drop-audio").arg("all");
        }
        for (k, v) in &req.headers {
            cmd.arg("--header").arg(format!("{k}: {v}"));
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                progress.status =
                    DownloadStatus::Failed(format!("Failed to start N_m3u8DL-RE: {e}"));
                let _ = tx.send(progress);
                return;
            }
        };

        let stderr_handle = child.stderr.take().map(|stderr| {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                let mut collected = String::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !collected.is_empty() {
                        collected.push('\n');
                    }
                    collected.push_str(&line);
                }
                collected
            })
        });

        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.contains("Muxing") || line.contains("muxing") || line.contains("MUX") {
                    progress.status = DownloadStatus::Muxing;
                    let _ = tx.send(progress.clone());
                }
            }
        }

        let stderr_output = match stderr_handle {
            Some(handle) => handle.await.unwrap_or_default(),
            None => String::new(),
        };

        match child.wait().await {
            Ok(exit) if exit.success() => {
                progress.status = DownloadStatus::Muxing;
                let _ = tx.send(progress.clone());

                match self.mux_output(&ffmpeg, &req.output_dir, &save_name).await {
                    Ok(_) => {
                        progress.status = DownloadStatus::Completed;
                    }
                    Err(e) => {
                        progress.status = DownloadStatus::Failed(format!("Mux failed: {e}"));
                    }
                }
            }
            Ok(exit) => {
                let has_ts = std::fs::read_dir(&req.output_dir)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .any(|e| e.path().extension().map(|x| x == "ts").unwrap_or(false))
                    })
                    .unwrap_or(false);

                if has_ts {
                    progress.status = DownloadStatus::Muxing;
                    let _ = tx.send(progress.clone());

                    match self.mux_output(&ffmpeg, &req.output_dir, &save_name).await {
                        Ok(_) => {
                            progress.status = DownloadStatus::Completed;
                        }
                        Err(e) => {
                            progress.status = DownloadStatus::Failed(format!("Mux failed: {e}"));
                        }
                    }
                } else {
                    let msg = if stderr_output.is_empty() {
                        format!("N_m3u8DL-RE exited: {exit}")
                    } else {
                        let error_lines: String = stderr_output
                            .lines()
                            .filter(|l| {
                                let t = l.trim();
                                !t.is_empty() && !t.starts_with("at ") && !t.starts_with("---")
                            })
                            .take(5)
                            .collect::<Vec<_>>()
                            .join("\n");
                        if error_lines.is_empty() {
                            format!("N_m3u8DL-RE exited: {exit}")
                        } else {
                            format!("N_m3u8DL-RE exited: {exit}\n{error_lines}")
                        }
                    };
                    progress.status = DownloadStatus::Failed(msg);
                }
            }
            Err(e) => {
                progress.status = DownloadStatus::Failed(format!("Process error: {e}"));
            }
        }
        let _ = tx.send(progress);
    }

    async fn mux_output(
        &self,
        ffmpeg: &std::path::Path,
        output_dir: &std::path::Path,
        save_name: &str,
    ) -> Result<(), String> {
        let ext = &self.config.process.extension;

        let mut ts_files: Vec<PathBuf> = std::fs::read_dir(output_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "ts").unwrap_or(false))
            .collect();

        if ts_files.is_empty() {
            return Err("No .ts files found after download".into());
        }

        ts_files.sort_by(|a, b| {
            let sa = std::fs::metadata(a).map(|m| m.len()).unwrap_or(0);
            let sb = std::fs::metadata(b).map(|m| m.len()).unwrap_or(0);
            sb.cmp(&sa)
        });

        let out_file = output_dir.join(format!("{save_name}.{ext}"));

        let vtt_files: Vec<PathBuf> = if self.config.process.merge_subtitle {
            std::fs::read_dir(output_dir)
                .map_err(|e| e.to_string())?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "vtt").unwrap_or(false))
                .collect()
        } else {
            Vec::new()
        };

        let mut mux_cmd = Command::new(ffmpeg);
        mux_cmd.arg("-y");
        for ts in &ts_files {
            mux_cmd.arg("-i").arg(ts);
        }
        for vtt in &vtt_files {
            mux_cmd.arg("-i").arg(vtt);
        }
        mux_cmd.arg("-map").arg("0:v:0").arg("-map").arg("0:a?");
        for i in 1..ts_files.len() {
            mux_cmd.arg("-map").arg(format!("{i}:a?"));
        }
        let ts_count = ts_files.len();
        for i in 0..vtt_files.len() {
            mux_cmd.arg("-map").arg(format!("{}:s?", ts_count + i));
        }
        mux_cmd.arg("-c:v").arg("copy").arg("-c:a").arg("copy");
        if !vtt_files.is_empty() {
            let sub_codec = if ext == "mkv" { "srt" } else { "mov_text" };
            mux_cmd.arg("-c:s").arg(sub_codec);
        }
        mux_cmd
            .arg(&out_file)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mux_result = mux_cmd.output().await.map_err(|e| e.to_string())?;

        if !mux_result.status.success() {
            let err = String::from_utf8_lossy(&mux_result.stderr);
            let lines: Vec<&str> = err
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty() && !t.starts_with("frame=")
                })
                .collect();
            let tail = lines
                .iter()
                .rev()
                .take(5)
                .rev()
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!("ffmpeg exited: {}\n{tail}", mux_result.status));
        }

        for ts in &ts_files {
            let _ = std::fs::remove_file(ts);
        }
        if self.config.process.merge_subtitle {
            if let Ok(entries) = std::fs::read_dir(output_dir) {
                for entry in entries.flatten() {
                    if entry
                        .path()
                        .extension()
                        .map(|x| x == "vtt")
                        .unwrap_or(false)
                    {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }

        let temp_dir = output_dir.join(save_name);
        if temp_dir.is_dir() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }

        Ok(())
    }

    #[inline]
    pub fn build_output_path(&self, title: &str, is_movie: bool) -> PathBuf {
        let base = if is_movie {
            self.config.movie_dir()
        } else {
            self.config.serie_dir()
        };
        base.join(sanitize_filename(title))
    }

    pub fn build_series_episode_path(&self, show: &str, season: u32) -> PathBuf {
        self.config
            .serie_dir()
            .join(sanitize_filename(show))
            .join(format!("S{season:02}"))
    }

    pub fn format_episode_name(&self, show: &str, season: u32, ep: u32, ep_name: &str) -> String {
        self.config
            .output
            .map_episode_name
            .replace("%(episode_name)", ep_name)
            .replace("%(season)", &format!("{season:02}"))
            .replace("%(episode)", &format!("{ep:02}"))
            .replace("%(show_name)", show)
    }
}

#[inline]
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_binary_returns_name_for_missing() {
        assert_eq!(
            find_binary("not_a_real_binary_xyz"),
            PathBuf::from("not_a_real_binary_xyz")
        );
    }

    #[test]
    fn bundled_bin_dir_returns_option() {
        let _ = bundled_bin_dir();
    }

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
        assert_eq!(
            engine.format_episode_name("Breaking Bad", 2, 5, "Mandala"),
            "Mandala S02E05"
        );
    }

    #[test]
    fn format_episode_name_with_show_name() {
        let mut cfg = AppConfig::default();
        cfg.output.map_episode_name = "%(show_name) - %(episode_name) S%(season)E%(episode)".into();
        let engine = DownloadEngine::new(cfg);
        assert_eq!(
            engine.format_episode_name("Lost", 1, 3, "Tabula Rasa"),
            "Lost - Tabula Rasa S01E03"
        );
    }

    #[test]
    fn build_output_path_movie() {
        let path = DownloadEngine::new(AppConfig::default()).build_output_path("Inception", true);
        assert!(path.ends_with("Movie/Inception"));
    }

    #[test]
    fn build_output_path_series() {
        let path =
            DownloadEngine::new(AppConfig::default()).build_output_path("Breaking Bad", false);
        assert!(path.ends_with("Serie/Breaking Bad"));
    }

    #[test]
    fn build_series_episode_path_includes_season() {
        let path =
            DownloadEngine::new(AppConfig::default()).build_series_episode_path("Breaking Bad", 2);
        assert!(path.ends_with("Serie/Breaking Bad/S02"));
    }

    #[test]
    fn new_progress_starts_queued() {
        let p = DownloadProgress::new(uuid::Uuid::nil(), "test".into());
        assert!(matches!(p.status, DownloadStatus::Queued));
    }
}
