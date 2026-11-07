use super::{between, os};
use crate::deploy::{
    Session, VERSION, asset, binary, copy_dir, read, remove, say, sha256, tidy, write,
};
use crate::settings::{SYNC_FILE, SyncConfig};
use futures::future::try_join_all;
use serde::Deserialize;
use std::env::consts::ARCH;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const WASM: &str = "wasm32-unknown-unknown";
const PUBLIC: &str = "dx/streamvault/release/web/public";
const ENTRY: &str = r#"<script type="module" async src=""#;
const BOOT: &str = include_str!("../../../assets/js/boot.js");

#[derive(Deserialize)]
struct Manifest {
    files: Vec<File>,
}

#[derive(Deserialize)]
struct File {
    name: String,
    sha256: String,
}

pub async fn build(session: &Session) -> Result<PathBuf, String> {
    match &session.source {
        Some(root) => compile(session, root).await,
        None => download(session, asset).await,
    }
}

async fn download(session: &Session, url: impl Fn(&str) -> String) -> Result<PathBuf, String> {
    say(format!("Downloading StreamVault {VERSION}…"));
    let manifest = session.download(&url("tv.json"), "tv.json").await?;
    let manifest: Manifest = serde_json::from_str(&read(&manifest)?)
        .map_err(|e| format!("tv.json is not a TV release: {e}"))?;
    session
        .download(&url("index.html"), "public/index.html")
        .await?;
    try_join_all(manifest.files.iter().map(|file| async {
        let name = plain(&file.name)?;
        let saved = session
            .download(&url(name), &format!("public/assets/{name}"))
            .await?;
        if sha256(&saved)? == file.sha256 {
            Ok(())
        } else {
            Err(format!("{name} is not the file tv.json lists."))
        }
    }))
    .await?;
    Ok(session.scratch.join("public"))
}

fn plain(name: &str) -> Result<&str, String> {
    Path::new(name)
        .file_name()
        .is_some_and(|file| file == name)
        .then_some(name)
        .ok_or_else(|| format!("tv.json lists {name}, which is not a file name."))
}

async fn compile(session: &Session, root: &Path) -> Result<PathBuf, String> {
    let dx = dx(session, root).await?;
    target(session).await?;
    say(format!("Building StreamVault {VERSION}…"));
    let public = session.target_dir(root).await?.join(PUBLIC);
    remove(&public);
    session
        .run(
            Command::new(dx)
                .args([
                    "build",
                    "--release",
                    "--platform",
                    "web",
                    "--debug-symbols",
                    "false",
                ])
                .current_dir(root)
                .env("DX_HOME", session.scratch.join("dx-home")),
        )
        .await?;
    Ok(public)
}

pub fn stage(public: &Path, dir: &Path, sync: Option<&SyncConfig>) -> Result<(), String> {
    copy_dir(public, dir)?;
    write(
        &dir.join("index.html"),
        boot(&read(&public.join("index.html"))?)?,
    )?;
    write(&dir.join("boot.js"), BOOT)?;
    if let Some(sync) = sync {
        write(
            &dir.join(SYNC_FILE),
            serde_json::to_string(sync).map_err(|e| e.to_string())?,
        )?;
    }
    Ok(())
}

fn boot(index: &str) -> Result<String, String> {
    let entry =
        between(index, ENTRY, "\"").ok_or("The web build has no app script in index.html.")?;
    Ok(index.replace(
        &format!(r#"{ENTRY}{entry}"></script>"#),
        &format!(
            r#"<script src="boot.js" data-version="{VERSION}" data-entry="{entry}"></script>"#
        ),
    ))
}

async fn dx(session: &Session, root: &Path) -> Result<PathBuf, String> {
    let lock = read(&root.join("Cargo.lock"))?;
    let version = between(&lock, "name = \"dioxus\"\nversion = \"", "\"")
        .ok_or("Cargo.lock has no dioxus version.")?;
    let installed = Command::new("dx")
        .arg("--version")
        .output()
        .await
        .is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout).starts_with(&format!("dioxus {version} "))
        });
    if installed {
        return Ok(PathBuf::from("dx"));
    }
    say(format!("Installing dx {version}…"));
    let url = format!(
        "https://github.com/DioxusLabs/dioxus/releases/download/v{version}/dx-{ARCH}-{}.{}",
        os::TARGET,
        os::ARCHIVE
    );
    let archive = session
        .download(&url, &format!("dx.{}", os::ARCHIVE))
        .await?;
    let dir = session.scratch.join("dx");
    let dx = binary(&dir, "dx");
    let name = dx
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    session.unpack(&archive, &dir, &[&name]).await?;
    Ok(dx)
}

async fn target(session: &Session) -> Result<(), String> {
    let installed = session
        .output(Command::new("rustup").args(["target", "list", "--installed"]))
        .await?;
    if installed.lines().all(|target| target != WASM) {
        session
            .run(Command::new("rustup").args(["target", "add", WASM]))
            .await?;
        session.on_exit(|| tidy(Path::new("rustup"), &["target", "remove", WASM]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::serve;

    #[tokio::test]
    async fn a_file_that_is_not_the_one_tv_json_lists_is_refused() {
        let (site, _) = serve(&[
            (200, r#"{ "files": [{ "name": "app.js", "sha256": "other" }] }"#),
            (200, "<html>"),
            (200, "abc"),
            (
                200,
                r#"{ "files": [{ "name": "app.js", "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" }] }"#,
            ),
            (200, "<html>"),
            (200, "abc"),
        ])
        .await;
        let session = Session::new("deploy_bundle").unwrap();
        let url = |name: &str| format!("{site}/{name}");
        let refused = download(&session, url).await.unwrap_err();
        assert!(
            refused.contains("is not the file tv.json lists"),
            "{refused}"
        );
        assert!(download(&session, url).await.is_ok());
        remove(&session.out);
        remove(&session.log);
    }

    #[test]
    fn the_page_starts_through_boot_js() {
        let index =
            r#"<head><script type="module" async src="/./assets/app-1.js"></script></head>"#;
        assert_eq!(
            boot(index).unwrap(),
            format!(
                r#"<head><script src="boot.js" data-version="{VERSION}" data-entry="/./assets/app-1.js"></script></head>"#
            )
        );
        assert!(boot("<head></head>").is_err());
    }

    #[test]
    fn boot_js_updates_from_the_same_releases() {
        let releases = format!("const RELEASES = \"{}\";", crate::release::RELEASES);
        assert!(BOOT.contains(&releases));
    }

    #[test]
    fn only_plain_file_names_are_downloaded() {
        let manifest: Manifest = serde_json::from_str(
            r#"{"version":"1.1.4","shell":1,"entry":"app-1.js","files":[{"name":"app-1.js","sha256":"00"}]}"#,
        )
        .unwrap();
        assert_eq!(plain(&manifest.files[0].name), Ok("app-1.js"));
        assert!(plain("../app.js").is_err());
        assert!(plain("assets/app.js").is_err());
        assert!(plain("..").is_err());
    }
}
