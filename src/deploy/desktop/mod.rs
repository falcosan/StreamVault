#[cfg(not(windows))]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use macos as os;
#[cfg(windows)]
use windows as os;

use super::{Session, VERSION, asset, binary, find, say};
use rsa::sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
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
    Ok(binary(
        &session.target_dir(root).await?.join("release"),
        "streamvault",
    ))
}

async fn tools(session: &Session) -> Result<[PathBuf; 2], String> {
    say("Installing FFmpeg and N_m3u8DL-RE…");
    let (platform, downloader_sha256) = os::DOWNLOADER;
    let downloader = format!(
        "https://github.com/nilaoda/N_m3u8DL-RE/releases/download/v0.6.0-beta/N_m3u8DL-RE_v0.6.0-beta_{platform}_20260629.{}",
        os::DOWNLOADER_ARCHIVE
    );
    let (ffmpeg, downloader) = tokio::try_join!(
        tool(session, os::FFMPEG, "ffmpeg"),
        tool(session, (&downloader, downloader_sha256), "N_m3u8DL-RE")
    )?;
    Ok([ffmpeg, downloader])
}

async fn tool(
    session: &Session,
    (url, pinned): (&str, &str),
    name: &str,
) -> Result<PathBuf, String> {
    let archive = url.rsplit_once('/').map_or(url, |(_, file)| file);
    let archive = session.download(url, archive).await?;
    if sha256(&archive)? != pinned {
        return Err(format!(
            "{url} is not the file this installer was built for."
        ));
    }
    let dir = session.scratch.join(name);
    session.unpack(&archive, &dir, &[]).await?;
    let wanted = binary(&dir, name);
    let file = wanted.file_name().unwrap_or_default();
    find(&dir, &|found| file == found).ok_or_else(|| format!("{url} has no {}.", file.display()))
}

fn sha256(file: &Path) -> Result<String, String> {
    let mut reader =
        std::fs::File::open(file).map_err(|e| format!("Cannot read {}: {e}", file.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Cannot read {}: {e}", file.display()))?;
        if read == 0 {
            return Ok(format!("{:x}", hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;
    use crate::deploy::{remove, write};

    #[test]
    fn a_download_is_checked_by_its_sha256() {
        let dir = scratch_dir("deploy_sha256");
        write(&dir.join("abc"), "abc").unwrap();
        assert_eq!(
            sha256(&dir.join("abc")).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(sha256(&dir.join("missing")).is_err());
        remove(&dir);
    }
}
