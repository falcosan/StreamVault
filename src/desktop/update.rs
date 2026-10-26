use super::{Device, os};
use crate::app::AppState;
use crate::release::{self, RELEASES};
use dioxus::logger::tracing::warn;
use std::path::Path;
use std::process::ExitStatus;

pub async fn watch(device: Device) {
    device.update.watch(|| release::latest(RELEASES)).await;
}

pub fn install(app: AppState) {
    app.device.update.install(app, run(app));
}

async fn run(app: AppState) -> Result<(), String> {
    let mut relaunch = os::relaunch();
    let installer = std::env::temp_dir().join(os::INSTALLER);
    release::download(RELEASES, os::INSTALLER, &installer).await?;
    let status = reinstall(&installer).await;
    if let Err(e) = tokio::fs::remove_file(&installer).await {
        warn!("{} remove error: {e}", installer.display());
    }
    let status = status.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(status.to_string());
    }
    match relaunch.spawn() {
        Ok(_) => dioxus::desktop::window().close(),
        Err(e) => app.report("Update installed, restart StreamVault", e),
    }
    Ok(())
}

async fn reinstall(installer: &Path) -> std::io::Result<ExitStatus> {
    os::installer(installer)?.arg("desktop").status().await
}
