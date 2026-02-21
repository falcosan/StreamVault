use crate::gui::app::Message;
use crate::gui::style;
use crate::playback::PlaybackState;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Fill};

pub fn player_view(state: &PlaybackState, title: &str) -> Element<'_, Message> {
    let header = row![
        button(text("Back").size(14))
            .on_press(Message::NavigateSearch)
            .padding(8),
        Space::with_width(16),
        text(title).size(20).color(style::TEXT_PRIMARY),
    ]
    .align_y(Alignment::Center)
    .padding(20);

    let status_text = match state {
        PlaybackState::Stopped => "Stopped",
        PlaybackState::Playing(_) => "Playing",
        PlaybackState::Paused(_) => "Paused",
    };

    let status_color = match state {
        PlaybackState::Stopped => style::TEXT_SECONDARY,
        PlaybackState::Playing(_) => style::SUCCESS,
        PlaybackState::Paused(_) => style::WARNING,
    };

    let status_display = container(
        column![
            text("Now Playing").size(14).color(style::TEXT_SECONDARY),
            Space::with_height(8),
            text(title).size(18).color(style::TEXT_PRIMARY),
            Space::with_height(4),
            text(status_text).size(14).color(status_color),
        ]
        .padding(20),
    )
    .width(Fill)
    .style(style::card_style);

    let is_active = !matches!(state, PlaybackState::Stopped);
    let is_playing = matches!(state, PlaybackState::Playing(_));

    let play_pause = if is_playing {
        button(text("Pause").center().width(80))
            .on_press(Message::PlayerPause)
            .padding(10)
    } else {
        button(text("Resume").center().width(80))
            .on_press(Message::PlayerResume)
            .padding(10)
    };

    let controls = row![
        button(text("-10s").center().width(60))
            .on_press_maybe(if is_active {
                Some(Message::PlayerSeekBackward)
            } else {
                None
            })
            .padding(10),
        Space::with_width(8),
        play_pause,
        Space::with_width(8),
        button(text("Stop").center().width(60))
            .on_press_maybe(if is_active {
                Some(Message::PlayerStop)
            } else {
                None
            })
            .padding(10),
        Space::with_width(8),
        button(text("+10s").center().width(60))
            .on_press_maybe(if is_active {
                Some(Message::PlayerSeekForward)
            } else {
                None
            })
            .padding(10),
    ]
    .align_y(Alignment::Center);

    let volume_controls = row![
        button(text("Vol -").center().width(60))
            .on_press_maybe(if is_active {
                Some(Message::PlayerVolumeDown)
            } else {
                None
            })
            .padding(8),
        Space::with_width(8),
        button(text("Vol +").center().width(60))
            .on_press_maybe(if is_active {
                Some(Message::PlayerVolumeUp)
            } else {
                None
            })
            .padding(8),
    ]
    .align_y(Alignment::Center);

    let content = column![
        header,
        Space::with_height(40),
        status_display,
        Space::with_height(30),
        container(controls).width(Fill).center_x(Fill),
        Space::with_height(16),
        container(volume_controls).width(Fill).center_x(Fill),
    ]
    .padding(20);

    container(content)
        .width(Fill)
        .height(Fill)
        .into()
}
