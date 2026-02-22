use crate::config::AppConfig;
use crate::gui::{self, Screen};
use crate::providers::{Episode, MediaEntry, Provider, Season, StreamingCommunityProvider};
use crate::util::{DownloadEngine, DownloadProgress, DownloadRequest};
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::mpsc;

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSPanel, NSWindowOrderingMode, NSWindowStyleMask,
};
#[cfg(target_os = "macos")]
use objc2_av_foundation::AVPlayer;
#[cfg(target_os = "macos")]
use objc2_av_kit::AVPlayerView;
#[cfg(target_os = "macos")]
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSString, NSURL};

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
    fn play(url: &str, title: &str, mtm: MainThreadMarker) -> Result<Self, String> {
        let ns_url_str: Retained<NSString> = NSString::from_str(url);
        let ns_url =
            NSURL::URLWithString(&ns_url_str).ok_or_else(|| format!("Invalid URL: {url}"))?;
        let player = unsafe { AVPlayer::initWithURL(AVPlayer::alloc(mtm), &ns_url) };
        let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(PLAYER_W, PLAYER_H));
        let pv = unsafe { AVPlayerView::initWithFrame(AVPlayerView::alloc(mtm), frame) };
        unsafe { pv.setPlayer(Some(&player)) };

        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::Miniaturizable;
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        panel.setContentView(Some(&pv));
        panel.setTitle(&NSString::from_str(title));

        let app = NSApplication::sharedApplication(mtm);
        if let Some(main_win) = app.mainWindow() {
            unsafe {
                main_win.addChildWindow_ordered(&panel, NSWindowOrderingMode::Above);
            }
            let mf = main_win.frame();
            let pf = CGRect::new(
                CGPoint::new(
                    mf.origin.x + (mf.size.width - PLAYER_W) / 2.0,
                    mf.origin.y + (mf.size.height - PLAYER_H) / 2.0,
                ),
                CGSize::new(PLAYER_W, PLAYER_H),
            );
            panel.setFrame_display(pf, true);
        }
        panel.makeKeyAndOrderFront(None);
        unsafe { player.play() };
        Ok(Self {
            player,
            panel,
            _view: pv,
            mtm,
            playing: true,
        })
    }

    #[inline]
    fn pause(&mut self) {
        unsafe { self.player.pause() };
        self.playing = false;
    }
    #[inline]
    fn resume(&mut self) {
        unsafe { self.player.play() };
        self.playing = true;
    }

    fn stop(&mut self) {
        unsafe { self.player.pause() };
        self.playing = false;
        let app = NSApplication::sharedApplication(self.mtm);
        if let Some(w) = app.mainWindow() {
            w.removeChildWindow(&self.panel);
        }
        self.panel.orderOut(None);
        self.panel.close();
    }

    #[inline]
    fn is_playing(&self) -> bool {
        self.playing
    }
}

#[component]
pub fn App() -> Element {
    let mut screen = use_signal(|| Screen::Home);
    let config = use_hook(AppConfig::load);
    let provider: Arc<dyn Provider> = use_hook(|| {
        Arc::new(StreamingCommunityProvider::with_config(
            StreamingCommunityProvider::default_base_url().to_string(),
            config.requests.timeout,
        ))
    });
    let mut provider_online = use_signal(|| false);
    let search_query = use_signal(String::new);
    let mut search_results: Signal<Vec<MediaEntry>> = use_signal(Vec::new);
    let mut is_searching = use_signal(|| false);
    let mut selected_entry: Signal<Option<MediaEntry>> = use_signal(|| None);
    let mut seasons: Signal<Vec<Season>> = use_signal(Vec::new);
    let mut episodes: Signal<Vec<Episode>> = use_signal(Vec::new);
    let mut selected_season: Signal<Option<u32>> = use_signal(|| None);
    let mut is_loading = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let mut downloads: Signal<Vec<DownloadProgress>> = use_signal(Vec::new);
    let mut playing_title = use_signal(String::new);
    let mut native_playing = use_signal(|| false);

    #[cfg(target_os = "macos")]
    let native_player: Rc<RefCell<Option<NativePlayer>>> =
        use_hook(|| Rc::new(RefCell::new(None::<NativePlayer>)));

    let dl_tx: mpsc::UnboundedSender<DownloadProgress> = use_hook(|| {
        let (tx, mut rx) = mpsc::unbounded_channel::<DownloadProgress>();
        spawn(async move {
            while let Some(p) = rx.recv().await {
                let mut dls = downloads.write();
                if let Some(existing) = dls.iter_mut().find(|d| d.id == p.id) {
                    *existing = p;
                }
            }
        });
        tx
    });

    let config_clone = config.clone();

    {
        let provider = provider.clone();
        use_future(move || {
            let p = provider.clone();
            async move {
                provider_online.set(p.search("test").await.is_ok());
            }
        });
    }

    let on_search_submit = {
        let provider = provider.clone();
        move |q: String| {
            if q.trim().is_empty() || is_searching() {
                return;
            }
            is_searching.set(true);
            screen.set(Screen::Search);
            let p = provider.clone();
            spawn(async move {
                match p.search(&q).await {
                    Ok(entries) => search_results.set(entries),
                    Err(_) => search_results.set(Vec::new()),
                }
                is_searching.set(false);
            });
        }
    };

    let on_select_entry = {
        let provider = provider.clone();
        move |idx: usize| {
            let results = search_results();
            if let Some(entry) = results.get(idx).cloned() {
                let is_movie = entry.is_movie();
                selected_entry.set(Some(entry.clone()));
                screen.set(Screen::Details);
                seasons.set(Vec::new());
                episodes.set(Vec::new());
                selected_season.set(None);
                if !is_movie {
                    is_loading.set(true);
                    let p = provider.clone();
                    spawn(async move {
                        match p.get_seasons(&entry).await {
                            Ok(s) => {
                                let first_num = s.first().map(|f| f.number);
                                seasons.set(s);
                                if let Some(n) = first_num {
                                    selected_season.set(Some(n));
                                    if let Some(e) = selected_entry() {
                                        is_loading.set(true);
                                        match p.get_episodes(&e, n).await {
                                            Ok(eps) => episodes.set(eps),
                                            Err(_) => episodes.set(Vec::new()),
                                        }
                                    }
                                }
                            }
                            Err(_) => seasons.set(Vec::new()),
                        }
                        is_loading.set(false);
                    });
                }
            }
        }
    };

    let on_select_season = {
        let provider = provider.clone();
        move |n: u32| {
            selected_season.set(Some(n));
            episodes.set(Vec::new());
            is_loading.set(true);
            let p = provider.clone();
            spawn(async move {
                if let Some(entry) = selected_entry() {
                    match p.get_episodes(&entry, n).await {
                        Ok(eps) => episodes.set(eps),
                        Err(_) => episodes.set(Vec::new()),
                    }
                }
                is_loading.set(false);
            });
        }
    };

    let on_play_movie = {
        let provider = provider.clone();
        #[cfg(target_os = "macos")]
        let native_player = native_player.clone();
        move |_: ()| {
            if let Some(entry) = selected_entry() {
                error_msg.set(None);
                let title = entry.display_title();
                let p = provider.clone();
                #[cfg(target_os = "macos")]
                let np_rc = native_player.clone();
                spawn(async move {
                    match p.get_stream_url(&entry, None, None).await {
                        Ok(stream) => {
                            #[cfg(target_os = "macos")]
                            {
                                eprintln!("[StreamVault] Playing: {}", stream.url);
                                playing_title.set(title.clone());
                                let mut np = np_rc.borrow_mut();
                                if let Some(ref mut existing) = *np {
                                    existing.stop();
                                }
                                let mtm = MainThreadMarker::new().expect("main thread");
                                match NativePlayer::play(&stream.url, &title, mtm) {
                                    Ok(player) => {
                                        native_playing.set(player.is_playing());
                                        *np = Some(player);
                                        screen.set(Screen::Player);
                                    }
                                    Err(e) => {
                                        eprintln!("[StreamVault] Player error: {e}");
                                        error_msg.set(Some(format!("Playback error: {e}")));
                                    }
                                }
                            }
                        }
                        Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                    }
                });
            }
        }
    };

    let on_play_episode = {
        let provider = provider.clone();
        #[cfg(target_os = "macos")]
        let native_player = native_player.clone();
        move |(s, ep_num): (u32, u32)| {
            if let Some(entry) = selected_entry() {
                error_msg.set(None);
                let episode = episodes().iter().find(|x| x.number == ep_num).cloned();
                let title = format!("{} S{s:02}E{ep_num:02}", entry.name);
                let p = provider.clone();
                #[cfg(target_os = "macos")]
                let np_rc = native_player.clone();
                spawn(async move {
                    match p.get_stream_url(&entry, episode.as_ref(), Some(s)).await {
                        Ok(stream) => {
                            #[cfg(target_os = "macos")]
                            {
                                eprintln!("[StreamVault] Playing: {}", stream.url);
                                playing_title.set(title.clone());
                                let mut np = np_rc.borrow_mut();
                                if let Some(ref mut existing) = *np {
                                    existing.stop();
                                }
                                let mtm = MainThreadMarker::new().expect("main thread");
                                match NativePlayer::play(&stream.url, &title, mtm) {
                                    Ok(player) => {
                                        native_playing.set(player.is_playing());
                                        *np = Some(player);
                                        screen.set(Screen::Player);
                                    }
                                    Err(e) => {
                                        eprintln!("[StreamVault] Player error: {e}");
                                        error_msg.set(Some(format!("Playback error: {e}")));
                                    }
                                }
                            }
                        }
                        Err(e) => error_msg.set(Some(format!("Failed to get stream: {e}"))),
                    }
                });
            }
        }
    };

    let on_dl_movie = {
        let provider = provider.clone();
        let config = config_clone.clone();
        let dl_tx = dl_tx.clone();
        move |_: ()| {
            if let Some(entry) = selected_entry() {
                let p = provider.clone();
                let cfg = config.clone();
                let tx = dl_tx.clone();
                spawn(async move {
                    match p.get_stream_url(&entry, None, None).await {
                        Ok(stream) => {
                            let title = entry.display_title();
                            let id = uuid::Uuid::new_v4();
                            let engine = DownloadEngine::new(cfg);
                            let req = DownloadRequest {
                                id,
                                title: title.clone(),
                                stream_url: stream.url,
                                output_dir: engine.build_output_path(&title, true),
                                filename: title.clone(),
                                headers: stream.headers,
                            };
                            downloads.write().push(DownloadProgress::new(id, title));
                            screen.set(Screen::Downloads);
                            engine.download(req, tx).await;
                        }
                        Err(e) => error_msg.set(Some(format!("Download failed: {e}"))),
                    }
                });
            }
        }
    };

    let on_dl_episode = {
        let provider = provider.clone();
        let config = config_clone.clone();
        let dl_tx = dl_tx.clone();
        move |(season, ep_num): (u32, u32)| {
            if let Some(entry) = selected_entry() {
                let ep = episodes().iter().find(|e| e.number == ep_num).cloned();
                let p = provider.clone();
                let cfg = config.clone();
                let tx = dl_tx.clone();
                let show = entry.name.clone();
                spawn(async move {
                    match p.get_stream_url(&entry, ep.as_ref(), Some(season)).await {
                        Ok(stream) => {
                            let engine = DownloadEngine::new(cfg);
                            let fname = engine.format_episode_name(
                                &show,
                                season,
                                ep_num,
                                ep.as_ref().map(|e| e.name.as_str()).unwrap_or(""),
                            );
                            let id = uuid::Uuid::new_v4();
                            let req = DownloadRequest {
                                id,
                                title: fname.clone(),
                                stream_url: stream.url,
                                output_dir: engine.build_output_path(&fname, false),
                                filename: fname.clone(),
                                headers: stream.headers,
                            };
                            downloads.write().push(DownloadProgress::new(id, fname));
                            screen.set(Screen::Downloads);
                            engine.download(req, tx).await;
                        }
                        Err(e) => error_msg.set(Some(format!("Download failed: {e}"))),
                    }
                });
            }
        }
    };

    let on_pause = {
        #[cfg(target_os = "macos")]
        let native_player = native_player.clone();
        move |_: ()| {
            #[cfg(target_os = "macos")]
            {
                let mut np = native_player.borrow_mut();
                if let Some(ref mut p) = *np {
                    p.pause();
                    native_playing.set(false);
                }
            }
        }
    };

    let on_resume = {
        #[cfg(target_os = "macos")]
        let native_player = native_player.clone();
        move |_: ()| {
            #[cfg(target_os = "macos")]
            {
                let mut np = native_player.borrow_mut();
                if let Some(ref mut p) = *np {
                    p.resume();
                    native_playing.set(true);
                }
            }
        }
    };

    let on_stop = {
        #[cfg(target_os = "macos")]
        let native_player = native_player.clone();
        move |_: ()| {
            #[cfg(target_os = "macos")]
            {
                let mut np = native_player.borrow_mut();
                if let Some(ref mut p) = *np {
                    p.stop();
                }
                *np = None;
            }
            native_playing.set(false);
            playing_title.set(String::new());
            screen.set(Screen::Search);
        }
    };

    let current_entry = selected_entry();

    rsx! {
        style { dangerous_inner_html: gui::GLOBAL_CSS }
        div { class: "app",
            gui::Navbar { screen, search_query, is_searching: ReadOnlySignal::from(is_searching), on_search_submit }
            if let Some(ref err) = error_msg() {
                div { class: "error-bar",
                    span { "{err}" }
                    div { class: "fill", style: "flex:1;" }
                    button { class: "dismiss", onclick: move |_| error_msg.set(None), "✕" }
                }
            }
            div { class: "content",
                match screen() {
                    Screen::Home => rsx! {
                        gui::HomeView {
                            provider_online: ReadOnlySignal::from(provider_online),
                            search_results: ReadOnlySignal::from(search_results),
                            on_select: on_select_entry,
                        }
                    },
                    Screen::Search => rsx! {
                        gui::SearchView {
                            search_query: ReadOnlySignal::from(search_query),
                            search_results: ReadOnlySignal::from(search_results),
                            is_searching: ReadOnlySignal::from(is_searching),
                            on_select: on_select_entry,
                        }
                    },
                    Screen::Details => {
                        if let Some(ref entry) = current_entry {
                            rsx! {
                                gui::DetailsView {
                                    entry: entry.clone(),
                                    seasons: ReadOnlySignal::from(seasons),
                                    episodes: ReadOnlySignal::from(episodes),
                                    selected_season: ReadOnlySignal::from(selected_season),
                                    is_loading: ReadOnlySignal::from(is_loading),
                                    on_select_season,
                                    on_play_movie,
                                    on_play_episode,
                                    on_dl_movie,
                                    on_dl_episode,
                                    on_back: move |_| screen.set(Screen::Search),
                                }
                            }
                        } else {
                            rsx! {
                                gui::HomeView {
                                    provider_online: ReadOnlySignal::from(provider_online),
                                    search_results: ReadOnlySignal::from(search_results),
                                    on_select: on_select_entry,
                                }
                            }
                        }
                    },
                    Screen::Player => rsx! {
                        gui::PlayerView {
                            playing: ReadOnlySignal::from(native_playing),
                            playing_title: ReadOnlySignal::from(playing_title),
                            on_pause,
                            on_resume,
                            on_stop,
                        }
                    },
                    Screen::Downloads => rsx! {
                        gui::DownloadsView {
                            downloads: ReadOnlySignal::from(downloads),
                        }
                    },
                }
            }
        }
    }
}
