use crate::config::AppConfig;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

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
        let mut report = |status| {
            progress.status = status;
            let _ = tx.send(progress.clone());
        };
        report(DownloadStatus::Downloading);
        let work_dir = work_dir(req.id);
        let status = match self.fetch(&req, &work_dir, &mut report).await {
            Ok(()) => DownloadStatus::Completed,
            Err(e) => DownloadStatus::Failed(e),
        };
        let _ = tokio::fs::remove_dir_all(&work_dir).await;
        report(status);
    }

    async fn fetch(
        &self,
        req: &DownloadRequest,
        work_dir: &Path,
        report: &mut impl FnMut(DownloadStatus),
    ) -> Result<(), String> {
        tokio::fs::create_dir_all(&req.output_dir)
            .await
            .map_err(|e| format!("Cannot create output dir: {e}"))?;
        let tmp_dir = work_dir.join("tmp");
        tokio::fs::create_dir_all(&tmp_dir)
            .await
            .map_err(|e| format!("Cannot create tmp dir: {e}"))?;

        let ffmpeg = find_binary("ffmpeg");
        let save_name = sanitize_filename(&req.filename);
        let mut child = self
            .downloader(req, &save_name, work_dir, &tmp_dir, &ffmpeg)
            .spawn()
            .map_err(|e| format!("Failed to start N_m3u8DL-RE: {e}"))?;

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

        let mut muxing = false;
        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !muxing
                    && (line.contains("Muxing") || line.contains("muxing") || line.contains("MUX"))
                {
                    muxing = true;
                    report(DownloadStatus::Muxing);
                }
            }
        }

        let stderr_output = match stderr_handle {
            Some(handle) => handle.await.unwrap_or_default(),
            None => String::new(),
        };

        let exit = child
            .wait()
            .await
            .map_err(|e| format!("Process error: {e}"))?;
        let (media, subtitles) = scan_tracks(work_dir, self.config.process.merge_subtitle).await;
        if !exit.success() && media.is_empty() {
            return Err(exit_message(exit, &stderr_output));
        }
        if !muxing {
            report(DownloadStatus::Muxing);
        }

        let file_name = format!("{save_name}.{}", self.config.process.extension);
        let muxed = work_dir.join(&file_name);
        self.mux_output(&ffmpeg, &muxed, &media, &subtitles)
            .await
            .map_err(|e| format!("Mux failed: {e}"))?;
        move_file(&muxed, &req.output_dir.join(&file_name))
            .await
            .map_err(|e| format!("Cannot save output: {e}"))
    }

    fn downloader(
        &self,
        req: &DownloadRequest,
        save_name: &str,
        save_dir: &Path,
        tmp_dir: &Path,
        ffmpeg: &Path,
    ) -> Command {
        let mut cmd = Command::new(find_binary("N_m3u8DL-RE"));
        cmd.arg(&req.stream_url)
            .arg("--save-name")
            .arg(save_name)
            .arg("--save-dir")
            .arg(save_dir)
            .arg("--tmp-dir")
            .arg(tmp_dir)
            .arg("--ffmpeg-binary-path")
            .arg(ffmpeg)
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
        cmd
    }

    async fn mux_output(
        &self,
        ffmpeg: &Path,
        out_file: &Path,
        media: &[PathBuf],
        subtitles: &[PathBuf],
    ) -> Result<(), String> {
        if media.is_empty() {
            return Err("No .ts files found after download".into());
        }

        let mut mux_cmd = Command::new(ffmpeg);
        mux_cmd.arg("-y");
        for f in media.iter().chain(subtitles) {
            mux_cmd.arg("-i").arg(f);
        }
        for i in 0..media.len() {
            mux_cmd.arg("-map").arg(format!("{i}:v?"));
            mux_cmd.arg("-map").arg(format!("{i}:a?"));
        }
        for i in 0..subtitles.len() {
            mux_cmd.arg("-map").arg(format!("{}:s?", media.len() + i));
        }
        mux_cmd.arg("-c:v").arg("copy").arg("-c:a").arg("copy");
        if !subtitles.is_empty() {
            let sub_codec = if self.config.process.extension == "mkv" {
                "srt"
            } else {
                "mov_text"
            };
            mux_cmd.arg("-c:s").arg(sub_codec);
        }
        mux_cmd
            .arg(out_file)
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

fn work_dir(id: uuid::Uuid) -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("StreamVault")
        .join(id.to_string())
}

async fn scan_tracks(dir: &Path, with_subtitles: bool) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut media: Vec<(PathBuf, u64)> = Vec::new();
    let mut subtitles = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let path = entry.path();
            match path.extension().and_then(|x| x.to_str()) {
                Some("ts") => {
                    let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
                    media.push((path, size));
                }
                Some("vtt") if with_subtitles => subtitles.push(path),
                _ => {}
            }
        }
    }
    media.sort_unstable_by_key(|(_, size)| std::cmp::Reverse(*size));
    (media.into_iter().map(|(path, _)| path).collect(), subtitles)
}

async fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if tokio::fs::rename(from, to).await.is_err() {
        tokio::fs::copy(from, to).await?;
    }
    Ok(())
}

fn exit_message(exit: ExitStatus, stderr: &str) -> String {
    let details = stderr
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("at ") && !t.starts_with("---")
        })
        .take(5)
        .collect::<Vec<_>>()
        .join("\n");
    if details.is_empty() {
        format!("N_m3u8DL-RE exited: {exit}")
    } else {
        format!("N_m3u8DL-RE exited: {exit}\n{details}")
    }
}

fn bundled_bin_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let resources_bin = exe_dir.parent().map(|c| c.join("Resources").join("bin"));
    if let Some(p) = resources_bin {
        if p.is_dir() {
            return Some(p);
        }
    }
    let dev_bin = exe_dir.join("bin");
    if dev_bin.is_dir() {
        return Some(dev_bin);
    }
    None
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

pub fn find_binary(name: &str) -> PathBuf {
    if let Some(bin_dir) = bundled_bin_dir() {
        let bundled = bin_dir.join(name);
        if is_executable(&bundled) {
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
        .find(|p| is_executable(p))
        .unwrap_or_else(|| PathBuf::from(name))
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
    use crate::test_support::scratch_dir;

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

    #[test]
    fn work_dir_is_unique_per_download_and_outside_output_tree() {
        let id = uuid::Uuid::new_v4();
        let dir = work_dir(id);
        assert!(dir.ends_with(format!("StreamVault/{id}")));
        assert!(!dir.starts_with(AppConfig::default().download_dir()));
        assert_ne!(dir, work_dir(uuid::Uuid::new_v4()));
    }

    #[tokio::test]
    async fn scan_tracks_orders_media_by_size_and_filters_subtitles() {
        let dir = scratch_dir("scan_tracks");
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        std::fs::write(dir.join("small.ts"), [0u8; 1]).unwrap();
        std::fs::write(dir.join("large.ts"), [0u8; 8]).unwrap();
        std::fs::write(dir.join("it.vtt"), "WEBVTT").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let (media, subtitles) = scan_tracks(&dir, true).await;
        assert_eq!(media, vec![dir.join("large.ts"), dir.join("small.ts")]);
        assert_eq!(subtitles, vec![dir.join("it.vtt")]);
        assert!(scan_tracks(&dir, false).await.1.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scan_tracks_on_missing_dir_is_empty() {
        let (media, subtitles) = scan_tracks(Path::new("/nonexistent/sv"), true).await;
        assert!(media.is_empty() && subtitles.is_empty());
    }

    #[tokio::test]
    async fn move_file_delivers_content_to_destination() {
        let dir = scratch_dir("move_file");
        let (from, to) = (dir.join("work/a.mp4"), dir.join("out/a.mp4"));
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&from, "video").unwrap();
        move_file(&from, &to).await.unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "video");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exit_message_keeps_first_meaningful_stderr_lines() {
        use std::os::unix::process::ExitStatusExt;
        let exit = ExitStatus::from_raw(256);
        let stderr = "boom\n   at Foo.Bar()\n---\n\nl2\nl3\nl4\nl5\nl6";
        assert_eq!(
            exit_message(exit, stderr),
            format!("N_m3u8DL-RE exited: {exit}\nboom\nl2\nl3\nl4\nl5")
        );
        assert_eq!(
            exit_message(exit, ""),
            format!("N_m3u8DL-RE exited: {exit}")
        );
    }

    #[tokio::test]
    async fn mux_output_without_media_fails() {
        let engine = DownloadEngine::new(AppConfig::default());
        let err = engine
            .mux_output(Path::new("ffmpeg"), Path::new("/tmp/out.mp4"), &[], &[])
            .await
            .unwrap_err();
        assert_eq!(err, "No .ts files found after download");
    }
}
