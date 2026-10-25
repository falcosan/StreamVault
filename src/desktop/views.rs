use super::download::{Download, Status};
use super::{play_download, reveal, update};
use crate::app::{AppState, Screen};
use crate::gui::poster::card_background;
use crate::gui::{BackButton, CLOSE_ICON, PLAY_ICON, count};
use crate::providers::{Episode, MediaEntry};
use dioxus::prelude::*;

const FOLDER_ICON: &str = include_str!("../../assets/img/vector/folder.svg");
const TRASH_ICON: &str = include_str!("../../assets/img/vector/trash.svg");
const DOWNLOAD_ICON: &str = include_str!("../../assets/img/vector/download.svg");
const UPDATE_ICON: &str = include_str!("../../assets/img/vector/update.svg");
const CHECK_ICON: &str = include_str!("../../assets/img/vector/check.svg");

#[component]
pub fn NavTools() -> Element {
    let app = use_context::<AppState>();
    let updating = *app.device.updating.read();
    let update = updating || *app.device.update_available.read();
    rsx! {
        if update {
            button {
                class: if updating { "update-btn updating" } else { "update-btn" },
                aria_label: if updating { "Updating" } else { "Install update" },
                disabled: updating,
                onclick: move |_| update::install(app),
                span { class: "update-icon", dangerous_inner_html: UPDATE_ICON }
                if !updating {
                    span { class: "update-dot" }
                }
            }
        }
    }
}

#[component]
pub fn EntryActions(entry: MediaEntry, season: Option<u32>, episode: Option<Episode>) -> Element {
    let app = use_context::<AppState>();
    let status = app.device.status(&entry, season, episode.as_ref());
    let progress = status.as_ref().map_or(0, percent);
    let state = status.as_ref().map_or("idle", phase);
    let (label, glyph) = match &status {
        None => ("Download".to_owned(), Some(DOWNLOAD_ICON)),
        Some(Status::Queued) => ("Queued".to_owned(), None),
        Some(Status::Downloading(percent)) => (format!("Downloading {percent}%"), None),
        Some(Status::Muxing) => ("Finishing".to_owned(), None),
        Some(Status::Completed) => ("Downloaded".to_owned(), Some(CHECK_ICON)),
        Some(Status::Failed(_)) => ("Retry download".to_owned(), Some(UPDATE_ICON)),
    };
    let noun = episode.as_ref().map_or_else(
        || "this movie".to_owned(),
        |e| format!("episode {}", e.number),
    );
    let action = match &status {
        None => format!("Download {noun}"),
        Some(Status::Completed) => format!("Play {noun}"),
        Some(Status::Failed(_)) => format!("Retry the download of {noun}"),
        Some(_) => format!("Show the download of {noun}"),
    };
    let compact = episode.is_some();
    let onclick = move |_| match &status {
        None | Some(Status::Failed(_)) => {
            app.device.download(app, &entry, season, episode.as_ref())
        }
        Some(Status::Completed) => app.play(entry.clone(), season, episode.clone()),
        Some(_) => app.navigate(Screen::Downloads),
    };
    let icon = match glyph {
        Some(svg) => rsx! { span { class: "dl-glyph", dangerous_inner_html: svg } },
        None => rsx! { span { class: "dl-ring" } },
    };
    rsx! {
        if compact {
            button {
                class: "dl-icon-button {state}",
                style: "--progress: {progress}%;",
                aria_label: "{action}",
                title: "{label}",
                onclick,
                {icon}
            }
        } else {
            button {
                class: "dl-button {state}",
                style: "--progress: {progress}%;",
                aria_label: "{action}",
                onclick,
                {icon}
                "{label}"
            }
        }
    }
}

#[component]
pub fn DownloadsView() -> Element {
    let app = use_context::<AppState>();
    let downloads = app.device.downloads.read();
    let active: Vec<Download> = downloads
        .iter()
        .filter(|d| d.status != Status::Completed)
        .cloned()
        .collect();
    let ready: Vec<Download> = downloads
        .iter()
        .rev()
        .filter(|d| d.status == Status::Completed)
        .cloned()
        .collect();
    let size = ready.iter().filter_map(|d| d.size).sum();
    let summary = summary(&active, ready.len(), size);
    rsx! {
        div { class: "downloads-view",
            header { class: "dl-header",
                BackButton {}
                div { class: "dl-heading",
                    h2 { class: "dl-title", "Downloads" }
                    if !downloads.is_empty() {
                        span { class: "dl-summary", "{summary}" }
                    }
                }
            }
            if downloads.is_empty() {
                div { class: "dl-empty",
                    span { class: "dl-empty-icon", dangerous_inner_html: DOWNLOAD_ICON }
                    h3 { class: "dl-empty-title", "No downloads yet" }
                    p { class: "dl-empty-text",
                        "Download a movie or an episode from its page and watch it here, even offline."
                    }
                    button {
                        class: "btn-accent dl-empty-action",
                        onclick: move |_| app.navigate(Screen::Home),
                        "Browse titles"
                    }
                }
            }
            if !active.is_empty() {
                DownloadSection { title: "Downloading", downloads: active }
            }
            if !ready.is_empty() {
                DownloadSection { title: "Ready to watch", downloads: ready }
            }
        }
    }
}

#[component]
fn DownloadSection(title: &'static str, downloads: Vec<Download>) -> Element {
    rsx! {
        section { class: "dl-section",
            h3 { class: "dl-section-title", "{title}" }
            div { class: "dl-list",
                for download in downloads {
                    DownloadItem { key: "{download.id}", download }
                }
            }
        }
    }
}

#[component]
fn DownloadItem(download: Download) -> Element {
    let app = use_context::<AppState>();
    let name = &download.entry.name;
    let title = download
        .entry
        .title(download.season, download.episode.as_ref());
    let background = card_background(name, thumbnail(&download));
    let ready = download.status == Status::Completed;
    let id = download.id;
    let play = ready.then(|| download.clone());
    rsx! {
        div { class: "dl-item {phase(&download.status)}",
            button {
                class: "dl-thumb",
                style: "{background}",
                disabled: !ready,
                aria_label: "Play {title}",
                onclick: move |_| {
                    if let Some(download) = play.clone() {
                        play_download(app, download);
                    }
                },
                if ready {
                    span { class: "dl-thumb-play", dangerous_inner_html: PLAY_ICON }
                }
            }
            div { class: "dl-info",
                span { class: "dl-name", "{name}" }
                span { class: "dl-detail", "{detail(&download)}" }
                div { class: "dl-status",
                    {match &download.status {
                        Status::Queued => rsx! {
                            div { class: "dl-bar", aria_hidden: true }
                            span { class: "dl-state", "Waiting to start" }
                        },
                        Status::Downloading(percent) => rsx! {
                            div {
                                class: "dl-bar",
                                role: "progressbar",
                                aria_label: "{title} download progress",
                                aria_valuemin: 0,
                                aria_valuemax: 100,
                                aria_valuenow: "{percent}",
                                div { class: "dl-bar-fill", style: "width: {percent}%;" }
                            }
                            span { class: "dl-state", "{percent}%" }
                        },
                        Status::Muxing => rsx! {
                            div {
                                class: "dl-bar indeterminate",
                                role: "progressbar",
                                aria_label: "{title} finishing",
                            }
                            span { class: "dl-state", "Finishing" }
                        },
                        Status::Completed => rsx! {
                            span { class: "dl-state",
                                span { class: "dl-state-icon", dangerous_inner_html: CHECK_ICON }
                                "{ready_label(download.size)}"
                            }
                        },
                        Status::Failed(error) => rsx! {
                            span { class: "dl-state", title: "{error}",
                                "Download failed: {error.lines().next().unwrap_or_default()}"
                            }
                        },
                    }}
                }
            }
            div { class: "dl-actions",
                {match &download.status {
                    Status::Completed => {
                        let (shown, deleted) = (download.file.clone(), download.file.clone());
                        rsx! {
                            button {
                                class: "dl-action",
                                aria_label: "Show {title} in Finder",
                                title: "Show in Finder",
                                onclick: move |_| reveal(app, shown.clone()),
                                span { class: "dl-glyph", dangerous_inner_html: FOLDER_ICON }
                            }
                            button {
                                class: "dl-action danger",
                                aria_label: "Delete {title}",
                                title: "Delete",
                                onclick: move |_| app.device.delete_download(app, deleted.clone()),
                                span { class: "dl-glyph", dangerous_inner_html: TRASH_ICON }
                            }
                        }
                    }
                    Status::Failed(_) => {
                        let retry = download.clone();
                        rsx! {
                            button {
                                class: "dl-action",
                                aria_label: "Retry {title}",
                                title: "Retry",
                                onclick: move |_| {
                                    app.device.download(
                                        app,
                                        &retry.entry,
                                        retry.season,
                                        retry.episode.as_ref(),
                                    )
                                },
                                span { class: "dl-glyph", dangerous_inner_html: UPDATE_ICON }
                            }
                            button {
                                class: "dl-action danger",
                                aria_label: "Remove {title}",
                                title: "Remove",
                                onclick: move |_| app.device.dismiss(id),
                                span { class: "dl-glyph", dangerous_inner_html: CLOSE_ICON }
                            }
                        }
                    }
                    Status::Queued | Status::Downloading(_) => rsx! {
                        button {
                            class: "dl-action danger",
                            aria_label: "Cancel {title}",
                            title: "Cancel",
                            onclick: move |_| app.device.dismiss(id),
                            span { class: "dl-glyph", dangerous_inner_html: CLOSE_ICON }
                        }
                    },
                    Status::Muxing => VNode::empty(),
                }}
            }
        }
    }
}

fn phase(status: &Status) -> &'static str {
    match status {
        Status::Queued => "queued",
        Status::Downloading(_) => "downloading",
        Status::Muxing => "muxing",
        Status::Completed => "completed",
        Status::Failed(_) => "failed",
    }
}

fn percent(status: &Status) -> u8 {
    match status {
        Status::Downloading(percent) => *percent,
        Status::Muxing | Status::Completed => 100,
        Status::Queued | Status::Failed(_) => 0,
    }
}

fn thumbnail(download: &Download) -> Option<&str> {
    download
        .episode
        .as_ref()
        .and_then(|e| e.image_url.as_deref())
        .or(download.entry.backdrop_url.as_deref())
        .or(download.entry.image_url.as_deref())
}

fn detail(download: &Download) -> String {
    match (download.season, &download.episode) {
        (Some(season), Some(episode)) => episode.label(season),
        _ => download
            .entry
            .year
            .as_ref()
            .map_or_else(|| "Movie".to_owned(), |year| format!("Movie · {year}")),
    }
}

fn size_label(bytes: u64) -> String {
    let megabytes = bytes.saturating_add(500_000) / 1_000_000;
    if megabytes < 1000 {
        return format!("{megabytes} MB");
    }
    let tenths = bytes.saturating_add(50_000_000) / 100_000_000;
    format!("{}.{} GB", tenths / 10, tenths % 10)
}

fn ready_label(size: Option<u64>) -> String {
    match size {
        Some(bytes) => format!("Ready · {}", size_label(bytes)),
        None => "Ready".to_owned(),
    }
}

fn summary(active: &[Download], ready: usize, size: u64) -> String {
    let running = active.iter().filter(|d| d.status.is_active()).count();
    let failed = active.len() - running;
    let mut parts = Vec::new();
    if running > 0 {
        parts.push(format!("{running} downloading"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    if ready > 0 {
        parts.push(format!("{} ready", count(ready, "title")));
    }
    if size > 0 {
        parts.push(size_label(size));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::super::config::AppConfig;
    use super::*;

    fn download(status: Status) -> Download {
        Download {
            status,
            ..Download::new(&AppConfig::default(), &MediaEntry::default(), None, None)
        }
    }

    #[test]
    fn progress_counts_finishing_and_done_as_full() {
        assert_eq!(percent(&Status::Downloading(42)), 42);
        assert_eq!(percent(&Status::Muxing), 100);
        assert_eq!(percent(&Status::Completed), 100);
        assert_eq!(percent(&Status::Queued), 0);
    }

    #[test]
    fn sizes_read_like_finder() {
        assert_eq!(size_label(734_003_200), "734 MB");
        assert_eq!(size_label(999_400_000), "999 MB");
        assert_eq!(size_label(999_600_000), "1.0 GB");
        assert_eq!(size_label(1_610_612_736), "1.6 GB");
        assert_eq!(ready_label(Some(1_610_612_736)), "Ready · 1.6 GB");
        assert_eq!(ready_label(None), "Ready");
    }

    #[test]
    fn details_name_the_episode_or_the_movie_year() {
        let mut item = download(Status::Queued);
        assert_eq!(detail(&item), "Movie");
        item.entry.year = Some("2010".into());
        assert_eq!(detail(&item), "Movie · 2010");
        item.season = Some(2);
        item.episode = Some(Episode::numbered(5, "Mandala"));
        assert_eq!(detail(&item), "S02E05 - Mandala");
        item.episode = Some(Episode::numbered(5, ""));
        assert_eq!(detail(&item), "S02E05");
    }

    #[test]
    fn the_summary_counts_what_is_running_failed_and_ready() {
        let active = [
            download(Status::Downloading(10)),
            download(Status::Queued),
            download(Status::Failed("boom".into())),
        ];
        assert_eq!(
            summary(&active, 3, 1_610_612_736),
            "2 downloading · 1 failed · 3 titles ready · 1.6 GB"
        );
        assert_eq!(summary(&[], 1, 0), "1 title ready");
    }

    #[test]
    fn thumbnails_prefer_the_episode_still() {
        let mut item = download(Status::Completed);
        item.entry.image_url = Some("poster".into());
        assert_eq!(thumbnail(&item), Some("poster"));
        item.entry.backdrop_url = Some("backdrop".into());
        assert_eq!(thumbnail(&item), Some("backdrop"));
        item.episode = Some(Episode {
            image_url: Some("still".into()),
            ..Episode::numbered(1, "Pilot")
        });
        assert_eq!(thumbnail(&item), Some("still"));
    }
}
