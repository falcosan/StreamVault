use crate::config::AppConfig;
use crate::gui::{self, Screen};
use crate::providers::{MediaEntry, Provider, RaiPlayProvider, StreamingCommunityProvider};
use crate::util::{DownloadEngine, DownloadProgress, DownloadRequest};
use dioxus::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;

#[component]
pub fn App() -> Element {
    let mut screen = use_signal(|| Screen::Home);
    let config = use_hook(AppConfig::load);
    let providers: Vec<Arc<dyn Provider>> = use_hook(|| {
        vec![
            Arc::new(StreamingCommunityProvider::with_config(
                config.requests.timeout,
            )) as Arc<dyn Provider>,
            Arc::new(RaiPlayProvider::with_config(config.requests.timeout)),
        ]
    });
    let mut provider_online = use_signal(|| false);
    let mut catalog_pending = use_signal(|| providers.len());
    let mut search_pending = use_signal(|| 0usize);
    let search_query = use_signal(String::new);
    let mut search_results: Signal<Vec<MediaEntry>> = use_signal(Vec::new);
    let mut is_searching = use_signal(|| false);
    let mut selected_entry: Signal<Option<MediaEntry>> = use_signal(|| None);
    let mut seasons: Signal<Vec<crate::providers::Season>> = use_signal(Vec::new);
    let mut episodes: Signal<Vec<crate::providers::Episode>> = use_signal(Vec::new);
    let mut selected_season: Signal<Option<u32>> = use_signal(|| None);
    let mut is_loading = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let mut downloads: Signal<Vec<DownloadProgress>> = use_signal(Vec::new);
    let mut playing_title = use_signal(String::new);
    let mut stream_url: Signal<Option<String>> = use_signal(|| None);
    let mut playing_season: Signal<Option<u32>> = use_signal(|| None);
    let mut playing_episode_num: Signal<Option<u32>> = use_signal(|| None);
    let mut catalog: Signal<Vec<MediaEntry>> = use_signal(Vec::new);
    let mut catalog_loading = use_signal(|| true);
    let mut history: Signal<Vec<Screen>> = use_signal(Vec::new);

    let dl_tx: mpsc::UnboundedSender<DownloadProgress> = use_hook(|| {
        let (tx, mut rx) = mpsc::unbounded_channel::<DownloadProgress>();
        spawn(async move {
            while let Some(p) = rx.recv().await {
                let mut dls = downloads.write();
                if let Some(existing) = dls.iter_mut().find(|d| d.id == p.id) {
                    *existing = p;
                }
            }
        });
        tx
    });

    let config_clone = config.clone();

    {
        let providers = providers.clone();
        use_future(move || {
            let providers = providers.clone();
            async move {
                for p in &providers {
                    p.init().await;
                }
                let per_provider = 100 / providers.len();
                for (idx, p) in providers.iter().enumerate() {
                    let p = p.clone();
                    spawn(async move {
                        if let Ok(entries) = p.get_catalog(per_provider).await {
                            provider_online.set(true);
                            let mut cat = catalog.write();
                            for mut e in entries {
                                e.provider = idx;
                                cat.push(e);
                            }
                        }
                        let prev = catalog_pending();
                        catalog_pending.set(prev.saturating_sub(1));
                        if prev <= 1 {
                            catalog_loading.set(false);
                        }
                    });
                }
            }
        });
    }

    let on_search_submit = {
        let providers = providers.clone();
        move |q: String| {
            if q.trim().is_empty() || is_searching() {
                return;
            }
            is_searching.set(true);
            search_results.set(Vec::new());
            history.write().push(screen());
            screen.set(Screen::Search);
            search_pending.set(providers.len());
            for (idx, p) in providers.iter().enumerate() {
                let p = p.clone();
                let q = q.clone();
                spawn(async move {
                    if let Ok(entries) = p.search(&q).await {
                        let mut results = search_results.write();
                        for mut e in entries {
                            e.provider = idx;
                            results.push(e);
                        }
                    }
                    let prev = search_pending();
                    search_pending.set(prev.saturating_sub(1));
                    if prev <= 1 {
                        is_searching.set(false);
                    }
                });
            }
        }
    };

    let on_select_entry = {
        let providers = providers.clone();
        move |entry: MediaEntry| {
            let is_movie = entry.is_movie();
            selected_entry.set(Some(entry.clone()));
            history.write().push(screen());
            screen.set(Screen::Details);
            seasons.set(Vec::new());
            episodes.set(Vec::new());
            selected_season.set(None);
            if !is_movie {
                is_loading.set(true);
                let p = providers[entry.provider].clone();
                spawn(async move {
                    match p.get_seasons(&entry).await {
                        Ok(s) => {
                            let first_num = s.first().map(|f| f.number);
                            seasons.set(s);
                            if let Some(n) = first_num {
                                selected_season.set(Some(n));
                                if let Some(e) = selected_entry() {
                                    is_loading.set(true);
                                    match p.get_episodes(&e, n).await {
                                        Ok(eps) => episodes.set(eps),
                                        Err(_) => episodes.set(Vec::new()),
                                    }
                                }
                            }
                        }
                        Err(_) => seasons.set(Vec::new()),
                    }
                    is_loading.set(false);
                });
            }
        }
    };

    let on_select_season = {
        let providers = providers.clone();
        move |n: u32| {
            selected_season.set(Some(n));
            episodes.set(Vec::new());
            is_loading.set(true);
            let providers = providers.clone();
            spawn(async move {
                if let Some(entry) = selected_entry() {
                    let p = providers[entry.provider].clone();
                    match p.get_episodes(&entry, n).await {
                        Ok(eps) => episodes.set(eps),
                        Err(_) => episodes.set(Vec::new()),
                    }
                }
                is_loading.set(false);
            });
        }
    };

    let on_play_movie = {
        let providers = providers.clone();
        move |_: ()| {
            if let Some(entry) = selected_entry() {
                error_msg.set(None);
                let title = entry.display_title();
                let p = providers[entry.provider].clone();
                let current = screen();
                spawn(async move {
                    match p.get_stream_url(&entry, None, None).await {
                        Ok(stream) => {
                            eprintln!("[StreamVault] Playing: {}", stream.url);
                            playing_title.set(title);
                            stream_url.set(Some(stream.url));
                            history.write().push(current);
                            screen.set(Screen::Player);
                        }
                        Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                    }
                });
            }
        }
    };

    let on_play_episode = {
        let providers = providers.clone();
        move |(s, ep_num): (u32, u32)| {
            if let Some(entry) = selected_entry() {
                error_msg.set(None);
                let episode = episodes().iter().find(|x| x.number == ep_num).cloned();
                let title = format!("{} S{s:02}E{ep_num:02}", entry.name);
                let p = providers[entry.provider].clone();
                let current = screen();
                spawn(async move {
                    match p.get_stream_url(&entry, episode.as_ref(), Some(s)).await {
                        Ok(stream) => {
                            eprintln!("[StreamVault] Playing: {}", stream.url);
                            playing_title.set(title);
                            stream_url.set(Some(stream.url));
                            playing_season.set(Some(s));
                            playing_episode_num.set(Some(ep_num));
                            history.write().push(current);
                            screen.set(Screen::Player);
                        }
                        Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                    }
                });
            }
        }
    };

    let on_dl_movie = {
        let providers = providers.clone();
        let config = config_clone.clone();
        let dl_tx = dl_tx.clone();
        move |_: ()| {
            if let Some(entry) = selected_entry() {
                let p = providers[entry.provider].clone();
                let cfg = config.clone();
                let tx = dl_tx.clone();
                let current = screen();
                spawn(async move {
                    match p.get_stream_url(&entry, None, None).await {
                        Ok(stream) => {
                            let title = entry.display_title();
                            let id = uuid::Uuid::new_v4();
                            let engine = DownloadEngine::new(cfg);
                            let req = DownloadRequest {
                                id,
                                title: title.clone(),
                                stream_url: stream.url,
                                output_dir: engine.build_output_path(&title, true),
                                filename: title.clone(),
                                headers: stream.headers,
                            };
                            downloads.write().push(DownloadProgress::new(id, title));
                            history.write().push(current);
                            screen.set(Screen::Downloads);
                            engine.download(req, tx).await;
                        }
                        Err(e) => error_msg.set(Some(format!("Download failed: {e}"))),
                    }
                });
            }
        }
    };

    let on_dl_episode = {
        let providers = providers.clone();
        let config = config_clone.clone();
        let dl_tx = dl_tx.clone();
        move |(season, ep_num): (u32, u32)| {
            if let Some(entry) = selected_entry() {
                let ep = episodes().iter().find(|e| e.number == ep_num).cloned();
                let p = providers[entry.provider].clone();
                let cfg = config.clone();
                let tx = dl_tx.clone();
                let show = entry.name.clone();
                let current = screen();
                spawn(async move {
                    match p.get_stream_url(&entry, ep.as_ref(), Some(season)).await {
                        Ok(stream) => {
                            let engine = DownloadEngine::new(cfg);
                            let fname = engine.format_episode_name(
                                &show,
                                season,
                                ep_num,
                                ep.as_ref().map(|e| e.name.as_str()).unwrap_or(""),
                            );
                            let id = uuid::Uuid::new_v4();
                            let req = DownloadRequest {
                                id,
                                title: fname.clone(),
                                stream_url: stream.url,
                                output_dir: engine.build_series_episode_path(&show, season),
                                filename: fname.clone(),
                                headers: stream.headers,
                            };
                            downloads.write().push(DownloadProgress::new(id, fname));
                            history.write().push(current);
                            screen.set(Screen::Downloads);
                            engine.download(req, tx).await;
                        }
                        Err(e) => error_msg.set(Some(format!("Download failed: {e}"))),
                    }
                });
            }
        }
    };

    let on_stop = move |_: ()| {
        stream_url.set(None);
        playing_title.set(String::new());
        playing_season.set(None);
        playing_episode_num.set(None);
        let prev = history.write().pop().unwrap_or(Screen::Home);
        screen.set(prev);
    };

    let has_next_episode = use_memo(move || {
        let Some(cur) = playing_episode_num() else {
            return false;
        };
        episodes().iter().any(|e| e.number > cur)
    });

    let on_next_episode = {
        let providers = providers.clone();
        move |_: ()| {
            let Some(cur) = playing_episode_num() else {
                return;
            };
            let Some(s) = playing_season() else { return };
            let eps = episodes();
            let next = eps
                .iter()
                .filter(|e| e.number > cur)
                .min_by_key(|e| e.number);
            if let (Some(next_ep), Some(entry)) = (next, selected_entry()) {
                error_msg.set(None);
                let ep_num = next_ep.number;
                let episode = next_ep.clone();
                let title = format!("{} S{s:02}E{ep_num:02}", entry.name);
                let p = providers[entry.provider].clone();
                spawn(async move {
                    match p.get_stream_url(&entry, Some(&episode), Some(s)).await {
                        Ok(stream) => {
                            eprintln!("[StreamVault] Playing: {}", stream.url);
                            playing_title.set(title);
                            stream_url.set(Some(stream.url));
                            playing_episode_num.set(Some(ep_num));
                        }
                        Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                    }
                });
            }
        }
    };

    let current_entry = selected_entry();

    rsx! {
        style { dangerous_inner_html: crate::style::GLOBAL_CSS }
        div { class: "app",
            gui::Navbar { screen, history, search_query, is_searching: ReadSignal::from(is_searching), on_search_submit }
            if let Some(ref err) = error_msg() {
                div { class: "error-bar",
                    span { "{err}" }
                    div { class: "fill", style: "flex:1;" }
                    button { class: "dismiss", onclick: move |_| error_msg.set(None), "✕" }
                }
            }
            div { class: "content",
                match screen() {
                    Screen::Home => rsx! {
                        gui::HomeView {
                            catalog: ReadSignal::from(catalog),
                            is_loading: ReadSignal::from(catalog_loading),
                            on_select: on_select_entry,
                        }
                    },
                    Screen::Search => rsx! {
                        gui::SearchView {
                            search_query: ReadSignal::from(search_query),
                            search_results: ReadSignal::from(search_results),
                            is_searching: ReadSignal::from(is_searching),
                            on_select: on_select_entry,
                        }
                    },
                    Screen::Details => {
                        if let Some(ref entry) = current_entry {
                            rsx! {
                                gui::DetailsView {
                                    entry: entry.clone(),
                                    seasons: ReadSignal::from(seasons),
                                    episodes: ReadSignal::from(episodes),
                                    selected_season: ReadSignal::from(selected_season),
                                    is_loading: ReadSignal::from(is_loading),
                                    on_select_season,
                                    on_play_movie,
                                    on_play_episode,
                                    on_dl_movie,
                                    on_dl_episode,
                                    on_back: move |_| {
                                        let prev = history.write().pop().unwrap_or(Screen::Home);
                                        screen.set(prev);
                                    },
                                }
                            }
                        } else {
                            rsx! {
                                gui::HomeView {
                                    catalog: ReadSignal::from(catalog),
                                    is_loading: ReadSignal::from(catalog_loading),
                                    on_select: on_select_entry,
                                }
                            }
                        }
                    },
                    Screen::Player => rsx! {
                        gui::PlayerView {
                            stream_url: ReadSignal::from(stream_url),
                            playing_title: ReadSignal::from(playing_title),
                            has_next_episode: ReadSignal::from(has_next_episode),
                            on_stop,
                            on_next_episode,
                        }
                    },
                    Screen::Downloads => rsx! {
                        gui::DownloadsView {
                            downloads: ReadSignal::from(downloads),
                            on_back: move |_| {
                                let prev = history.write().pop().unwrap_or(Screen::Home);
                                screen.set(prev);
                            },
                        }
                    },
                }
            }
        }
    }
}
