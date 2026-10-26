#[cfg(not(windows))]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use macos as os;
#[cfg(windows)]
use windows as os;

use super::{Session, VERSION, asset, binary, find, say};
use std::path::PathBuf;
use tokio::process::Command;

pub async fn deploy() -> Result<(), String> {
    os::install(&Session::new("desktop")?).await
}

async fn app(session: &Session) -> Result<PathBuf, String> {
    let Some(root) = &session.source else {
        say(format!("Downloading StreamVault {VERSION}…"));
        return session.download(&asset(os::APP), os::APP).await;
    };
    say(format!("Building StreamVault {VERSION}…"));
    session
        .run(
            Command::new("cargo")
                .args(["build", "--release"])
                .current_dir(root),
        )
        .await?;
    Ok(binary(&root.join("target/release"), "streamvault"))
}

async fn tools(session: &Session) -> Result<[PathBuf; 2], String> {
    say("Installing FFmpeg and N_m3u8DL-RE…");
    let downloader = format!(
        "https://github.com/nilaoda/N_m3u8DL-RE/releases/download/v0.5.1-beta/N_m3u8DL-RE_v0.5.1-beta_{}_20251029.{}",
        os::DOWNLOADER,
        os::DOWNLOADER_ARCHIVE
    );
    let (ffmpeg, downloader) = tokio::try_join!(
        tool(session, os::FFMPEG, "ffmpeg"),
        tool(session, &downloader, "N_m3u8DL-RE")
    )?;
    Ok([ffmpeg, downloader])
}

async fn tool(session: &Session, url: &str, name: &str) -> Result<PathBuf, String> {
    let archive = url.rsplit_once('/').map_or(url, |(_, file)| file);
    let archive = session.download(url, archive).await?;
    let dir = session.scratch.join(name);
    session.unpack(&archive, &dir, &[]).await?;
    let wanted = binary(&dir, name);
    let file = wanted.file_name().unwrap_or_default();
    find(&dir, &|found| file == found).ok_or(format!("{url} has no {}.", file.display()))
}
