use dioxus::prelude::*;

use super::helpers::{format_score, poster_color};

#[component]
pub fn DetailsView(
    entry: crate::providers::MediaEntry,
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
    let prov = entry.provider_name.clone();
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
