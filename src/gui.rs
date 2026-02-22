use crate::config::AppConfig;
use crate::providers::{Episode, MediaEntry, Season, StreamUrl};
use crate::util::{DownloadProgress, DownloadStatus};
use iced::color;
use iced::widget::{
    button, column, container, progress_bar, row, scrollable, text, text_input, toggler, Space,
};
use iced::{Alignment, Element, Fill, Theme};

pub const NAVBAR_H: u16 = 48;
const POSTER_W: f32 = 230.0;
const POSTER_H: f32 = 130.0;
const ROW_GAP: u16 = 8;

#[derive(Clone, Copy)]
pub struct Pal {
    pub bg: iced::Color,
    pub surface: iced::Color,
    pub surface2: iced::Color,
    pub border: iced::Color,
    pub accent: iced::Color,
    pub warn: iced::Color,
    pub danger: iced::Color,
    pub success: iced::Color,
    pub text: iced::Color,
    pub text2: iced::Color,
    pub text3: iced::Color,
    pub navbar: iced::Color,
}

impl Pal {
    #[inline]
    pub fn light() -> Self {
        Self {
            bg: color!(0xf0, 0xf0, 0xf0),
            surface: color!(0xff, 0xff, 0xff),
            surface2: color!(0xe8, 0xe8, 0xe8),
            border: color!(0xd0, 0xd0, 0xd0),
            accent: color!(0xe5, 0x09, 0x14),
            warn: color!(0xf5, 0xb0, 0x14),
            danger: color!(0xe5, 0x09, 0x14),
            success: color!(0x46, 0xd3, 0x69),
            text: color!(0x14, 0x14, 0x14),
            text2: color!(0x5a, 0x5a, 0x5a),
            text3: color!(0x90, 0x90, 0x90),
            navbar: color!(0x22, 0x22, 0x22),
        }
    }

    #[inline]
    pub fn dark() -> Self {
        Self {
            bg: color!(0x14, 0x14, 0x14),
            surface: color!(0x1a, 0x1a, 0x1a),
            surface2: color!(0x25, 0x25, 0x25),
            border: color!(0x30, 0x30, 0x30),
            accent: color!(0xe5, 0x09, 0x14),
            warn: color!(0xf5, 0xb0, 0x14),
            danger: color!(0xe5, 0x09, 0x14),
            success: color!(0x46, 0xd3, 0x69),
            text: color!(0xe5, 0xe5, 0xe5),
            text2: color!(0xa0, 0xa0, 0xa0),
            text3: color!(0x68, 0x68, 0x68),
            navbar: color!(0x0c, 0x0c, 0x0c),
        }
    }

    #[inline]
    pub fn from_dark(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    #[inline]
    pub fn theme(&self) -> Theme {
        Theme::custom(
            "StreamVault".into(),
            iced::theme::Palette {
                background: self.bg,
                text: self.text,
                primary: self.accent,
                success: self.success,
                danger: self.danger,
            },
        )
    }
}

#[derive(Debug, Clone)]
pub enum Screen {
    Home,
    Search,
    Details,
    Player,
    Downloads,
    Settings,
}

#[derive(Debug, Clone)]
pub enum Msg {
    NavHome,
    NavSearch,
    NavDownloads,
    NavSettings,
    SearchInput(String),
    SearchSubmit,
    SearchDone(Result<Vec<MediaEntry>, String>),
    SelectEntry(usize),
    SeasonsLoaded(Result<Vec<Season>, String>),
    SelectSeason(u32),
    EpisodesLoaded(Result<Vec<Episode>, String>),
    PlayMovie,
    PlayEpisode(u32, u32),
    StreamResolved(Result<(StreamUrl, String), String>),
    Pause,
    Resume,
    Stop,
    DlMovie,
    DlEpisode(u32, u32),
    DlStreamResolved(Result<(StreamUrl, String, bool), String>),
    ProviderStatus(bool),
    CfgDarkMode(bool),
    CfgRootPath(String),
    CfgMovieFolder(String),
    CfgSerieFolder(String),
    CfgEpFormat(String),
    CfgThreads(String),
    CfgRetry(String),
    CfgSelVideo(String),
    CfgSelAudio(String),
    CfgSelSub(String),
    CfgMaxSpeed(String),
    CfgExtension(String),
    CfgTimeout(String),
    CfgProxyUrl(String),
    CfgConcurrent(bool),
    CfgMergeAudio(bool),
    CfgMergeSub(bool),
    CfgGpu(bool),
    CfgProxy(bool),
    CfgSave,
    DismissError,
    Tick,
}

const POSTER_COLORS: &[(u8, u8, u8)] = &[
    (0x8B, 0x1A, 0x1A),
    (0x0D, 0x3B, 0x66),
    (0x3B, 0x0A, 0x5C),
    (0x14, 0x40, 0x14),
    (0x6B, 0x3A, 0x00),
    (0x5C, 0x0E, 0x0E),
    (0x0A, 0x2A, 0x4A),
    (0x40, 0x0A, 0x50),
    (0x4A, 0x0E, 0x0E),
    (0x00, 0x3A, 0x3A),
    (0x3E, 0x21, 0x23),
    (0x1B, 0x2A, 0x41),
    (0x2D, 0x1B, 0x00),
    (0x1A, 0x0A, 0x2E),
    (0x0E, 0x33, 0x1A),
    (0x33, 0x1A, 0x0E),
];

fn name_hash(name: &str) -> usize {
    name.bytes()
        .fold(0u32, |a, b| a.wrapping_mul(37).wrapping_add(b as u32)) as usize
}

fn poster_bg(name: &str) -> iced::Color {
    let (r, g, b) = POSTER_COLORS[name_hash(name) % POSTER_COLORS.len()];
    color!(r, g, b)
}

fn poster_bg_hover(name: &str) -> iced::Color {
    let (r, g, b) = POSTER_COLORS[name_hash(name) % POSTER_COLORS.len()];
    color!(
        r.saturating_add(35),
        g.saturating_add(35),
        b.saturating_add(35)
    )
}

fn accent_btn(p: Pal) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = match status {
            button::Status::Hovered => color!(0xb8, 0x07, 0x10),
            button::Status::Pressed => color!(0x90, 0x05, 0x0c),
            _ => p.accent,
        };
        button::Style {
            background: Some(iced::Background::Color(bg)),
            text_color: iced::Color::WHITE,
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

fn ghost_btn(p: Pal) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = match status {
            button::Status::Hovered => p.surface2,
            button::Status::Pressed => p.border,
            _ => iced::Color::TRANSPARENT,
        };
        button::Style {
            background: Some(iced::Background::Color(bg)),
            text_color: p.text,
            border: iced::Border {
                color: p.border,
                width: 1.0,
                radius: 3.0.into(),
            },
            ..Default::default()
        }
    }
}

fn transparent_btn() -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| button::Style {
        background: None,
        text_color: iced::Color::WHITE,
        ..Default::default()
    }
}

fn card_style(p: Pal) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(p.surface)),
        border: iced::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn navbar<'a>(p: Pal, screen: &Screen, query: &'a str, searching: bool) -> Element<'a, Msg> {
    let logo = button(text("STREAMVAULT").size(20).color(p.accent))
        .on_press(Msg::NavHome)
        .padding(0)
        .style(transparent_btn());

    let link = |label: &str, active: bool, msg: Msg| -> Element<'static, Msg> {
        let label = label.to_string();
        let c = if active {
            iced::Color::WHITE
        } else {
            color!(0x80, 0x80, 0x80)
        };
        button(text(label).size(13).color(c))
            .on_press(msg)
            .padding([4, 10])
            .style(move |_, status| {
                let bg = if matches!(status, button::Status::Hovered) && !active {
                    Some(iced::Background::Color(color!(0x30, 0x30, 0x30)))
                } else {
                    None
                };
                button::Style {
                    background: bg,
                    text_color: c,
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .into()
    };

    let search_input = text_input("Search...", query)
        .on_input(Msg::SearchInput)
        .on_submit(Msg::SearchSubmit)
        .padding([5, 10])
        .size(13)
        .width(200);

    let search_go = button(
        text(if searching { "..." } else { "Go" })
            .size(12)
            .center()
            .width(32)
            .color(iced::Color::WHITE),
    )
    .on_press_maybe(if searching {
        None
    } else {
        Some(Msg::SearchSubmit)
    })
    .padding([5, 8])
    .style(accent_btn(p));

    container(
        row![
            logo,
            Space::with_width(24),
            link("Home", matches!(screen, Screen::Home), Msg::NavHome),
            link(
                "Browse",
                matches!(screen, Screen::Search | Screen::Details),
                Msg::NavSearch
            ),
            link(
                "Downloads",
                matches!(screen, Screen::Downloads),
                Msg::NavDownloads
            ),
            link(
                "Settings",
                matches!(screen, Screen::Settings),
                Msg::NavSettings
            ),
            Space::with_width(Fill),
            search_input,
            Space::with_width(4),
            search_go,
        ]
        .align_y(Alignment::Center)
        .spacing(2)
        .padding([0, 20]),
    )
    .width(Fill)
    .height(NAVBAR_H)
    .center_y(NAVBAR_H)
    .style(move |_: &_| container::Style {
        background: Some(iced::Background::Color(p.navbar)),
        ..Default::default()
    })
    .into()
}

pub fn home_view(p: Pal, online: bool, results: &[MediaEntry]) -> Element<'_, Msg> {
    if results.is_empty() {
        let (sc, st) = if online {
            (p.success, "Provider Online")
        } else {
            (p.danger, "Provider Offline")
        };
        let dot = container(Space::new(6, 6)).style(move |_: &_| container::Style {
            background: Some(iced::Background::Color(sc)),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        return container(
            column![
                Space::with_height(80),
                text("STREAMVAULT").size(52).color(p.accent),
                Space::with_height(6),
                text("Stream. Download. Watch.").size(16).color(p.text2),
                Space::with_height(28),
                row![dot, Space::with_width(8), text(st).size(12).color(sc)]
                    .align_y(Alignment::Center),
                Space::with_height(32),
                text("Search for movies and series to get started")
                    .size(14)
                    .color(p.text3),
            ]
            .align_x(Alignment::Center),
        )
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .into();
    }

    let all: Vec<(usize, &MediaEntry)> = results.iter().enumerate().collect();
    let movies: Vec<(usize, &MediaEntry)> = results
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_movie())
        .collect();
    let series: Vec<(usize, &MediaEntry)> = results
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_movie())
        .collect();

    let mut content = column![Space::with_height(12)];
    if !all.is_empty() {
        content = content.push(media_row(p, "Top Picks", &all));
    }
    if !movies.is_empty() {
        content = content.push(media_row(p, "Movies", &movies));
    }
    if !series.is_empty() {
        content = content.push(media_row(p, "Series", &series));
    }
    content = content.push(Space::with_height(24));

    scrollable(content).height(Fill).into()
}

fn media_row<'a>(p: Pal, title: &str, items: &[(usize, &'a MediaEntry)]) -> Element<'a, Msg> {
    let header = container(text(title.to_string()).size(16).color(p.text)).padding(iced::Padding {
        top: 10.0,
        right: 20.0,
        bottom: 6.0,
        left: 20.0,
    });

    let mut cards = row![].spacing(ROW_GAP);
    for &(idx, entry) in items {
        cards = cards.push(poster_card(p, idx, entry));
    }

    let cards_scroll = scrollable(container(cards).padding([0, 20])).direction(
        scrollable::Direction::Horizontal(scrollable::Scrollbar::default()),
    );

    column![header, cards_scroll, Space::with_height(10)]
        .spacing(0)
        .into()
}

fn poster_card<'a>(p: Pal, idx: usize, entry: &'a MediaEntry) -> Element<'a, Msg> {
    let bg = poster_bg(&entry.name);
    let bg_h = poster_bg_hover(&entry.name);
    let yr = entry.year_display().to_string();
    let is_movie = entry.is_movie();
    let kind_label = if is_movie { "MOVIE" } else { "SERIES" };
    let kind_color = if is_movie {
        p.accent
    } else {
        color!(0x00, 0x91, 0xd5)
    };
    let title = entry.name.clone();

    let badge = container(text(kind_label).size(8).color(iced::Color::WHITE))
        .padding([1, 6])
        .style(move |_: &_| container::Style {
            background: Some(iced::Background::Color(kind_color)),
            border: iced::Border {
                radius: 2.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let overlay_bottom = container(
        column![
            text(title).size(12).color(iced::Color::WHITE),
            row![
                badge,
                Space::with_width(6),
                text(yr).size(10).color(color!(0xb0, 0xb0, 0xb0)),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(3),
    )
    .width(Fill)
    .padding([6, 10])
    .style(move |_: &_| container::Style {
        background: Some(iced::Background::Color(iced::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.75,
        })),
        ..Default::default()
    });

    let face = container(column![Space::with_height(Fill), overlay_bottom])
        .width(POSTER_W)
        .height(POSTER_H);

    button(face)
        .on_press(Msg::SelectEntry(idx))
        .padding(0)
        .style(move |_, status| {
            let c = match status {
                button::Status::Hovered => bg_h,
                _ => bg,
            };
            button::Style {
                background: Some(iced::Background::Color(c)),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

pub fn search_view<'a>(
    p: Pal,
    query: &'a str,
    results: &'a [MediaEntry],
    loading: bool,
) -> Element<'a, Msg> {
    if results.is_empty() && !loading {
        let msg = if query.is_empty() {
            "Use the search bar to find movies and series"
        } else {
            "No results found — try different keywords"
        };
        return container(text(msg).size(16).color(p.text3))
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into();
    }

    if loading {
        return container(text("Searching...").size(16).color(p.text3))
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill)
            .into();
    }

    let header = container(
        row![
            text("Browse").size(18).color(p.text),
            Space::with_width(10),
            text(format!("{} results", results.len()))
                .size(12)
                .color(p.text3),
        ]
        .align_y(Alignment::Center),
    )
    .padding([14, 20]);

    let all: Vec<(usize, &MediaEntry)> = results.iter().enumerate().collect();
    let movies: Vec<(usize, &MediaEntry)> = results
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_movie())
        .collect();
    let series: Vec<(usize, &MediaEntry)> = results
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_movie())
        .collect();

    let mut body = column![].spacing(4);
    if !all.is_empty() {
        body = body.push(media_row(p, "All Results", &all));
    }
    if !movies.is_empty() {
        body = body.push(media_row(p, "Movies", &movies));
    }
    if !series.is_empty() {
        body = body.push(media_row(p, "Series", &series));
    }
    body = body.push(Space::with_height(24));

    column![header, scrollable(body).height(Fill)].into()
}

pub fn details_view<'a>(
    p: Pal,
    entry: &'a MediaEntry,
    seasons: &'a [Season],
    episodes: &'a [Episode],
    sel_season: Option<u32>,
    loading: bool,
) -> Element<'a, Msg> {
    let bg = poster_bg(&entry.name);
    let kind_c = if entry.is_movie() {
        p.accent
    } else {
        color!(0x00, 0x91, 0xd5)
    };
    let kind_l = if entry.is_movie() { "MOVIE" } else { "SERIES" };

    let hero = container(column![
        Space::with_height(Fill),
        container(
            column![
                row![
                    container(text(kind_l).size(10).color(iced::Color::WHITE))
                        .padding([2, 10])
                        .style(move |_: &_| container::Style {
                            background: Some(iced::Background::Color(kind_c)),
                            border: iced::Border {
                                radius: 3.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                    Space::with_width(10),
                    text(entry.year_display().to_string())
                        .size(13)
                        .color(color!(0xbb, 0xbb, 0xbb)),
                ]
                .align_y(Alignment::Center),
                Space::with_height(6),
                text(entry.name.clone()).size(30).color(iced::Color::WHITE),
            ]
            .padding([16, 24]),
        )
        .width(Fill)
        .style(move |_: &_| container::Style {
            background: Some(iced::Background::Color(iced::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.6,
            })),
            ..Default::default()
        }),
    ])
    .width(Fill)
    .height(160)
    .style(move |_: &_| container::Style {
        background: Some(iced::Background::Color(bg)),
        ..Default::default()
    });

    let back = button(text("← Back").size(13).color(p.text2))
        .on_press(Msg::NavSearch)
        .padding([6, 14])
        .style(ghost_btn(p));

    if entry.is_movie() {
        return scrollable(column![
            hero,
            container(
                row![
                    back,
                    Space::with_width(Fill),
                    button(
                        text("▶  Play Now")
                            .size(14)
                            .center()
                            .width(140)
                            .color(iced::Color::WHITE),
                    )
                    .on_press(Msg::PlayMovie)
                    .padding([10, 20])
                    .style(accent_btn(p)),
                    Space::with_width(8),
                    button(
                        text("⬇  Download")
                            .size(14)
                            .center()
                            .width(140)
                            .color(p.text),
                    )
                    .on_press(Msg::DlMovie)
                    .padding([10, 20])
                    .style(ghost_btn(p)),
                ]
                .align_y(Alignment::Center)
                .padding([14, 24]),
            ),
        ])
        .height(Fill)
        .into();
    }

    let toolbar = container(row![back].align_y(Alignment::Center).padding([10, 24]));

    let tabs: Element<'a, Msg> = if seasons.is_empty() && loading {
        container(text("Loading seasons...").size(13).color(p.text3))
            .padding([10, 24])
            .into()
    } else {
        let mut r = row![].spacing(6);
        for s in seasons {
            let is_sel = sel_season == Some(s.number);
            let label = format!("Season {}", s.number);
            let b = button(text(label).size(12).center().color(if is_sel {
                iced::Color::WHITE
            } else {
                p.text2
            }))
            .padding([7, 14]);
            let styled = if is_sel {
                b.style(accent_btn(p))
            } else {
                b.on_press(Msg::SelectSeason(s.number)).style(ghost_btn(p))
            };
            r = r.push(styled);
        }
        container(scrollable(r).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        )))
        .padding([6, 24])
        .into()
    };

    let eps: Element<'a, Msg> = if episodes.is_empty() && loading {
        container(text("Loading episodes...").size(13).color(p.text3))
            .padding([10, 24])
            .into()
    } else if episodes.is_empty() {
        container(text("Select a season above").size(14).color(p.text3))
            .width(Fill)
            .center_x(Fill)
            .padding([30, 24])
            .into()
    } else {
        let mut col = column![].spacing(3).padding([4, 24]);
        for ep in episodes {
            col = col.push(episode_row(p, ep, sel_season.unwrap_or(1)));
        }
        col = col.push(Space::with_height(16));
        scrollable(col).height(Fill).into()
    };

    column![hero, toolbar, tabs, eps].into()
}

fn episode_row(p: Pal, ep: &Episode, season: u32) -> Element<'_, Msg> {
    let dur = ep.duration.map(|d| format!("{d} min")).unwrap_or_default();
    let n = ep.number;

    let num = text(format!("{n}"))
        .size(18)
        .color(p.text3)
        .center()
        .width(30);

    let info = column![
        text(ep.name.clone()).size(13).color(p.text),
        text(dur).size(11).color(p.text3),
    ]
    .spacing(2);

    let play = button(text("▶").size(12).center().color(iced::Color::WHITE))
        .on_press(Msg::PlayEpisode(season, n))
        .padding([7, 14])
        .style(accent_btn(p));

    let dl = button(text("⬇").size(12).center().color(p.text2))
        .on_press(Msg::DlEpisode(season, n))
        .padding([7, 10])
        .style(ghost_btn(p));

    button(
        row![
            num,
            Space::with_width(12),
            info.width(Fill),
            play,
            Space::with_width(4),
            dl,
        ]
        .align_y(Alignment::Center)
        .padding([8, 12]),
    )
    .on_press(Msg::PlayEpisode(season, n))
    .width(Fill)
    .style(move |_: &Theme, status| {
        let c = match status {
            button::Status::Hovered => p.surface2,
            _ => p.surface,
        };
        button::Style {
            background: Some(iced::Background::Color(c)),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .into()
}

pub fn player_view<'a>(p: Pal, playing: bool, title: &'a str) -> Element<'a, Msg> {
    let (st, sc) = if playing {
        ("Playing", p.success)
    } else {
        ("Paused", p.warn)
    };

    let dot = container(Space::new(8, 8)).style(move |_: &_| container::Style {
        background: Some(iced::Background::Color(sc)),
        border: iced::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let card = container(
        column![
            row![
                text("NOW PLAYING").size(10).color(p.text3),
                Space::with_width(Fill),
                row![dot, Space::with_width(6), text(st).size(11).color(sc)]
                    .align_y(Alignment::Center),
            ]
            .width(Fill),
            Space::with_height(14),
            text(title).size(22).color(p.text),
            Space::with_height(10),
            text("Use the external player window for video controls")
                .size(12)
                .color(p.text3),
        ]
        .padding(20),
    )
    .width(Fill)
    .style(card_style(p));

    let toggle = if playing {
        button(
            text("⏸  Pause")
                .size(14)
                .center()
                .width(140)
                .color(iced::Color::WHITE),
        )
        .on_press(Msg::Pause)
        .padding([10, 20])
        .style(accent_btn(p))
    } else {
        button(
            text("▶  Resume")
                .size(14)
                .center()
                .width(140)
                .color(iced::Color::WHITE),
        )
        .on_press(Msg::Resume)
        .padding([10, 20])
        .style(accent_btn(p))
    };
    let stop = button(text("■  Stop").size(14).center().width(140).color(p.text))
        .on_press(Msg::Stop)
        .padding([10, 20])
        .style(ghost_btn(p));

    container(
        column![
            Space::with_height(40),
            card,
            Space::with_height(20),
            container(row![toggle, Space::with_width(10), stop])
                .width(Fill)
                .center_x(Fill),
        ]
        .padding(20),
    )
    .width(Fill)
    .height(Fill)
    .into()
}

pub fn downloads_view(p: Pal, downloads: &[DownloadProgress]) -> Element<'_, Msg> {
    let header = container(
        row![
            text("Downloads").size(18).color(p.text),
            Space::with_width(Fill),
            text(format!(
                "{} item{}",
                downloads.len(),
                if downloads.len() == 1 { "" } else { "s" }
            ))
            .size(12)
            .color(p.text3),
        ]
        .align_y(Alignment::Center),
    )
    .padding([14, 20]);

    let body: Element<'_, Msg> = if downloads.is_empty() {
        container(
            column![
                text("No downloads yet").size(16).color(p.text2),
                Space::with_height(6),
                text("Start downloading to see progress here")
                    .size(13)
                    .color(p.text3),
            ]
            .align_x(Alignment::Center),
        )
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .into()
    } else {
        let mut col = column![].spacing(6).padding([4, 20]);
        for dl in downloads {
            col = col.push(dl_card(p, dl));
        }
        scrollable(col).height(Fill).into()
    };

    column![header, body].into()
}

fn dl_card(p: Pal, pr: &DownloadProgress) -> Element<'_, Msg> {
    let (st, sc) = match &pr.status {
        DownloadStatus::Queued => ("Queued", p.text3),
        DownloadStatus::Downloading => ("Downloading", p.accent),
        DownloadStatus::Muxing => ("Muxing", p.warn),
        DownloadStatus::Completed => ("Completed", p.success),
        DownloadStatus::Failed(ref e) => (e.as_str(), p.danger),
    };
    let speed = if pr.speed.is_empty() {
        String::new()
    } else {
        format!("{}  ·  {} / {}", pr.speed, pr.downloaded, pr.total)
    };

    container(
        column![
            row![
                text(&pr.title).size(13).color(p.text).width(Fill),
                text(st).size(11).color(sc),
            ]
            .align_y(Alignment::Center),
            Space::with_height(8),
            progress_bar(0.0..=100.0, pr.percent as f32).height(3),
            Space::with_height(5),
            row![
                text(format!("{:.1}%", pr.percent)).size(11).color(p.text2),
                Space::with_width(Fill),
                text(speed).size(11).color(p.text3),
            ],
        ]
        .spacing(0)
        .padding(14),
    )
    .width(Fill)
    .style(card_style(p))
    .into()
}

pub fn settings_view(p: Pal, cfg: &AppConfig) -> Element<'_, Msg> {
    let appearance = section(
        p,
        "Appearance",
        column![toggle_row(
            p,
            "Dark Mode",
            cfg.dark_mode,
            Msg::CfgDarkMode as fn(bool) -> Msg
        )]
        .spacing(8),
    );
    let output = section(
        p,
        "Output",
        column![
            input_row(
                p,
                "Download Path",
                cfg.output.root_path.clone(),
                Msg::CfgRootPath
            ),
            input_row(
                p,
                "Movie Folder",
                cfg.output.movie_folder_name.clone(),
                Msg::CfgMovieFolder
            ),
            input_row(
                p,
                "Series Folder",
                cfg.output.serie_folder_name.clone(),
                Msg::CfgSerieFolder
            ),
            input_row(
                p,
                "Episode Format",
                cfg.output.map_episode_name.clone(),
                Msg::CfgEpFormat
            ),
        ]
        .spacing(8),
    );
    let download = section(
        p,
        "Download",
        column![
            input_row(
                p,
                "Threads",
                cfg.download.thread_count.to_string(),
                Msg::CfgThreads
            ),
            input_row(
                p,
                "Retry Count",
                cfg.download.retry_count.to_string(),
                Msg::CfgRetry
            ),
            input_row(
                p,
                "Video Select",
                cfg.download.select_video.clone(),
                Msg::CfgSelVideo
            ),
            input_row(
                p,
                "Audio Select",
                cfg.download.select_audio.clone(),
                Msg::CfgSelAudio
            ),
            input_row(
                p,
                "Subtitle Select",
                cfg.download.select_subtitle.clone(),
                Msg::CfgSelSub
            ),
            input_row(
                p,
                "Max Speed",
                cfg.download.max_speed.clone(),
                Msg::CfgMaxSpeed
            ),
            toggle_row(
                p,
                "Concurrent DL",
                cfg.download.concurrent_download,
                Msg::CfgConcurrent as fn(bool) -> Msg
            ),
        ]
        .spacing(8),
    );
    let process = section(
        p,
        "Processing",
        column![
            input_row(
                p,
                "Extension",
                cfg.process.extension.clone(),
                Msg::CfgExtension
            ),
            toggle_row(
                p,
                "Merge Audio",
                cfg.process.merge_audio,
                Msg::CfgMergeAudio as fn(bool) -> Msg
            ),
            toggle_row(
                p,
                "Merge Subtitles",
                cfg.process.merge_subtitle,
                Msg::CfgMergeSub as fn(bool) -> Msg
            ),
            toggle_row(
                p,
                "Use GPU",
                cfg.process.use_gpu,
                Msg::CfgGpu as fn(bool) -> Msg
            ),
        ]
        .spacing(8),
    );
    let network = section(
        p,
        "Network",
        column![
            input_row(
                p,
                "Timeout (s)",
                cfg.requests.timeout.to_string(),
                Msg::CfgTimeout
            ),
            toggle_row(
                p,
                "Use Proxy",
                cfg.requests.use_proxy,
                Msg::CfgProxy as fn(bool) -> Msg
            ),
            input_row(
                p,
                "Proxy URL",
                cfg.requests.proxy_url.clone(),
                Msg::CfgProxyUrl
            ),
        ]
        .spacing(8),
    );

    scrollable(
        column![
            text("Settings").size(18).color(p.text),
            Space::with_height(16),
            appearance,
            Space::with_height(8),
            output,
            Space::with_height(8),
            download,
            Space::with_height(8),
            process,
            Space::with_height(8),
            network,
            Space::with_height(16),
            button(
                text("Save Settings")
                    .size(14)
                    .center()
                    .width(180)
                    .color(iced::Color::WHITE),
            )
            .on_press(Msg::CfgSave)
            .padding([10, 24])
            .style(accent_btn(p)),
            Space::with_height(20),
        ]
        .padding(20),
    )
    .height(Fill)
    .into()
}

fn section<'a>(p: Pal, title: &str, content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    let hdr = container(text(title.to_string()).size(12).color(p.text3))
        .width(Fill)
        .padding([8, 14])
        .style(move |_: &_| container::Style {
            background: Some(iced::Background::Color(p.surface2)),
            border: iced::Border {
                radius: iced::border::Radius {
                    top_left: 4.0,
                    top_right: 4.0,
                    bottom_left: 0.0,
                    bottom_right: 0.0,
                },
                ..Default::default()
            },
            ..Default::default()
        });
    container(column![hdr, container(content.into()).padding(14)])
        .width(Fill)
        .style(card_style(p))
        .into()
}

fn input_row<F: Fn(String) -> Msg + 'static>(
    p: Pal,
    label: &str,
    val: String,
    on_change: F,
) -> Element<'static, Msg> {
    row![
        text(label.to_string()).size(13).color(p.text2).width(140),
        text_input("", &val)
            .on_input(on_change)
            .padding(6)
            .size(13)
            .width(Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(10)
    .into()
}

fn toggle_row(p: Pal, label: &str, val: bool, on_toggle: fn(bool) -> Msg) -> Element<'static, Msg> {
    row![
        text(label.to_string()).size(13).color(p.text2).width(140),
        toggler(val).on_toggle(on_toggle),
    ]
    .align_y(Alignment::Center)
    .spacing(10)
    .into()
}
