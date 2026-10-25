mod config;
mod download;
mod media;
mod storage;
mod update;
mod views;

use crate::app::{AppState, Playback, Screen};
use crate::providers::{Episode, MediaEntry};
use crate::sync::SyncConfig;
use crate::update::Update;
use config::{AppConfig, config_dir};
use dioxus::core::{Task, spawn_forever};
use dioxus::desktop::{
    Config, LogicalSize, WindowBuilder, tao, use_asset_handler, use_wry_event_handler,
};
use dioxus::prelude::*;
use download::{Download, Status};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
pub use storage::{load_replica, save_replica};
use tokio::sync::Semaphore;
pub use update::install as install_update;
use uuid::Uuid;
pub use views::{DownloadsView, EntryActions};

pub const NAV: &[(Screen, &str)] = &[(Screen::Downloads, "Downloads")];
const MAX_DOWNLOADS: usize = 2;
static DOWNLOAD_SLOTS: Semaphore = Semaphore::const_new(MAX_DOWNLOADS);

pub fn launch(app: fn() -> Element) {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_disable_context_menu(true).with_window(
                WindowBuilder::new()
                    .with_title("StreamVault")
                    .with_inner_size(LogicalSize::new(1200.0, 800.0))
                    .with_min_inner_size(LogicalSize::new(800.0, 600.0)),
            ),
        )
        .launch(app);
}

pub async fn sync_config() -> SyncConfig {
    storage::load(&config_dir().join("sync.json"))
}

pub fn focus(selector: &str) {
    document::eval(&format!(
        "document.querySelector({selector:?})?.focus({{ preventScroll: true }});"
    ));
}

pub fn focus_key() -> Option<String> {
    None
}

pub fn restore_focus(_: Option<String>) {}

#[component]
pub fn Head() -> Element {
    VNode::empty()
}

#[derive(Clone, Copy)]
pub struct Device {
    pub update: Update,
    downloads: Signal<Vec<Download>>,
    tasks: Signal<HashMap<Uuid, Task>>,
    config: CopyValue<AppConfig>,
}

impl Device {
    pub fn new() -> Self {
        Self {
            update: Update::default(),
            downloads: Signal::new(storage::load_downloads()),
            tasks: Signal::default(),
            config: CopyValue::new(AppConfig::load()),
        }
    }

    pub fn timeout(self) -> Duration {
        self.config.read().timeout()
    }

    pub fn local(self, playback: Playback) -> Playback {
        let (entry, season, episode) =
            (&playback.entry, playback.season, playback.episode.as_ref());
        let file = download::saved_file(&self.downloads.peek(), entry, season, episode)
            .or_else(|| download::local_file(&self.config.read(), entry, season, episode));
        match file {
            Some(file) => with_file(playback, file),
            None => playback,
        }
    }

    fn status(
        self,
        entry: &MediaEntry,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) -> Option<Status> {
        let saved = download::status(&self.downloads.read(), entry, season, episode);
        saved.or_else(|| {
            download::local_file(&self.config.read(), entry, season, episode)
                .map(|_| Status::Completed)
        })
    }

    fn download(
        mut self,
        app: AppState,
        entry: &MediaEntry,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) {
        if self
            .status(entry, season, episode)
            .is_some_and(|s| s.is_active() || s == Status::Completed)
        {
            return;
        }
        let download = Download::new(&self.config.read(), entry, season, episode);
        let mut downloads = self.downloads.write();
        downloads.retain(|d| !d.is_for(entry, season, episode));
        downloads.push(download.clone());
        storage::save_downloads(&downloads);
        drop(downloads);
        self.fetch(app, download);
    }

    fn dismiss(mut self, id: Uuid) {
        let Some(index) = self
            .downloads
            .peek()
            .iter()
            .position(|d| d.id == id && d.status != Status::Muxing)
        else {
            return;
        };
        let dismissed = self.downloads.write().remove(index);
        storage::save_downloads(&self.downloads.peek());
        if let Some(task) = self.tasks.write().remove(&id) {
            task.cancel();
            spawn_forever(async move { download::discard(&dismissed).await });
        }
    }

    fn delete_download(mut self, app: AppState, file: PathBuf) {
        spawn_forever(async move {
            if let Err(e) = download::delete(&file).await {
                return app.report("Cannot delete download", e);
            }
            self.downloads
                .retain(|d| d.status != Status::Completed || d.file != file);
            storage::save_downloads(&self.downloads.peek());
        });
    }

    fn fetch(mut self, app: AppState, download: Download) {
        let config = self.config.read().clone();
        let id = download.id;
        let task = spawn_forever(async move {
            let _slot = DOWNLOAD_SLOTS.acquire().await;
            let report = move |status| self.set_status(id, status);
            let stream = app
                .stream(&download.entry, download.season, download.episode.as_ref())
                .await;
            match stream {
                Ok(stream) => download::run(&config, &download, &stream, report).await,
                Err(e) => report(Status::Failed(e.to_string())),
            }
            self.tasks.write().remove(&id);
        });
        self.tasks.write().insert(id, task);
    }

    fn resume_downloads(self, app: AppState) {
        for download in self.downloads.peek().iter() {
            if download.status == Status::Queued {
                self.fetch(app, download.clone());
            }
        }
    }

    fn stop_downloads(mut self) {
        for task in self.tasks.take().into_values() {
            task.cancel();
        }
    }

    fn set_status(mut self, id: Uuid, status: Status) {
        let finished = matches!(status, Status::Completed | Status::Failed(_));
        if let Some(download) = self.downloads.write().iter_mut().find(|d| d.id == id) {
            if status == Status::Completed {
                download.size = std::fs::metadata(&download.file).ok().map(|m| m.len());
            }
            download.status = status;
        }
        if finished {
            storage::save_downloads(&self.downloads.peek());
        }
    }
}

pub fn use_device(app: AppState) {
    let device = app.device;
    use_hook(move || device.resume_downloads(app));
    use_future(move || update::watch(device));
    use_wry_event_handler(move |event, _| {
        if matches!(event, tao::event::Event::LoopDestroyed) {
            device.stop_downloads();
        }
    });
    use_asset_handler(media::ROUTE, move |request, responder| {
        let file = app
            .playback
            .peek()
            .as_ref()
            .and_then(|p| served(p, request.uri().path()))
            .map(Path::to_path_buf);
        let range = media::requested_range(&request);
        tokio::spawn(async move {
            responder.respond(media::respond(file.as_deref(), range.as_deref()).await);
        });
    });
}

fn play_download(app: AppState, download: Download) {
    if !download.file.is_file() {
        return app.report("File not found", download.file.display());
    }
    app.prepare_details(&download.entry, None);
    app.play(download.entry, download.season, download.episode);
}

fn reveal(app: AppState, file: PathBuf) {
    spawn_forever(async move {
        let revealed = tokio::process::Command::new("open")
            .arg("-R")
            .arg(&file)
            .status()
            .await
            .is_ok_and(|s| s.success());
        if !revealed {
            app.report("Cannot show in Finder", file.display());
        }
    });
}

fn with_file(playback: Playback, file: PathBuf) -> Playback {
    Playback {
        url: Some(media::url(Uuid::new_v4())),
        subtitles: download::subtitles(&file),
        file: Some(file),
        ..playback
    }
}

fn served<'a>(playback: &'a Playback, path: &str) -> Option<&'a Path> {
    match path.strip_prefix(playback.url.as_deref()?)? {
        "" => playback.file.as_deref(),
        rest => {
            let index: usize = rest.strip_prefix('/')?.parse().ok()?;
            playback.subtitles.get(index).map(|s| s.file.as_path())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;

    #[test]
    fn local_playback_gets_a_fresh_media_url() {
        let streaming = Playback::new(MediaEntry::default(), None, None, 12.0);
        let first = with_file(streaming.clone(), PathBuf::from("/a.mp4"));
        let second = with_file(streaming, PathBuf::from("/a.mp4"));
        assert!(
            first
                .url
                .as_deref()
                .is_some_and(|u| u.starts_with("/media/"))
        );
        assert_eq!(first.file.as_deref(), Some(Path::new("/a.mp4")));
        assert_eq!(first.start, 12.0);
        assert_ne!(first.url, second.url);
    }

    #[test]
    fn local_playback_serves_only_its_video_and_subtitles() {
        let dir = scratch_dir("served");
        let video = dir.join("Ep.mp4");
        for file in ["Ep.mp4", "Ep.ita.3.vtt", "Ep.forced-ita.2.vtt"] {
            std::fs::write(dir.join(file), "x").unwrap();
        }
        let playback = with_file(
            Playback::new(MediaEntry::default(), None, None, 0.0),
            video.clone(),
        );
        let url = playback.url.clone().unwrap();
        assert_eq!(
            playback.tracks(),
            [
                (format!("{url}/0"), "forced-ita".to_owned()),
                (format!("{url}/1"), "ita".to_owned())
            ]
        );
        assert_eq!(served(&playback, &url), Some(video.as_path()));
        assert_eq!(
            served(&playback, &format!("{url}/1")),
            Some(dir.join("Ep.ita.3.vtt").as_path())
        );
        for path in [
            format!("{url}/2"),
            format!("{url}/x"),
            format!("{url}0"),
            "/media/other".into(),
        ] {
            assert_eq!(served(&playback, &path), None);
        }
        let streaming = Playback::new(MediaEntry::default(), None, None, 0.0);
        assert_eq!(served(&streaming, &url), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
