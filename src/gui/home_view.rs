use super::poster::{PosterCard, card_background};
use super::{CLOSE_ICON, LOGO};
use crate::app::AppState;
use crate::watch::WatchItem;
use dioxus::prelude::*;

#[component]
pub fn HomeView() -> Element {
    let app = use_context::<AppState>();
    let catalog = app.catalog.items.read();
    let watching = app.watch.read();

    if catalog.is_empty() && *app.catalog.loading.read() {
        return rsx! {
            div { class: "center-msg", role: "progressbar", aria_label: "Loading",
                div { class: "splash-logo", dangerous_inner_html: LOGO }
            }
        };
    }

    rsx! {
        div { class: "catalog-view",
            if !watching.is_empty() {
                section { class: "continue-section",
                    div { class: "section-header",
                        h2 { class: "section-title", "Continue Watching" }
                    }
                    div { class: "continue-row",
                        for item in watching.iter() {
                            ContinueCard {
                                key: "{item.entry.provider}-{item.entry.id}",
                                item: item.clone(),
                            }
                        }
                    }
                }
            }
            if catalog.is_empty() {
                div { class: "center-msg compact",
                    span { class: "empty-msg", "No media available" }
                    button {
                        class: "btn-accent",
                        "data-focus": "retry",
                        onclick: move |_| app.reload(),
                        "Retry"
                    }
                }
            }
            div { class: "media-grid",
                for entry in catalog.iter() {
                    PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone() }
                }
            }
            if *app.catalog.loading_more.read() {
                div { class: "loading-msg", role: "status", "Loading more..." }
            }
        }
    }
}

#[component]
fn ContinueCard(item: WatchItem) -> Element {
    let app = use_context::<AppState>();
    let image = item
        .episode
        .as_ref()
        .and_then(|ep| ep.image_url.as_deref())
        .or(item.entry.image_url.as_deref());
    let background = card_background(&item.entry.name, image);
    let subtitle = subtitle(&item);
    let progress = format!("{:.1}%", item.progress_pct());
    let name = item.entry.name.clone();
    let key = item.entry.key();
    rsx! {
        div { class: "continue-card", style: "{background}",
            button {
                class: "continue-open",
                "data-focus": "watch-{key.0}-{key.1}",
                onclick: move |_| app.resume(item.clone()),
                span { class: "continue-overlay",
                    span { class: "continue-name", "{name}" }
                    if let Some(subtitle) = subtitle {
                        span { class: "continue-episode", "{subtitle}" }
                    }
                    span { class: "continue-progress",
                        span { style: "width: {progress};" }
                    }
                }
            }
            button {
                class: "continue-remove",
                aria_label: "Remove {name} from Continue Watching",
                onclick: move |_| app.remove_watch(key),
                span { class: "close-icon", dangerous_inner_html: CLOSE_ICON }
            }
        }
    }
}

fn subtitle(item: &WatchItem) -> Option<String> {
    let (season, episode) = item.season.zip(item.episode.as_ref())?;
    Some(episode.label(season))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Episode, MediaEntry};
    use crate::watch::start_of;

    #[test]
    fn continue_cards_name_the_episode_without_a_dangling_separator() {
        let entry = MediaEntry::default();
        assert_eq!(
            subtitle(&start_of(entry.clone(), 1, Episode::numbered(2, "Pilot"))).as_deref(),
            Some("S01E02 - Pilot")
        );
        assert_eq!(
            subtitle(&start_of(entry.clone(), 1, Episode::numbered(2, ""))).as_deref(),
            Some("S01E02")
        );
        let movie = WatchItem {
            season: None,
            episode: None,
            ..start_of(entry, 1, Episode::numbered(2, "Pilot"))
        };
        assert_eq!(subtitle(&movie), None);
    }
}
