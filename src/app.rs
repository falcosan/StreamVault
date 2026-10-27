use crate::gui::{CLOSE_ICON, DetailsView, HomeView, Navbar, PlayerView, SearchView};
use crate::platform::{self, Device};
use crate::player::Player;
use crate::prefs::PlayerPrefs;
use crate::providers::{
    Episode, MediaEntry, Provider, ProviderResult, Providers, StreamUrl, next_episode,
};
use crate::search;
use crate::sync::{self, Replica};
use crate::time::now_ms;
use crate::watch::{self, WatchItem};
use dioxus::core::{Task, spawn_forever};
use dioxus::prelude::*;
use futures::future::{FutureExt, LocalBoxFuture, Shared};
use std::collections::HashSet;
use std::fmt::Display;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const CSS: &str = include_str!("../assets/style/style.css");
const LOAD_MORE_THRESHOLD: f64 = 1200.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Home,
    Search,
    Details,
    Player,
    #[cfg(feature = "desktop")]
    Downloads,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subtitle {
    pub file: PathBuf,
    pub language: String,
}

#[derive(Clone)]
pub struct Playback {
    pub entry: MediaEntry,
    pub season: Option<u32>,
    pub episode: Option<Episode>,
    pub start: f64,
    pub url: Option<String>,
    pub file: Option<PathBuf>,
    pub subtitles: Vec<Subtitle>,
}

impl Playback {
    pub fn new(
        entry: MediaEntry,
        season: Option<u32>,
        episode: Option<Episode>,
        start: f64,
    ) -> Self {
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

    pub fn tracks(&self) -> Vec<(String, String)> {
        let url = self.url.as_deref().unwrap_or_default();
        self.subtitles
            .iter()
            .enumerate()
            .map(|(index, subtitle)| (format!("{url}/{index}"), subtitle.language.clone()))
            .collect()
    }

    pub fn title(&self) -> String {
        self.entry.title(self.season, self.episode.as_ref())
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
    done: CopyValue<bool>,
    scroll: CopyValue<f64>,
}

#[derive(Clone, Copy, Default)]
pub struct Search {
    pub query: Signal<String>,
    pub results: Signal<Vec<MediaEntry>>,
    pub busy: Signal<bool>,
    task: CopyValue<Option<Task>>,
}

#[derive(Clone, Copy, Default)]
pub struct Details {
    pub entry: Signal<Option<MediaEntry>>,
    pub seasons: Signal<Vec<u32>>,
    pub season: Signal<Option<u32>>,
    pub episodes: Signal<Vec<Episode>>,
    pub loading: Signal<bool>,
    task: CopyValue<Option<Task>>,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub screen: Signal<Screen>,
    history: CopyValue<Vec<(Screen, Option<String>)>>,
    pub error: Signal<Option<String>>,
    pub catalog: Catalog,
    pub search: Search,
    pub details: Details,
    pub playback: Signal<Option<Playback>>,
    pub player: Player,
    stream_task: CopyValue<Option<Task>>,
    pub watch: Signal<Vec<WatchItem>>,
    pub prefs: Signal<PlayerPrefs>,
    pub device: Device,
    replica: CopyValue<Replica>,
    providers: CopyValue<Shared<LocalBoxFuture<'static, Providers>>>,
}

impl AppState {
    fn new() -> Self {
        let device = Device::new();
        Self {
            screen: Signal::default(),
            history: CopyValue::default(),
            error: Signal::default(),
            catalog: Catalog {
                items: Signal::default(),
                loading: Signal::new(true),
                loading_more: Signal::default(),
                done: CopyValue::default(),
                scroll: CopyValue::default(),
            },
            search: Search::default(),
            details: Details::default(),
            playback: Signal::default(),
            player: Player::new(),
            stream_task: CopyValue::default(),
            watch: Signal::default(),
            prefs: Signal::default(),
            replica: CopyValue::new(Replica::default()),
            providers: CopyValue::new(load_providers(device.timeout())),
            device,
        }
    }

    pub fn navigate(mut self, to: Screen) {
        let from = *self.screen.peek();
        if from == to {
            return;
        }
        if from == Screen::Player {
            self.clear_playback();
        }
        visit(&mut self.history.write(), (from, platform::focus_key()), to);
        self.error.set(None);
        self.screen.set(to);
    }

    pub fn back(mut self) -> bool {
        let Some((screen, focus)) = self.history.pop() else {
            return false;
        };
        if *self.screen.peek() == Screen::Player {
            self.clear_playback();
        }
        self.error.set(None);
        platform::restore_focus(focus);
        self.screen.set(screen);
        true
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
            let found = self.providers().await.search(&query).await;
            self.search.results.set(search::rank(found, &query));
            self.search.busy.set(false);
        });
    }

    pub fn reload(mut self) {
        self.providers.set(load_providers(self.device.timeout()));
        spawn_forever(self.load_catalog());
    }

    async fn load_catalog(mut self) {
        self.error.set(None);
        self.catalog.loading.set(true);
        let entries = self.providers().await.catalog().await;
        self.catalog.items.set(entries);
        self.catalog.done.set(false);
        self.catalog.loading.set(false);
    }

    fn load_more(mut self) {
        if *self.catalog.loading.peek()
            || *self.catalog.loading_more.peek()
            || *self.catalog.done.peek()
            || self.catalog.items.peek().is_empty()
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
            match self.providers().await.catalog_more(&mut seen).await {
                Some(fresh) => self.catalog.items.write().extend(fresh),
                None => self.catalog.done.set(true),
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
            let watching = self
                .watch
                .peek()
                .iter()
                .find(|item| item.entry.key() == entry.key())
                .and_then(|item| item.season);
            self.load_seasons(entry, watching);
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
            let episodes = async { self.provider(&entry).await?.episodes(&entry, season).await };
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
        let provider = self.provider(entry).await?;
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

    pub fn play(self, entry: MediaEntry, season: Option<u32>, episode: Option<Episode>) {
        let start = watch::resume_time(&self.watch.peek(), &entry, season, episode.as_ref());
        self.start(
            self.device
                .local(Playback::new(entry, season, episode, start)),
        );
    }

    pub fn play_episode(self, season: u32, episode: Episode) {
        if let Some(entry) = self.details.entry.peek().clone() {
            self.play(entry, Some(season), Some(episode));
        }
    }

    pub fn resume(self, item: WatchItem) {
        let playback = self.device.local(Playback::new(
            item.entry,
            item.season,
            item.episode,
            item.current_time,
        ));
        let season = playback.season.filter(|_| playback.file.is_none());
        self.prepare_details(&playback.entry, season);
        self.start(playback);
    }

    pub fn start(self, playback: Playback) {
        self.show_player(playback.clone());
        if playback.url.is_some() {
            cancel(self.stream_task);
        } else {
            restart(self.stream_task, self.load_stream(playback));
        }
    }

    pub fn prepare_details(mut self, entry: &MediaEntry, season: Option<u32>) {
        let loaded = self.details.entry.peek().as_ref() == Some(entry)
            && !self.details.seasons.peek().is_empty()
            && !self.details.episodes.peek().is_empty()
            && *self.details.season.peek() == season;
        self.details.entry.set(Some(entry.clone()));
        match season {
            Some(_) if loaded => {}
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
        self.end_session();
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
            || watch::next_season(&self.details.seasons.read(), *season).is_some()
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
            return self.play(playback.entry, Some(season), Some(next));
        }
        let Some(next_season) = watch::next_season(&self.details.seasons.peek(), season) else {
            return;
        };
        self.end_session();
        self.error.set(None);
        if let Some(current) = self.playback.write().as_mut() {
            current.url = None;
        }
        restart(self.stream_task, async move {
            let entry = playback.entry;
            let episodes = async {
                self.provider(&entry)
                    .await?
                    .episodes(&entry, next_season)
                    .await
            };
            let episodes = match episodes.await {
                Ok(episodes) => episodes,
                Err(e) => return self.report("Failed to load next season", e),
            };
            let Some(first) = watch::first_episode(&episodes).cloned() else {
                return self.report("Failed to load next season", "no episodes");
            };
            self.details.episodes.set(episodes);
            self.details.season.set(Some(next_season));
            let next = self
                .device
                .local(Playback::new(entry, Some(next_season), Some(first), 0.0));
            self.playback.set(Some(next.clone()));
            if next.url.is_none() {
                self.load_stream(next).await;
            }
        });
    }

    pub fn show_playing_details(self) {
        let Some(playback) = self.playback.peek().clone() else {
            return;
        };
        self.prepare_details(&playback.entry, playback.season);
        self.navigate(Screen::Details);
    }

    pub fn progress(self, time: f64, duration: f64) {
        let playback = self.playback.peek();
        let Some(playback) = playback.as_ref() else {
            return;
        };
        let episodes = self.details.episodes.peek();
        self.update_watch(|items| watch::record(items, playback.at(time, duration), &episodes));
    }

    pub fn finished(self) {
        let Some(playback) = self.playback.peek().clone() else {
            return;
        };
        let item = playback.at(0.0, 0.0);
        let handled = {
            let episodes = self.details.episodes.peek();
            self.update_watch(|items| watch::finish(items, &item, &episodes))
        };
        if handled {
            return;
        }
        let Some(season) = item.season else {
            return;
        };
        spawn_forever(async move {
            if let Err(e) = self.queue_next(item, season).await {
                warn!("next episode lookup failed: {e}");
            }
        });
    }

    async fn queue_next(self, item: WatchItem, season: u32) -> ProviderResult<()> {
        let entry = &item.entry;
        let provider = self.provider(entry).await?;
        let episodes = provider.episodes(entry, season).await?;
        if self.update_watch(|items| watch::finish(items, &item, &episodes)) {
            return Ok(());
        }
        let Some(next) = watch::next_season(&provider.seasons(entry).await?, season) else {
            self.remove_watch(entry.key());
            return Ok(());
        };
        let episodes = provider.episodes(entry, next).await?;
        if let Some(first) = watch::first_episode(&episodes) {
            self.update_watch(|items| {
                watch::upsert(items, watch::start_of(entry.clone(), next, first.clone()));
                true
            });
        }
        Ok(())
    }

    pub fn remove_watch(self, key: (usize, u64)) {
        self.update_watch(|items| {
            watch::remove(items, key);
            true
        });
    }

    pub fn change_prefs(mut self, prefs: PlayerPrefs) {
        if self.replica.write().record_prefs(&prefs, now_ms()) {
            self.publish();
        }
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

    pub async fn stream(
        self,
        entry: &MediaEntry,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) -> ProviderResult<StreamUrl> {
        self.provider(entry)
            .await?
            .stream(entry, season, episode)
            .await
    }

    async fn provider(self, entry: &MediaEntry) -> ProviderResult<Arc<dyn Provider>> {
        self.providers().await.get(entry.provider)
    }

    async fn providers(self) -> Providers {
        let providers = self.providers.read().clone();
        providers.await
    }

    fn end_session(self) {
        let status = *self.player.status.peek();
        self.progress(status.time, status.duration);
        self.player.reset();
    }

    fn clear_playback(mut self) {
        self.end_session();
        cancel(self.stream_task);
        self.playback.set(None);
    }

    fn update_watch(mut self, change: impl FnOnce(&mut Vec<WatchItem>) -> bool) -> bool {
        let mut items = self.watch.peek().clone();
        let changed = change(&mut items);
        if changed {
            self.replica.write().record_watch(&items, now_ms());
            self.publish();
        }
        changed
    }

    async fn load_state(mut self) {
        let replica = platform::load_replica()
            .await
            .unwrap_or_else(Replica::fresh);
        self.replica.set(replica);
        self.publish();
    }

    async fn sync(self) {
        sync::run(self.replica, platform::sync_config().await, move || {
            self.publish()
        })
        .await;
    }

    fn publish(mut self) {
        let replica = self.replica.peek();
        platform::save_replica(&replica);
        let watch = replica.watch();
        if *self.watch.peek() != watch {
            self.watch.set(watch);
        }
        let prefs = replica.prefs().unwrap_or_default();
        if *self.prefs.peek() != prefs {
            self.prefs.set(prefs);
        }
    }

    pub fn report(mut self, context: &str, error: impl Display) {
        self.error.set(Some(format!("{context}: {error}")));
    }
}

fn visit(history: &mut Vec<(Screen, Option<String>)>, from: (Screen, Option<String>), to: Screen) {
    if to == Screen::Home {
        history.clear();
    } else if from.0 != Screen::Player {
        history.push(from);
    } else if history.last().is_some_and(|(screen, _)| *screen == to) {
        history.pop();
    }
}

pub fn restart(mut slot: CopyValue<Option<Task>>, future: impl Future<Output = ()> + 'static) {
    cancel(slot);
    slot.set(Some(spawn_forever(future)));
}

fn load_providers(timeout: Duration) -> Shared<LocalBoxFuture<'static, Providers>> {
    Providers::load(timeout).boxed_local().shared()
}

pub fn cancel(mut slot: CopyValue<Option<Task>>) {
    if let Some(task) = slot.take() {
        task.cancel();
    }
}

#[component]
pub fn App() -> Element {
    let mut app = use_context_provider(AppState::new);
    platform::use_device(app);
    use_future(move || app.player.run(app));
    use_future(move || async move {
        app.load_state().await;
        app.sync().await;
    });
    use_future(move || app.load_catalog());
    use_effect(move || {
        let top = match *app.screen.read() {
            Screen::Home => *app.catalog.scroll.peek(),
            _ => 0.0,
        };
        document::eval(&format!(
            "document.querySelector('.content').scrollTop = {top};"
        ));
    });

    let screen = *app.screen.read();
    let page = match screen {
        Screen::Home => rsx! { HomeView {} },
        Screen::Search => rsx! { SearchView {} },
        Screen::Details => match app.details.entry.cloned() {
            Some(entry) => rsx! { DetailsView { entry } },
            None => rsx! { HomeView {} },
        },
        Screen::Player => rsx! { PlayerView {} },
        #[cfg(feature = "desktop")]
        Screen::Downloads => rsx! { platform::DownloadsView {} },
    };

    rsx! {
        style { dangerous_inner_html: CSS }
        platform::Head {}
        div { class: "app",
            if screen != Screen::Player {
                Navbar {}
            }
            if let Some(message) = app.error.cloned() {
                div { class: "error-bar", role: "alert",
                    span { "{message}" }
                    button {
                        class: "dismiss",
                        aria_label: "Dismiss error",
                        onclick: move |_| app.error.set(None),
                        span { class: "close-icon", dangerous_inner_html: CLOSE_ICON }
                    }
                }
            }
            main {
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
                {page}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_title_depends_on_episode() {
        let entry = MediaEntry {
            name: "Lost".into(),
            year: Some("2004".into()),
            ..Default::default()
        };
        let movie = Playback::new(entry.clone(), None, None, 0.0);
        assert_eq!(movie.title(), "Lost (2004)");
        let episode = Playback::new(entry, Some(1), Some(Episode::numbered(2, "Pilot")), 0.0);
        assert_eq!(episode.title(), "Lost S01E02 - Pilot");
    }

    #[test]
    fn leaving_the_player_for_the_screen_below_it_does_not_stack_it_twice() {
        let mut history = Vec::new();
        visit(&mut history, (Screen::Home, None), Screen::Player);
        visit(&mut history, (Screen::Player, None), Screen::Details);
        assert_eq!(history, [(Screen::Home, None)]);
        visit(
            &mut history,
            (Screen::Details, Some("play".into())),
            Screen::Player,
        );
        visit(&mut history, (Screen::Player, None), Screen::Details);
        assert_eq!(history, [(Screen::Home, None)]);
    }

    #[test]
    fn going_home_starts_a_fresh_history() {
        let mut history = Vec::new();
        visit(&mut history, (Screen::Home, None), Screen::Search);
        visit(&mut history, (Screen::Search, None), Screen::Details);
        visit(&mut history, (Screen::Details, None), Screen::Home);
        assert!(history.is_empty());
    }

    #[test]
    fn sidecar_tracks_are_served_under_the_playback_url() {
        let playback = Playback {
            url: Some("/media/x".into()),
            subtitles: vec![
                Subtitle {
                    file: PathBuf::from("/a.forced-ita.2.vtt"),
                    language: "forced-ita".into(),
                },
                Subtitle {
                    file: PathBuf::from("/a.ita.3.vtt"),
                    language: "ita".into(),
                },
            ],
            ..Playback::new(MediaEntry::default(), None, None, 0.0)
        };
        assert_eq!(
            playback.tracks(),
            [
                ("/media/x/0".to_owned(), "forced-ita".to_owned()),
                ("/media/x/1".to_owned(), "ita".to_owned())
            ]
        );
        assert!(
            Playback::new(MediaEntry::default(), None, None, 0.0)
                .tracks()
                .is_empty()
        );
    }
}
