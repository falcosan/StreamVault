use crate::config::AppConfig;
use crate::gui::app::Message;
use crate::gui::style;
use iced::widget::{button, column, container, row, scrollable, text, text_input, toggler, Space};
use iced::{Alignment, Element, Fill};

pub fn settings_view(config: &AppConfig) -> Element<'_, Message> {
    let header = text("Settings").size(24).color(style::TEXT_PRIMARY);

    let output_section = section(
        "Output",
        column![
            field_row_owned("Download Path", config.output.root_path.clone(), Message::SettingsUpdateRootPath),
            field_row_owned("Movie Folder", config.output.movie_folder_name.clone(), Message::SettingsUpdateMovieFolder),
            field_row_owned("Series Folder", config.output.serie_folder_name.clone(), Message::SettingsUpdateSerieFolder),
            field_row_owned("Episode Format", config.output.map_episode_name.clone(), Message::SettingsUpdateEpisodeFormat),
        ]
        .spacing(8),
    );

    let download_section = section(
        "Download",
        column![
            field_row_owned("Threads", config.download.thread_count.to_string(), Message::SettingsUpdateThreadCount),
            field_row_owned("Retry Count", config.download.retry_count.to_string(), Message::SettingsUpdateRetryCount),
            field_row_owned("Video Select", config.download.select_video.clone(), Message::SettingsUpdateSelectVideo),
            field_row_owned("Audio Select", config.download.select_audio.clone(), Message::SettingsUpdateSelectAudio),
            field_row_owned("Subtitle Select", config.download.select_subtitle.clone(), Message::SettingsUpdateSelectSubtitle),
            field_row_owned("Max Speed", config.download.max_speed.clone(), Message::SettingsUpdateMaxSpeed),
            toggle_row(
                "Concurrent Download",
                config.download.concurrent_download,
                Message::SettingsToggleConcurrent as fn(bool) -> Message,
            ),
        ]
        .spacing(8),
    );

    let process_section = section(
        "Processing",
        column![
            field_row_owned("Output Extension", config.process.extension.clone(), Message::SettingsUpdateExtension),
            toggle_row("Merge Audio", config.process.merge_audio, Message::SettingsToggleMergeAudio as fn(bool) -> Message),
            toggle_row("Merge Subtitles", config.process.merge_subtitle, Message::SettingsToggleMergeSubtitle as fn(bool) -> Message),
            toggle_row("Use GPU", config.process.use_gpu, Message::SettingsToggleGpu as fn(bool) -> Message),
        ]
        .spacing(8),
    );

    let network_section = section(
        "Network",
        column![
            field_row_owned("Timeout (s)", config.requests.timeout.to_string(), Message::SettingsUpdateTimeout),
            toggle_row("Use Proxy", config.requests.use_proxy, Message::SettingsToggleProxy as fn(bool) -> Message),
            field_row_owned("Proxy URL", config.requests.proxy_url.clone(), Message::SettingsUpdateProxyUrl),
        ]
        .spacing(8),
    );

    let save_button = button(text("Save Settings").center().width(150))
        .on_press(Message::SettingsSave)
        .padding(12);

    let content = column![
        header,
        Space::with_height(20),
        output_section,
        Space::with_height(16),
        download_section,
        Space::with_height(16),
        process_section,
        Space::with_height(16),
        network_section,
        Space::with_height(20),
        save_button,
        Space::with_height(20),
    ]
    .padding(20);

    scrollable(content).height(Fill).into()
}

fn section<'a>(title: &str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        column![
            text(title.to_string()).size(18).color(style::TEXT_PRIMARY),
            Space::with_height(12),
            content.into(),
        ]
        .padding(16),
    )
    .width(Fill)
    .style(style::card_style)
    .into()
}

fn field_row_owned<F>(label: &str, value: String, on_change: F) -> Element<'static, Message>
where
    F: Fn(String) -> Message + 'static,
{
    row![
        text(label.to_string())
            .size(14)
            .color(style::TEXT_SECONDARY)
            .width(150),
        text_input("", &value)
            .on_input(on_change)
            .padding(8)
            .size(14)
            .width(Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(12)
    .into()
}

fn toggle_row(
    label: &str,
    value: bool,
    on_toggle: fn(bool) -> Message,
) -> Element<'static, Message> {
    row![
        text(label.to_string())
            .size(14)
            .color(style::TEXT_SECONDARY)
            .width(150),
        toggler(value).on_toggle(on_toggle),
    ]
    .align_y(Alignment::Center)
    .spacing(12)
    .into()
}
