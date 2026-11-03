use super::poster::{GHOST_CARDS, GhostCards, PosterCard};
use crate::app::AppState;
use crate::i18n::text;
use dioxus::prelude::*;

#[component]
pub fn SearchView() -> Element {
    let app = use_context::<AppState>();
    let results = app.search.results.read();
    let busy = *app.search.busy.read();
    let t = text();

    if !busy && results.is_empty() {
        return rsx! {
            div { class: "center-msg", role: "status",
                span { class: "empty-msg", "{t.no_results}" }
            }
        };
    }

    rsx! {
        div { class: "catalog-view",
            div { class: "section-header",
                h2 { class: "section-title", "{t.results}" }
                span { class: "section-count",
                    if busy {
                        span { class: "skeleton-line inline skeleton" }
                    } else {
                        "{(t.result_count)(results.len())}"
                    }
                }
            }
            if busy {
                GhostCards { count: GHOST_CARDS, label: t.searching }
            } else {
                div { class: "media-grid",
                    for entry in results.iter() {
                        PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone() }
                    }
                }
            }
        }
    }
}
