mod app;
#[cfg(feature = "desktop")]
mod desktop;
mod gui;
#[cfg(test)]
mod http_test;
#[cfg(all(test, target_os = "macos"))]
mod js_test;
mod player;
mod prefs;
mod providers;
mod search;
mod sync;
mod time;
#[cfg(feature = "tv")]
mod tv;
mod watch;

#[cfg(feature = "desktop")]
use desktop as platform;
#[cfg(feature = "tv")]
use tv as platform;

fn main() {
    platform::launch(app::App);
}
