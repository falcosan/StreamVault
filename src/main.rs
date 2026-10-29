#![cfg_attr(
    all(feature = "desktop", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(feature = "app")]
mod app;
#[cfg(test)]
mod check;
#[cfg(feature = "deploy")]
mod deploy;
#[cfg(feature = "desktop")]
mod desktop;
#[cfg(any(feature = "desktop", feature = "deploy"))]
mod folders;
#[cfg(feature = "app")]
mod gui;
#[cfg(feature = "app")]
mod i18n;
#[cfg(feature = "app")]
mod player;
#[cfg(feature = "app")]
mod prefs;
#[cfg(feature = "app")]
mod providers;
#[cfg(any(feature = "desktop", feature = "deploy"))]
mod release;
#[cfg(feature = "app")]
mod search;
mod settings;
#[cfg(feature = "app")]
mod sync;
#[cfg(feature = "app")]
mod time;
#[cfg(feature = "tv")]
mod tv;
#[cfg(feature = "app")]
mod update;
#[cfg(feature = "app")]
mod watch;

#[cfg(feature = "desktop")]
use desktop as platform;
#[cfg(feature = "tv")]
use tv as platform;

#[cfg(feature = "app")]
fn main() {
    platform::launch(app::App);
}

#[cfg(feature = "deploy")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    deploy::main().await
}
