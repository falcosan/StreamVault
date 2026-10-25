use super::count;
use super::poster::PosterCard;
use crate::app::AppState;
use dioxus::prelude::*;

#[component]
pub fn SearchView() -> Element {
    let app = use_context::<AppState>();
    let results = app.search.results.read();
    let busy = *app.search.busy.read();

    if busy || results.is_empty() {
        let message = if busy {
            "Searching..."
        } else {
            "No results found"
        };
        return rsx! {
            div { class: "center-msg", role: "status",
                span { class: "empty-msg", "{message}" }
            }
        };
    }

    rsx! {
        div { class: "catalog-view",
            div { class: "section-header",
                h2 { class: "section-title", "Results" }
                span { class: "section-count", "{count(results.len(), \"result\")}" }
            }
            div { class: "media-grid",
                for entry in results.iter() {
                    PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone() }
                }
            }
        }
    }
}
