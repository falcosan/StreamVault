use super::poster::{badge, format_score, poster_color};
use super::{BackButton, PLAY_ICON, count};
use crate::app::AppState;
use crate::platform::EntryActions;
use crate::providers::{Episode, MediaEntry};
use crate::watch::WatchItem;
use dioxus::prelude::*;

const SKELETON_ROWS: usize = 3;

#[component]
pub fn DetailsView(entry: MediaEntry) -> Element {
    let app = use_context::<AppState>();
    let movie = entry.is_movie();
    let (tone, label) = badge(&entry);
    let score = entry.score.as_deref().map(format_score);
    let seasons = app.details.seasons.read().len();
    let resume = app
        .watch
        .read()
        .iter()
        .find(|item| item.entry.key() == entry.key())
        .cloned();
    let art = entry
        .backdrop_url
        .clone()
        .or_else(|| entry.image_url.clone());
    let poster = entry
        .image_url
        .clone()
        .filter(|_| entry.backdrop_url.is_none());
    let color = poster_color(&entry.name);

    rsx! {
        div { class: if movie { "details movie" } else { "details" },
            section {
                class: if poster.is_some() { "hero ambient" } else { "hero" },
                style: "background-color: {color};",
                if let Some(url) = art {
                    img { class: "hero-art", src: "{url}", alt: "" }
                }
                BackButton {}
                div { class: "hero-content",
                    h1 { class: "hero-title", "{entry.name}" }
                    div { class: "hero-meta",
                        span { class: "badge {tone}", "{label}" }
                        if let Some(year) = &entry.year {
                            span { class: "hero-fact", "{year}" }
                        }
                        if let Some(score) = score {
                            span { class: "hero-fact hero-score", "★ {score}" }
                        }
                        if seasons > 0 {
                            span { class: "hero-fact", "{count(seasons, \"Season\")}" }
                        }
                        span { class: "hero-fact", "{entry.provider_name}" }
                    }
                    if let Some(description) = &entry.description {
                        p { class: "hero-desc", "{description}" }
                    }
                    div { class: "hero-actions",
                        PlayButton { entry: entry.clone(), resume: resume.clone() }
                        if movie {
                            EntryActions { entry: entry.clone(), season: None, episode: None }
                        }
                    }
                    if let Some(item) = resume.as_ref().filter(|item| item.current_time > 0.0) {
                        Resume { item: item.clone() }
                    }
                }
                if let Some(url) = &poster {
                    img { class: "hero-poster", src: "{url}", alt: "{entry.name}" }
                }
            }
            if !movie {
                Episodes { entry, resume }
            }
        }
    }
}

#[component]
fn PlayButton(entry: MediaEntry, resume: Option<WatchItem>) -> Element {
    let app = use_context::<AppState>();
    let first = app
        .details
        .season
        .read()
        .zip(app.details.episodes.read().first().cloned());
    let started = resume.as_ref().is_some_and(|item| item.current_time > 0.0);
    let verb = if started { "Resume" } else { "Play" };
    let target = match &resume {
        Some(item) => item.season.zip(item.episode.as_ref().map(|e| e.number)),
        None => first
            .as_ref()
            .map(|(season, episode)| (*season, episode.number)),
    };
    let label = match target {
        Some((season, number)) => format!("{verb} S{season}:E{number}"),
        None => verb.to_owned(),
    };
    let ready = resume.is_some() || entry.is_movie() || first.is_some();
    rsx! {
        button {
            class: "btn-accent hero-play",
            "data-focus": "play",
            disabled: !ready,
            onclick: move |_| match (&resume, &first) {
                (Some(item), _) => app.resume(item.clone()),
                (None, Some((season, episode))) => app.play_episode(*season, episode.clone()),
                (None, None) => app.play(entry.clone(), None, None, 0.0),
            },
            span { class: "play-icon", dangerous_inner_html: PLAY_ICON }
            "{label}"
        }
    }
}

#[component]
fn Resume(item: WatchItem) -> Element {
    let left = (item.duration - item.current_time).max(0.0);
    let minutes = (left / 60.0).ceil() as u32;
    rsx! {
        div { class: "resume",
            div { class: "resume-bar",
                div { style: "width: {item.progress_pct():.1}%;" }
            }
            span { "{runtime(minutes)} left" }
        }
    }
}

#[component]
fn Episodes(entry: MediaEntry, resume: Option<WatchItem>) -> Element {
    let app = use_context::<AppState>();
    let loading = *app.details.loading.read();
    let seasons = app.details.seasons.read();
    let episodes = app.details.episodes.read();
    let selected = *app.details.season.read();
    let season = selected.unwrap_or(1);
    let progress = |episode: &Episode| {
        resume
            .as_ref()
            .filter(|item| item.season == Some(season) && item.current_time > 0.0)
            .filter(|item| item.episode.as_ref().map(|e| e.number) == Some(episode.number))
            .map(WatchItem::progress_pct)
    };

    rsx! {
        section { class: "episodes",
            div { class: "episodes-head",
                h2 { class: "section-title", "Episodes" }
                if seasons.len() > 1 {
                    div { class: "season-chips",
                        for &number in seasons.iter() {
                            button {
                                key: "{number}",
                                class: if selected == Some(number) { "season-chip active" } else { "season-chip" },
                                "data-focus": "season-{number}",
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
                } else if let Some(number) = selected {
                    span { class: "season-label", "Season {number}" }
                }
            }
            if episodes.is_empty() && loading {
                div { class: "episode-list", aria_busy: true,
                    for index in 0..SKELETON_ROWS {
                        div { key: "{index}", class: "episode skeleton",
                            div { class: "episode-thumb" }
                            div { class: "episode-body",
                                span { class: "skeleton-line" }
                                span { class: "skeleton-line short" }
                            }
                        }
                    }
                }
            } else if episodes.is_empty() {
                p { class: "hint", "No episodes available" }
            } else {
                div { class: "episode-list",
                    for episode in episodes.iter() {
                        EpisodeRow {
                            key: "{season}-{episode.number}",
                            entry: entry.clone(),
                            season,
                            episode: episode.clone(),
                            progress: progress(episode),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EpisodeRow(entry: MediaEntry, season: u32, episode: Episode, progress: Option<f64>) -> Element {
    let app = use_context::<AppState>();
    let number = episode.number;
    let title = if episode.name.is_empty() {
        format!("Episode {number}")
    } else {
        episode.name.clone()
    };
    let meta = match episode.duration {
        Some(minutes) => format!("E{number} · {}", runtime(minutes)),
        None => format!("E{number}"),
    };
    let play = episode.clone();
    rsx! {
        div { class: "episode-item",
            button {
                class: "episode",
                "data-focus": "episode-{season}-{number}",
                aria_label: "Play episode {number}: {title}",
                onclick: move |_| app.play_episode(season, play.clone()),
                div { class: "episode-thumb",
                    if let Some(url) = &episode.image_url {
                        img { src: "{url}", alt: "", loading: "lazy" }
                    } else {
                        span { class: "episode-fallback", "{number}" }
                    }
                    span { class: "episode-play", dangerous_inner_html: PLAY_ICON }
                    if let Some(progress) = progress {
                        div { class: "episode-progress",
                            div { style: "width: {progress:.1}%;" }
                        }
                    }
                }
                div { class: "episode-body",
                    span { class: "episode-title", "{title}" }
                    span { class: "episode-meta", "{meta}" }
                    if let Some(description) = &episode.description {
                        p { class: "episode-desc", "{description}" }
                    }
                }
            }
            EntryActions { entry, season: Some(season), episode: Some(episode.clone()) }
        }
    }
}

fn runtime(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_reads_like_a_streaming_app() {
        assert_eq!(runtime(0), "0 min");
        assert_eq!(runtime(56), "56 min");
        assert_eq!(runtime(60), "1 h");
        assert_eq!(runtime(99), "1 h 39 min");
    }
}
