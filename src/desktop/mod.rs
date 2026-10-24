mod config;
mod download;
mod media;
mod storage;
#[cfg(test)]
mod test_support;
mod update;
mod views;

use crate::app::{AppState, Playback, Screen};
use crate::providers::{Episode, MediaEntry};
use crate::sync::SyncConfig;
use config::{AppConfig, config_dir, read_json};
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
use uuid::Uuid;
pub use views::{DownloadsView, EntryActions, NavTools};

pub const NAV: &[(Screen, &str)] = &[(Screen::Downloads, "Downloads")];

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

pub fn sync_config() -> SyncConfig {
    read_json(&config_dir().join("sync.json")).unwrap_or_default()
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
    pub downloads: Signal<Vec<Download>>,
    tasks: Signal<HashMap<Uuid, Task>>,
    config: CopyValue<AppConfig>,
    pub update_available: Signal<bool>,
    pub updating: Signal<bool>,
}

impl Device {
    pub fn new() -> Self {
        Self {
            downloads: Signal::new(storage::load_downloads()),
            tasks: Signal::default(),
            config: CopyValue::new(AppConfig::load()),
            update_available: Signal::default(),
            updating: Signal::default(),
        }
    }

    pub fn timeout(self) -> Duration {
        self.config.read().timeout()
    }

    pub fn local(self, playback: Playback) -> Playback {
        let file = download::local_file(
            &self.config.read(),
            &playback.entry,
            playback.season,
            playback.episode.as_ref(),
        );
        match file {
            Some(file) => with_file(playback, file),
            None => playback,
        }
    }

    pub fn is_downloaded(
        self,
        entry: &MediaEntry,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) -> bool {
        self.downloads.read();
        download::local_file(&self.config.read(), entry, season, episode).is_some()
    }

    pub fn download(
        mut self,
        app: AppState,
        entry: &MediaEntry,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) {
        let download = Download::new(&self.config.read(), entry, season, episode);
        self.downloads.push(download.clone());
        storage::save_downloads(&self.downloads.peek());
        app.navigate(Screen::Downloads);
        self.fetch(app, download);
    }

    pub fn play_download(self, app: AppState, download: Download) {
        if !download.file.is_file() {
            return app.report("File not found", download.file.display());
        }
        app.prepare_details(&download.entry, None);
        let playback = Playback::new(download.entry, download.season, download.episode, 0.0);
        app.start(with_file(playback, download.file));
    }

    pub fn delete_download(mut self, app: AppState, file: PathBuf) {
        spawn_forever(async move {
            if let Err(e) = download::delete(&file).await {
                return app.report("Cannot delete download", e);
            }
            self.downloads
                .retain(|d| d.status != Status::Completed || d.file != file);
            storage::save_downloads(&self.downloads.peek());
        });
    }

    pub fn reveal(self, app: AppState, file: PathBuf) {
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

    fn fetch(mut self, app: AppState, download: Download) {
        let config = self.config.read().clone();
        let id = download.id;
        let task = spawn_forever(async move {
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
    use_future(move || update::check(device));
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
    use super::test_support::scratch_dir;
    use super::*;

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
