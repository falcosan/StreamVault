use super::LOGO;
use super::poster::{PosterCard, card_background};
use crate::app::AppState;
use crate::state::WatchItem;
use dioxus::prelude::*;

#[component]
pub fn HomeView() -> Element {
    let app = use_context::<AppState>();
    let catalog = app.catalog.items.read();
    let watching = app.watch.read();

    if catalog.is_empty() && *app.catalog.loading.read() {
        return rsx! {
            div { class: "splash-screen",
                div { class: "splash-logo", dangerous_inner_html: LOGO }
            }
        };
    }
    if catalog.is_empty() && watching.is_empty() {
        return rsx! {
            div { class: "center-msg", span { class: "empty-msg", "No media available" } }
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
                p { class: "empty-msg catalog-empty", "No media available" }
            }
            div { class: "media-grid",
                for entry in catalog.iter() {
                    PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone() }
                }
            }
            if *app.catalog.loading_more.read() {
                div { class: "loading-msg", "Loading more..." }
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
    let subtitle = match (item.season, &item.episode) {
        (Some(season), Some(ep)) => Some(format!("S{season:02}E{:02} - {}", ep.number, ep.name)),
        _ => None,
    };
    let progress = format!("{:.1}%", item.progress_pct());
    let name = item.entry.name.clone();
    let key = item.entry.key();
    rsx! {
        div { class: "continue-card", style: "{background}",
            button {
                class: "continue-open",
                onclick: move |_| app.resume(item.clone()),
                div { class: "continue-overlay",
                    div { class: "continue-name", "{name}" }
                    if let Some(subtitle) = subtitle {
                        div { class: "continue-episode", "{subtitle}" }
                    }
                    div { class: "continue-progress",
                        div { class: "continue-progress-bar", style: "width: {progress};" }
                    }
                }
            }
            button {
                class: "continue-remove",
                aria_label: "Remove {name} from Continue Watching",
                onclick: move |_| app.remove_watch(key),
                span { class: "continue-remove-icon", "✕" }
            }
        }
    }
}
