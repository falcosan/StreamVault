use super::Device;
use crate::app::AppState;
use std::path::Path;

const REPOSITORY: &str = "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main";

pub async fn check(device: Device) {
    device.update.check(latest()).await;
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
            "set -o pipefail; curl -fsSL '{REPOSITORY}/scripts/package.sh' | bash"
        ))
        .current_dir(dir)
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(status.to_string());
    }
    match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn()) {
        Ok(_) => dioxus::desktop::window().close(),
        Err(e) => app.report("Update installed, restart StreamVault", e),
    }
    Ok(())
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
}
