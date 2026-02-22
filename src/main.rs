mod app;
mod config;
mod gui;
mod providers;
mod util;

fn main() -> iced::Result {
    iced::application("StreamVault", app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        .run_with(app::App::new)
}
