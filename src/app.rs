use crate::config::AppConfig;
use crate::gui::{self, Msg, Screen};
use crate::providers::{Episode, MediaEntry, Provider, StreamingCommunityProvider};
use crate::util::{DownloadEngine, DownloadProgress, DownloadRequest};
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Fill, Subscription, Task as IcedTask, Theme};
use std::sync::Arc;
use tokio::sync::mpsc;

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSBackingStoreType, NSPanel, NSWindowOrderingMode, NSWindowStyleMask};
#[cfg(target_os = "macos")]
use objc2_av_foundation::AVPlayer;
#[cfg(target_os = "macos")]
use objc2_av_kit::AVPlayerView;
#[cfg(target_os = "macos")]
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSString, NSURL};

const TICK_MS: u64 = 500;
#[cfg(target_os = "macos")]
const PLAYER_W: f64 = 960.0;
#[cfg(target_os = "macos")]
const PLAYER_H: f64 = 540.0;

#[cfg(target_os = "macos")]
pub struct NativePlayer {
    player: Retained<AVPlayer>,
    panel: Retained<NSPanel>,
    _view: Retained<AVPlayerView>,
    mtm: MainThreadMarker,
    playing: bool,
}

#[cfg(target_os = "macos")]
impl NativePlayer {
    // Creates AVPlayer + AVPlayerView, attaches to a floating NSPanel centered on the main window
    fn play(url: &str, title: &str, mtm: MainThreadMarker) -> Result<Self, String> {
        let ns_url_str: Retained<NSString> = NSString::from_str(url);
        let ns_url = NSURL::URLWithString(&ns_url_str).ok_or_else(|| format!("Invalid URL: {url}"))?;
        let player = unsafe { AVPlayer::initWithURL(AVPlayer::alloc(mtm), &ns_url) };
        let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(PLAYER_W, PLAYER_H));
        let pv = unsafe { AVPlayerView::initWithFrame(AVPlayerView::alloc(mtm), frame) };
        unsafe { pv.setPlayer(Some(&player)) };

        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable | NSWindowStyleMask::Miniaturizable;
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm), frame, style, NSBackingStoreType::Buffered, false);
        panel.setContentView(Some(&pv));
        panel.setTitle(&NSString::from_str(title));

        let app = NSApplication::sharedApplication(mtm);
        if let Some(main_win) = app.mainWindow() {
            unsafe { main_win.addChildWindow_ordered(&panel, NSWindowOrderingMode::Above); }
            let mf = main_win.frame();
            let pf = CGRect::new(
                CGPoint::new(mf.origin.x + (mf.size.width - PLAYER_W) / 2.0,
                             mf.origin.y + (mf.size.height - PLAYER_H) / 2.0),
                CGSize::new(PLAYER_W, PLAYER_H));
            panel.setFrame_display(pf, true);
        }
        panel.makeKeyAndOrderFront(None);
        unsafe { player.play() };
        Ok(Self { player, panel, _view: pv, mtm, playing: true })
    }

    #[inline] fn pause(&mut self) { unsafe { self.player.pause() }; self.playing = false; }
    #[inline] fn resume(&mut self) { unsafe { self.player.play() }; self.playing = true; }

    fn stop(&mut self) {
        unsafe { self.player.pause() };
        self.playing = false;
        let app = NSApplication::sharedApplication(self.mtm);
        if let Some(w) = app.mainWindow() { w.removeChildWindow(&self.panel); }
        self.panel.orderOut(None);
        self.panel.close();
    }

    #[inline] fn is_playing(&self) -> bool { self.playing }
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
    seasons: Vec<crate::providers::Season>,
    episodes: Vec<Episode>,
    selected_season: Option<u32>,
    is_loading: bool,
    #[cfg(target_os = "macos")]
    native_player: Option<NativePlayer>,
    playing_title: String,
    error_msg: Option<String>,
    downloads: Vec<DownloadProgress>,
    dl_tx: mpsc::UnboundedSender<DownloadProgress>,
    dl_rx: Option<mpsc::UnboundedReceiver<DownloadProgress>>,
}

impl App {
    pub fn new() -> (Self, IcedTask<Msg>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let cfg = AppConfig::load();
        let p: Arc<dyn Provider> = Arc::new(StreamingCommunityProvider::with_config(
            StreamingCommunityProvider::default_base_url().to_string(), cfg.requests.timeout));
        let pc = p.clone();
        (Self {
            screen: Screen::Home, config: cfg, provider: p, provider_online: false,
            search_query: String::new(), search_results: Vec::new(), is_searching: false,
            selected_entry: None, seasons: Vec::new(), episodes: Vec::new(),
            selected_season: None, is_loading: false,
            #[cfg(target_os = "macos")]
            native_player: None,
            playing_title: String::new(), error_msg: None,
            downloads: Vec::new(), dl_tx: tx, dl_rx: Some(rx),
        }, IcedTask::perform(async move { pc.search("test").await.is_ok() }, Msg::ProviderStatus))
    }

    #[inline] pub fn theme(&self) -> Theme { Theme::CatppuccinMocha }

    #[inline]
    pub fn subscription(&self) -> Subscription<Msg> {
        iced::time::every(std::time::Duration::from_millis(TICK_MS)).map(|_| Msg::Tick)
    }

    pub fn update(&mut self, msg: Msg) -> IcedTask<Msg> {
        match msg {
            Msg::Tick => { self.drain_progress(); IcedTask::none() }

            Msg::NavHome => { self.screen = Screen::Home; IcedTask::none() }
            Msg::NavSearch => { self.screen = Screen::Search; IcedTask::none() }
            Msg::NavDownloads => { self.screen = Screen::Downloads; IcedTask::none() }
            Msg::NavSettings => { self.screen = Screen::Settings; IcedTask::none() }

            Msg::SearchInput(q) => { self.search_query = q; IcedTask::none() }
            Msg::SearchSubmit => {
                if self.search_query.trim().is_empty() || self.is_searching { return IcedTask::none(); }
                self.is_searching = true;
                let (p, q) = (self.provider.clone(), self.search_query.clone());
                IcedTask::perform(async move { p.search(&q).await.map_err(|e| e.to_string()) }, Msg::SearchDone)
            }
            Msg::SearchDone(r) => {
                self.is_searching = false;
                match r { Ok(e) => self.search_results = e, Err(_) => self.search_results.clear() }
                IcedTask::none()
            }

            Msg::SelectEntry(i) => {
                if let Some(entry) = self.search_results.get(i).cloned() {
                    let is_movie = entry.is_movie();
                    self.selected_entry = Some(entry.clone());
                    self.screen = Screen::Details;
                    self.seasons.clear(); self.episodes.clear();
                    self.selected_season = None; self.is_loading = !is_movie;
                    if is_movie { return IcedTask::none(); }
                    let p = self.provider.clone();
                    IcedTask::perform(async move { p.get_seasons(&entry).await.map_err(|e| e.to_string()) }, Msg::SeasonsLoaded)
                } else { IcedTask::none() }
            }
            Msg::SeasonsLoaded(r) => {
                self.is_loading = false;
                if let Ok(s) = r {
                    self.seasons = s;
                    if let Some(first) = self.seasons.first() {
                        let n = first.number;
                        return self.update(Msg::SelectSeason(n));
                    }
                }
                IcedTask::none()
            }
            Msg::SelectSeason(n) => {
                self.selected_season = Some(n); self.episodes.clear(); self.is_loading = true;
                if let Some(entry) = self.selected_entry.clone() {
                    let p = self.provider.clone();
                    IcedTask::perform(async move { p.get_episodes(&entry, n).await.map_err(|e| e.to_string()) }, Msg::EpisodesLoaded)
                } else { IcedTask::none() }
            }
            Msg::EpisodesLoaded(r) => {
                self.is_loading = false;
                if let Ok(eps) = r { self.episodes = eps; }
                IcedTask::none()
            }

            Msg::PlayEntry(i) => {
                if let Some(e) = self.search_results.get(i).cloned() {
                    let t = e.display_title();
                    self.resolve_play(e, None, None, t)
                } else { IcedTask::none() }
            }
            Msg::PlayMovie => {
                if let Some(e) = self.selected_entry.clone() {
                    self.error_msg = None;
                    let t = e.display_title();
                    self.resolve_play(e, None, None, t)
                } else { IcedTask::none() }
            }
            Msg::PlayEpisode(s, ep) => {
                if let Some(e) = self.selected_entry.clone() {
                    self.error_msg = None;
                    let episode = self.episodes.iter().find(|x| x.number == ep).cloned();
                    let t = format!("{} S{s:02}E{ep:02}", e.name);
                    self.resolve_play(e, episode, Some(s), t)
                } else { IcedTask::none() }
            }
            Msg::StreamResolved(r) => {
                match r {
                    Ok((stream, title)) => self.start_playback(&stream.url, &title),
                    Err(e) => { self.error_msg = Some(format!("Failed to get stream: {e}")); }
                }
                IcedTask::none()
            }

            Msg::Pause => {
                #[cfg(target_os = "macos")]
                if let Some(ref mut p) = self.native_player { p.pause(); }
                IcedTask::none()
            }
            Msg::Resume => {
                #[cfg(target_os = "macos")]
                if let Some(ref mut p) = self.native_player { p.resume(); }
                IcedTask::none()
            }
            Msg::Stop => {
                #[cfg(target_os = "macos")]
                if let Some(ref mut p) = self.native_player { p.stop(); }
                #[cfg(target_os = "macos")]
                { self.native_player = None; }
                self.playing_title.clear();
                self.screen = Screen::Search;
                IcedTask::none()
            }

            Msg::DlMovie => {
                if let Some(entry) = self.selected_entry.clone() {
                    let (p, t) = (self.provider.clone(), entry.display_title());
                    IcedTask::perform(async move {
                        let s = p.get_stream_url(&entry, None, None).await.map_err(|e| e.to_string())?;
                        Ok((s, t, true))
                    }, Msg::DlStreamResolved)
                } else { IcedTask::none() }
            }
            Msg::DlEpisode(season, ep_num) => {
                if let Some(entry) = self.selected_entry.clone() {
                    let ep = self.episodes.iter().find(|e| e.number == ep_num).cloned();
                    let (p, cfg, show) = (self.provider.clone(), self.config.clone(), entry.name.clone());
                    IcedTask::perform(async move {
                        let ep_ref = ep.as_ref();
                        let s = p.get_stream_url(&entry, ep_ref, Some(season)).await.map_err(|e| e.to_string())?;
                        let engine = DownloadEngine::new(cfg);
                        let fname = engine.format_episode_name(&show, season, ep_num,
                            ep_ref.map(|e| e.name.as_str()).unwrap_or(""));
                        Ok((s, fname, false))
                    }, Msg::DlStreamResolved)
                } else { IcedTask::none() }
            }
            Msg::DlStreamResolved(r) => {
                if let Ok((stream, fname, is_movie)) = r {
                    let id = uuid::Uuid::new_v4();
                    let engine = DownloadEngine::new(self.config.clone());
                    let req = DownloadRequest {
                        id, title: fname.clone(), stream_url: stream.url,
                        output_dir: engine.build_output_path(&fname, is_movie),
                        filename: fname.clone(), headers: stream.headers,
                    };
                    self.downloads.push(DownloadProgress::new(id, fname));
                    let tx = self.dl_tx.clone();
                    self.screen = Screen::Downloads;
                    IcedTask::perform(async move { engine.download(req, tx).await; }, |_: ()| Msg::Tick)
                } else { IcedTask::none() }
            }

            Msg::ProviderStatus(ok) => { self.provider_online = ok; IcedTask::none() }

            Msg::CfgRootPath(v) => { self.config.output.root_path = v; IcedTask::none() }
            Msg::CfgMovieFolder(v) => { self.config.output.movie_folder_name = v; IcedTask::none() }
            Msg::CfgSerieFolder(v) => { self.config.output.serie_folder_name = v; IcedTask::none() }
            Msg::CfgEpFormat(v) => { self.config.output.map_episode_name = v; IcedTask::none() }
            Msg::CfgThreads(v) => { if let Ok(n) = v.parse() { self.config.download.thread_count = n; } IcedTask::none() }
            Msg::CfgRetry(v) => { if let Ok(n) = v.parse() { self.config.download.retry_count = n; } IcedTask::none() }
            Msg::CfgSelVideo(v) => { self.config.download.select_video = v; IcedTask::none() }
            Msg::CfgSelAudio(v) => { self.config.download.select_audio = v; IcedTask::none() }
            Msg::CfgSelSub(v) => { self.config.download.select_subtitle = v; IcedTask::none() }
            Msg::CfgMaxSpeed(v) => { self.config.download.max_speed = v; IcedTask::none() }
            Msg::CfgExtension(v) => { self.config.process.extension = v; IcedTask::none() }
            Msg::CfgTimeout(v) => { if let Ok(n) = v.parse() { self.config.requests.timeout = n; } IcedTask::none() }
            Msg::CfgProxyUrl(v) => { self.config.requests.proxy_url = v; IcedTask::none() }
            Msg::CfgConcurrent(v) => { self.config.download.concurrent_download = v; IcedTask::none() }
            Msg::CfgMergeAudio(v) => { self.config.process.merge_audio = v; IcedTask::none() }
            Msg::CfgMergeSub(v) => { self.config.process.merge_subtitle = v; IcedTask::none() }
            Msg::CfgGpu(v) => { self.config.process.use_gpu = v; IcedTask::none() }
            Msg::CfgProxy(v) => { self.config.requests.use_proxy = v; IcedTask::none() }
            Msg::CfgSave => { self.config.save(); IcedTask::none() }

            Msg::DismissError => { self.error_msg = None; IcedTask::none() }
        }
    }

    #[inline]
    fn drain_progress(&mut self) {
        if let Some(ref mut rx) = self.dl_rx {
            while let Ok(p) = rx.try_recv() {
                if let Some(existing) = self.downloads.iter_mut().find(|d| d.id == p.id) { *existing = p; }
            }
        }
    }

    fn resolve_play(&self, entry: MediaEntry, ep: Option<Episode>, season: Option<u32>, title: String) -> IcedTask<Msg> {
        let p = self.provider.clone();
        IcedTask::perform(async move {
            let s = p.get_stream_url(&entry, ep.as_ref(), season).await.map_err(|e| e.to_string())?;
            Ok((s, title))
        }, Msg::StreamResolved)
    }

    #[cfg(target_os = "macos")]
    fn start_playback(&mut self, url: &str, title: &str) {
        eprintln!("[StreamVault] Playing: {url}");
        self.playing_title = title.to_string();
        self.error_msg = None;
        if let Some(ref mut p) = self.native_player { p.stop(); }
        let mtm = MainThreadMarker::new().expect("update() runs on main thread");
        match NativePlayer::play(url, title, mtm) {
            Ok(player) => { self.native_player = Some(player); self.screen = Screen::Player; }
            Err(e) => { eprintln!("[StreamVault] Player error: {e}"); self.error_msg = Some(format!("Playback error: {e}")); }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn start_playback(&mut self, url: &str, title: &str) {
        eprintln!("[StreamVault] Playback not supported on this platform: {url} - {title}");
        self.error_msg = Some("Playback not supported on this platform".into());
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let sidebar = self.sidebar();
        let content = match self.screen {
            Screen::Home => gui::home_view(self.provider_online),
            Screen::Search => gui::search_view(&self.search_query, &self.search_results, self.is_searching),
            Screen::Details => {
                if let Some(ref e) = self.selected_entry {
                    gui::details_view(e, &self.seasons, &self.episodes, self.selected_season, self.is_loading)
                } else { gui::home_view(self.provider_online) }
            }
            Screen::Player => {
                #[cfg(target_os = "macos")]
                let playing = self.native_player.as_ref().is_some_and(|p| p.is_playing());
                #[cfg(not(target_os = "macos"))]
                let playing = false;
                gui::player_view(playing, &self.playing_title)
            }
            Screen::Downloads => gui::downloads_view(&self.downloads),
            Screen::Settings => gui::settings_view(&self.config),
        };

        let mut main_col = column![].width(Fill).height(Fill);
        if let Some(ref err) = self.error_msg {
            main_col = main_col.push(
                container(row![
                    text(err.as_str()).size(13).color(gui::TEXT_PRI),
                    Space::with_width(Fill),
                    button(text("X").size(12)).on_press(Msg::DismissError).padding(4),
                ].align_y(Alignment::Center).padding(10))
                .width(Fill).style(|_: &_| container::Style {
                    background: Some(iced::Background::Color(gui::DANGER)), ..Default::default()
                })
            );
        }
        main_col = main_col.push(container(content).width(Fill).height(Fill).style(|_: &_| container::Style {
            background: Some(iced::Background::Color(gui::BG_DARK)), ..Default::default()
        }));
        row![sidebar, main_col].into()
    }

    fn sidebar(&self) -> Element<'_, Msg> {
        let s = &self.screen;
        let content = column![
            Space::with_height(20),
            text("SV").size(24).color(gui::ACCENT).center(),
            Space::with_height(30),
            gui::nav_button("Home", matches!(s, Screen::Home), Msg::NavHome),
            gui::nav_button("Search", matches!(s, Screen::Search | Screen::Details), Msg::NavSearch),
            gui::nav_button("Player", matches!(s, Screen::Player), Msg::NavHome),
            gui::nav_button("Downloads", matches!(s, Screen::Downloads), Msg::NavDownloads),
            gui::nav_button("Settings", matches!(s, Screen::Settings), Msg::NavSettings),
        ].width(gui::SIDEBAR_W).align_x(Alignment::Center);
        container(content).height(Fill).style(gui::sidebar_style).into()
    }
}
