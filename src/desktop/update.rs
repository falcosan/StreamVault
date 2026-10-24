use super::Device;
use crate::app::AppState;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use std::path::Path;

const REPOSITORY: &str = "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main";

pub async fn check(mut device: Device) {
    let Ok(response) = reqwest::get(format!("{REPOSITORY}/Cargo.toml")).await else {
        return;
    };
    let manifest = response.text().await.unwrap_or_default();
    if manifest_version(&manifest).is_some_and(|v| is_newer(v, env!("CARGO_PKG_VERSION"))) {
        device.update_available.set(true);
    }
}

pub fn install(mut app: AppState) {
    let mut device = app.device;
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
            .await
            .is_ok_and(|s| s.success());
        if installed {
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).spawn();
            }
            return dioxus::desktop::window().close();
        }
        device.updating.set(false);
        app.error.set(Some("Update failed".into()));
    });
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
