use std::path::{Path, PathBuf};

pub const TARGET: &str = "pc-windows-msvc";
pub const ARCHIVE: &str = "zip";
pub const SCRIPT: &str = ".bat";
pub const JDK_OS: &str = "windows";
pub const JDK_HOME: &str = "";
pub const ANDROID_OS: &str = "win";

pub fn android_home() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join(r"Android\Sdk")
}

pub fn system_java() -> Option<PathBuf> {
    None
}

pub fn open(url: &str) -> Result<(), String> {
    std::process::Command::new("rundll32")
        .arg("url.dll,FileProtocolHandler")
        .arg(url)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Cannot open {url}: {e}"))
}

pub fn private(_: &Path) -> Result<(), String> {
    Ok(())
}
