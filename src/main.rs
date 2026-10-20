mod app;
mod config;
mod download;
mod gui;
mod media;
mod providers;
mod search;
mod state;
mod sync;
#[cfg(test)]
mod test_support;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_disable_context_menu(true).with_window(
                WindowBuilder::new()
                    .with_title("StreamVault")
                    .with_inner_size(LogicalSize::new(1200.0, 800.0))
                    .with_min_inner_size(LogicalSize::new(800.0, 600.0)),
            ),
        )
        .launch(app::App);
}
