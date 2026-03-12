use crate::config::WatchItem;
use crate::providers::MediaEntry;
use crate::style::{LOGO_SVG, UPDATE_SVG};
use crate::util::{DownloadProgress, DownloadStatus};
use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen {
    Home,
    Search,
    Details,
    Player,
    Downloads,
}

#[component]
pub fn Navbar(
    screen: Signal<Screen>,
    history: Signal<Vec<Screen>>,
    search_query: Signal<String>,
    has_update: ReadSignal<bool>,
    is_updating: ReadSignal<bool>,
    is_searching: ReadSignal<bool>,
    on_update: EventHandler<()>,
    on_search_submit: EventHandler<String>,
) -> Element {
    let current = screen();
    let update = has_update();
    let updating = is_updating();
    let searching = is_searching();
    rsx! {
        nav { class: "navbar",
            button { class: "logo", onclick: move |_| {
                    if screen() != Screen::Home { history.write().push(screen()); screen.set(Screen::Home); }
                },
                div { class: "logo-icon", dangerous_inner_html: LOGO_SVG }
            }
            div { class: "nav-spacer" }
            button {
                class: if current == Screen::Home { "nav-link active" } else { "nav-link" },
                onclick: move |_| {
                    if screen() != Screen::Home { history.write().push(screen()); screen.set(Screen::Home); }
                },
                "Home"
            }
            button {
                class: if current == Screen::Downloads { "nav-link active" } else { "nav-link" },
                onclick: move |_| {
                    if screen() != Screen::Downloads { history.write().push(screen()); screen.set(Screen::Downloads); }
                },
                "Downloads"
            }
            div { class: "nav-fill" }
            div { class: "search-bar",
                input {
                    class: "search-input",
                    placeholder: "Search...",
                    value: "{search_query}",
                    oninput: move |e| search_query.set(e.value()),
                    onkeypress: {
                        let q = search_query;
                        move |e: KeyboardEvent| {
                            if e.key() == Key::Enter {
                                on_search_submit.call(q());
                            }
                        }
                    },
                }
                button {
                    class: "search-go",
                    disabled: searching,
                    onclick: {
                        let q = search_query;
                        move |_| on_search_submit.call(q())
                    },
                    div { class: "search-icon", dangerous_inner_html: r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>"# }
                }
            }
            if update || updating {
                button {
                    class: if updating { "update-btn updating" } else { "update-btn" },
                    onclick: move |_| { if !updating { on_update.call(()); } },
                    div { class: "update-icon", dangerous_inner_html: UPDATE_SVG }
                    if !updating { span { class: "update-dot" } }
                }
            }
        }
    }
}

#[component]
pub fn HomeView(
    catalog: ReadSignal<Vec<MediaEntry>>,
    is_loading: ReadSignal<bool>,
    continue_watching: ReadSignal<Vec<WatchItem>>,
    on_select: EventHandler<MediaEntry>,
    on_resume: EventHandler<WatchItem>,
    on_remove_watch: EventHandler<(usize, u64)>,
) -> Element {
    let items = catalog();
    let loading = is_loading();
    let watching = continue_watching();

    if items.is_empty() && loading {
        return rsx! {
            div { class: "splash-screen",
                div { class: "splash-logo", dangerous_inner_html: LOGO_SVG }
            }
        };
    }

    if items.is_empty() {
        return rsx! {
            div { class: "center-msg", span { class: "empty-msg", "No media available" } }
        };
    }

    rsx! {
        div { class: "catalog-view",
            if !watching.is_empty() {
                div { class: "continue-section",
                    div { class: "section-header",
                        span { class: "section-title", "Continue Watching" }
                    }
                    div { class: "continue-row",
                        for item in watching.iter() {
                            ContinueCard {
                                key: "{item.entry.provider}-{item.entry.id}",
                                item: item.clone(),
                                on_resume,
                                on_remove: on_remove_watch,
                            }
                        }
                    }
                }
            }
            div { class: "media-grid",
                for entry in items.iter() {
                    PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone(), on_select }
                }
            }
        }
    }
}

#[component]
fn ContinueCard(
    item: WatchItem,
    on_resume: EventHandler<WatchItem>,
    on_remove: EventHandler<(usize, u64)>,
) -> Element {
    let bg = poster_color(&item.entry.name);
    let img = item
        .episode
        .as_ref()
        .and_then(|ep| ep.image_url.as_ref())
        .or(item.entry.image_url.as_ref());
    let style = match img {
        Some(url) => format!("background-color: {bg}; background-image: url('{url}');"),
        None => format!("background-color: {bg};"),
    };
    let pct = format!("{:.1}%", item.progress_pct());
    let name = item.entry.name.clone();
    let subtitle = match (&item.season, &item.episode) {
        (Some(s), Some(ep)) => format!("S{s:02}E{:02} - {}", ep.number, ep.name),
        _ => String::new(),
    };
    let provider = item.entry.provider;
    let id = item.entry.id;

    rsx! {
        button {
            class: "continue-card",
            style: "{style}",
            onclick: {
                let item = item.clone();
                move |_| on_resume.call(item.clone())
            },
            button {
                class: "continue-remove",
                onclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    on_remove.call((provider, id));
                },
                "✕"
            }
            div { class: "continue-overlay",
                div { class: "continue-name", "{name}" }
                if !subtitle.is_empty() {
                    div { class: "continue-episode", "{subtitle}" }
                }
                div { class: "continue-progress",
                    div { class: "continue-progress-bar", style: "width: {pct};" }
                }
            }
        }
    }
}

#[component]
pub fn SearchView(
    search_query: ReadSignal<String>,
    search_results: ReadSignal<Vec<MediaEntry>>,
    is_searching: ReadSignal<bool>,
    on_select: EventHandler<MediaEntry>,
) -> Element {
    let results = search_results();
    let loading = is_searching();
    let query = search_query();

    if loading {
        return rsx! {
            div { class: "center-msg", span { class: "searching-msg", "Searching..." } }
        };
    }

    if results.is_empty() {
        let msg = if query.is_empty() {
            "Use the search bar to find movies and series"
        } else {
            "No results found"
        };
        return rsx! {
            div { class: "center-msg", span { class: "empty-msg", "{msg}" } }
        };
    }

    let count = results.len();
    let count_str = format!("{count} results");

    rsx! {
        div { class: "catalog-view",
            div { class: "section-header",
                span { class: "section-title", "Results" }
                span { class: "section-count", "{count_str}" }
            }
            div { class: "media-grid",
                for entry in results.iter() {
                    PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone(), on_select }
                }
            }
        }
    }
}

#[component]
fn PosterCard(entry: MediaEntry, on_select: EventHandler<MediaEntry>) -> Element {
    let bg = poster_color(&entry.name);
    let style = match &entry.image_url {
        Some(url) => format!("background-color: {bg}; background-image: url('{url}');"),
        None => format!("background-color: {bg};"),
    };
    let is_movie = entry.is_movie();
    let badge_class = if is_movie {
        "badge badge-movie"
    } else {
        "badge badge-series"
    };
    let yr = entry.year_display().to_string();
    let kind_label = if is_movie { "MOVIE" } else { "SERIES" };
    let prov = provider_label(entry.provider);

    rsx! {
        button {
            class: "poster-card",
            style: "{style}",
            onclick: move |_| on_select.call(entry.clone()),
            span { class: "poster-provider", "{prov}" }
            if let Some(ref sc) = entry.score {
                { let s = format_score(sc); rsx! { span { class: "poster-score", "★ {s}" } } }
            }
            div { class: "poster-overlay",
                div { class: "poster-title", "{entry.name}" }
                div { class: "poster-meta",
                    span { class: "{badge_class}", "{kind_label}" }
                    span { class: "poster-year", "{yr}" }
                }
            }
        }
    }
}

#[component]
pub fn DetailsView(
    entry: MediaEntry,
    seasons: ReadSignal<Vec<crate::providers::Season>>,
    episodes: ReadSignal<Vec<crate::providers::Episode>>,
    selected_season: ReadSignal<Option<u32>>,
    is_loading: ReadSignal<bool>,
    on_select_season: EventHandler<u32>,
    on_play_movie: EventHandler<()>,
    on_play_episode: EventHandler<(u32, u32)>,
    on_dl_movie: EventHandler<()>,
    on_dl_episode: EventHandler<(u32, u32)>,
    on_back: EventHandler<()>,
) -> Element {
    let bg = poster_color(&entry.name);
    let is_movie = entry.is_movie();
    let kind_color = if is_movie { "var(--accent)" } else { "#0091d5" };
    let kind_text = if is_movie {
        "var(--accent-text)"
    } else {
        "white"
    };
    let kind_label = if is_movie { "MOVIE" } else { "SERIES" };
    let yr = entry.year_display().to_string();
    let prov = provider_label(entry.provider);
    let name = entry.name.clone();
    let description = entry.description.clone();
    let score = entry.score.clone();
    let image_url = entry.image_url.clone();
    let loading = is_loading();
    let sel = selected_season();
    let seasons_list = seasons();
    let episodes_list = episodes();

    rsx! {
        div { style: "overflow-y: auto; height: 100%;",
            div { class: "details-toolbar",
                button { class: "btn-ghost", onclick: move |_| on_back.call(()), "← Back" }
            }

            div { class: "details-header",
                div { class: "details-info",
                    div { class: "details-title", "{name}" }
                    div { class: "details-meta",
                        span { class: "details-kind-badge", style: "background: {kind_color}; color: {kind_text};", "{kind_label}" }
                        span { class: "details-provider-badge", "{prov}" }
                        span { class: "details-year", "{yr}" }
                    }
                    if is_movie {
                        div { class: "details-actions",
                            button { class: "btn-accent", onclick: move |_| on_play_movie.call(()), "▶  Play Now" }
                            button { class: "btn-ghost", style: "padding: 10px 20px; font-size: 14px;", onclick: move |_| on_dl_movie.call(()), "⬇  Download" }
                        }
                    }
                    if let Some(ref desc) = description {
                        p { class: "details-desc", "{desc}" }
                    }
                    if let Some(ref sc) = score {
                        { let s = format_score(sc); rsx! { div { class: "details-score", "★ {s}" } } }
                    }
                }
                div { class: "details-poster",
                    if let Some(ref url) = image_url {
                        img { src: "{url}", alt: "{name}" }
                    } else {
                        div { class: "details-poster-placeholder", style: "background: {bg};", "🎬" }
                    }
                }
            }

            if !is_movie {
                if seasons_list.is_empty() && loading {
                    div { class: "loading-msg", "Loading seasons..." }
                } else {
                    div { class: "season-tabs",
                        for s in seasons_list.iter() {
                            { let n = s.number; let is_sel = sel == Some(n); rsx! {
                                button {
                                    class: if is_sel { "season-tab active" } else { "season-tab" },
                                    onclick: move |_| { if !is_sel { on_select_season.call(n); } },
                                    "Season {n}"
                                }
                            }}
                        }
                    }
                }

                if episodes_list.is_empty() && loading {
                    div { class: "loading-msg", "Loading episodes..." }
                } else if episodes_list.is_empty() {
                    div { class: "center-msg", style: "padding: 30px 24px;",
                        span { style: "font-size: 14px; color: var(--text3);", "Select a season above" }
                    }
                } else {
                    div { class: "episodes-list",
                        for ep in episodes_list.iter() {
                            { let season = sel.unwrap_or(1); let n = ep.number; let ep_name = ep.name.clone(); let dur = ep.duration.map(|d| format!("{d} min")).unwrap_or_default(); let ep_img = ep.image_url.clone(); rsx! {
                                EpisodeRow {
                                    season,
                                    key: "{n}",
                                    number: n,
                                    name: ep_name,
                                    duration: dur,
                                    image_url: ep_img,
                                    on_dl: on_dl_episode,
                                    on_play: on_play_episode,
                                }
                            }}
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EpisodeRow(
    number: u32,
    name: String,
    duration: String,
    image_url: Option<String>,
    season: u32,
    on_play: EventHandler<(u32, u32)>,
    on_dl: EventHandler<(u32, u32)>,
) -> Element {
    rsx! {
        button {
            class: "episode-row",
            onclick: move |_| on_play.call((season, number)),
            div { class: "ep-info",
                if let Some(ref url) = image_url {
                    img { class: "ep-thumb", src: "{url}", alt: "{name}", loading: "lazy" }
                }
                span { class: "ep-num", "{number}" }
                span { class: "ep-name", "{name}" }
            }
            button { class: "ep-play", onclick: move |e| { e.stop_propagation(); on_play.call((season, number)); }, "▶" }
            button { class: "ep-dl", onclick: move |e| { e.stop_propagation(); on_dl.call((season, number)); }, "⬇" }
        }
    }
}

#[component]
pub fn PlayerView(
    stream_url: ReadSignal<Option<String>>,
    playing_title: ReadSignal<String>,
    has_next_episode: ReadSignal<bool>,
    start_time: ReadSignal<Option<f64>>,
    on_stop: EventHandler<()>,
    on_next_episode: EventHandler<()>,
    on_time_update: EventHandler<(f64, f64)>,
    on_ended: EventHandler<()>,
) -> Element {
    let title = playing_title();
    let url = stream_url();
    let show_next = has_next_episode();

    use_future(move || async move {
        let mut seeked = false;
        let mut ended_sent = false;
        loop {
            let delay = if seeked { 3 } else { 1 };
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            let mut eval = document::eval(
                r#"
                const v = document.querySelector('.player-video');
                if (v && v.readyState >= 2 && !isNaN(v.duration)) {
                    dioxus.send([v.currentTime, v.duration, v.ended]);
                } else {
                    dioxus.send(null);
                }
                "#,
            );
            let Ok(val) = eval.recv::<serde_json::Value>().await else {
                continue;
            };
            let Some(arr) = val.as_array() else {
                continue;
            };
            let t = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
            let d = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let e = arr.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
            if !seeked {
                let seek = start_time()
                    .filter(|&t| t > 0.0)
                    .map(|t| format!("v.currentTime={t};"))
                    .unwrap_or_default();
                document::eval(&format!(
                    "const v=document.querySelector('.player-video');if(v){{{seek}v.play().catch(()=>{{}});}}"
                ));
                seeked = true;
            }
            if e && !ended_sent {
                ended_sent = true;
                on_ended.call(());
            } else if !e {
                ended_sent = false;
                if t > 10.0 {
                    on_time_update.call((t, d));
                }
            }
        }
    });

    rsx! {
        div {
            tabindex: "0",
            autofocus: true,
            class: "player-screen",
            onkeydown: move |e: KeyboardEvent| {
                let js: Option<&str> = match e.key() {
                    Key::ArrowLeft => Some("document.querySelector('.player-video').currentTime -= 15;"),
                    Key::ArrowRight => Some("document.querySelector('.player-video').currentTime += 15;"),
                    Key::Character(c) if c == " " => Some("const v=document.querySelector('.player-video');v.paused?v.play():v.pause();"),
                    _ => None,
                };
                if let Some(js) = js {
                    e.prevent_default();
                    document::eval(js);
                }
            },
            div { class: "player-top-bar",
                button { class: "btn-ghost", onclick: move |_| on_stop.call(()), "← Stop" }
                span { class: "player-title-text", "{title}" }
                if show_next {
                    button { class: "btn-next-episode", onclick: move |_| on_next_episode.call(()), "Next →" }
                }
            }
            div { class: "player-video-container",
                if let Some(ref src) = url {
                    video {
                        src: "{src}",
                        controls: true,
                        autoplay: true,
                        class: "player-video",
                    }
                }
            }
        }
    }
}

#[component]
pub fn DownloadsView(
    downloads: ReadSignal<Vec<DownloadProgress>>,
    on_back: EventHandler<()>,
) -> Element {
    let dls = downloads();
    let count_str = format!(
        "{} item{}",
        dls.len(),
        if dls.len() == 1 { "" } else { "s" }
    );
    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            div { class: "details-toolbar",
                button { class: "btn-ghost", onclick: move |_| on_back.call(()), "← Back" }
            }
            div { class: "dl-header",
                span { class: "dl-title", "Downloads" }
                span { class: "dl-count", "{count_str}" }
            }
            if dls.is_empty() {
                div { class: "dl-empty",
                    span { class: "dl-empty-title", "No downloads yet" }
                    span { class: "dl-empty-sub", "Start downloading to see progress here" }
                }
            } else {
                div { class: "dl-list",
                    for dl in dls.iter() {
                        DlCard { key: "{dl.id}", progress: dl.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn DlCard(progress: DownloadProgress) -> Element {
    let (status_text, status_color): (&str, &str) = match &progress.status {
        DownloadStatus::Queued => ("Queued", "var(--text3)"),
        DownloadStatus::Downloading => ("Downloading...", "var(--accent)"),
        DownloadStatus::Muxing => ("Muxing...", "var(--warn)"),
        DownloadStatus::Completed => ("Completed", "var(--success)"),
        DownloadStatus::Failed(ref e) => (e.as_str(), "var(--danger)"),
    };

    rsx! {
        div { class: "dl-card",
            div { class: "dl-card-top",
                span { class: "dl-card-title", "{progress.title}" }
                span { class: "dl-card-status", color: "{status_color}", "{status_text}" }
            }
        }
    }
}

const POSTER_COLORS: &[(u8, u8, u8)] = &[
    (0x8B, 0x1A, 0x1A),
    (0x0D, 0x3B, 0x66),
    (0x3B, 0x0A, 0x5C),
    (0x14, 0x40, 0x14),
    (0x6B, 0x3A, 0x00),
    (0x5C, 0x0E, 0x0E),
    (0x0A, 0x2A, 0x4A),
    (0x40, 0x0A, 0x50),
    (0x4A, 0x0E, 0x0E),
    (0x00, 0x3A, 0x3A),
    (0x3E, 0x21, 0x23),
    (0x1B, 0x2A, 0x41),
    (0x2D, 0x1B, 0x00),
    (0x1A, 0x0A, 0x2E),
    (0x0E, 0x33, 0x1A),
    (0x33, 0x1A, 0x0E),
];

fn name_hash(name: &str) -> usize {
    name.bytes()
        .fold(0u32, |a, b| a.wrapping_mul(37).wrapping_add(b as u32)) as usize
}

fn poster_color(name: &str) -> String {
    let (r, g, b) = POSTER_COLORS[name_hash(name) % POSTER_COLORS.len()];
    format!("rgb({r},{g},{b})")
}

fn format_score(s: &str) -> String {
    match s.parse::<f64>() {
        Ok(v) => format!("{v:.1}"),
        Err(_) => s.to_string(),
    }
}

fn provider_label(idx: usize) -> &'static str {
    match idx {
        0 => "StreamingCommunity",
        1 => "RaiPlay",
        2 => "Nove",
        3 => "AnimeUnity",
        _ => "Unknown",
    }
}
