use crate::config::WatchItem;
use crate::providers::MediaEntry;
use crate::style::LOGO_SVG;
use dioxus::prelude::*;

use super::helpers::{poster_color, PosterCard};

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
            span {
                class: "continue-remove-icon",
                    "✕"
                }
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
