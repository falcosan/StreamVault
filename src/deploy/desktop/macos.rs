use crate::deploy::{Session, VERSION, copy, copy_dir, replace_dir, say, write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::process::Command;

pub const APP: &str = "StreamVault-macos";

pub const FFMPEG: (&str, &str) = if cfg!(target_arch = "aarch64") {
    (
        "https://www.osxexperts.net/ffmpeg71arm.zip",
        "0878f3313311c2c1b2c818e7c955c0bd828c97b357fa86211b42a5c36d01e36f",
    )
} else {
    (
        "https://evermeet.cx/ffmpeg/ffmpeg-7.1.zip",
        "5a1303c7babaffff3c32c141ff49c7f44bd3b3b3e7dcea992fd7d04b6558ef43",
    )
};
pub const DOWNLOADER: (&str, &str) = if cfg!(target_arch = "aarch64") {
    (
        "osx-arm64",
        "ca181b7e8976564766c34e3cefe48fff409f58289ca970ade062668c01c53918",
    )
} else {
    (
        "osx-x64",
        "4dab8d256ae3164557f48a3326c8fa60da99e37536ee1589ce6c841180fa8204",
    )
};
pub const DOWNLOADER_ARCHIVE: &str = "tar.gz";
const BUNDLE: &str = "StreamVault.app";
const PLIST: &str = include_str!("../../../resources/desktop/macos/Info.plist");
const ICON: &[u8] = include_bytes!("../../../resources/desktop/macos/AppIcon.icns");

pub async fn install(session: &Session) -> Result<(), String> {
    let binary = super::app(session).await?;
    let app = session.scratch.join(BUNDLE);
    let contents = app.join("Contents");
    let plist = contents.join("Info.plist");
    let executable = contents.join("MacOS/streamvault");
    copy(&binary, &executable)?;
    executable_by_all(&executable)?;
    write(&plist, PLIST)?;
    write(&contents.join("Resources/AppIcon.icns"), ICON)?;
    for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
        session
            .run(
                Command::new("plutil")
                    .args(["-replace", key, "-string", VERSION])
                    .arg(&plist),
            )
            .await?;
    }
    let mut bins = Vec::new();
    for tool in super::tools(session).await? {
        let bin = contents
            .join("Resources/bin")
            .join(tool.file_name().unwrap_or_default());
        copy(&tool, &bin)?;
        executable_by_all(&bin)?;
        bins.push(bin);
    }
    session
        .run(Command::new("xattr").arg("-cr").arg(&app))
        .await?;
    for code in bins.iter().chain([&app]) {
        session
            .run(
                Command::new("codesign")
                    .args(["--force", "--sign", "-"])
                    .arg(code),
            )
            .await?;
    }
    let installed = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(BUNDLE);
    replace_dir(&installed, |staged| copy_dir(&app, staged))?;
    say(format!(
        "StreamVault {VERSION} installed: {}",
        installed.display()
    ));
    Ok(())
}

fn executable_by_all(file: &Path) -> Result<(), String> {
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Cannot make {} executable: {e}", file.display()))
}
