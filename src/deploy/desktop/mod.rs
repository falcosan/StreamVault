#[cfg(not(windows))]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use macos as os;
#[cfg(windows)]
use windows as os;

use super::{Session, VERSION, asset, binary, find, say, sha256};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{scratch_dir, serve};
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

    #[tokio::test]
    async fn a_download_that_is_not_the_pinned_file_is_refused() {
        let (site, _) = serve(&[(200, "abc"), (200, "abc")]).await;
        let session = Session::new("deploy_pin").unwrap();
        let url = format!("{site}/tool.zip");
        let refused = tool(&session, (&url, "other"), "ffmpeg").await.unwrap_err();
        assert!(refused.contains("is not the file"), "{refused}");
        let pinned = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let unpacked = tool(&session, (&url, pinned), "ffmpeg").await.unwrap_err();
        assert!(!unpacked.contains("is not the file"), "{unpacked}");
        remove(&session.out);
        remove(&session.log);
    }
}
