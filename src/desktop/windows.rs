use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub const INSTALLER: &str = "StreamVault-install-windows-x64.exe";
pub const HLS: Option<&str> = Some(include_str!("../../assets/js/hls.min.js"));
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
