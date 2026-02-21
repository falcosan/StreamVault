use crate::gui::messages::Message;
use crate::gui::style;
use crate::provider::{Episode, MediaEntry, Season};
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Fill};

pub fn details_view<'a>(
    entry: &'a MediaEntry,
    seasons: &'a [Season],
    episodes: &'a [Episode],
    selected_season: Option<u32>,
    is_loading: bool,
) -> Element<'a, Message> {
    let header = column![
        row![
            button(text("Back").size(14))
                .on_press(Message::NavigateSearch)
                .padding(8),
            Space::with_width(16),
            text(entry.name.clone()).size(24).color(style::TEXT_PRIMARY),
        ]
        .align_y(Alignment::Center),
        Space::with_height(4),
        row![
            text(if entry.is_movie() { "Movie" } else { "Series" })
                .size(14)
                .color(style::ACCENT_HOVER),
            Space::with_width(12),
            text(entry.year_display().to_string())
                .size(14)
                .color(style::TEXT_SECONDARY),
        ],
    ]
    .padding(20);

    if entry.is_movie() {
        let movie_actions = container(
            column![
                Space::with_height(20),
                row![
                    button(text("Play").center().width(120))
                        .on_press(Message::PlayMovie)
                        .padding(12),
                    Space::with_width(12),
                    button(text("Download").center().width(120))
                        .on_press(Message::DownloadMovie)
                        .padding(12),
                ],
            ]
            .padding(20),
        );

        return column![header, movie_actions].into();
    }

    let season_tabs: Element<'a, Message> = if seasons.is_empty() && is_loading {
        container(text("Loading seasons...").size(14).color(style::TEXT_SECONDARY))
            .padding(20)
            .into()
    } else {
        let tabs: Vec<Element<'a, Message>> = seasons
            .iter()
            .map(|season| {
                let is_selected = selected_season == Some(season.number);
                let label = format!("S{:02}", season.number);

                let btn = button(text(label).size(13).center().width(50)).padding(8);

                let btn = if is_selected {
                    btn
                } else {
                    btn.on_press(Message::SelectSeason(season.number))
                };

                btn.into()
            })
            .collect();

        let mut tab_row = row![].spacing(6).padding(20);
        for tab in tabs {
            tab_row = tab_row.push(tab);
        }

        scrollable(tab_row)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .into()
    };

    let episodes_list: Element<'a, Message> = if episodes.is_empty() && is_loading {
        container(text("Loading episodes...").size(14).color(style::TEXT_SECONDARY))
            .padding(20)
            .into()
    } else if episodes.is_empty() {
        container(text("Select a season").size(14).color(style::TEXT_SECONDARY))
            .padding(20)
            .into()
    } else {
        let ep_cards: Vec<Element<'a, Message>> = episodes
            .iter()
            .map(|ep| episode_card(ep, selected_season.unwrap_or(1)))
            .collect();

        let mut ep_col = column![].spacing(6).padding(20);
        for card in ep_cards {
            ep_col = ep_col.push(card);
        }

        scrollable(ep_col).height(Fill).into()
    };

    column![header, season_tabs, episodes_list].into()
}

fn episode_card(episode: &Episode, season_number: u32) -> Element<'_, Message> {
    let duration_text = episode
        .duration
        .map(|d| format!("{d} min"))
        .unwrap_or_default();

    let info = column![
        text(format!("E{:02} - {}", episode.number, episode.name))
            .size(14)
            .color(style::TEXT_PRIMARY),
        text(duration_text).size(12).color(style::TEXT_SECONDARY),
    ];

    let actions = row![
        button(text("Play").size(12).center().width(50))
            .on_press(Message::PlayEpisode(season_number, episode.number))
            .padding(5),
        Space::with_width(4),
        button(text("DL").size(12).center().width(40))
            .on_press(Message::DownloadEpisode(season_number, episode.number))
            .padding(5),
    ];

    let content = row![info.width(Fill), actions]
        .align_y(Alignment::Center)
        .padding(10);

    container(content)
        .width(Fill)
        .style(style::card_style)
        .into()
}
