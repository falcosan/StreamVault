use crate::gui::messages::Message;
use crate::gui::style;
use crate::provider::MediaEntry;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Alignment, Element, Fill};

pub fn search_view<'a>(
    query: &'a str,
    results: &'a [MediaEntry],
    is_loading: bool,
) -> Element<'a, Message> {
    let search_bar = row![
        text_input("Search movies and series...", query)
            .on_input(Message::SearchInputChanged)
            .on_submit(Message::SearchSubmit)
            .padding(12)
            .size(16)
            .width(Fill),
        Space::with_width(10),
        button(
            text(if is_loading { "Searching..." } else { "Search" })
                .center()
                .width(100)
        )
        .on_press_maybe(if is_loading {
            None
        } else {
            Some(Message::SearchSubmit)
        })
        .padding(12),
    ]
    .align_y(Alignment::Center)
    .padding(20);

    let results_content: Element<'a, Message> = if results.is_empty() && !is_loading {
        container(
            text(if query.is_empty() {
                "Type to search for movies and series"
            } else {
                "No results found"
            })
            .size(16)
            .color(style::TEXT_SECONDARY),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(40)
        .into()
    } else {
        let cards: Vec<Element<'a, Message>> = results
            .iter()
            .enumerate()
            .map(|(idx, entry)| result_card(idx, entry))
            .collect();

        let mut results_col = column![].spacing(8).padding(20);
        for card in cards {
            results_col = results_col.push(card);
        }

        scrollable(results_col).height(Fill).into()
    };

    column![search_bar, results_content].into()
}

fn result_card<'a>(index: usize, entry: &'a MediaEntry) -> Element<'a, Message> {
    let type_label = if entry.is_movie() { "Movie" } else { "Series" };
    let type_color = if entry.is_movie() {
        style::ACCENT_HOVER
    } else {
        style::WARNING
    };

    let year_display = entry.year_display().to_string();

    let info = column![
        text(entry.name.clone()).size(16).color(style::TEXT_PRIMARY),
        Space::with_height(4),
        row![
            container(text(type_label).size(11).color(style::TEXT_PRIMARY))
                .padding([2, 8])
                .style(move |_: &_| container::Style {
                    background: Some(iced::Background::Color(type_color)),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            Space::with_width(8),
            text(year_display).size(13).color(style::TEXT_SECONDARY),
        ]
        .align_y(Alignment::Center),
    ];

    let actions = row![
        button(text("Details").size(13).center().width(70))
            .on_press(Message::SelectEntry(index))
            .padding(6),
        Space::with_width(6),
        button(text("Play").size(13).center().width(50))
            .on_press(Message::PlayEntry(index))
            .padding(6),
    ];

    let card_content = row![info.width(Fill), actions]
        .align_y(Alignment::Center)
        .padding(12)
        .spacing(10);

    container(card_content)
        .width(Fill)
        .style(style::card_style)
        .into()
}
