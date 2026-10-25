mod app;
#[cfg(test)]
mod check;
#[cfg(feature = "desktop")]
mod desktop;
mod gui;
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
