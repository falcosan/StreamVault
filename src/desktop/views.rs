use super::download::{Download, Status};
use super::update;
use crate::app::AppState;
use crate::gui::{BackButton, PLAY_ICON, count};
use crate::providers::{Episode, MediaEntry};
use dioxus::prelude::*;

const FOLDER_ICON: &str = include_str!("../../assets/img/vector/folder.svg");
const TRASH_ICON: &str = include_str!("../../assets/img/vector/trash.svg");
const DOWNLOAD_ICON: &str = include_str!("../../assets/img/vector/download.svg");
const UPDATE_ICON: &str = include_str!("../../assets/img/vector/update.svg");

#[component]
pub fn NavTools() -> Element {
    let app = use_context::<AppState>();
    let updating = *app.device.updating.read();
    if !updating && !*app.device.update_available.read() {
        return VNode::empty();
    }
    rsx! {
        button {
            class: if updating { "update-btn updating" } else { "update-btn" },
            aria_label: if updating { "Updating" } else { "Install update" },
            disabled: updating,
            onclick: move |_| update::install(app),
            div { class: "update-icon", dangerous_inner_html: UPDATE_ICON }
            if !updating {
                span { class: "update-dot" }
            }
        }
    }
}

#[component]
pub fn EntryActions(entry: MediaEntry, season: Option<u32>, episode: Option<Episode>) -> Element {
    let app = use_context::<AppState>();
    let downloaded = app.device.is_downloaded(&entry, season, episode.as_ref());
    let number = episode.as_ref().map(|e| e.number);
    let download = move |_| app.device.download(app, &entry, season, episode.as_ref());
    rsx! {
        if downloaded {
            span { class: "downloaded-badge", "Downloaded" }
        }
        if let Some(number) = number {
            button {
                class: "btn-subtle",
                aria_label: "Download episode {number}",
                onclick: download,
                span { class: "download-icon", dangerous_inner_html: DOWNLOAD_ICON }
            }
        } else {
            button { class: "btn-ghost btn-large", onclick: download, "Download" }
        }
    }
}

#[component]
pub fn DownloadsView() -> Element {
    let app = use_context::<AppState>();
    let downloads = app.device.downloads.read();
    rsx! {
        div { class: "downloads-view",
            div { class: "dl-header",
                BackButton {}
                h2 { class: "dl-title", "Downloads" }
                span { class: "dl-count", "{count(downloads.len(), \"item\")}" }
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
    let (file, deleted) = (download.file.clone(), download.file.clone());
    let target = download.clone();
    rsx! {
        div { class: "dl-card",
            span { class: "dl-card-title", "{download.title}" }
            span { class: "dl-card-status {class}", "{text}" }
            if download.status == Status::Completed {
                div { class: "dl-actions",
                    button {
                        class: "btn-subtle",
                        aria_label: "Delete {download.title}",
                        title: "Delete",
                        onclick: move |_| app.device.delete_download(app, deleted.clone()),
                        span { class: "trash-icon", dangerous_inner_html: TRASH_ICON }
                    }
                    button {
                        class: "btn-subtle",
                        aria_label: "Show {download.title} in Finder",
                        title: "Show in Finder",
                        onclick: move |_| app.device.reveal(app, file.clone()),
                        span { class: "folder-icon", dangerous_inner_html: FOLDER_ICON }
                    }
                    button {
                        class: "btn-play",
                        aria_label: "Play {download.title}",
                        onclick: move |_| app.device.play_download(app, target.clone()),
                        span { class: "play-icon", dangerous_inner_html: PLAY_ICON }
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
