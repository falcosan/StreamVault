mod app;
mod config;
mod gui;
mod providers;
mod search;
mod style;
mod util;

fn main() {
    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::new().with_window(
                dioxus::desktop::WindowBuilder::new()
                    .with_title("StreamVault")
                    .with_inner_size(dioxus::desktop::LogicalSize::new(1200.0, 800.0))
                    .with_min_inner_size(dioxus::desktop::LogicalSize::new(800.0, 600.0)),
            ),
        )
        .launch(app::App);
}
