use crate::config::{
    advance_watch_item, load_watch_items, remove_watch_item, save_watch_items, upsert_watch_item,
    AppConfig, WatchItem,
};
use crate::gui::{self, Screen};
use crate::providers::{
    AnimeUnityProvider, MediaEntry, MediaType, NoveProvider, Provider, RaiPlayProvider,
    StreamingCommunityProvider,
};
use crate::search;
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
            Arc::new(NoveProvider::with_config(config.requests.timeout)),
            Arc::new(AnimeUnityProvider::with_config(config.requests.timeout)),
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
    let mut has_update = use_signal(|| false);
    let mut is_updating = use_signal(|| false);
    let mut continue_watching: Signal<Vec<WatchItem>> = use_signal(load_watch_items);
    let mut resume_time: Signal<Option<f64>> = use_signal(|| None);
    let mut playing_episode: Signal<Option<crate::providers::Episode>> = use_signal(|| None);

    use_future(move || async move {
        let Ok(resp) = reqwest::get(
            "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main/Cargo.toml",
        )
        .await
        else {
            return;
        };
        let text = resp.text().await.unwrap_or_default();
        let remote = text
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("version")
                    .filter(|r| r.starts_with(|c: char| [' ', '='].contains(&c)))
            })
            .map(|v| v.trim_start_matches([' ', '=']).trim().trim_matches('"'));
        if remote.is_some_and(|v| v != env!("CARGO_PKG_VERSION")) {
            has_update.set(true);
        }
    });

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
                let per_provider = 100 / providers.len();
                for (idx, p) in providers.iter().enumerate() {
                    let p = p.clone();
                    spawn(async move {
                        p.init().await;
                        if let Ok(entries) = p.get_catalog(per_provider).await {
                            provider_online.set(true);
                            let mut cat = catalog.write();
                            for mut e in entries {
                                e.provider = idx;
                                e.provider_name = p.name().to_string();
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

    let on_update = move |_: ()| {
        is_updating.set(true);
        spawn(async move {
            let dir = std::env::current_exe()
                .ok()
                .and_then(|p| {
                    p.ancestors()
                        .find(|a| a.extension().is_some_and(|e| e == "app"))
                        .map(|a| a.parent().unwrap_or(a).to_path_buf())
                })
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let script = format!(
                "cd '{}' && curl -fsSL 'https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main/scripts/package.sh' | bash",
                dir.display()
            );
            let ok = tokio::process::Command::new("bash")
                .arg("-c")
                .arg(&script)
                .status()
                .await
                .is_ok_and(|s| s.success());
            if ok {
                if let Ok(exe) = std::env::current_exe() {
                    std::process::Command::new(exe).spawn().ok();
                }
                std::process::exit(0);
            }
            is_updating.set(false);
        });
    };

    let on_search_submit = {
        let providers = providers.clone();
        move |q: String| {
            let q = q.trim().to_string();
            if q.is_empty() {
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
                        search_results
                            .write()
                            .extend(entries.into_iter().map(|mut e| {
                                e.provider = idx;
                                e.provider_name = p.name().to_string();
                                e
                            }));
                    }

                    let new_pending = search_pending().saturating_sub(1);
                    search_pending.set(new_pending);

                    if new_pending == 0 {
                        let unsorted = std::mem::take(&mut *search_results.write());
                        search_results.set(search::rank_results(unsorted, &q));
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
                playing_episode.set(None);
                resume_time.set(None);
                playing_title.set(entry.display_title());
                stream_url.set(None);
                history.write().push(screen());
                screen.set(Screen::Player);
                let p = providers[entry.provider].clone();
                spawn(async move {
                    match p.get_stream_url(&entry, None, None).await {
                        Ok(stream) => {
                            eprintln!("[StreamVault] Playing: {}", stream.url);
                            stream_url.set(Some(stream.url));
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
                playing_episode.set(episode.clone());
                resume_time.set(None);
                playing_title.set(match &episode {
                    Some(ep) => entry.episode_title(s, ep),
                    None => format!("{} S{s:02}E{ep_num:02}", entry.name),
                });
                playing_season.set(Some(s));
                playing_episode_num.set(Some(ep_num));
                stream_url.set(None);
                history.write().push(screen());
                screen.set(Screen::Player);
                let p = providers[entry.provider].clone();
                spawn(async move {
                    match p.get_stream_url(&entry, episode.as_ref(), Some(s)).await {
                        Ok(stream) => {
                            eprintln!("[StreamVault] Playing: {}", stream.url);
                            stream_url.set(Some(stream.url));
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
        playing_episode.set(None);
        resume_time.set(None);
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
                playing_episode.set(Some(episode.clone()));
                resume_time.set(None);
                playing_title.set(entry.episode_title(s, &episode));
                playing_episode_num.set(Some(ep_num));
                stream_url.set(None);
                let p = providers[entry.provider].clone();
                spawn(async move {
                    match p.get_stream_url(&entry, Some(&episode), Some(s)).await {
                        Ok(stream) => {
                            eprintln!("[StreamVault] Playing: {}", stream.url);
                            stream_url.set(Some(stream.url));
                        }
                        Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                    }
                });
            }
        }
    };

    let on_back = move |_| {
        let prev = history.write().pop().unwrap_or(Screen::Home);
        screen.set(prev);
    };

    let on_time_update = move |(current, dur): (f64, f64)| {
        if current < 10.0 || dur <= 0.0 {
            return;
        }
        let Some(entry) = selected_entry() else {
            return;
        };
        if dur - current > 20.0 {
            let item = WatchItem {
                entry,
                current_time: current,
                duration: dur,
                season: playing_season(),
                episode: playing_episode(),
            };
            let mut items = continue_watching.write();
            upsert_watch_item(&mut items, item);
            save_watch_items(&items);
            return;
        }
        let mut items = continue_watching.write();
        let stored = items
            .iter()
            .find(|i| i.entry.provider == entry.provider && i.entry.id == entry.id);
        match stored {
            None => return,
            Some(i) if i.episode.as_ref().map(|e| e.number) != playing_episode_num() => return,
            _ => {}
        }
        let cur_ep = match (playing_episode_num(), playing_season()) {
            (Some(ep), Some(s)) if entry.media_type == MediaType::Series => (ep, s),
            _ => {
                remove_watch_item(&mut items, entry.provider, entry.id);
                save_watch_items(&items);
                return;
            }
        };
        let eps = episodes();
        if advance_watch_item(&mut items, entry, &eps, cur_ep.0, cur_ep.1) {
            save_watch_items(&items);
        }
    };

    let on_ended = {
        let providers = providers.clone();
        move |_: ()| {
            let Some(entry) = selected_entry() else {
                return;
            };
            let cur_ep = match (playing_episode_num(), playing_season()) {
                (Some(ep), Some(s)) if entry.media_type == MediaType::Series => (ep, s),
                _ => {
                    let mut items = continue_watching.write();
                    remove_watch_item(&mut items, entry.provider, entry.id);
                    save_watch_items(&items);
                    return;
                }
            };
            let eps = episodes();
            {
                let mut items = continue_watching.write();
                if advance_watch_item(&mut items, entry.clone(), &eps, cur_ep.0, cur_ep.1) {
                    save_watch_items(&items);
                    return;
                }
            }

            let p = providers[entry.provider].clone();
            spawn(async move {
                let Ok(all_seasons) = p.get_seasons(&entry).await else {
                    return;
                };
                let next_season = all_seasons
                    .iter()
                    .filter(|sn| sn.number > cur_ep.1)
                    .min_by_key(|sn| sn.number);
                let Some(ns) = next_season else {
                    let mut items = continue_watching.write();
                    remove_watch_item(&mut items, entry.provider, entry.id);
                    save_watch_items(&items);
                    return;
                };
                let Ok(new_eps) = p.get_episodes(&entry, ns.number).await else {
                    return;
                };
                let Some(first_ep) = new_eps.into_iter().min_by_key(|e| e.number) else {
                    return;
                };
                let item = WatchItem {
                    entry,
                    current_time: 0.0,
                    duration: first_ep.duration.map(|d| d as f64).unwrap_or(0.0),
                    season: Some(ns.number),
                    episode: Some(first_ep),
                };
                let mut items = continue_watching.write();
                upsert_watch_item(&mut items, item);
                save_watch_items(&items);
            });
        }
    };

    let on_remove_watch = move |(provider, id): (usize, u64)| {
        let mut items = continue_watching.write();
        remove_watch_item(&mut items, provider, id);
        save_watch_items(&items);
    };

    let on_resume = {
        let providers = providers.clone();
        move |item: WatchItem| {
            {
                let mut items = continue_watching.write();
                upsert_watch_item(&mut items, item.clone());
                save_watch_items(&items);
            }
            selected_entry.set(Some(item.entry.clone()));
            playing_episode.set(item.episode.clone());
            error_msg.set(None);
            playing_title.set(match (&item.season, &item.episode) {
                (Some(s), Some(ep)) => item.entry.episode_title(*s, ep),
                _ => item.entry.display_title(),
            });
            playing_season.set(item.season);
            playing_episode_num.set(item.episode.as_ref().map(|e| e.number));
            resume_time.set(Some(item.current_time));
            stream_url.set(None);
            history.write().push(screen());
            screen.set(Screen::Player);
            let p = providers[item.entry.provider].clone();
            let WatchItem {
                entry,
                episode,
                season,
                ..
            } = item;
            spawn(async move {
                match p.get_stream_url(&entry, episode.as_ref(), season).await {
                    Ok(stream) => {
                        eprintln!("[StreamVault] Playing: {}", stream.url);
                        stream_url.set(Some(stream.url));
                        if let Some(s) = season {
                            if let Ok(eps) = p.get_episodes(&entry, s).await {
                                episodes.set(eps);
                            }
                        }
                    }
                    Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                }
            });
        }
    };

    let current_entry = selected_entry();

    rsx! {
        style { dangerous_inner_html: crate::style::GLOBAL_CSS }
        div { class: "app",
            gui::Navbar {
                screen,
                history,
                search_query,
                has_update: ReadSignal::from(has_update),
                is_updating: ReadSignal::from(is_updating),
                is_searching: ReadSignal::from(is_searching),
                on_update,
                on_search_submit,
            }
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
                            continue_watching: ReadSignal::from(continue_watching),
                            on_select: on_select_entry,
                            on_resume,
                            on_remove_watch,
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
                                    on_back,
                                }
                            }
                        } else {
                            rsx! {
                                gui::HomeView {
                                    catalog: ReadSignal::from(catalog),
                                    is_loading: ReadSignal::from(catalog_loading),
                                    continue_watching: ReadSignal::from(continue_watching),
                                    on_select: on_select_entry,
                                    on_resume,
                                    on_remove_watch,
                                }
                            }
                        }
                    },
                    Screen::Player => rsx! {
                        gui::PlayerView {
                            stream_url: ReadSignal::from(stream_url),
                            playing_title: ReadSignal::from(playing_title),
                            has_next_episode: ReadSignal::from(has_next_episode),
                            start_time: ReadSignal::from(resume_time),
                            on_stop,
                            on_next_episode,
                            on_time_update,
                            on_ended,
                        }
                    },
                    Screen::Downloads => rsx! {
                        gui::DownloadsView {
                            on_back,
                            downloads: ReadSignal::from(downloads),
                        }
                    },
                }
            }
        }
    }
}
