use crate::app::AppState;
use crate::download::{Download, Status};
use dioxus::prelude::*;

const FOLDER_ICON: &str = include_str!("../../assets/img/vector/folder.svg");

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
    let app = use_context::<AppState>();
    let (text, class) = match &download.status {
        Status::Queued => ("Queued".into(), "queued"),
        Status::Downloading(percent) => (format!("Downloading... {percent}%"), "downloading"),
        Status::Muxing => ("Muxing...".into(), "muxing"),
        Status::Completed => ("Completed".into(), "completed"),
        Status::Failed(e) => (e.clone(), "failed"),
    };
    let file = download.file.clone();
    let target = download.clone();
    rsx! {
        div { class: "dl-card",
            span { class: "dl-card-title", "{download.title}" }
            span { class: "dl-card-status {class}", "{text}" }
            if download.status == Status::Completed {
                div { class: "dl-actions",
                    button {
                        class: "btn-subtle",
                        aria_label: "Show {download.title} in Finder",
                        title: "Show in Finder",
                        onclick: move |_| app.reveal(file.clone()),
                        span { class: "folder-icon", dangerous_inner_html: FOLDER_ICON }
                    }
                    button {
                        class: "btn-play",
                        aria_label: "Play {download.title}",
                        onclick: move |_| app.play_download(target.clone()),
                        "▶"
                    }
                }
            }
            if let Status::Downloading(percent) = download.status {
                progress {
                    class: "dl-progress",
                    aria_label: "{download.title} download progress",
                    max: 100,
                    value: "{percent}",
                }
            }
        }
    }
}
