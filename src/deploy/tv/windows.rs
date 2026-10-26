use crate::deploy::Session;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub const TARGET: &str = "pc-windows-msvc";
pub const ARCHIVE: &str = "zip";
pub const SCRIPT: &str = ".bat";
pub const JDK_OS: &str = "windows";
pub const JDK_HOME: &str = "";
pub const ANDROID_OS: &str = "win";
pub const PACKAGE_MANAGER: &str = "package-manager-cli.exe";

pub fn tizen_home() -> PathBuf {
    PathBuf::from(r"C:\tizen-studio")
}

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

pub async fn install_tizen_studio(
    session: &Session,
    studio: &str,
    home: &Path,
) -> Result<(), String> {
    let installer = session
        .download(
            &format!("{studio}/web-cli_Tizen_Studio_6.1_windows-64.exe"),
            "tizen-installer.exe",
        )
        .await?;
    session
        .run(
            Command::new(installer)
                .args(["--accept-license", "--no-java-check"])
                .arg(home),
        )
        .await
}
