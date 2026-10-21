use crate::config::AppConfig;
use crate::download::{self, Download, Status, Subtitle};
use crate::gui::{DetailsView, DownloadsView, HomeView, Navbar, PlayerView, SearchView};
use crate::media;
use crate::providers::{
    Episode, MediaEntry, Provider, ProviderResult, Providers, StreamUrl, next_episode,
};
use crate::search;
use crate::state::{self, PlayerPrefs, WatchItem};
use crate::sync::{self, SyncConfig};
use dioxus::core::{Task, spawn_forever};
use dioxus::desktop::{tao, use_asset_handler, use_wry_event_handler};
use dioxus::prelude::*;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

const CSS: &str = include_str!("../assets/style/style.css");
const REPOSITORY: &str = "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main";
const LOAD_MORE_THRESHOLD: f64 = 600.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Home,
    Search,
    Details,
    Player,
    Downloads,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Playback {
    pub entry: MediaEntry,
    pub season: Option<u32>,
    pub episode: Option<Episode>,
    pub start: f64,
    pub url: Option<String>,
    file: Option<PathBuf>,
    subtitles: Vec<Subtitle>,
}

impl Playback {
    fn new(entry: MediaEntry, season: Option<u32>, episode: Option<Episode>, start: f64) -> Self {
        Self {
            entry,
            season,
            episode,
            start,
            url: None,
            file: None,
            subtitles: Vec::new(),
        }
    }

    fn with_file(self, file: PathBuf) -> Self {
        Self {
            url: Some(media::url(Uuid::new_v4())),
            subtitles: download::subtitles(&file),
            file: Some(file),
            ..self
        }
    }

    pub fn tracks(&self) -> Vec<(String, String)> {
        let url = self.url.as_deref().unwrap_or_default();
        self.subtitles
            .iter()
            .enumerate()
            .map(|(index, subtitle)| (format!("{url}/{index}"), subtitle.language.clone()))
            .collect()
    }

    fn served(&self, path: &str) -> Option<&Path> {
        match path.strip_prefix(self.url.as_deref()?)? {
            "" => self.file.as_deref(),
            rest => {
                let index: usize = rest.strip_prefix('/')?.parse().ok()?;
                self.subtitles.get(index).map(|s| s.file.as_path())
            }
        }
    }

    pub fn title(&self) -> String {
        match (self.season, &self.episode) {
            (Some(season), Some(episode)) => self.entry.episode_title(season, episode),
            _ => self.entry.display_title(),
        }
    }

    fn at(&self, current_time: f64, duration: f64) -> WatchItem {
        WatchItem {
            entry: self.entry.clone(),
            current_time,
            duration,
            season: self.season,
            episode: self.episode.clone(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Catalog {
    pub items: Signal<Vec<MediaEntry>>,
    pub loading: Signal<bool>,
    pub loading_more: Signal<bool>,
    done: Signal<bool>,
    scroll: Signal<f64>,
}

#[derive(Clone, Copy, Default)]
pub struct Search {
    pub query: Signal<String>,
    pub results: Signal<Vec<MediaEntry>>,
    pub busy: Signal<bool>,
    task: Signal<Option<Task>>,
}

#[derive(Clone, Copy, Default)]
pub struct Details {
    pub entry: Signal<Option<MediaEntry>>,
    pub seasons: Signal<Vec<u32>>,
    pub season: Signal<Option<u32>>,
    pub episodes: Signal<Vec<Episode>>,
    pub loading: Signal<bool>,
    task: Signal<Option<Task>>,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub screen: Signal<Screen>,
    history: Signal<Vec<Screen>>,
    pub error: Signal<Option<String>>,
    pub catalog: Catalog,
    pub search: Search,
    pub details: Details,
    pub playback: Signal<Option<Playback>>,
    stream_task: Signal<Option<Task>>,
    pub downloads: Signal<Vec<Download>>,
    download_tasks: Signal<HashMap<Uuid, Task>>,
    pub watch: Signal<Vec<WatchItem>>,
    pub prefs: Signal<PlayerPrefs>,
    pub update_available: Signal<bool>,
    pub updating: Signal<bool>,
    providers: CopyValue<Providers>,
    config: CopyValue<AppConfig>,
}

impl AppState {
    fn new() -> Self {
        let config = AppConfig::load();
        Self {
            screen: Signal::default(),
            history: Signal::default(),
            error: Signal::default(),
            catalog: Catalog {
                items: Signal::default(),
                loading: Signal::new(true),
                loading_more: Signal::default(),
                done: Signal::default(),
                scroll: Signal::default(),
            },
            search: Search::default(),
            details: Details::default(),
            playback: Signal::default(),
            stream_task: Signal::default(),
            downloads: Signal::new(state::load_downloads()),
            download_tasks: Signal::default(),
            watch: Signal::new(state::load_watch()),
            prefs: Signal::new(state::load_prefs()),
            update_available: Signal::default(),
            updating: Signal::default(),
            providers: CopyValue::new(Providers::new(config.timeout())),
            config: CopyValue::new(config),
        }
    }

    pub fn navigate(mut self, to: Screen) {
        let from = *self.screen.peek();
        if from != to {
            self.history.push(from);
            self.screen.set(to);
        }
    }

    pub fn back(mut self) {
        match self.history.pop().unwrap_or_default() {
            Screen::Player => {
                self.clear_playback();
                self.screen.set(Screen::Home);
            }
            screen => self.screen.set(screen),
        }
    }

    pub fn stop(mut self) {
        self.clear_playback();
        let previous = self.history.pop().unwrap_or_default();
        self.screen.set(previous);
    }

    pub fn search(mut self) {
        let query = self.search.query.peek().trim().to_owned();
        if query.is_empty() {
            return;
        }
        self.search.busy.set(true);
        self.search.results.set(Vec::new());
        self.navigate(Screen::Search);
        restart(self.search.task, async move {
            let found = self.providers().search(&query).await;
            self.search.results.set(search::rank(found, &query));
            self.search.busy.set(false);
        });
    }

    fn load_more(mut self) {
        if *self.catalog.loading.peek()
            || *self.catalog.loading_more.peek()
            || *self.catalog.done.peek()
        {
            return;
        }
        self.catalog.loading_more.set(true);
        spawn_forever(async move {
            let mut seen: HashSet<_> = self
                .catalog
                .items
                .peek()
                .iter()
                .map(MediaEntry::key)
                .collect();
            match self.providers().catalog_more(&mut seen).await {
                Ok(Some(fresh)) => self.catalog.items.write().extend(fresh),
                Ok(None) => self.catalog.done.set(true),
                Err(e) => eprintln!("[StreamVault] catalog page failed: {e}"),
            }
            self.catalog.loading_more.set(false);
        });
    }

    pub fn open(mut self, entry: MediaEntry) {
        self.details.entry.set(Some(entry.clone()));
        self.details.season.set(None);
        self.navigate(Screen::Details);
        if entry.is_movie() {
            self.clear_episodes();
        } else {
            self.load_seasons(entry, None);
        }
    }

    pub fn select_season(mut self, season: u32) {
        let Some(entry) = self.details.entry.peek().clone() else {
            return;
        };
        self.details.season.set(Some(season));
        self.details.episodes.set(Vec::new());
        self.details.loading.set(true);
        restart(self.details.task, async move {
            let episodes = async { self.provider(&entry)?.episodes(&entry, season).await };
            match episodes.await {
                Ok(episodes) => self.details.episodes.set(episodes),
                Err(e) => self.report("Failed to load episodes", e),
            }
            self.details.loading.set(false);
        });
    }

    fn load_seasons(mut self, entry: MediaEntry, preferred: Option<u32>) {
        self.details.loading.set(true);
        self.details.seasons.set(Vec::new());
        self.details.episodes.set(Vec::new());
        restart(self.details.task, async move {
            if let Err(e) = self.fill_details(&entry, preferred).await {
                self.report("Failed to load episodes", e);
            }
            self.details.loading.set(false);
        });
    }

    async fn fill_details(
        mut self,
        entry: &MediaEntry,
        preferred: Option<u32>,
    ) -> ProviderResult<()> {
        let provider = self.provider(entry)?;
        let seasons = provider.seasons(entry).await?;
        let pick = preferred
            .filter(|p| seasons.contains(p))
            .or(seasons.first().copied());
        self.details.seasons.set(seasons);
        if let Some(season) = pick {
            self.details.season.set(Some(season));
            self.details
                .episodes
                .set(provider.episodes(entry, season).await?);
        }
        Ok(())
    }

    pub fn play(
        self,
        entry: MediaEntry,
        season: Option<u32>,
        episode: Option<Episode>,
        start: f64,
    ) {
        self.start(self.local(Playback::new(entry, season, episode, start)));
    }

    pub fn play_download(self, download: Download) {
        if !download.file.is_file() {
            return self.report("File not found", download.file.display());
        }
        self.focus(&download.entry, None);
        let playback = Playback::new(download.entry, download.season, download.episode, 0.0);
        self.start(playback.with_file(download.file));
    }

    pub fn reveal(self, file: PathBuf) {
        spawn_forever(async move {
            let revealed = tokio::process::Command::new("open")
                .arg("-R")
                .arg(&file)
                .status()
                .await
                .is_ok_and(|s| s.success());
            if !revealed {
                self.report("Cannot show in Finder", file.display());
            }
        });
    }

    pub fn play_episode(self, season: u32, episode: Episode) {
        if let Some(entry) = self.details.entry.peek().clone() {
            self.play(entry, Some(season), Some(episode), 0.0);
        }
    }

    pub fn resume(self, item: WatchItem) {
        self.update_watch(|items| {
            state::upsert(items, item.clone());
            true
        });
        let playback = self.local(Playback::new(
            item.entry,
            item.season,
            item.episode,
            item.current_time,
        ));
        let season = playback.season.filter(|_| playback.file.is_none());
        self.focus(&playback.entry, season);
        self.start(playback);
    }

    fn local(self, playback: Playback) -> Playback {
        let file = download::local_file(
            &self.config.read(),
            &playback.entry,
            playback.season,
            playback.episode.as_ref(),
        );
        match file {
            Some(file) => playback.with_file(file),
            None => playback,
        }
    }

    fn start(self, playback: Playback) {
        self.show_player(playback.clone());
        if playback.file.is_some() {
            cancel(self.stream_task);
        } else {
            restart(self.stream_task, self.load_stream(playback));
        }
    }

    fn focus(mut self, entry: &MediaEntry, season: Option<u32>) {
        self.details.entry.set(Some(entry.clone()));
        match season {
            Some(season) => self.load_seasons(entry.clone(), Some(season)),
            None => self.clear_episodes(),
        }
    }

    fn clear_episodes(mut self) {
        cancel(self.details.task);
        self.details.loading.set(false);
        self.details.seasons.set(Vec::new());
        self.details.episodes.set(Vec::new());
    }

    fn show_player(mut self, playback: Playback) {
        self.error.set(None);
        self.playback.set(Some(playback));
        self.navigate(Screen::Player);
    }

    pub fn has_next(self) -> bool {
        let playback = self.playback.read();
        let Some(Playback {
            season: Some(season),
            episode: Some(current),
            ..
        }) = playback.as_ref()
        else {
            return false;
        };
        next_episode(&self.details.episodes.read(), current.number).is_some()
            || self.details.seasons.read().iter().any(|s| s > season)
    }

    pub fn play_next(mut self) {
        let Some(playback) = self.playback.peek().clone() else {
            return;
        };
        let (Some(season), Some(current)) = (playback.season, &playback.episode) else {
            return;
        };
        let next = next_episode(&self.details.episodes.peek(), current.number).cloned();
        if let Some(next) = next {
            return self.play(playback.entry, Some(season), Some(next), 0.0);
        }
        let next_season = self
            .details
            .seasons
            .peek()
            .iter()
            .copied()
            .filter(|s| *s > season)
            .min();
        let Some(next_season) = next_season else {
            return;
        };
        self.error.set(None);
        if let Some(current) = self.playback.write().as_mut() {
            current.url = None;
        }
        restart(self.stream_task, async move {
            let entry = playback.entry;
            let episodes = async { self.provider(&entry)?.episodes(&entry, next_season).await };
            let episodes = match episodes.await {
                Ok(episodes) => episodes,
                Err(e) => return self.report("Failed to load next season", e),
            };
            let Some(first) = episodes.iter().min_by_key(|e| e.number).cloned() else {
                return;
            };
            self.details.episodes.set(episodes);
            self.details.season.set(Some(next_season));
            let next = self.local(Playback::new(entry, Some(next_season), Some(first), 0.0));
            self.playback.set(Some(next.clone()));
            if next.file.is_none() {
                self.load_stream(next).await;
            }
        });
    }

    pub fn show_playing_details(mut self) {
        let Some(playback) = self.playback.peek().clone() else {
            return;
        };
        let reusable = self.details.entry.peek().as_ref() == Some(&playback.entry)
            && !self.details.seasons.peek().is_empty()
            && !self.details.episodes.peek().is_empty()
            && (playback.season.is_none() || *self.details.season.peek() == playback.season);
        self.details.entry.set(Some(playback.entry.clone()));
        self.navigate(Screen::Details);
        if !playback.entry.is_movie() && !reusable {
            self.load_seasons(playback.entry, playback.season);
        }
    }

    pub fn progress(self, time: f64, duration: f64) {
        let playback = self.playback.peek();
        let Some(playback) = playback.as_ref() else {
            return;
        };
        let episodes = self.details.episodes.peek();
        self.update_watch(|items| state::record(items, playback.at(time, duration), &episodes));
    }

    pub fn finished(self) {
        let Some(playback) = self.playback.peek().clone() else {
            return;
        };
        let item = playback.at(0.0, 0.0);
        let handled = {
            let episodes = self.details.episodes.peek();
            self.update_watch(|items| state::finish(items, &item, &episodes))
        };
        if handled {
            return;
        }
        let Some(season) = item.season else {
            return;
        };
        spawn_forever(async move {
            if let Err(e) = self.queue_next_season(item.entry, season).await {
                eprintln!("[StreamVault] next season lookup failed: {e}");
            }
        });
    }

    async fn queue_next_season(self, entry: MediaEntry, season: u32) -> ProviderResult<()> {
        let provider = self.provider(&entry)?;
        let next = provider
            .seasons(&entry)
            .await?
            .into_iter()
            .filter(|s| *s > season)
            .min();
        let Some(next) = next else {
            self.remove_watch(entry.key());
            return Ok(());
        };
        let episodes = provider.episodes(&entry, next).await?;
        if let Some(first) = episodes.into_iter().min_by_key(|e| e.number) {
            self.update_watch(|items| {
                state::upsert(items, state::start_of(entry, next, first));
                true
            });
        }
        Ok(())
    }

    pub fn remove_watch(self, key: (usize, u64)) {
        self.update_watch(|items| {
            state::remove(items, key);
            true
        });
    }

    pub fn change_prefs(mut self, changes: Map<String, Value>) {
        let merged = self.prefs.peek().merged(changes);
        if merged != *self.prefs.peek() {
            state::save_prefs(&merged);
            self.prefs.set(merged);
        }
    }

    pub fn download(mut self, entry: &MediaEntry, season: Option<u32>, episode: Option<&Episode>) {
        let download = Download::new(&self.config.read(), entry, season, episode);
        self.downloads.push(download.clone());
        state::save_downloads(&self.downloads.peek());
        self.navigate(Screen::Downloads);
        self.fetch(download);
    }

    fn resume_downloads(self) {
        for download in self.downloads.peek().iter() {
            if download.status == Status::Queued {
                self.fetch(download.clone());
            }
        }
    }

    fn fetch(mut self, download: Download) {
        let config = self.config.read().clone();
        let id = download.id;
        let task = spawn_forever(async move {
            let report = move |status| self.set_status(id, status);
            let stream = self
                .stream(&download.entry, download.season, download.episode.as_ref())
                .await;
            match stream {
                Ok(stream) => download::run(&config, &download, &stream, report).await,
                Err(e) => report(Status::Failed(e.to_string())),
            }
            self.download_tasks.write().remove(&id);
        });
        self.download_tasks.write().insert(id, task);
    }

    fn stop_downloads(mut self) {
        for task in self.download_tasks.take().into_values() {
            task.cancel();
        }
    }

    pub fn download_episode(self, season: u32, episode: &Episode) {
        if let Some(entry) = self.details.entry.peek().clone() {
            self.download(&entry, Some(season), Some(episode));
        }
    }

    pub fn update(mut self) {
        self.updating.set(true);
        spawn_forever(async move {
            let dir = std::env::current_exe()
                .ok()
                .and_then(|exe| {
                    exe.ancestors()
                        .find(|a| a.extension().is_some_and(|e| e == "app"))
                        .and_then(Path::parent)
                        .map(Path::to_path_buf)
                })
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let installed = tokio::process::Command::new("bash")
                .arg("-c")
                .arg(format!(
                    "set -o pipefail; curl -fsSL '{REPOSITORY}/scripts/package.sh' | bash"
                ))
                .current_dir(dir)
                .status()
                .await
                .is_ok_and(|s| s.success());
            if installed {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).spawn();
                }
                return dioxus::desktop::window().close();
            }
            self.updating.set(false);
            self.error.set(Some("Update failed".into()));
        });
    }

    async fn load_catalog(mut self) {
        let entries = self.providers().catalog().await;
        self.catalog.items.set(entries);
        self.catalog.loading.set(false);
    }

    async fn check_update(mut self) {
        let Ok(response) = reqwest::get(format!("{REPOSITORY}/Cargo.toml")).await else {
            return;
        };
        let manifest = response.text().await.unwrap_or_default();
        if manifest_version(&manifest).is_some_and(|v| is_newer(v, env!("CARGO_PKG_VERSION"))) {
            self.update_available.set(true);
        }
    }

    async fn sync(mut self) {
        let Some(config) = SyncConfig::load() else {
            return;
        };
        sync::run(config, move |watch, prefs| {
            self.watch.set(watch);
            self.prefs.set(prefs);
        })
        .await;
    }

    async fn load_stream(mut self, playback: Playback) {
        let stream = self
            .stream(&playback.entry, playback.season, playback.episode.as_ref())
            .await;
        match stream {
            Ok(stream) => {
                if let Some(current) = self.playback.write().as_mut() {
                    current.url = Some(stream.url);
                }
            }
            Err(e) => self.report("Failed to get stream", e),
        }
    }

    async fn stream(
        self,
        entry: &MediaEntry,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) -> ProviderResult<StreamUrl> {
        self.provider(entry)?.stream(entry, season, episode).await
    }

    fn provider(self, entry: &MediaEntry) -> ProviderResult<Arc<dyn Provider>> {
        self.providers.read().get(entry.provider)
    }

    fn providers(self) -> Providers {
        self.providers.read().clone()
    }

    fn clear_playback(mut self) {
        cancel(self.stream_task);
        self.playback.set(None);
    }

    fn update_watch(mut self, change: impl FnOnce(&mut Vec<WatchItem>) -> bool) -> bool {
        let mut items = self.watch.write();
        let changed = change(&mut items);
        if changed {
            state::save_watch(&items);
        }
        changed
    }

    fn playing_file(self, path: &str) -> Option<PathBuf> {
        self.playback
            .peek()
            .as_ref()
            .and_then(|p| p.served(path))
            .map(Path::to_path_buf)
    }

    fn set_status(mut self, id: Uuid, status: Status) {
        let finished = matches!(status, Status::Completed | Status::Failed(_));
        if let Some(download) = self.downloads.write().iter_mut().find(|d| d.id == id) {
            download.status = status;
        }
        if finished {
            state::save_downloads(&self.downloads.peek());
        }
    }

    fn report(mut self, context: &str, error: impl Display) {
        self.error.set(Some(format!("{context}: {error}")));
    }
}

fn restart(mut slot: Signal<Option<Task>>, future: impl Future<Output = ()> + 'static) {
    cancel(slot);
    slot.set(Some(spawn_forever(future)));
}

fn cancel(mut slot: Signal<Option<Task>>) {
    if let Some(task) = slot.take() {
        task.cancel();
    }
}

fn manifest_version(manifest: &str) -> Option<&str> {
    manifest.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix("version")?
            .trim_start()
            .strip_prefix('=')?;
        Some(value.trim().trim_matches('"'))
    })
}

fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |v: &str| {
        v.split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    match (parse(remote), parse(local)) {
        (Some(remote), Some(local)) => remote > local,
        _ => remote != local,
    }
}

#[component]
pub fn App() -> Element {
    let mut app = use_context_provider(AppState::new);
    use_future(move || app.load_catalog());
    use_future(move || app.check_update());
    use_future(move || app.sync());
    use_hook(move || app.resume_downloads());
    use_wry_event_handler(move |event, _| {
        if matches!(event, tao::event::Event::LoopDestroyed) {
            app.stop_downloads();
        }
    });
    use_asset_handler(media::ROUTE, move |request, responder| {
        let file = app.playing_file(request.uri().path());
        let range = media::requested_range(&request);
        tokio::spawn(async move {
            responder.respond(media::respond(file.as_deref(), range.as_deref()).await);
        });
    });
    use_effect(move || {
        if *app.screen.read() == Screen::Home {
            let top = *app.catalog.scroll.peek();
            document::eval(&format!(
                "document.querySelector('.content').scrollTop = {top};"
            ));
        }
    });

    rsx! {
        style { dangerous_inner_html: CSS }
        div { class: "app",
            Navbar {}
            if let Some(message) = app.error.cloned() {
                div { class: "error-bar", role: "alert",
                    span { "{message}" }
                    button {
                        class: "dismiss",
                        aria_label: "Dismiss error",
                        onclick: move |_| app.error.set(None),
                        "✕"
                    }
                }
            }
            div {
                class: "content",
                onscroll: move |e: Event<ScrollData>| {
                    if *app.screen.peek() != Screen::Home {
                        return;
                    }
                    let top = e.scroll_top();
                    app.catalog.scroll.set(top);
                    if top + f64::from(e.client_height())
                        >= f64::from(e.scroll_height()) - LOAD_MORE_THRESHOLD
                    {
                        app.load_more();
                    }
                },
                match *app.screen.read() {
                    Screen::Home => rsx! { HomeView {} },
                    Screen::Search => rsx! { SearchView {} },
                    Screen::Details => match app.details.entry.cloned() {
                        Some(entry) => rsx! { DetailsView { entry } },
                        None => rsx! { HomeView {} },
                    },
                    Screen::Player => rsx! { PlayerView {} },
                    Screen::Downloads => rsx! { DownloadsView {} },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch_dir;

    #[test]
    fn manifest_version_reads_package_version() {
        let manifest = "[package]\nname = \"x\"\nversion = \"1.2.3\"\n\n[dependencies]\nfoo = { version = \"9\" }\n";
        assert_eq!(manifest_version(manifest), Some("1.2.3"));
        assert_eq!(manifest_version("[package]\nversioned = 1\n"), None);
    }

    #[test]
    fn is_newer_compares_numerically() {
        assert!(is_newer("1.0.31", "1.0.30"));
        assert!(is_newer("1.0.100", "1.0.99"));
        assert!(!is_newer("1.0.30", "1.0.30"));
        assert!(!is_newer("1.0.29", "1.0.30"));
        assert!(is_newer("1.1.0-beta", "1.0.30"));
    }

    #[test]
    fn playback_title_depends_on_episode() {
        let entry = MediaEntry {
            name: "Lost".into(),
            year: Some("2004".into()),
            ..Default::default()
        };
        let movie = Playback::new(entry.clone(), None, None, 0.0);
        assert_eq!(movie.title(), "Lost (2004)");
        let pilot = Episode {
            id: 1,
            number: 2,
            name: "Pilot".into(),
            duration: None,
            image_url: None,
        };
        let episode = Playback::new(entry, Some(1), Some(pilot), 0.0);
        assert_eq!(episode.title(), "Lost S01E02 - Pilot");
    }

    #[test]
    fn local_playback_gets_a_fresh_media_url() {
        let streaming = Playback::new(MediaEntry::default(), None, None, 12.0);
        assert_eq!(
            (streaming.url.as_deref(), streaming.file.as_deref()),
            (None, None)
        );
        let first = streaming.clone().with_file(PathBuf::from("/a.mp4"));
        let second = streaming.with_file(PathBuf::from("/a.mp4"));
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
        let playback =
            Playback::new(MediaEntry::default(), None, None, 0.0).with_file(video.clone());
        let url = playback.url.clone().unwrap();
        assert_eq!(
            playback.tracks(),
            [
                (format!("{url}/0"), "forced-ita".to_owned()),
                (format!("{url}/1"), "ita".to_owned())
            ]
        );
        assert_eq!(playback.served(&url), Some(video.as_path()));
        assert_eq!(
            playback.served(&format!("{url}/1")),
            Some(dir.join("Ep.ita.3.vtt").as_path())
        );
        for path in [
            format!("{url}/2"),
            format!("{url}/x"),
            format!("{url}0"),
            "/media/other".into(),
        ] {
            assert_eq!(playback.served(&path), None);
        }
        let streaming = Playback::new(MediaEntry::default(), None, None, 0.0);
        assert_eq!(streaming.served(&url), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
