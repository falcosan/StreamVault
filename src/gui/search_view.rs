use super::poster::PosterCard;
use crate::app::AppState;
use crate::i18n::text;
use dioxus::prelude::*;

#[component]
pub fn SearchView() -> Element {
    let app = use_context::<AppState>();
    let results = app.search.results.read();
    let busy = *app.search.busy.read();
    let t = text();

    if busy || results.is_empty() {
        let message = if busy { t.searching } else { t.no_results };
        return rsx! {
            div { class: "center-msg", role: "status",
                span { class: "empty-msg", "{message}" }
            }
        };
    }

    rsx! {
        div { class: "catalog-view",
            div { class: "section-header",
                h2 { class: "section-title", "{t.results}" }
                span { class: "section-count", "{(t.result_count)(results.len())}" }
            }
            div { class: "media-grid",
                for entry in results.iter() {
                    PosterCard { key: "{entry.provider}-{entry.id}", entry: entry.clone() }
                }
            }
        }
    }
}
