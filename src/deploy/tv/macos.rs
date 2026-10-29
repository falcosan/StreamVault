use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const TARGET: &str = "apple-darwin";
pub const ARCHIVE: &str = "tar.gz";
pub const SCRIPT: &str = "";
pub const JDK_OS: &str = "mac";
pub const JDK_HOME: &str = "Contents/Home";
pub const ANDROID_OS: &str = "mac";

pub fn android_home() -> PathBuf {
    home().join("Library/Android/sdk")
}

pub fn system_java() -> Option<PathBuf> {
    let output = std::process::Command::new("/usr/libexec/java_home")
        .output()
        .ok()?;
    let home = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && !home.is_empty()).then(|| PathBuf::from(home))
}

pub fn open(url: &str) -> Result<(), String> {
    let opened = std::process::Command::new("open").arg(url).status();
    opened
        .is_ok_and(|status| status.success())
        .then_some(())
        .ok_or_else(|| format!("Cannot open {url}"))
}

pub fn private(dir: &Path) -> Result<(), String> {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}
