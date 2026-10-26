use crate::deploy::{Session, remove};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub const TARGET: &str = "apple-darwin";
pub const ARCHIVE: &str = "tar.gz";
pub const SCRIPT: &str = "";
pub const JDK_OS: &str = "mac";
pub const JDK_HOME: &str = "Contents/Home";
pub const ANDROID_OS: &str = "mac";
pub const PACKAGE_MANAGER: &str = "package-manager-cli.bin";

pub fn tizen_home() -> PathBuf {
    home().join("tizen-studio")
}

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

pub async fn install_tizen_studio(
    session: &Session,
    studio: &str,
    home: &Path,
) -> Result<(), String> {
    let image = session
        .download(
            &format!("{studio}/web-ide_Tizen_Studio_6.1_macos-64.dmg"),
            "tizen.dmg",
        )
        .await?;
    let packages = self::home().join(".package-manager");
    if !packages.exists() {
        let packages = packages.clone();
        session.on_exit(move || remove(&packages));
    }
    let mount = session.scratch.join("tizen");
    session
        .run(
            Command::new("hdiutil")
                .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
                .arg(&mount)
                .arg(&image),
        )
        .await?;
    let installer = mount.join("installer.app/Contents/Resources/Java");
    let mut installed = session
        .unpack(&installer.join("tizen-sdk.zip"), &packages, &["jdk/*"])
        .await;
    if installed.is_ok() {
        installed = session
            .run(
                Command::new(packages.join("jdk/Contents/Home/bin/java"))
                    .arg("-cp")
                    .arg(installer.join("installer.jar"))
                    .args([
                        "org.tizen.installer.cli.CLIMain",
                        "--accept-license",
                        "--no-java-check",
                    ])
                    .arg(home),
            )
            .await;
    }
    session
        .run(Command::new("hdiutil").arg("detach").arg(&mount))
        .await?;
    installed
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}
