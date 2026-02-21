use crate::config::AppConfig;
use crate::download::{DownloadEngine, DownloadProgress, DownloadRequest};
use crate::gui::screens;
use crate::gui::style;
use crate::playback::{PlaybackEngine, PlaybackState};
use crate::provider::{Episode, MediaEntry, Provider, Season, StreamUrl, StreamingCommunityProvider};
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Fill, Subscription, Task as IcedTask, Theme};
use std::sync::Arc;
use tokio::sync::mpsc;

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
pub enum Message {
    NavigateHome,
    NavigateSearch,
    NavigateDownloads,
    NavigateSettings,

    SearchInputChanged(String),
    SearchSubmit,
    SearchCompleted(Result<Vec<MediaEntry>, String>),

    SelectEntry(usize),
    SeasonsLoaded(Result<Vec<Season>, String>),
    SelectSeason(u32),
    EpisodesLoaded(Result<Vec<Episode>, String>),

    PlayEntry(usize),
    PlayMovie,
    PlayEpisode(u32, u32),
    StreamUrlResolved(Result<(StreamUrl, String), String>),
    PlaybackStarted(Result<(), String>),

    PlayerPause,
    PlayerResume,
    PlayerStop,
    PlayerSeekForward,
    PlayerSeekBackward,
    PlayerVolumeUp,
    PlayerVolumeDown,
    PlayerCommandDone(Result<(), String>),

    DismissError,

    DownloadMovie,
    DownloadEpisode(u32, u32),
    DownloadStreamResolved(Result<(StreamUrl, String, bool), String>),
    DownloadProgressUpdate(DownloadProgress),

    ProviderStatusChecked(bool),

    SettingsUpdateRootPath(String),
    SettingsUpdateMovieFolder(String),
    SettingsUpdateSerieFolder(String),
    SettingsUpdateEpisodeFormat(String),
    SettingsUpdateThreadCount(String),
    SettingsUpdateRetryCount(String),
    SettingsUpdateSelectVideo(String),
    SettingsUpdateSelectAudio(String),
    SettingsUpdateSelectSubtitle(String),
    SettingsUpdateMaxSpeed(String),
    SettingsUpdateExtension(String),
    SettingsUpdateTimeout(String),
    SettingsUpdateProxyUrl(String),
    SettingsToggleConcurrent(bool),
    SettingsToggleMergeAudio(bool),
    SettingsToggleMergeSubtitle(bool),
    SettingsToggleGpu(bool),
    SettingsToggleProxy(bool),
    SettingsSave,

    Tick,
}

pub struct App {
    screen: Screen,
    config: AppConfig,
    provider: Arc<dyn Provider>,
    provider_online: bool,

    search_query: String,
    search_results: Vec<MediaEntry>,
    is_searching: bool,

    selected_entry: Option<MediaEntry>,
    seasons: Vec<Season>,
    episodes: Vec<Episode>,
    selected_season: Option<u32>,
    is_loading_details: bool,

    playback_state: PlaybackState,
    playing_title: String,
    error_message: Option<String>,

    downloads: Vec<DownloadProgress>,
    download_tx: mpsc::UnboundedSender<DownloadProgress>,
    download_rx: Option<mpsc::UnboundedReceiver<DownloadProgress>>,
}

impl App {
    pub fn new() -> (Self, IcedTask<Message>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let provider: Arc<dyn Provider> = Arc::new(StreamingCommunityProvider::new());

        let app = Self {
            screen: Screen::Home,
            config: AppConfig::load(),
            provider: provider.clone(),
            provider_online: false,
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            selected_entry: None,
            seasons: Vec::new(),
            episodes: Vec::new(),
            selected_season: None,
            is_loading_details: false,
            playback_state: PlaybackState::Stopped,
            playing_title: String::new(),
            error_message: None,
            downloads: Vec::new(),
            download_tx: tx,
            download_rx: Some(rx),
        };

        let check_task = IcedTask::perform(check_provider(provider), Message::ProviderStatusChecked);

        (app, check_task)
    }

    pub fn theme(&self) -> Theme {
        Theme::CatppuccinMocha
    }

    pub fn subscription(&self) -> Subscription<Message> {
        iced::time::every(std::time::Duration::from_millis(500)).map(|_| Message::Tick)
    }

    pub fn update(&mut self, message: Message) -> IcedTask<Message> {
        match message {
            Message::Tick => {
                self.drain_download_progress();
                IcedTask::none()
            }

            Message::NavigateHome => {
                self.screen = Screen::Home;
                IcedTask::none()
            }
            Message::NavigateSearch => {
                self.screen = Screen::Search;
                IcedTask::none()
            }
            Message::NavigateDownloads => {
                self.screen = Screen::Downloads;
                IcedTask::none()
            }
            Message::NavigateSettings => {
                self.screen = Screen::Settings;
                IcedTask::none()
            }

            Message::SearchInputChanged(query) => {
                self.search_query = query;
                IcedTask::none()
            }
            Message::SearchSubmit => {
                if self.search_query.trim().is_empty() || self.is_searching {
                    return IcedTask::none();
                }
                self.is_searching = true;
                let provider = self.provider.clone();
                let query = self.search_query.clone();
                IcedTask::perform(
                    async move {
                        provider
                            .search(&query)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::SearchCompleted,
                )
            }
            Message::SearchCompleted(result) => {
                self.is_searching = false;
                match result {
                    Ok(entries) => self.search_results = entries,
                    Err(_) => self.search_results.clear(),
                }
                IcedTask::none()
            }

            Message::SelectEntry(index) => {
                if let Some(entry) = self.search_results.get(index).cloned() {
                    self.selected_entry = Some(entry.clone());
                    self.screen = Screen::Details;
                    self.seasons.clear();
                    self.episodes.clear();
                    self.selected_season = None;
                    self.is_loading_details = true;

                    if entry.is_movie() {
                        self.is_loading_details = false;
                        return IcedTask::none();
                    }

                    let provider = self.provider.clone();
                    IcedTask::perform(
                        async move {
                            provider
                                .get_seasons(&entry)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        Message::SeasonsLoaded,
                    )
                } else {
                    IcedTask::none()
                }
            }
            Message::SeasonsLoaded(result) => {
                self.is_loading_details = false;
                if let Ok(seasons) = result {
                    self.seasons = seasons;
                    if let Some(first) = self.seasons.first() {
                        let num = first.number;
                        return self.update(Message::SelectSeason(num));
                    }
                }
                IcedTask::none()
            }
            Message::SelectSeason(number) => {
                self.selected_season = Some(number);
                self.episodes.clear();
                self.is_loading_details = true;

                if let Some(entry) = self.selected_entry.clone() {
                    let provider = self.provider.clone();
                    IcedTask::perform(
                        async move {
                            provider
                                .get_episodes(&entry, number)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        Message::EpisodesLoaded,
                    )
                } else {
                    IcedTask::none()
                }
            }
            Message::EpisodesLoaded(result) => {
                self.is_loading_details = false;
                if let Ok(eps) = result {
                    self.episodes = eps;
                }
                IcedTask::none()
            }

            Message::PlayEntry(index) => {
                if let Some(entry) = self.search_results.get(index).cloned() {
                    let title = entry.display_title();
                    let provider = self.provider.clone();
                    IcedTask::perform(
                        async move {
                            let stream = provider
                                .get_stream_url(&entry, None, None)
                                .await
                                .map_err(|e| e.to_string())?;
                            Ok((stream, title))
                        },
                        Message::StreamUrlResolved,
                    )
                } else {
                    IcedTask::none()
                }
            }
            Message::PlayMovie => {
                if let Some(entry) = self.selected_entry.clone() {
                    self.error_message = None;
                    let title = entry.display_title();
                    let provider = self.provider.clone();
                    eprintln!("[StreamVault] PlayMovie: resolving stream for '{title}'");
                    IcedTask::perform(
                        async move {
                            let stream = provider
                                .get_stream_url(&entry, None, None)
                                .await
                                .map_err(|e| e.to_string())?;
                            Ok((stream, title))
                        },
                        Message::StreamUrlResolved,
                    )
                } else {
                    IcedTask::none()
                }
            }
            Message::PlayEpisode(season, ep_num) => {
                if let Some(entry) = self.selected_entry.clone() {
                    self.error_message = None;
                    let episode = self.episodes.iter().find(|e| e.number == ep_num).cloned();
                    let title = format!(
                        "{} S{:02}E{:02}",
                        entry.name, season, ep_num
                    );
                    let provider = self.provider.clone();
                    IcedTask::perform(
                        async move {
                            let stream = provider
                                .get_stream_url(&entry, episode.as_ref(), Some(season))
                                .await
                                .map_err(|e| e.to_string())?;
                            Ok((stream, title))
                        },
                        Message::StreamUrlResolved,
                    )
                } else {
                    IcedTask::none()
                }
            }
            Message::StreamUrlResolved(result) => match result {
                Ok((stream, title)) => {
                    self.playing_title = title;
                    self.screen = Screen::Player;
                    self.playback_state = PlaybackState::Stopped;
                    self.error_message = None;
                    let url = stream.url.clone();
                    eprintln!("[StreamVault] Launching mpv with URL: {url}");
                    IcedTask::perform(
                        async move {
                            let mut engine = PlaybackEngine::new();
                            engine.play(&url).await.map_err(|e| e.to_string())?;
                            std::mem::forget(engine);
                            Ok(())
                        },
                        Message::PlaybackStarted,
                    )
                }
                Err(e) => {
                    eprintln!("[StreamVault] Stream URL error: {e}");
                    self.error_message = Some(format!("Failed to get stream: {e}"));
                    IcedTask::none()
                }
            },

            Message::PlaybackStarted(result) => match result {
                Ok(()) => {
                    eprintln!("[StreamVault] mpv launched successfully");
                    self.playback_state =
                        PlaybackState::Playing(self.playing_title.clone());
                    IcedTask::none()
                }
                Err(e) => {
                    eprintln!("[StreamVault] mpv launch error: {e}");
                    self.error_message = Some(format!("Playback error: {e}"));
                    self.playback_state = PlaybackState::Stopped;
                    IcedTask::none()
                }
            },

            Message::PlayerPause => {
                self.playback_state =
                    PlaybackState::Paused(self.playing_title.clone());
                IcedTask::perform(
                    async {
                        PlaybackEngine::send_ipc_static(
                            "\"set_property\", \"pause\", true",
                        )
                        .await
                    },
                    Message::PlayerCommandDone,
                )
            }
            Message::PlayerResume => {
                self.playback_state =
                    PlaybackState::Playing(self.playing_title.clone());
                IcedTask::perform(
                    async {
                        PlaybackEngine::send_ipc_static(
                            "\"set_property\", \"pause\", false",
                        )
                        .await
                    },
                    Message::PlayerCommandDone,
                )
            }
            Message::PlayerStop => {
                self.playback_state = PlaybackState::Stopped;
                self.playing_title.clear();
                IcedTask::perform(
                    async { PlaybackEngine::send_ipc_static("\"quit\"").await },
                    Message::PlayerCommandDone,
                )
            }
            Message::PlayerSeekForward => IcedTask::perform(
                async { PlaybackEngine::send_ipc_static("\"seek\", 10").await },
                Message::PlayerCommandDone,
            ),
            Message::PlayerSeekBackward => IcedTask::perform(
                async {
                    PlaybackEngine::send_ipc_static("\"seek\", -10").await
                },
                Message::PlayerCommandDone,
            ),
            Message::PlayerVolumeUp => IcedTask::perform(
                async {
                    PlaybackEngine::send_ipc_static("\"add\", \"volume\", 5")
                        .await
                },
                Message::PlayerCommandDone,
            ),
            Message::PlayerVolumeDown => IcedTask::perform(
                async {
                    PlaybackEngine::send_ipc_static("\"add\", \"volume\", -5")
                        .await
                },
                Message::PlayerCommandDone,
            ),
            Message::PlayerCommandDone(result) => {
                if let Err(e) = result {
                    eprintln!("[StreamVault] Player command error: {e}");
                }
                IcedTask::none()
            }

            Message::DownloadMovie => {
                if let Some(entry) = self.selected_entry.clone() {
                    let provider = self.provider.clone();
                    let title = entry.display_title();
                    IcedTask::perform(
                        async move {
                            let stream = provider
                                .get_stream_url(&entry, None, None)
                                .await
                                .map_err(|e| e.to_string())?;
                            Ok((stream, title, true))
                        },
                        Message::DownloadStreamResolved,
                    )
                } else {
                    IcedTask::none()
                }
            }
            Message::DownloadEpisode(season, ep_num) => {
                if let Some(entry) = self.selected_entry.clone() {
                    let episode = self.episodes.iter().find(|e| e.number == ep_num).cloned();
                    let provider = self.provider.clone();
                    let config = self.config.clone();
                    let show_name = entry.name.clone();
                    IcedTask::perform(
                        async move {
                            let ep_ref = episode.as_ref();
                            let stream = provider
                                .get_stream_url(&entry, ep_ref, Some(season))
                                .await
                                .map_err(|e| e.to_string())?;
                            let engine = DownloadEngine::new(config);
                            let filename = engine.format_episode_name(
                                &show_name,
                                season,
                                ep_num,
                                ep_ref.map(|e| e.name.as_str()).unwrap_or(""),
                            );
                            Ok((stream, filename, false))
                        },
                        Message::DownloadStreamResolved,
                    )
                } else {
                    IcedTask::none()
                }
            }
            Message::DownloadStreamResolved(result) => {
                if let Ok((stream, filename, is_movie)) = result {
                    let id = uuid::Uuid::new_v4();
                    let engine = DownloadEngine::new(self.config.clone());
                    let output_dir = engine.build_output_path(&filename, is_movie);

                    let request = DownloadRequest {
                        id,
                        title: filename.clone(),
                        stream_url: stream.url,
                        output_dir,
                        filename: filename.clone(),
                        headers: stream.headers,
                    };

                    let progress = DownloadProgress::new(id, filename);
                    self.downloads.push(progress);

                    let tx = self.download_tx.clone();
                    self.screen = Screen::Downloads;

                    IcedTask::perform(
                        async move {
                            engine.download(request, tx).await;
                        },
                        |_: ()| Message::Tick,
                    )
                } else {
                    IcedTask::none()
                }
            }
            Message::DownloadProgressUpdate(progress) => {
                if let Some(existing) = self.downloads.iter_mut().find(|d| d.id == progress.id) {
                    *existing = progress;
                }
                IcedTask::none()
            }

            Message::ProviderStatusChecked(online) => {
                self.provider_online = online;
                IcedTask::none()
            }

            Message::SettingsUpdateRootPath(v) => {
                self.config.output.root_path = v;
                IcedTask::none()
            }
            Message::SettingsUpdateMovieFolder(v) => {
                self.config.output.movie_folder_name = v;
                IcedTask::none()
            }
            Message::SettingsUpdateSerieFolder(v) => {
                self.config.output.serie_folder_name = v;
                IcedTask::none()
            }
            Message::SettingsUpdateEpisodeFormat(v) => {
                self.config.output.map_episode_name = v;
                IcedTask::none()
            }
            Message::SettingsUpdateThreadCount(v) => {
                if let Ok(n) = v.parse() {
                    self.config.download.thread_count = n;
                }
                IcedTask::none()
            }
            Message::SettingsUpdateRetryCount(v) => {
                if let Ok(n) = v.parse() {
                    self.config.download.retry_count = n;
                }
                IcedTask::none()
            }
            Message::SettingsUpdateSelectVideo(v) => {
                self.config.download.select_video = v;
                IcedTask::none()
            }
            Message::SettingsUpdateSelectAudio(v) => {
                self.config.download.select_audio = v;
                IcedTask::none()
            }
            Message::SettingsUpdateSelectSubtitle(v) => {
                self.config.download.select_subtitle = v;
                IcedTask::none()
            }
            Message::SettingsUpdateMaxSpeed(v) => {
                self.config.download.max_speed = v;
                IcedTask::none()
            }
            Message::SettingsUpdateExtension(v) => {
                self.config.process.extension = v;
                IcedTask::none()
            }
            Message::SettingsUpdateTimeout(v) => {
                if let Ok(n) = v.parse() {
                    self.config.requests.timeout = n;
                }
                IcedTask::none()
            }
            Message::SettingsUpdateProxyUrl(v) => {
                self.config.requests.proxy_url = v;
                IcedTask::none()
            }
            Message::SettingsToggleConcurrent(val) => {
                self.config.download.concurrent_download = val;
                IcedTask::none()
            }
            Message::SettingsToggleMergeAudio(val) => {
                self.config.process.merge_audio = val;
                IcedTask::none()
            }
            Message::SettingsToggleMergeSubtitle(val) => {
                self.config.process.merge_subtitle = val;
                IcedTask::none()
            }
            Message::SettingsToggleGpu(val) => {
                self.config.process.use_gpu = val;
                IcedTask::none()
            }
            Message::SettingsToggleProxy(val) => {
                self.config.requests.use_proxy = val;
                IcedTask::none()
            }
            Message::SettingsSave => {
                self.config.save();
                IcedTask::none()
            }

            Message::DismissError => {
                self.error_message = None;
                IcedTask::none()
            }
        }
    }

    fn drain_download_progress(&mut self) {
        if let Some(ref mut rx) = self.download_rx {
            while let Ok(progress) = rx.try_recv() {
                if let Some(existing) = self.downloads.iter_mut().find(|d| d.id == progress.id) {
                    *existing = progress;
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = self.sidebar();
        let content = match self.screen {
            Screen::Home => screens::home_view(self.provider_online),
            Screen::Search => {
                screens::search_view(&self.search_query, &self.search_results, self.is_searching)
            }
            Screen::Details => {
                if let Some(ref entry) = self.selected_entry {
                    screens::details_view(
                        entry,
                        &self.seasons,
                        &self.episodes,
                        self.selected_season,
                        self.is_loading_details,
                    )
                } else {
                    screens::home_view(self.provider_online)
                }
            }
            Screen::Player => {
                screens::player_view(&self.playback_state, &self.playing_title)
            }
            Screen::Downloads => screens::downloads_view(&self.downloads),
            Screen::Settings => screens::settings_view(&self.config),
        };

        let mut main_col = column![].width(Fill).height(Fill);

        if let Some(ref err) = self.error_message {
            let error_bar = container(
                row![
                    text(err.as_str()).size(13).color(style::TEXT_PRIMARY),
                    Space::with_width(Fill),
                    button(text("X").size(12))
                        .on_press(Message::DismissError)
                        .padding(4),
                ]
                .align_y(Alignment::Center)
                .padding(10),
            )
            .width(Fill)
            .style(|_: &_| container::Style {
                background: Some(iced::Background::Color(style::DANGER)),
                ..Default::default()
            });

            main_col = main_col.push(error_bar);
        }

        main_col = main_col.push(
            container(content)
                .width(Fill)
                .height(Fill)
                .style(|_: &_| container::Style {
                    background: Some(iced::Background::Color(style::BG_DARK)),
                    ..Default::default()
                }),
        );

        row![sidebar, main_col].into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let is_home = matches!(self.screen, Screen::Home);
        let is_search = matches!(self.screen, Screen::Search | Screen::Details);
        let is_player = matches!(self.screen, Screen::Player);
        let is_downloads = matches!(self.screen, Screen::Downloads);
        let is_settings = matches!(self.screen, Screen::Settings);

        let sidebar_content = column![
            Space::with_height(20),
            text("SV").size(24).color(style::ACCENT_HOVER).center(),
            Space::with_height(30),
            nav_button("Home", is_home, Message::NavigateHome),
            nav_button("Search", is_search, Message::NavigateSearch),
            nav_button("Player", is_player, Message::NavigateHome),
            nav_button("Downloads", is_downloads, Message::NavigateDownloads),
            nav_button("Settings", is_settings, Message::NavigateSettings),
        ]
        .width(160)
        .align_x(Alignment::Center);

        container(sidebar_content)
            .height(Fill)
            .style(style::sidebar_style)
            .into()
    }
}

fn nav_button(label: &str, is_active: bool, msg: Message) -> Element<'_, Message> {
    let color = if is_active {
        style::TEXT_PRIMARY
    } else {
        style::TEXT_SECONDARY
    };

    button(text(label).size(14).color(color).width(Fill))
        .on_press(msg)
        .padding([10, 16])
        .width(Fill)
        .into()
}

async fn check_provider(provider: Arc<dyn Provider>) -> bool {
    provider.search("test").await.is_ok()
}
