use crate::providers::MediaEntry;
use dioxus::prelude::*;

use super::helpers::PosterCard;

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
