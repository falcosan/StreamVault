use super::poster::{badge, format_score, poster_color};
use crate::app::AppState;
use crate::providers::{Episode, MediaEntry};
use dioxus::prelude::*;

#[component]
pub fn DetailsView(entry: MediaEntry) -> Element {
    let app = use_context::<AppState>();
    let movie = entry.is_movie();
    let (tone, label) = badge(&entry);
    let color = poster_color(&entry.name);
    let year = entry.year_display().to_owned();
    let score = entry.score.as_deref().map(format_score);
    let (play, download) = (entry.clone(), entry.clone());

    rsx! {
        div { class: "details-view",
            div { class: "toolbar",
                button { class: "btn-ghost", onclick: move |_| app.back(), "← Back" }
            }
            div { class: "details-header",
                div { class: "details-info",
                    h1 { class: "details-title", "{entry.name}" }
                    div { class: "details-meta",
                        span { class: "details-kind-badge {tone}", "{label}" }
                        span { class: "details-provider-badge", "{entry.provider_name}" }
                        span { class: "details-year", "{year}" }
                    }
                    if movie {
                        div { class: "details-actions",
                            button {
                                class: "btn-accent",
                                onclick: move |_| app.play(play.clone(), None, None, 0.0),
                                "▶  Play Now"
                            }
                            button {
                                class: "btn-ghost btn-large",
                                onclick: move |_| app.download(download.clone(), None, None),
                                "⬇  Download"
                            }
                        }
                    }
                    if let Some(description) = &entry.description {
                        p { class: "details-desc", "{description}" }
                    }
                    if let Some(score) = score {
                        div { class: "details-score", "★ {score}" }
                    }
                }
                div { class: "details-poster",
                    if let Some(url) = &entry.image_url {
                        img { src: "{url}", alt: "{entry.name}" }
                    } else {
                        div {
                            class: "details-poster-placeholder",
                            style: "background: {color};",
                            "🎬"
                        }
                    }
                }
            }
            if !movie {
                Episodes {}
            }
        }
    }
}

#[component]
fn Episodes() -> Element {
    let app = use_context::<AppState>();
    let loading = *app.details.loading.read();
    let seasons = app.details.seasons.read();
    let episodes = app.details.episodes.read();
    let selected = *app.details.season.read();
    let season = selected.unwrap_or(1);

    rsx! {
        if seasons.is_empty() && loading {
            div { class: "loading-msg", "Loading seasons..." }
        } else {
            div { class: "season-tabs",
                for &number in seasons.iter() {
                    button {
                        class: if selected == Some(number) { "season-tab active" } else { "season-tab" },
                        aria_pressed: selected == Some(number),
                        onclick: move |_| {
                            if selected != Some(number) {
                                app.select_season(number);
                            }
                        },
                        "Season {number}"
                    }
                }
            }
        }
        if episodes.is_empty() && loading {
            div { class: "loading-msg", "Loading episodes..." }
        } else if episodes.is_empty() {
            div { class: "center-msg compact",
                span { class: "hint", "Select a season above" }
            }
        } else {
            div { class: "episodes-list",
                for (index, episode) in episodes.iter().enumerate() {
                    EpisodeRow { key: "{index}", season, episode: episode.clone() }
                }
            }
        }
    }
}

#[component]
fn EpisodeRow(season: u32, episode: Episode) -> Element {
    let app = use_context::<AppState>();
    let number = episode.number;
    let (open, play, download) = (episode.clone(), episode.clone(), episode.clone());
    rsx! {
        div { class: "episode-row",
            button {
                class: "ep-info",
                onclick: move |_| app.play_episode(season, open.clone()),
                if let Some(url) = &episode.image_url {
                    img { class: "ep-thumb", src: "{url}", alt: "", loading: "lazy" }
                }
                span { class: "ep-num", "{number}" }
                span { class: "ep-name", "{episode.name}" }
            }
            button {
                class: "btn-play",
                aria_label: "Play episode {number}",
                onclick: move |_| app.play_episode(season, play.clone()),
                "▶"
            }
            button {
                class: "btn-subtle",
                aria_label: "Download episode {number}",
                onclick: move |_| app.download_episode(season, download.clone()),
                "⬇"
            }
        }
    }
}
