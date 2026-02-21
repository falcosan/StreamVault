use crate::gui::app::Message;
use crate::gui::style;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Fill};

pub fn home_view(provider_online: bool) -> Element<'static, Message> {
    let title = text("StreamVault")
        .size(36)
        .color(style::TEXT_PRIMARY);

    let subtitle = text("Stream, Download, Watch")
        .size(16)
        .color(style::TEXT_SECONDARY);

    let status_color = if provider_online {
        style::SUCCESS
    } else {
        style::DANGER
    };

    let status_text = if provider_online {
        "Online"
    } else {
        "Offline"
    };

    let provider_card = container(
        column![
            text("StreamingCommunity").size(18).color(style::TEXT_PRIMARY),
            Space::with_height(8),
            row![
                container(Space::new(10, 10))
                    .style(move |_: &_| container::Style {
                        background: Some(iced::Background::Color(status_color)),
                        border: iced::Border {
                            radius: 5.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                Space::with_width(8),
                text(status_text).size(14).color(style::TEXT_SECONDARY),
            ]
            .align_y(Alignment::Center),
            Space::with_height(16),
            button(text("Search").center().width(Fill))
                .width(Fill)
                .on_press(Message::NavigateSearch),
        ]
        .width(280)
        .padding(20),
    )
    .style(style::card_style);

    let content = column![
        Space::with_height(60),
        title,
        Space::with_height(4),
        subtitle,
        Space::with_height(40),
        text("Providers").size(20).color(style::TEXT_PRIMARY),
        Space::with_height(16),
        provider_card,
    ]
    .align_x(Alignment::Center)
    .padding(40);

    container(content)
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .into()
}
