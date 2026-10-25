mod remote;
mod storage;
mod update;

use crate::app::{AppState, Playback, Screen};
use crate::player::SEEK_SECONDS;
use crate::providers::{Episode, MediaEntry};
use crate::update::Update;
use dioxus::prelude::*;
use remote::{Direction, Key};
pub use remote::{focus, focus_key, restore as restore_focus};
use std::time::Duration;
pub use storage::{load_replica, save_replica, sync_config};
pub use update::install as install_update;
use wasm_bindgen::prelude::*;

pub const NAV: &[(Screen, &str)] = &[];
const CSS: &str = include_str!("../../assets/style/tv.css");
const HLS: Asset = asset!(
    "/assets/js/hls.min.js",
    AssetOptions::js().with_minify(false)
);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SKIP_SECONDS: f64 = 30.0;
const SEEK_BAR: &str = "#seek";
const OSD: &str = ".player-osd *";
const CONTROLS: &str = ".player-controls";
const MENU: &str = ".player-menu";

pub fn launch(app: fn() -> Element) {
    console_error_panic_hook::set_once();
    dioxus::launch(app);
}

#[derive(Clone, Copy)]
pub struct Device {
    pub update: Update,
}

impl Device {
    pub fn new() -> Self {
        Self {
            update: Update::default(),
        }
    }

    pub fn timeout(self) -> Duration {
        REQUEST_TIMEOUT
    }

    pub fn local(self, playback: Playback) -> Playback {
        playback
    }
}

pub fn use_device(app: AppState) {
    use_hook(move || {
        remote::install(move |key| handle(app, key));
        pause_when_hidden(app);
    });
    use_future(update::confirm);
    use_future(move || update::watch(app.device));
}

#[component]
pub fn Head() -> Element {
    rsx! {
        document::Script { src: HLS }
        style { dangerous_inner_html: CSS }
    }
}

#[component]
pub fn EntryActions(entry: MediaEntry, season: Option<u32>, episode: Option<Episode>) -> Element {
    VNode::empty()
}

fn handle(app: AppState, key: Key) -> bool {
    if remote::editing() {
        return match key {
            Key::Select | Key::Done => {
                remote::blur();
                app.search();
                true
            }
            Key::Back | Key::Cancel => remote::focus("#search-go"),
            _ => false,
        };
    }
    if *app.screen.peek() == Screen::Player {
        return player(app, key);
    }
    match key {
        Key::Move(direction) => remote::navigate(direction),
        Key::Select if remote::is_focused("#search-go") => remote::focus("#search-input"),
        Key::Select => remote::activate(),
        Key::Back => app.back() || remote::back_to_top() || remote::exit(),
        _ => false,
    }
}

fn player(app: AppState, key: Key) -> bool {
    let player = app.player;
    let idle = player.wake();
    let attached = app
        .playback
        .peek()
        .as_ref()
        .is_some_and(|p| p.url.is_some());
    if !attached {
        if matches!(key, Key::Back | Key::Stop) {
            app.back();
        }
        return true;
    }
    let menu = *player.menu.peek();
    if let Some(menu) = menu {
        return match key {
            Key::Move(direction) => {
                remote::navigate_in(direction, MENU);
                true
            }
            Key::Select => remote::activate(),
            Key::Back => {
                player.close(menu);
                true
            }
            key => transport(app, key),
        };
    }
    if (idle && !player.status.peek().paused) || !remote::is_focused(OSD) {
        remote::focus(SEEK_BAR);
        if matches!(key, Key::Move(Direction::Up | Direction::Down)) {
            return true;
        }
    }
    let on_seek_bar = remote::is_focused(SEEK_BAR);
    match key {
        Key::Select if on_seek_bar => player.toggle(),
        Key::Select => {
            remote::activate();
        }
        Key::Move(Direction::Left) if on_seek_bar => player.seek(-SEEK_SECONDS),
        Key::Move(Direction::Right) if on_seek_bar => player.seek(SEEK_SECONDS),
        Key::Move(Direction::Down) if on_seek_bar => {
            remote::focus("#control-play");
        }
        Key::Move(Direction::Up) => {
            remote::focus(SEEK_BAR);
        }
        Key::Move(direction) => {
            remote::navigate_in(direction, CONTROLS);
        }
        key => return transport(app, key),
    }
    true
}

fn transport(app: AppState, key: Key) -> bool {
    let player = app.player;
    match key {
        Key::Back | Key::Stop => {
            app.back();
        }
        Key::PlayPause => player.toggle(),
        Key::Play | Key::Pause => {
            if player.status.peek().paused == (key == Key::Play) {
                player.toggle();
            }
        }
        Key::Rewind => player.seek(-SKIP_SECONDS),
        Key::FastForward => player.seek(SKIP_SECONDS),
        _ => return false,
    }
    true
}

fn pause_when_hidden(app: AppState) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let listener = Closure::<dyn FnMut()>::new({
        let document = document.clone();
        move || {
            if document.hidden() && !app.player.status.peek().paused {
                app.player.toggle();
            }
        }
    });
    let _ = document
        .add_event_listener_with_callback("visibilitychange", listener.as_ref().unchecked_ref());
    listener.forget();
}
