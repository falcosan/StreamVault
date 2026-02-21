mod config;
mod provider;
mod download;
mod playback;
mod gui;

use gui::App;
use iced::application;

fn main() -> iced::Result {
    application("StreamVault", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .run_with(App::new)
}
