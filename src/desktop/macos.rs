use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub const INSTALLER: &str = "StreamVault-install-macos";
pub const HLS: Option<&str> = None;
const RELAUNCH: &str = "while kill -0 \"$1\" 2>/dev/null; do sleep 0.1; done; exec open -n \"$2\"";

pub fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

pub fn bin_dirs() -> impl Iterator<Item = PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        .map(PathBuf::from)
        .into_iter()
        .chain(dirs::home_dir().map(|home| home.join(".local/bin")))
}

pub fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub async fn reveal(file: &Path) -> bool {
    Command::new("open")
        .arg("-R")
        .arg(file)
        .status()
        .await
        .is_ok_and(|s| s.success())
}

pub fn installer(file: &Path) -> std::io::Result<Command> {
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755))?;
    let mut command = Command::new(file);
    command.current_dir(app_dir());
    Ok(command)
}

pub fn relaunch() -> std::process::Command {
    reopen(std::process::id())
}

fn reopen(pid: u32) -> std::process::Command {
    let mut command = std::process::Command::new("sh");
    command
        .args(["-c", RELAUNCH, "relaunch"])
        .arg(pid.to_string())
        .arg(app_dir().join("StreamVault.app"))
        .env_remove("XPC_SERVICE_NAME")
        .env_remove("XPC_FLAGS");
    command
}

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.ancestors()
                .find(|a| a.extension().is_some_and(|e| e == "app"))
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;
    use std::time::Duration;

    #[test]
    fn the_updated_app_opens_once_the_old_one_has_quit() {
        let dir = scratch_dir("relaunch");
        let (open, opened) = (dir.join("open"), dir.join("opened"));
        std::fs::write(
            &open,
            format!("#!/bin/sh\necho \"$@\" > '{}'\n", opened.display()),
        )
        .unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut old = std::process::Command::new("sleep")
            .arg("1")
            .spawn()
            .unwrap();
        let mut command = reopen(old.id());
        let waiter = command
            .env("PATH", format!("{}:/bin:/usr/bin", dir.display()))
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!opened.exists());
        old.wait().unwrap();
        assert!(waiter.wait_with_output().unwrap().status.success());
        assert_eq!(
            std::fs::read_to_string(&opened).unwrap(),
            format!("-n {}\n", app_dir().join("StreamVault.app").display())
        );
        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_owned())
            .collect();
        assert_eq!(removed, ["XPC_FLAGS", "XPC_SERVICE_NAME"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_installer_runs_from_the_app_folder() {
        let dir = scratch_dir("installer");
        let file = dir.join(INSTALLER);
        std::fs::write(&file, "").unwrap();
        let command = installer(&file).unwrap();
        assert!(is_executable(&file));
        assert_eq!(command.as_std().get_program(), file.as_os_str());
        assert_eq!(
            command.as_std().get_current_dir(),
            Some(app_dir().as_path())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binaries_are_executable_files() {
        let dir = scratch_dir("executable");
        let file = dir.join("tool");
        std::fs::write(&file, "").unwrap();
        assert!(!is_executable(&file));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable(&file));
        assert!(!is_executable(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
