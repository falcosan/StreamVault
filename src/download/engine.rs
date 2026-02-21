use super::progress::{DownloadProgress, DownloadStatus};
use crate::config::AppConfig;
use crate::util::find_binary;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

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
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn download(
        &self,
        request: DownloadRequest,
        progress_tx: mpsc::UnboundedSender<DownloadProgress>,
    ) {
        let mut progress = DownloadProgress::new(request.id, request.title.clone());
        progress.status = DownloadStatus::Downloading;
        let _ = progress_tx.send(progress.clone());

        let n_m3u8dl = find_binary("N_m3u8DL-RE");
        let ffmpeg = find_binary("ffmpeg");

        let mut cmd = Command::new(&n_m3u8dl);
        cmd.arg(&request.stream_url)
            .arg("--save-name")
            .arg(&request.filename)
            .arg("--save-dir")
            .arg(&request.output_dir)
            .arg("--tmp-dir")
            .arg(request.output_dir.join("tmp"))
            .arg("--ffmpeg-binary-path")
            .arg(&ffmpeg)
            .arg("--no-log")
            .arg("--binary-merge")
            .arg("--del-after-done")
            .arg("--mux-after-done")
            .arg(format!("format={}", self.config.process.extension))
            .arg("--auto-subtitle-fix")
            .arg("false")
            .arg("--check-segments-count")
            .arg(if self.config.download.concurrent_download {
                "true"
            } else {
                "false"
            });

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

        if self.config.download.select_video != "best" {
            cmd.arg("--select-video")
                .arg(&self.config.download.select_video);
        }

        cmd.arg("--select-audio")
            .arg(&self.config.download.select_audio);

        cmd.arg("--select-subtitle")
            .arg(&self.config.download.select_subtitle);

        for (key, value) in &request.headers {
            cmd.arg("--header").arg(format!("{key}: {value}"));
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                progress.status =
                    DownloadStatus::Failed(format!("Failed to start N_m3u8DL-RE: {e}"));
                let _ = progress_tx.send(progress);
                return;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                progress.parse_n_m3u8dl_line(&line);
                if line.contains("Muxing") || line.contains("muxing") || line.contains("MUX") {
                    progress.status = DownloadStatus::Muxing;
                } else {
                    progress.status = DownloadStatus::Downloading;
                }
                let _ = progress_tx.send(progress.clone());
            }
        }

        let status = child.wait().await;
        match status {
            Ok(exit) if exit.success() => {
                progress.status = DownloadStatus::Completed;
                progress.percent = 100.0;
            }
            Ok(exit) => {
                progress.status =
                    DownloadStatus::Failed(format!("N_m3u8DL-RE exited with code: {exit}"));
            }
            Err(e) => {
                progress.status = DownloadStatus::Failed(format!("Process error: {e}"));
            }
        }

        let _ = progress_tx.send(progress);
    }

    pub fn build_output_path(&self, title: &str, is_movie: bool) -> PathBuf {
        let base = if is_movie {
            self.config.movie_dir()
        } else {
            self.config.serie_dir()
        };
        base.join(sanitize_filename(title))
    }

    pub fn format_episode_name(
        &self,
        show_name: &str,
        season: u32,
        episode: u32,
        episode_name: &str,
    ) -> String {
        self.config
            .output
            .map_episode_name
            .replace("%(episode_name)", episode_name)
            .replace("%(season)", &format!("{season:02}"))
            .replace("%(episode)", &format!("{episode:02}"))
            .replace("%(show_name)", show_name)
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}
