use crate::app::AppState;
use crate::download::{Download, Status};
use dioxus::prelude::*;

#[component]
pub fn DownloadsView() -> Element {
    let app = use_context::<AppState>();
    let downloads = app.downloads.read();
    let count = match downloads.len() {
        1 => "1 item".to_owned(),
        n => format!("{n} items"),
    };
    rsx! {
        div { class: "downloads-view",
            div { class: "toolbar",
                button { class: "btn-ghost", onclick: move |_| app.back(), "← Back" }
            }
            div { class: "dl-header",
                h2 { class: "dl-title", "Downloads" }
                span { class: "dl-count", "{count}" }
            }
            if downloads.is_empty() {
                div { class: "dl-empty",
                    span { class: "dl-empty-title", "No downloads yet" }
                    span { class: "dl-empty-sub", "Start downloading to see progress here" }
                }
            } else {
                div { class: "dl-list",
                    for download in downloads.iter() {
                        DownloadCard { key: "{download.id}", download: download.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn DownloadCard(download: Download) -> Element {
    let (text, class) = match &download.status {
        Status::Queued => ("Queued", "queued"),
        Status::Downloading => ("Downloading...", "downloading"),
        Status::Muxing => ("Muxing...", "muxing"),
        Status::Completed => ("Completed", "completed"),
        Status::Failed(e) => (e.as_str(), "failed"),
    };
    rsx! {
        div { class: "dl-card",
            span { class: "dl-card-title", "{download.title}" }
            span { class: "dl-card-status {class}", role: "status", "{text}" }
        }
    }
}
