use super::Device;
use crate::app::AppState;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use std::path::Path;

const REPOSITORY: &str = "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main";

pub async fn check(mut device: Device) {
    match manifest(&format!("{REPOSITORY}/Cargo.toml")).await {
        Ok(manifest) => {
            if manifest_version(&manifest).is_some_and(|v| is_newer(v, env!("CARGO_PKG_VERSION"))) {
                device.update_available.set(true);
            }
        }
        Err(e) => warn!("update check failed: {e}"),
    }
}

pub fn install(app: AppState) {
    let mut device = app.device;
    if *device.updating.peek() {
        return;
    }
    device.updating.set(true);
    spawn_forever(async move {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.ancestors()
                    .find(|a| a.extension().is_some_and(|e| e == "app"))
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let installed = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(format!(
                "set -o pipefail; curl -fsSL '{REPOSITORY}/scripts/package.sh' | bash"
            ))
            .current_dir(dir)
            .status()
            .await;
        device.updating.set(false);
        match installed {
            Ok(status) if status.success() => {}
            Ok(status) => return app.report("Update failed", status),
            Err(e) => return app.report("Update failed", e),
        }
        match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn()) {
            Ok(_) => dioxus::desktop::window().close(),
            Err(e) => app.report("Update installed, restart StreamVault", e),
        }
    });
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

fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |v: &str| {
        v.split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    match (parse(remote), parse(local)) {
        (Some(remote), Some(local)) => remote > local,
        _ => remote != local,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::serve;

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

    #[test]
    fn is_newer_compares_numerically() {
        assert!(is_newer("1.0.31", "1.0.30"));
        assert!(is_newer("1.0.100", "1.0.99"));
        assert!(!is_newer("1.0.30", "1.0.30"));
        assert!(!is_newer("1.0.29", "1.0.30"));
        assert!(is_newer("1.1.0-beta", "1.0.30"));
    }
}
