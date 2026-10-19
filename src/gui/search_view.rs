use super::poster::PosterCard;
use crate::app::AppState;
use dioxus::prelude::*;

#[component]
pub fn SearchView() -> Element {
    let app = use_context::<AppState>();
    let results = app.search.results.read();

    if *app.search.busy.read() {
        return rsx! {
            div { class: "center-msg", span { class: "empty-msg", "Searching..." } }
        };
    }
    if results.is_empty() {
        let message = if app.search.query.read().trim().is_empty() {
            "Use the search bar to find movies and series"
        } else {
            "No results found"
        };
        return rsx! {
            div { class: "center-msg", span { class: "empty-msg", "{message}" } }
        };
    }

    rsx! {
        div { class: "catalog-view",
            div { class: "section-header",
                h2 { class: "section-title", "Results" }
                span { class: "section-count", "{results.len()} results" }
            }
            div { class: "media-grid",
                for entry in results.iter() {
                    PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone() }
                }
            }
        }
    }
}
