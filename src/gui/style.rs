use iced::color;
use iced::widget::{button, container, text};
use iced::{Element, Fill, Theme};

use crate::gui::messages::Message;

pub const BG_DARK: iced::Color = color!(0x1a, 0x1a, 0x2e);
pub const BG_CARD: iced::Color = color!(0x16, 0x21, 0x3e);
pub const BG_SIDEBAR: iced::Color = color!(0x0f, 0x0f, 0x23);
pub const BORDER_CARD: iced::Color = color!(0x2a, 0x2a, 0x4a);
pub const ACCENT_HOVER: iced::Color = color!(0x15, 0x52, 0xab);
pub const TEXT_PRIMARY: iced::Color = color!(0xe0, 0xe0, 0xe0);
pub const TEXT_SECONDARY: iced::Color = color!(0x8a, 0x8a, 0x9a);
pub const SUCCESS: iced::Color = color!(0x2e, 0xcc, 0x71);
pub const DANGER: iced::Color = color!(0xe7, 0x4c, 0x3c);
pub const WARNING: iced::Color = color!(0xf3, 0x9c, 0x12);
pub const SIDEBAR_WIDTH: u16 = 160;
pub const CARD_BORDER_RADIUS: f32 = 8.0;
pub const CARD_BORDER_WIDTH: f32 = 1.0;

pub fn card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(BG_CARD)),
        border: iced::Border {
            color: BORDER_CARD,
            width: CARD_BORDER_WIDTH,
            radius: CARD_BORDER_RADIUS.into(),
        },
        ..Default::default()
    }
}

pub fn sidebar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(BG_SIDEBAR)),
        ..Default::default()
    }
}

pub fn nav_button(label: &str, is_active: bool, msg: Message) -> Element<'_, Message> {
    let color = if is_active {
        TEXT_PRIMARY
    } else {
        TEXT_SECONDARY
    };

    button(text(label).size(14).color(color).width(Fill))
        .on_press(msg)
        .padding([10, 16])
        .width(Fill)
        .into()
}
