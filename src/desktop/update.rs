use super::Device;
use crate::app::AppState;
use std::path::Path;

const REPOSITORY: &str = "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main";
const RELAUNCH: &str = "while kill -0 \"$1\" 2>/dev/null; do sleep 0.1; done; exec open -n \"$2\"";

pub async fn watch(device: Device) {
    device.update.watch(latest).await;
}

pub fn install(app: AppState) {
    app.device.update.install(app, run(app));
}

async fn latest() -> Result<String, String> {
    let manifest = manifest(&format!("{REPOSITORY}/Cargo.toml"))
        .await
        .map_err(|e| e.to_string())?;
    manifest_version(&manifest)
        .map(str::to_owned)
        .ok_or_else(|| "Cargo.toml has no version".to_owned())
}

async fn run(app: AppState) -> Result<(), String> {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.ancestors()
                .find(|a| a.extension().is_some_and(|e| e == "app"))
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let status = tokio::process::Command::new("bash")
        .arg("-c")
        .arg(format!(
            "set -o pipefail; curl -fsSL '{REPOSITORY}/scripts/macos/package.sh' | bash"
        ))
        .current_dir(&dir)
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(status.to_string());
    }
    match relaunch(std::process::id(), &dir.join("StreamVault.app")).spawn() {
        Ok(_) => dioxus::desktop::window().close(),
        Err(e) => app.report("Update installed, restart StreamVault", e),
    }
    Ok(())
}

fn relaunch(pid: u32, app: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("sh");
    command
        .args(["-c", RELAUNCH, "relaunch"])
        .arg(pid.to_string())
        .arg(app)
        .env_remove("XPC_SERVICE_NAME")
        .env_remove("XPC_FLAGS");
    command
}

async fn manifest(url: &str) -> reqwest::Result<String> {
    reqwest::get(url).await?.error_for_status()?.text().await
}

fn manifest_version(manifest: &str) -> Option<&str> {
    manifest.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix("version")?
            .trim_start()
            .strip_prefix('=')?;
        Some(value.trim().trim_matches('"'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{scratch_dir, serve};
    use std::os::unix::fs::PermissionsExt;
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
        let mut command = relaunch(old.id(), Path::new("/Applications/StreamVault.app"));
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
            "-n /Applications/StreamVault.app\n"
        );
        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_owned())
            .collect();
        assert_eq!(removed, ["XPC_FLAGS", "XPC_SERVICE_NAME"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn manifest_fetch_fails_on_http_errors() {
        let (url, _) = serve(&[(404, "Not Found"), (200, "version = \"9.9.9\"")]).await;
        assert!(manifest(&url).await.is_err());
        assert_eq!(manifest(&url).await.unwrap(), "version = \"9.9.9\"");
    }

    #[test]
    fn manifest_version_reads_package_version() {
        let manifest = "[package]\nname = \"x\"\nversion = \"1.2.3\"\n\n[dependencies]\nfoo = { version = \"9\" }\n";
        assert_eq!(manifest_version(manifest), Some("1.2.3"));
        assert_eq!(manifest_version("[package]\nversioned = 1\n"), None);
    }
}
