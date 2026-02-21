use crate::download::{DownloadProgress, DownloadStatus};
use crate::gui::app::Message;
use crate::gui::style;
use iced::widget::{column, container, progress_bar, row, scrollable, text, Space};
use iced::{Alignment, Element, Fill};

pub fn downloads_view(downloads: &[DownloadProgress]) -> Element<'_, Message> {
    let header = column![
        text("Downloads").size(24).color(style::TEXT_PRIMARY),
        Space::with_height(4),
        text(format!("{} items", downloads.len()))
            .size(14)
            .color(style::TEXT_SECONDARY),
    ]
    .padding(20);

    let content: Element<'_, Message> = if downloads.is_empty() {
        container(
            text("No downloads yet")
                .size(16)
                .color(style::TEXT_SECONDARY),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(60)
        .into()
    } else {
        let cards: Vec<Element<'_, Message>> = downloads
            .iter()
            .map(download_card)
            .collect();

        let mut col = column![].spacing(8).padding(20);
        for card in cards {
            col = col.push(card);
        }

        scrollable(col).height(Fill).into()
    };

    column![header, content].into()
}

fn download_card(progress: &DownloadProgress) -> Element<'_, Message> {
    let (status_text, status_color) = match &progress.status {
        DownloadStatus::Queued => ("Queued", style::TEXT_SECONDARY),
        DownloadStatus::Downloading => ("Downloading", style::ACCENT_HOVER),
        DownloadStatus::Muxing => ("Muxing", style::WARNING),
        DownloadStatus::Completed => ("Completed", style::SUCCESS),
        DownloadStatus::Failed(_) => ("Failed", style::DANGER),
    };

    let speed_info = if progress.speed.is_empty() {
        String::new()
    } else {
        format!(
            " | {} | {}/{}",
            progress.speed, progress.downloaded, progress.total
        )
    };

    let content = column![
        row![
            text(&progress.title)
                .size(14)
                .color(style::TEXT_PRIMARY)
                .width(Fill),
            text(status_text).size(12).color(status_color),
        ]
        .align_y(Alignment::Center),
        Space::with_height(8),
        progress_bar(0.0..=100.0, progress.percent as f32).height(6),
        Space::with_height(4),
        text(format!("{:.1}%{speed_info}", progress.percent))
            .size(11)
            .color(style::TEXT_SECONDARY),
    ]
    .spacing(2)
    .padding(12);

    container(content)
        .width(Fill)
        .style(style::card_style)
        .into()
}
