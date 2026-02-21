mod config;
mod download;
mod gui;
mod playback;
mod provider;
mod util;

use gui::App;
use iced::application;

fn main() -> iced::Result {
    application("StreamVault", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .run_with(App::new)
}
