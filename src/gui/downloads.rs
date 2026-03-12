use crate::util::{DownloadProgress, DownloadStatus};
use dioxus::prelude::*;

#[component]
pub fn DownloadsView(
    downloads: ReadSignal<Vec<DownloadProgress>>,
    on_back: EventHandler<()>,
) -> Element {
    let dls = downloads();
    let count_str = format!(
        "{} item{}",
        dls.len(),
        if dls.len() == 1 { "" } else { "s" }
    );
    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            div { class: "details-toolbar",
                button { class: "btn-ghost", onclick: move |_| on_back.call(()), "← Back" }
            }
            div { class: "dl-header",
                span { class: "dl-title", "Downloads" }
                span { class: "dl-count", "{count_str}" }
            }
            if dls.is_empty() {
                div { class: "dl-empty",
                    span { class: "dl-empty-title", "No downloads yet" }
                    span { class: "dl-empty-sub", "Start downloading to see progress here" }
                }
            } else {
                div { class: "dl-list",
                    for dl in dls.iter() {
                        DlCard { key: "{dl.id}", progress: dl.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn DlCard(progress: DownloadProgress) -> Element {
    let (status_text, status_color): (&str, &str) = match &progress.status {
        DownloadStatus::Queued => ("Queued", "var(--text3)"),
        DownloadStatus::Downloading => ("Downloading...", "var(--accent)"),
        DownloadStatus::Muxing => ("Muxing...", "var(--warn)"),
        DownloadStatus::Completed => ("Completed", "var(--success)"),
        DownloadStatus::Failed(ref e) => (e.as_str(), "var(--danger)"),
    };

    rsx! {
        div { class: "dl-card",
            div { class: "dl-card-top",
                span { class: "dl-card-title", "{progress.title}" }
                span { class: "dl-card-status", color: "{status_color}", "{status_text}" }
            }
        }
    }
}
