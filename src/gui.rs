use crate::config::AppConfig;
use crate::providers::{Episode, MediaEntry, Season, StreamUrl};
use crate::util::{DownloadProgress, DownloadStatus};
use iced::color;
use iced::widget::{
    button, column, container, progress_bar, row, scrollable, text, text_input, toggler, Space,
};
use iced::{Alignment, Element, Fill, Theme};

pub const BG_DARK: iced::Color = color!(0x1a, 0x1a, 0x2e);
pub const BG_CARD: iced::Color = color!(0x16, 0x21, 0x3e);
pub const BG_SIDEBAR: iced::Color = color!(0x0f, 0x0f, 0x23);
pub const BORDER_CARD: iced::Color = color!(0x2a, 0x2a, 0x4a);
pub const ACCENT: iced::Color = color!(0x15, 0x52, 0xab);
pub const TEXT_PRI: iced::Color = color!(0xe0, 0xe0, 0xe0);
pub const TEXT_SEC: iced::Color = color!(0x8a, 0x8a, 0x9a);
pub const SUCCESS: iced::Color = color!(0x2e, 0xcc, 0x71);
pub const DANGER: iced::Color = color!(0xe7, 0x4c, 0x3c);
pub const WARNING: iced::Color = color!(0xf3, 0x9c, 0x12);
pub const SIDEBAR_W: u16 = 160;
pub const CARD_RADIUS: f32 = 8.0;

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
    PlayEntry(usize),
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

#[inline]
pub fn card_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(BG_CARD)),
        border: iced::Border {
            color: BORDER_CARD,
            width: 1.0,
            radius: CARD_RADIUS.into(),
        },
        ..Default::default()
    }
}

#[inline]
pub fn sidebar_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(BG_SIDEBAR)),
        ..Default::default()
    }
}

#[inline]
pub fn nav_button(label: &str, active: bool, msg: Msg) -> Element<'_, Msg> {
    let c = if active { TEXT_PRI } else { TEXT_SEC };
    button(text(label).size(14).color(c).width(Fill))
        .on_press(msg)
        .padding([10, 16])
        .width(Fill)
        .into()
}

pub fn home_view(online: bool) -> Element<'static, Msg> {
    let (sc, st) = if online {
        (SUCCESS, "Online")
    } else {
        (DANGER, "Offline")
    };
    let card = container(
        column![
            text("StreamingCommunity").size(18).color(TEXT_PRI),
            Space::with_height(8),
            row![
                container(Space::new(10, 10)).style(move |_: &_| container::Style {
                    background: Some(iced::Background::Color(sc)),
                    border: iced::Border {
                        radius: 5.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                Space::with_width(8),
                text(st).size(14).color(TEXT_SEC),
            ]
            .align_y(Alignment::Center),
            Space::with_height(16),
            button(text("Search").center().width(Fill))
                .width(Fill)
                .on_press(Msg::NavSearch),
        ]
        .width(280)
        .padding(20),
    )
    .style(card_style);

    container(
        column![
            Space::with_height(60),
            text("StreamVault").size(36).color(TEXT_PRI),
            Space::with_height(4),
            text("Stream, Download, Watch").size(16).color(TEXT_SEC),
            Space::with_height(40),
            text("Providers").size(20).color(TEXT_PRI),
            Space::with_height(16),
            card,
        ]
        .align_x(Alignment::Center)
        .padding(40),
    )
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .into()
}

pub fn search_view<'a>(
    query: &'a str,
    results: &'a [MediaEntry],
    loading: bool,
) -> Element<'a, Msg> {
    let bar = row![
        text_input("Search movies and series...", query)
            .on_input(Msg::SearchInput)
            .on_submit(Msg::SearchSubmit)
            .padding(12)
            .size(16)
            .width(Fill),
        Space::with_width(10),
        button(
            text(if loading { "Searching..." } else { "Search" })
                .center()
                .width(100)
        )
        .on_press_maybe(if loading {
            None
        } else {
            Some(Msg::SearchSubmit)
        })
        .padding(12),
    ]
    .align_y(Alignment::Center)
    .padding(20);

    let body: Element<'a, Msg> = if results.is_empty() && !loading {
        container(
            text(if query.is_empty() {
                "Type to search for movies and series"
            } else {
                "No results found"
            })
            .size(16)
            .color(TEXT_SEC),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(40)
        .into()
    } else {
        let mut col = column![].spacing(8).padding(20);
        for (i, entry) in results.iter().enumerate() {
            col = col.push(result_card(i, entry));
        }
        scrollable(col).height(Fill).into()
    };
    column![bar, body].into()
}

fn result_card<'a>(idx: usize, entry: &'a MediaEntry) -> Element<'a, Msg> {
    let (lbl, clr) = if entry.is_movie() {
        ("Movie", ACCENT)
    } else {
        ("Series", WARNING)
    };
    let yr = entry.year_display().to_string();
    let info = column![
        text(entry.name.clone()).size(16).color(TEXT_PRI),
        Space::with_height(4),
        row![
            container(text(lbl).size(11).color(TEXT_PRI))
                .padding([2, 8])
                .style(move |_: &_| container::Style {
                    background: Some(iced::Background::Color(clr)),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            Space::with_width(8),
            text(yr).size(13).color(TEXT_SEC),
        ]
        .align_y(Alignment::Center),
    ];
    let actions = row![
        button(text("Details").size(13).center().width(70))
            .on_press(Msg::SelectEntry(idx))
            .padding(6),
        Space::with_width(6),
        button(text("Play").size(13).center().width(50))
            .on_press(Msg::PlayEntry(idx))
            .padding(6),
    ];
    container(
        row![info.width(Fill), actions]
            .align_y(Alignment::Center)
            .padding(12)
            .spacing(10),
    )
    .width(Fill)
    .style(card_style)
    .into()
}

pub fn details_view<'a>(
    entry: &'a MediaEntry,
    seasons: &'a [Season],
    episodes: &'a [Episode],
    sel_season: Option<u32>,
    loading: bool,
) -> Element<'a, Msg> {
    let header = column![
        row![
            button(text("Back").size(14))
                .on_press(Msg::NavSearch)
                .padding(8),
            Space::with_width(16),
            text(entry.name.clone()).size(24).color(TEXT_PRI),
        ]
        .align_y(Alignment::Center),
        Space::with_height(4),
        row![
            text(if entry.is_movie() { "Movie" } else { "Series" })
                .size(14)
                .color(ACCENT),
            Space::with_width(12),
            text(entry.year_display().to_string())
                .size(14)
                .color(TEXT_SEC),
        ],
    ]
    .padding(20);

    if entry.is_movie() {
        return column![
            header,
            container(
                column![
                    Space::with_height(20),
                    row![
                        button(text("Play").center().width(120))
                            .on_press(Msg::PlayMovie)
                            .padding(12),
                        Space::with_width(12),
                        button(text("Download").center().width(120))
                            .on_press(Msg::DlMovie)
                            .padding(12),
                    ],
                ]
                .padding(20)
            )
        ]
        .into();
    }

    let tabs: Element<'a, Msg> = if seasons.is_empty() && loading {
        container(text("Loading seasons...").size(14).color(TEXT_SEC))
            .padding(20)
            .into()
    } else {
        let mut r = row![].spacing(6).padding(20);
        for s in seasons {
            let is_sel = sel_season == Some(s.number);
            let b = button(
                text(format!("S{:02}", s.number))
                    .size(13)
                    .center()
                    .width(50),
            )
            .padding(8);
            r = r.push(if is_sel {
                b
            } else {
                b.on_press(Msg::SelectSeason(s.number))
            });
        }
        scrollable(r)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .into()
    };

    let eps: Element<'a, Msg> = if episodes.is_empty() && loading {
        container(text("Loading episodes...").size(14).color(TEXT_SEC))
            .padding(20)
            .into()
    } else if episodes.is_empty() {
        container(text("Select a season").size(14).color(TEXT_SEC))
            .padding(20)
            .into()
    } else {
        let mut col = column![].spacing(6).padding(20);
        for ep in episodes {
            col = col.push(episode_card(ep, sel_season.unwrap_or(1)));
        }
        scrollable(col).height(Fill).into()
    };
    column![header, tabs, eps].into()
}

fn episode_card(ep: &Episode, season: u32) -> Element<'_, Msg> {
    let dur = ep.duration.map(|d| format!("{d} min")).unwrap_or_default();
    let info = column![
        text(format!("E{:02} - {}", ep.number, ep.name))
            .size(14)
            .color(TEXT_PRI),
        text(dur).size(12).color(TEXT_SEC),
    ];
    let actions = row![
        button(text("Play").size(12).center().width(50))
            .on_press(Msg::PlayEpisode(season, ep.number))
            .padding(5),
        Space::with_width(4),
        button(text("DL").size(12).center().width(40))
            .on_press(Msg::DlEpisode(season, ep.number))
            .padding(5),
    ];
    container(
        row![info.width(Fill), actions]
            .align_y(Alignment::Center)
            .padding(10),
    )
    .width(Fill)
    .style(card_style)
    .into()
}

pub fn player_view<'a>(playing: bool, title: &'a str) -> Element<'a, Msg> {
    let header = row![
        button(text("Back").size(14)).on_press(Msg::Stop).padding(8),
        Space::with_width(16),
        text(title).size(20).color(TEXT_PRI),
    ]
    .align_y(Alignment::Center)
    .padding(20);

    let (st, sc) = if playing {
        ("Playing", SUCCESS)
    } else {
        ("Paused", WARNING)
    };
    let status = container(
        column![
            text("Now Playing").size(14).color(TEXT_SEC),
            Space::with_height(8),
            text(title).size(18).color(TEXT_PRI),
            Space::with_height(4),
            text(st).size(14).color(sc),
            Space::with_height(8),
            text("Use the video player window controls for playback")
                .size(12)
                .color(TEXT_SEC),
        ]
        .padding(20),
    )
    .width(Fill)
    .style(card_style);

    let toggle = if playing {
        button(text("Pause").center().width(120))
            .on_press(Msg::Pause)
            .padding(10)
    } else {
        button(text("Resume").center().width(120))
            .on_press(Msg::Resume)
            .padding(10)
    };
    let ctrls = row![
        toggle,
        Space::with_width(10),
        button(text("Stop & Close").center().width(120))
            .on_press(Msg::Stop)
            .padding(10)
    ]
    .align_y(Alignment::Center);

    container(
        column![
            header,
            Space::with_height(40),
            status,
            Space::with_height(30),
            container(ctrls).width(Fill).center_x(Fill)
        ]
        .padding(20),
    )
    .width(Fill)
    .height(Fill)
    .into()
}

pub fn downloads_view(downloads: &[DownloadProgress]) -> Element<'_, Msg> {
    let header = column![
        text("Downloads").size(24).color(TEXT_PRI),
        Space::with_height(4),
        text(format!("{} items", downloads.len()))
            .size(14)
            .color(TEXT_SEC),
    ]
    .padding(20);

    let body: Element<'_, Msg> = if downloads.is_empty() {
        container(text("No downloads yet").size(16).color(TEXT_SEC))
            .width(Fill)
            .center_x(Fill)
            .padding(60)
            .into()
    } else {
        let mut col = column![].spacing(8).padding(20);
        for dl in downloads {
            col = col.push(dl_card(dl));
        }
        scrollable(col).height(Fill).into()
    };
    column![header, body].into()
}

fn dl_card(p: &DownloadProgress) -> Element<'_, Msg> {
    let (st, sc) = match &p.status {
        DownloadStatus::Queued => ("Queued", TEXT_SEC),
        DownloadStatus::Downloading => ("Downloading", ACCENT),
        DownloadStatus::Muxing => ("Muxing", WARNING),
        DownloadStatus::Completed => ("Completed", SUCCESS),
        DownloadStatus::Failed(ref e) => (e.as_str(), DANGER),
    };
    let speed = if p.speed.is_empty() {
        String::new()
    } else {
        format!(" | {} | {}/{}", p.speed, p.downloaded, p.total)
    };
    container(
        column![
            row![
                text(&p.title).size(14).color(TEXT_PRI).width(Fill),
                text(st).size(12).color(sc)
            ]
            .align_y(Alignment::Center),
            Space::with_height(8),
            progress_bar(0.0..=100.0, p.percent as f32).height(6),
            Space::with_height(4),
            text(format!("{:.1}%{speed}", p.percent))
                .size(11)
                .color(TEXT_SEC),
        ]
        .spacing(2)
        .padding(12),
    )
    .width(Fill)
    .style(card_style)
    .into()
}

pub fn settings_view(cfg: &AppConfig) -> Element<'_, Msg> {
    let output = section(
        "Output",
        column![
            field(
                "Download Path",
                cfg.output.root_path.clone(),
                Msg::CfgRootPath
            ),
            field(
                "Movie Folder",
                cfg.output.movie_folder_name.clone(),
                Msg::CfgMovieFolder
            ),
            field(
                "Series Folder",
                cfg.output.serie_folder_name.clone(),
                Msg::CfgSerieFolder
            ),
            field(
                "Episode Format",
                cfg.output.map_episode_name.clone(),
                Msg::CfgEpFormat
            ),
        ]
        .spacing(8),
    );

    let download = section(
        "Download",
        column![
            field(
                "Threads",
                cfg.download.thread_count.to_string(),
                Msg::CfgThreads
            ),
            field(
                "Retry Count",
                cfg.download.retry_count.to_string(),
                Msg::CfgRetry
            ),
            field(
                "Video Select",
                cfg.download.select_video.clone(),
                Msg::CfgSelVideo
            ),
            field(
                "Audio Select",
                cfg.download.select_audio.clone(),
                Msg::CfgSelAudio
            ),
            field(
                "Subtitle Select",
                cfg.download.select_subtitle.clone(),
                Msg::CfgSelSub
            ),
            field(
                "Max Speed",
                cfg.download.max_speed.clone(),
                Msg::CfgMaxSpeed
            ),
            toggle(
                "Concurrent Download",
                cfg.download.concurrent_download,
                Msg::CfgConcurrent as fn(bool) -> Msg
            ),
        ]
        .spacing(8),
    );

    let process = section(
        "Processing",
        column![
            field(
                "Output Extension",
                cfg.process.extension.clone(),
                Msg::CfgExtension
            ),
            toggle(
                "Merge Audio",
                cfg.process.merge_audio,
                Msg::CfgMergeAudio as fn(bool) -> Msg
            ),
            toggle(
                "Merge Subtitles",
                cfg.process.merge_subtitle,
                Msg::CfgMergeSub as fn(bool) -> Msg
            ),
            toggle(
                "Use GPU",
                cfg.process.use_gpu,
                Msg::CfgGpu as fn(bool) -> Msg
            ),
        ]
        .spacing(8),
    );

    let network = section(
        "Network",
        column![
            field(
                "Timeout (s)",
                cfg.requests.timeout.to_string(),
                Msg::CfgTimeout
            ),
            toggle(
                "Use Proxy",
                cfg.requests.use_proxy,
                Msg::CfgProxy as fn(bool) -> Msg
            ),
            field(
                "Proxy URL",
                cfg.requests.proxy_url.clone(),
                Msg::CfgProxyUrl
            ),
        ]
        .spacing(8),
    );

    scrollable(
        column![
            text("Settings").size(24).color(TEXT_PRI),
            Space::with_height(20),
            output,
            Space::with_height(16),
            download,
            Space::with_height(16),
            process,
            Space::with_height(16),
            network,
            Space::with_height(20),
            button(text("Save Settings").center().width(150))
                .on_press(Msg::CfgSave)
                .padding(12),
            Space::with_height(20),
        ]
        .padding(20),
    )
    .height(Fill)
    .into()
}

fn section<'a>(title: &str, content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(
        column![
            text(title.to_string()).size(18).color(TEXT_PRI),
            Space::with_height(12),
            content.into()
        ]
        .padding(16),
    )
    .width(Fill)
    .style(card_style)
    .into()
}

fn field<F: Fn(String) -> Msg + 'static>(
    label: &str,
    val: String,
    on_change: F,
) -> Element<'static, Msg> {
    row![
        text(label.to_string()).size(14).color(TEXT_SEC).width(150),
        text_input("", &val)
            .on_input(on_change)
            .padding(8)
            .size(14)
            .width(Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(12)
    .into()
}

fn toggle(label: &str, val: bool, on_toggle: fn(bool) -> Msg) -> Element<'static, Msg> {
    row![
        text(label.to_string()).size(14).color(TEXT_SEC).width(150),
        toggler(val).on_toggle(on_toggle),
    ]
    .align_y(Alignment::Center)
    .spacing(12)
    .into()
}
