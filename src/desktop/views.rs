use super::download::{Download, Status};
use super::{play_download, reveal};
use crate::app::{AppState, Screen};
use crate::gui::poster::card_background;
use crate::gui::{BackButton, CLOSE_ICON, PLAY_ICON, UPDATE_ICON};
use crate::i18n::{Text, text};
use crate::providers::{Episode, MediaEntry};
use dioxus::prelude::*;

const FOLDER_ICON: &str = include_str!("../../assets/img/vector/folder.svg");
const TRASH_ICON: &str = include_str!("../../assets/img/vector/trash.svg");
const DOWNLOAD_ICON: &str = include_str!("../../assets/img/vector/download.svg");
const CHECK_ICON: &str = include_str!("../../assets/img/vector/check.svg");

#[component]
pub fn EntryActions(entry: MediaEntry, season: Option<u32>, episode: Option<Episode>) -> Element {
    let app = use_context::<AppState>();
    let status = app.device.status(&entry, season, episode.as_ref());
    let progress = status.as_ref().map_or(0, percent);
    let state = status.as_ref().map_or("idle", phase);
    let t = &text().desktop;
    let (label, glyph) = match &status {
        None => (t.download.to_owned(), Some(DOWNLOAD_ICON)),
        Some(Status::Queued) => (t.queued.to_owned(), None),
        Some(Status::Downloading(percent)) => ((t.progress)(*percent), None),
        Some(Status::Muxing) => (t.finishing.to_owned(), None),
        Some(Status::Completed) => (t.downloaded.to_owned(), Some(CHECK_ICON)),
        Some(Status::Failed(_)) => (t.retry.to_owned(), Some(UPDATE_ICON)),
    };
    let number = episode.as_ref().map(|e| e.number);
    let action = match &status {
        None => (t.download_this)(number),
        Some(Status::Completed) => (t.play_this)(number),
        Some(Status::Failed(_)) => (t.retry_this)(number),
        Some(_) => (t.show_this)(number),
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
        button {
            class: if compact { "dl-icon-button {state}" } else { "dl-button {state}" },
            style: "--progress: {progress}%;",
            aria_label: "{action}",
            title: if compact { "{label}" },
            onclick,
            {icon}
            if !compact {
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
    let t = text();
    let summary = summary(t, &active, ready.len(), size);
    rsx! {
        div { class: "downloads-view",
            header { class: "dl-header",
                BackButton {}
                div { class: "dl-heading",
                    h2 { class: "dl-title", "{t.desktop.downloads}" }
                    if !downloads.is_empty() {
                        span { class: "dl-summary", "{summary}" }
                    }
                }
            }
            if downloads.is_empty() {
                div { class: "dl-empty",
                    span { class: "dl-empty-icon", dangerous_inner_html: DOWNLOAD_ICON }
                    h3 { class: "dl-empty-title", "{t.desktop.empty}" }
                    p { class: "dl-empty-text", "{t.desktop.empty_hint}" }
                    button {
                        class: "btn-accent dl-empty-action",
                        onclick: move |_| app.navigate(Screen::Home),
                        "{t.desktop.browse}"
                    }
                }
            }
            if !active.is_empty() {
                DownloadSection { title: t.desktop.active, downloads: active }
            }
            if !ready.is_empty() {
                DownloadSection { title: t.desktop.ready_to_watch, downloads: ready }
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
    let t = text();
    let play = ready.then(|| download.clone());
    rsx! {
        div { class: "dl-item {phase(&download.status)}",
            button {
                class: "dl-thumb",
                style: "{background}",
                disabled: !ready,
                aria_label: (t.desktop.play_title)(&title),
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
                span { class: "dl-detail", "{detail(t, &download)}" }
                div { class: "dl-status",
                    {match &download.status {
                        Status::Queued => rsx! {
                            div { class: "dl-bar", aria_hidden: true }
                            span { class: "dl-state", "{t.desktop.waiting}" }
                        },
                        Status::Downloading(percent) => rsx! {
                            div {
                                class: "dl-bar",
                                role: "progressbar",
                                aria_label: (t.desktop.progress_of)(&title),
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
                                aria_label: (t.desktop.finishing_of)(&title),
                            }
                            span { class: "dl-state", "{t.desktop.finishing}" }
                        },
                        Status::Completed => rsx! {
                            span { class: "dl-state",
                                span { class: "dl-state-icon", dangerous_inner_html: CHECK_ICON }
                                "{ready_label(t, download.size)}"
                            }
                        },
                        Status::Failed(error) => rsx! {
                            span { class: "dl-state", title: "{error}",
                                "{(t.desktop.failed)(error.lines().next().unwrap_or_default())}"
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
                                aria_label: (t.desktop.reveal_title)(&title),
                                title: t.desktop.reveal,
                                onclick: move |_| reveal(app, shown.clone()),
                                span { class: "dl-glyph", dangerous_inner_html: FOLDER_ICON }
                            }
                            button {
                                class: "dl-action danger",
                                aria_label: (t.desktop.delete_title)(&title),
                                title: t.desktop.delete,
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
                                aria_label: (t.desktop.retry_title)(&title),
                                title: t.retry,
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
                                aria_label: (t.desktop.remove_title)(&title),
                                title: t.desktop.remove,
                                onclick: move |_| app.device.dismiss(id),
                                span { class: "dl-glyph", dangerous_inner_html: CLOSE_ICON }
                            }
                        }
                    }
                    Status::Queued | Status::Downloading(_) => rsx! {
                        button {
                            class: "dl-action danger",
                            aria_label: (t.desktop.cancel_title)(&title),
                            title: t.desktop.cancel,
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

fn detail(t: &Text, download: &Download) -> String {
    match (download.season, &download.episode) {
        (Some(season), Some(episode)) => episode.label(season),
        _ => download.entry.year.as_ref().map_or_else(
            || t.movie.to_owned(),
            |year| format!("{} · {year}", t.movie),
        ),
    }
}

fn size_label(t: &Text, bytes: u64) -> String {
    let megabytes = bytes.saturating_add(500_000) / 1_000_000;
    if megabytes < 1000 {
        return format!("{megabytes} MB");
    }
    let tenths = bytes.saturating_add(50_000_000) / 100_000_000;
    format!("{}{}{} GB", tenths / 10, t.decimal, tenths % 10)
}

fn ready_label(t: &Text, size: Option<u64>) -> String {
    match size {
        Some(bytes) => format!("{} · {}", t.desktop.ready, size_label(t, bytes)),
        None => t.desktop.ready.to_owned(),
    }
}

fn summary(t: &Text, active: &[Download], ready: usize, size: u64) -> String {
    let running = active.iter().filter(|d| d.status.is_active()).count();
    let failed = active.len() - running;
    let mut parts = Vec::new();
    if running > 0 {
        parts.push((t.desktop.running)(running));
    }
    if failed > 0 {
        parts.push((t.desktop.failed_count)(failed));
    }
    if ready > 0 {
        parts.push((t.desktop.titles_ready)(ready));
    }
    if size > 0 {
        parts.push(size_label(t, size));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::super::config::AppConfig;
    use super::*;
    use crate::i18n::Lang;

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
        let en = Lang::En.text();
        assert_eq!(size_label(en, 734_003_200), "734 MB");
        assert_eq!(size_label(en, 999_400_000), "999 MB");
        assert_eq!(size_label(en, 999_600_000), "1.0 GB");
        assert_eq!(size_label(en, 1_610_612_736), "1.6 GB");
        assert_eq!(ready_label(en, Some(1_610_612_736)), "Ready · 1.6 GB");
        assert_eq!(ready_label(en, None), "Ready");
        assert_eq!(size_label(Lang::It.text(), 1_610_612_736), "1,6 GB");
    }

    #[test]
    fn details_name_the_episode_or_the_movie_year() {
        let en = Lang::En.text();
        let mut item = download(Status::Queued);
        assert_eq!(detail(en, &item), "Movie");
        item.entry.year = Some("2010".into());
        assert_eq!(detail(en, &item), "Movie · 2010");
        item.season = Some(2);
        item.episode = Some(Episode::numbered(5, "Mandala"));
        assert_eq!(detail(en, &item), "S02E05 - Mandala");
        item.episode = Some(Episode::numbered(5, ""));
        assert_eq!(detail(en, &item), "S02E05");
    }

    #[test]
    fn the_summary_counts_what_is_running_failed_and_ready() {
        let active = [
            download(Status::Downloading(10)),
            download(Status::Queued),
            download(Status::Failed("boom".into())),
        ];
        let en = Lang::En.text();
        assert_eq!(
            summary(en, &active, 3, 1_610_612_736),
            "2 downloading · 1 failed · 3 titles ready · 1.6 GB"
        );
        assert_eq!(summary(en, &[], 1, 0), "1 title ready");
        assert_eq!(
            summary(Lang::It.text(), &active, 1, 0),
            "2 in download · 1 non riuscito · 1 titolo pronto"
        );
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
