use crate::config::AppConfig;
use crate::gui::{self, Screen};
use crate::providers::{MediaEntry, Provider, StreamingCommunityProvider};
use crate::util::{DownloadEngine, DownloadProgress, DownloadRequest};
use dioxus::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;

#[component]
pub fn App() -> Element {
    let mut screen = use_signal(|| Screen::Home);
    let config = use_hook(AppConfig::load);
    let provider: Arc<dyn Provider> = use_hook(|| {
        Arc::new(StreamingCommunityProvider::with_config(
            StreamingCommunityProvider::default_base_url().to_string(),
            config.requests.timeout,
        ))
    });
    let mut provider_online = use_signal(|| false);
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
    let mut catalog: Signal<Vec<MediaEntry>> = use_signal(Vec::new);
    let mut catalog_loading = use_signal(|| true);
    let mut prev_screen = use_signal(|| Screen::Home);

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
        let provider = provider.clone();
        use_future(move || {
            let p = provider.clone();
            async move {
                match p.get_catalog().await {
                    Ok(entries) => {
                        provider_online.set(true);
                        catalog.set(entries);
                    }
                    Err(_) => provider_online.set(false),
                }
                catalog_loading.set(false);
            }
        });
    }

    let on_search_submit = {
        let provider = provider.clone();
        move |q: String| {
            if q.trim().is_empty() || is_searching() {
                return;
            }
            is_searching.set(true);
            screen.set(Screen::Search);
            let p = provider.clone();
            spawn(async move {
                match p.search(&q).await {
                    Ok(entries) => search_results.set(entries),
                    Err(_) => search_results.set(Vec::new()),
                }
                is_searching.set(false);
            });
        }
    };

    let on_select_entry = {
        let provider = provider.clone();
        move |entry: MediaEntry| {
            let is_movie = entry.is_movie();
            selected_entry.set(Some(entry.clone()));
            prev_screen.set(screen());
            screen.set(Screen::Details);
            seasons.set(Vec::new());
            episodes.set(Vec::new());
            selected_season.set(None);
            if !is_movie {
                is_loading.set(true);
                let p = provider.clone();
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
        let provider = provider.clone();
        move |n: u32| {
            selected_season.set(Some(n));
            episodes.set(Vec::new());
            is_loading.set(true);
            let p = provider.clone();
            spawn(async move {
                if let Some(entry) = selected_entry() {
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
        let provider = provider.clone();
        move |_: ()| {
            if let Some(entry) = selected_entry() {
                error_msg.set(None);
                let title = entry.display_title();
                let p = provider.clone();
                spawn(async move {
                    match p.get_stream_url(&entry, None, None).await {
                        Ok(stream) => {
                            eprintln!("[StreamVault] Playing: {}", stream.url);
                            playing_title.set(title);
                            stream_url.set(Some(stream.url));
                            screen.set(Screen::Player);
                        }
                        Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                    }
                });
            }
        }
    };

    let on_play_episode = {
        let provider = provider.clone();
        move |(s, ep_num): (u32, u32)| {
            if let Some(entry) = selected_entry() {
                error_msg.set(None);
                let episode = episodes().iter().find(|x| x.number == ep_num).cloned();
                let title = format!("{} S{s:02}E{ep_num:02}", entry.name);
                let p = provider.clone();
                spawn(async move {
                    match p.get_stream_url(&entry, episode.as_ref(), Some(s)).await {
                        Ok(stream) => {
                            eprintln!("[StreamVault] Playing: {}", stream.url);
                            playing_title.set(title);
                            stream_url.set(Some(stream.url));
                            screen.set(Screen::Player);
                        }
                        Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                    }
                });
            }
        }
    };

    let on_dl_movie = {
        let provider = provider.clone();
        let config = config_clone.clone();
        let dl_tx = dl_tx.clone();
        move |_: ()| {
            if let Some(entry) = selected_entry() {
                let p = provider.clone();
                let cfg = config.clone();
                let tx = dl_tx.clone();
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
        let provider = provider.clone();
        let config = config_clone.clone();
        let dl_tx = dl_tx.clone();
        move |(season, ep_num): (u32, u32)| {
            if let Some(entry) = selected_entry() {
                let ep = episodes().iter().find(|e| e.number == ep_num).cloned();
                let p = provider.clone();
                let cfg = config.clone();
                let tx = dl_tx.clone();
                let show = entry.name.clone();
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
                                output_dir: engine.build_output_path(&fname, false),
                                filename: fname.clone(),
                                headers: stream.headers,
                            };
                            downloads.write().push(DownloadProgress::new(id, fname));
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
        screen.set(Screen::Home);
    };

    let current_entry = selected_entry();

    rsx! {
        style { dangerous_inner_html: gui::GLOBAL_CSS }
        div { class: "app",
            gui::Navbar { screen, search_query, is_searching: ReadOnlySignal::from(is_searching), on_search_submit }
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
                            catalog: ReadOnlySignal::from(catalog),
                            is_loading: ReadOnlySignal::from(catalog_loading),
                            on_select: on_select_entry,
                        }
                    },
                    Screen::Search => rsx! {
                        gui::SearchView {
                            search_query: ReadOnlySignal::from(search_query),
                            search_results: ReadOnlySignal::from(search_results),
                            is_searching: ReadOnlySignal::from(is_searching),
                            on_select: on_select_entry,
                        }
                    },
                    Screen::Details => {
                        if let Some(ref entry) = current_entry {
                            rsx! {
                                gui::DetailsView {
                                    entry: entry.clone(),
                                    seasons: ReadOnlySignal::from(seasons),
                                    episodes: ReadOnlySignal::from(episodes),
                                    selected_season: ReadOnlySignal::from(selected_season),
                                    is_loading: ReadOnlySignal::from(is_loading),
                                    on_select_season,
                                    on_play_movie,
                                    on_play_episode,
                                    on_dl_movie,
                                    on_dl_episode,
                                    on_back: move |_| screen.set(prev_screen()),
                                }
                            }
                        } else {
                            rsx! {
                                gui::HomeView {
                                    catalog: ReadOnlySignal::from(catalog),
                                    is_loading: ReadOnlySignal::from(catalog_loading),
                                    on_select: on_select_entry,
                                }
                            }
                        }
                    },
                    Screen::Player => rsx! {
                        gui::PlayerView {
                            stream_url: ReadOnlySignal::from(stream_url),
                            playing_title: ReadOnlySignal::from(playing_title),
                            on_stop,
                        }
                    },
                    Screen::Downloads => rsx! {
                        gui::DownloadsView {
                            downloads: ReadOnlySignal::from(downloads),
                        }
                    },
                }
            }
        }
    }
}
