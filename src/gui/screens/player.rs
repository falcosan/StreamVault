use crate::gui::app::Message;
use crate::gui::style;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Fill};

pub fn player_view<'a>(is_playing: bool, title: &'a str) -> Element<'a, Message> {
    let header = row![
        button(text("Back").size(14))
            .on_press(Message::PlayerStop)
            .padding(8),
        Space::with_width(16),
        text(title).size(20).color(style::TEXT_PRIMARY),
    ]
    .align_y(Alignment::Center)
    .padding(20);

    let (status_text, status_color) = if is_playing {
        ("Playing", style::SUCCESS)
    } else {
        ("Paused", style::WARNING)
    };

    let status_display = container(
        column![
            text("Now Playing").size(14).color(style::TEXT_SECONDARY),
            Space::with_height(8),
            text(title).size(18).color(style::TEXT_PRIMARY),
            Space::with_height(4),
            text(status_text).size(14).color(status_color),
            Space::with_height(8),
            text("Use the video player window controls for playback")
                .size(12)
                .color(style::TEXT_SECONDARY),
        ]
        .padding(20),
    )
    .width(Fill)
    .style(style::card_style);

    let controls = row![
        button(text("Stop & Close").center().width(120))
            .on_press(Message::PlayerStop)
            .padding(10),
    ]
    .align_y(Alignment::Center);

    let content = column![
        header,
        Space::with_height(40),
        status_display,
        Space::with_height(30),
        container(controls).width(Fill).center_x(Fill),
    ]
    .padding(20);

    container(content)
        .width(Fill)
        .height(Fill)
        .into()
}
