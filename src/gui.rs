use crate::providers::MediaEntry;
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

pub const LOGO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 1200"><path fill="#f4fd37" d="M458.8 436.7C386 512 320.6 579.5 313.6 586.7L300.8 600l144.8-.1h144.9l137.2-134.2c75.4-73.8 144.4-141.3 153.2-150L897 300H591.4zm0 300C386 812 320.6 879.5 313.6 886.7L300.8 900l144.8-.1h144.9l137.2-134.2c75.4-73.8 144.4-141.3 153.2-150L897 600H591.4z"/></svg>"##;

pub const GLOBAL_CSS: &str = r#"
:root {
    --bg: #151515; --surface: #1c1c1c; --surface2: #272727;
    --border: #333333; --accent: #f4fd37; --accent-hover: #d4dd17;
    --accent-text: #151515;
    --warn: #f5b014; --danger: #e53935; --success: #46d369;
    --text: #e5e5e5; --text2: #a0a0a0; --text3: #686868;
    --navbar: #0d0d0d;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: var(--bg); color: var(--text); font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }
::-webkit-scrollbar { display: none; }
body { scrollbar-width: none; -ms-overflow-style: none; }
.app { display: flex; flex-direction: column; height: 100vh; }

.navbar {
    display: flex; align-items: center; gap: 2px; padding: 0 20px;
    height: 48px; min-height: 48px; background: var(--navbar);
}
.logo { background: none; border: none; cursor: pointer; padding: 0; display: flex; align-items: center; }
.logo:hover { opacity: 0.8; }
.logo-icon { height: 30px; width: 30px; }
.logo-icon svg { width: 100%; height: 100%; display: block; }
.nav-spacer { width: 24px; }
.nav-link {
    background: none; border: none; color: #808080; font-size: 13px;
    padding: 4px 10px; cursor: pointer; border-radius: 3px;
}
.nav-link:hover { background: #303030; color: #b0b0b0; }
.nav-link.active { color: var(--accent); }
.nav-fill { flex: 1; }
.search-input {
    background: var(--surface2); border: 1px solid var(--border); color: var(--text);
    padding: 5px 10px; font-size: 13px; width: 200px; border-radius: 3px; outline: none;
}
.search-input:focus { border-color: var(--accent); }
.search-go {
    background: var(--accent); border: none; color: var(--accent-text); font-size: 12px;
    padding: 5px 8px; min-width: 32px; cursor: pointer; border-radius: 3px; font-weight: 600;
}
.search-go:hover { background: var(--accent-hover); }
.search-go:disabled { opacity: 0.5; cursor: default; }

.content { flex: 1; overflow-y: auto; }

.error-bar {
    display: flex; align-items: center; gap: 8px; padding: 8px 20px;
    background: var(--danger); color: white; font-size: 13px;
}
.error-bar .dismiss { background: none; border: none; color: white; cursor: pointer; font-size: 12px; padding: 4px 8px; }
.error-bar .fill { flex: 1; }

.center-msg {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    height: 100%; text-align: center; padding: 20px;
}

.splash-screen {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    height: 100%; gap: 16px;
}
.splash-logo { width: 100px; height: 100px; }
.splash-logo svg { width: 100%; height: 100%; display: block; }
.splash-text { font-size: 22px; font-weight: bold; color: var(--accent); letter-spacing: 2px; }

.catalog-view { padding: 16px 0; }
.section-header { display: flex; align-items: center; gap: 10px; padding: 0 20px 12px; }
.section-title { font-size: 18px; color: var(--text); }
.section-count { font-size: 12px; color: var(--text3); }

.media-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 16px;
    padding: 0 20px 20px;
}

.poster-card {
    width: 100%; aspect-ratio: 2/3; border-radius: 6px;
    cursor: pointer; position: relative; overflow: hidden;
    border: none; padding: 0; text-align: left;
    background-size: cover; background-position: center;
    transition: transform 0.15s;
}
.poster-card:hover { transform: scale(1.03); }
.poster-overlay {
    position: absolute; bottom: 0; left: 0; right: 0;
    background: linear-gradient(transparent, rgba(0,0,0,0.85)); padding: 8px 10px;
}
.poster-title { font-size: 12px; color: white; margin-bottom: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.poster-meta { display: flex; align-items: center; gap: 6px; }
.badge {
    font-size: 8px; padding: 1px 6px; border-radius: 2px;
    text-transform: uppercase; font-weight: bold;
}
.badge-movie { background: var(--accent); color: var(--accent-text); }
.badge-series { background: #0091d5; color: white; }
.poster-year { font-size: 10px; color: #b0b0b0; }

.empty-msg { font-size: 16px; color: var(--text3); }
.searching-msg { font-size: 16px; color: var(--text3); }

.details-toolbar { display: flex; align-items: center; gap: 8px; padding: 10px 24px; }
.btn-ghost {
    background: transparent; border: 1px solid var(--border); color: var(--text);
    padding: 6px 14px; font-size: 13px; cursor: pointer; border-radius: 3px;
}
.btn-ghost:hover { background: var(--surface2); }
.btn-accent {
    background: var(--accent); border: none; color: var(--accent-text);
    padding: 10px 20px; font-size: 14px; font-weight: 600; cursor: pointer; border-radius: 3px; min-width: 140px; text-align: center;
}
.btn-accent:hover { background: var(--accent-hover); }

.details-header {
    display: flex; gap: 32px; padding: 8px 32px 24px;
}
.details-info {
    flex: 1; display: flex; flex-direction: column; gap: 8px; min-width: 0;
}
.details-title {
    font-size: 34px; font-weight: bold; color: white; line-height: 1.15;
}
.details-meta {
    display: flex; align-items: center; gap: 10px;
}
.details-kind-badge { font-size: 10px; padding: 2px 10px; border-radius: 3px; }
.details-year { font-size: 13px; color: #bbbbbb; }
.details-actions { display: flex; align-items: center; gap: 10px; margin-top: 6px; }
.details-desc {
    font-size: 14px; color: var(--text2); line-height: 1.6; margin-top: 4px; max-width: 600px;
}
.details-poster {
    width: 200px; flex-shrink: 0;
}
.details-poster img {
    width: 100%; border-radius: 8px; display: block;
}
.details-poster-placeholder {
    width: 100%; aspect-ratio: 2/3; border-radius: 8px;
    display: flex; align-items: center; justify-content: center;
    font-size: 48px; color: rgba(255,255,255,0.3);
}

.season-tabs { display: flex; gap: 6px; padding: 6px 24px; flex-wrap: wrap; }
.season-tab {
    font-size: 12px; padding: 7px 14px; cursor: pointer; border-radius: 3px;
    border: 1px solid var(--border); background: transparent; color: var(--text2);
}
.season-tab:hover { background: var(--surface2); }
.season-tab.active { background: var(--accent); border-color: var(--accent); color: var(--accent-text); font-weight: 600; cursor: default; }

.episodes-list { padding: 4px 24px; display: flex; flex-direction: column; gap: 3px; }
.episode-row {
    display: flex; align-items: center; gap: 12px; padding: 8px 12px;
    background: var(--surface); border-radius: 4px; cursor: pointer; border: none;
    width: 100%; text-align: left; color: var(--text);
}
.episode-row:hover { background: var(--surface2); }
.ep-num { font-size: 18px; color: var(--text3); width: 30px; text-align: center; flex-shrink: 0; }
.ep-info { flex: 1; }
.ep-name { font-size: 13px; color: var(--text); }
.ep-dur { font-size: 11px; color: var(--text3); margin-top: 2px; }
.ep-play {
    background: var(--accent); border: none; color: var(--accent-text);
    padding: 7px 14px; font-size: 12px; font-weight: 600; cursor: pointer; border-radius: 3px;
}
.ep-play:hover { background: var(--accent-hover); }
.ep-dl {
    background: transparent; border: 1px solid var(--border); color: var(--text2);
    padding: 7px 10px; font-size: 12px; cursor: pointer; border-radius: 3px;
}
.ep-dl:hover { background: var(--surface2); }

.player-screen { display: flex; flex-direction: column; height: 100%; background: #000; }
.player-top-bar {
    display: flex; align-items: center; gap: 12px; padding: 8px 16px;
    background: rgba(20,20,20,0.95); z-index: 1;
}
.player-title-text { font-size: 14px; color: var(--text); flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.player-video-container { flex: 1; display: flex; align-items: center; justify-content: center; background: #000; min-height: 0; }
.player-video { width: 100%; height: 100%; object-fit: contain; outline: none; }

.dl-header { display: flex; align-items: center; padding: 14px 20px; }
.dl-title { font-size: 18px; color: var(--text); flex: 1; }
.dl-count { font-size: 12px; color: var(--text3); }
.dl-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; }
.dl-empty-title { font-size: 16px; color: var(--text2); }
.dl-empty-sub { font-size: 13px; color: var(--text3); margin-top: 6px; }
.dl-list { padding: 4px 20px; display: flex; flex-direction: column; gap: 6px; }
.dl-card { background: var(--surface); border-radius: 4px; padding: 14px; }
.dl-card-top { display: flex; align-items: center; }
.dl-card-title { font-size: 13px; color: var(--text); flex: 1; }
.dl-card-status { font-size: 11px; }
.dl-progress { width: 100%; height: 3px; background: var(--surface2); border-radius: 2px; margin-top: 8px; overflow: hidden; }
.dl-progress-bar { height: 100%; background: var(--accent); border-radius: 2px; transition: width 0.3s; }
.dl-card-bottom { display: flex; align-items: center; margin-top: 5px; }
.dl-pct { font-size: 11px; color: var(--text2); }

.loading-msg { font-size: 13px; color: var(--text3); padding: 10px 24px; }
"#;

#[component]
pub fn Navbar(
    screen: Signal<Screen>,
    history: Signal<Vec<Screen>>,
    search_query: Signal<String>,
    is_searching: ReadOnlySignal<bool>,
    on_search_submit: EventHandler<String>,
) -> Element {
    let current = screen();
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
                if searching { "..." } else { "Go" }
            }
        }
    }
}

#[component]
pub fn HomeView(
    catalog: ReadOnlySignal<Vec<MediaEntry>>,
    is_loading: ReadOnlySignal<bool>,
    on_select: EventHandler<MediaEntry>,
) -> Element {
    let items = catalog();
    let loading = is_loading();

    if items.is_empty() && loading {
        return rsx! {
            div { class: "splash-screen",
                div { class: "splash-logo", dangerous_inner_html: LOGO_SVG }
                span { class: "splash-text", "StreamVault" }
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
            div { class: "media-grid",
                for entry in items.iter() {
                    PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone(), on_select }
                }
            }
        }
    }
}

#[component]
pub fn SearchView(
    search_query: ReadOnlySignal<String>,
    search_results: ReadOnlySignal<Vec<MediaEntry>>,
    is_searching: ReadOnlySignal<bool>,
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
    let kind_label = if is_movie { "MOVIE" } else { "SERIES" };
    let yr = entry.year_display().to_string();
    let name = entry.name.clone();
    let e = entry.clone();

    rsx! {
        button {
            class: "poster-card",
            style: "{style}",
            onclick: move |_| on_select.call(e.clone()),
            div { class: "poster-overlay",
                div { class: "poster-title", "{name}" }
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
    seasons: ReadOnlySignal<Vec<crate::providers::Season>>,
    episodes: ReadOnlySignal<Vec<crate::providers::Episode>>,
    selected_season: ReadOnlySignal<Option<u32>>,
    is_loading: ReadOnlySignal<bool>,
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
    let name = entry.name.clone();
    let description = entry.description.clone();
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
                            { let season = sel.unwrap_or(1); let n = ep.number; let ep_name = ep.name.clone(); let dur = ep.duration.map(|d| format!("{d} min")).unwrap_or_default(); rsx! {
                                EpisodeRow {
                                    key: "{n}",
                                    number: n,
                                    name: ep_name,
                                    duration: dur,
                                    season,
                                    on_play: on_play_episode,
                                    on_dl: on_dl_episode,
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
    season: u32,
    on_play: EventHandler<(u32, u32)>,
    on_dl: EventHandler<(u32, u32)>,
) -> Element {
    rsx! {
        button {
            class: "episode-row",
            onclick: move |_| on_play.call((season, number)),
            span { class: "ep-num", "{number}" }
            div { class: "ep-info",
                div { class: "ep-name", "{name}" }
                div { class: "ep-dur", "{duration}" }
            }
            button { class: "ep-play", onclick: move |e| { e.stop_propagation(); on_play.call((season, number)); }, "▶" }
            button { class: "ep-dl", onclick: move |e| { e.stop_propagation(); on_dl.call((season, number)); }, "⬇" }
        }
    }
}

#[component]
pub fn PlayerView(
    stream_url: ReadOnlySignal<Option<String>>,
    playing_title: ReadOnlySignal<String>,
    on_stop: EventHandler<()>,
) -> Element {
    let title = playing_title();
    let url = stream_url();

    rsx! {
        div { class: "player-screen",
            div { class: "player-top-bar",
                button { class: "btn-ghost", onclick: move |_| on_stop.call(()), "← Stop" }
                span { class: "player-title-text", "{title}" }
            }
            div { class: "player-video-container",
                if let Some(ref src) = url {
                    video {
                        class: "player-video",
                        src: "{src}",
                        controls: true,
                        autoplay: true,
                    }
                }
            }
        }
    }
}

#[component]
pub fn DownloadsView(downloads: ReadOnlySignal<Vec<DownloadProgress>>) -> Element {
    let dls = downloads();
    let count_str = format!(
        "{} item{}",
        dls.len(),
        if dls.len() == 1 { "" } else { "s" }
    );
    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
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
    let (status_text, status_color) = match &progress.status {
        DownloadStatus::Queued => ("Queued".to_string(), "var(--text3)"),
        DownloadStatus::Downloading => ("Downloading".to_string(), "var(--accent)"),
        DownloadStatus::Muxing => ("Muxing".to_string(), "var(--warn)"),
        DownloadStatus::Completed => ("Completed".to_string(), "var(--success)"),
        DownloadStatus::Failed(ref e) => (e.clone(), "var(--danger)"),
    };
    let pct = progress.percent;
    let pct_str = format!("{pct:.1}%");
    let width_str = format!("{pct}%");

    rsx! {
        div { class: "dl-card",
            div { class: "dl-card-top",
                span { class: "dl-card-title", "{progress.title}" }
                span { class: "dl-card-status", color: "{status_color}", "{status_text}" }
            }
            div { class: "dl-progress",
                div { class: "dl-progress-bar", style: "width: {width_str};" }
            }
            div { class: "dl-card-bottom",
                span { class: "dl-pct", "{pct_str}" }
            }
        }
    }
}
