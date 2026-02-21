#[cfg(target_os = "macos")]
mod native_player;

#[cfg(target_os = "macos")]
pub use native_player::NativeVideoPlayer;
