use iced::color;
use iced::widget::container;
use iced::Theme;

pub const BG_DARK: iced::Color = color!(0x1a, 0x1a, 0x2e);
pub const BG_CARD: iced::Color = color!(0x16, 0x21, 0x3e);
pub const ACCENT_HOVER: iced::Color = color!(0x15, 0x52, 0xab);
pub const TEXT_PRIMARY: iced::Color = color!(0xe0, 0xe0, 0xe0);
pub const TEXT_SECONDARY: iced::Color = color!(0x8a, 0x8a, 0x9a);
pub const SUCCESS: iced::Color = color!(0x2e, 0xcc, 0x71);
pub const DANGER: iced::Color = color!(0xe7, 0x4c, 0x3c);
pub const WARNING: iced::Color = color!(0xf3, 0x9c, 0x12);

pub fn card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(BG_CARD)),
        border: iced::Border {
            color: color!(0x2a, 0x2a, 0x4a),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

pub fn sidebar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(color!(0x0f, 0x0f, 0x23))),
        ..Default::default()
    }
}
