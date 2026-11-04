use dioxus::logger::tracing::warn;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY, VK_K, VK_LWIN,
};

pub const INSTALLER: &str = "StreamVault-install-windows-x64.exe";
pub const HLS: Option<&str> = Some(include_str!("../../assets/js/hls.min.js"));
pub const CAST_SCREEN: Option<fn()> = Some(open_cast);
const CAST_SHORTCUT: [(VIRTUAL_KEY, KEYBD_EVENT_FLAGS); 4] = [
    (VK_LWIN, 0),
    (VK_K, 0),
    (VK_K, KEYEVENTF_KEYUP),
    (VK_LWIN, KEYEVENTF_KEYUP),
];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

pub fn bin_dirs() -> impl Iterator<Item = PathBuf> {
    std::iter::empty()
}

pub fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub async fn reveal(file: &Path) -> bool {
    Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", file.display()))
        .status()
        .await
        .is_ok()
}

pub fn installer(file: &Path) -> std::io::Result<Command> {
    Ok(command(file))
}

pub fn relaunch() -> std::process::Command {
    std::process::Command::new(std::env::current_exe().unwrap_or_default())
}

fn open_cast() {
    let inputs = CAST_SHORTCUT.map(key_input);
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        warn!("cast shortcut failed: {}", std::io::Error::last_os_error());
    }
}

fn key_input((key, flags): (VIRTUAL_KEY, KEYBD_EVENT_FLAGS)) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: flags,
                ..KEYBDINPUT::default()
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;

    #[test]
    fn the_updated_app_starts_from_the_same_executable() {
        let relaunch = relaunch();
        assert_eq!(relaunch.get_program(), std::env::current_exe().unwrap());
        assert_eq!(relaunch.get_args().count(), 0);
    }

    #[test]
    fn webview2_streams_hls_through_hls_js() {
        assert!(HLS.is_some_and(|script| script.contains(".Hls=")));
    }

    #[test]
    fn the_installer_runs_the_downloaded_file() {
        let file = Path::new(INSTALLER);
        assert_eq!(installer(file).unwrap().as_std().get_program(), file);
    }

    #[test]
    fn binaries_are_files() {
        let dir = scratch_dir("executable");
        let file = dir.join("tool.exe");
        std::fs::write(&file, "").unwrap();
        assert!(is_executable(&file));
        assert!(!is_executable(&dir));
        assert!(!is_executable(&dir.join("missing.exe")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
