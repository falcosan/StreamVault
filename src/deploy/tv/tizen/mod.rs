mod certificate;
mod sdb;
mod widget;

use super::{ICON, between, bundle, discover, os};
use crate::deploy::{Session, VERSION, remove, say, wait_until, write};
use crate::settings::SyncConfig;
use certificate::{Certificate, Signers};
use sdb::Sdb;
use serde_json::Value;
use std::net::Ipv4Addr;
use std::path::Path;

pub const API: u16 = 8001;
const INSTALLED: &str = "install completed";
const WAITING: &str = "Waiting for the TV to accept the connection (restart it if this lasts)…";
const PACKAGE: &str = "/home/owner/share/tmp/sdk_tools/StreamVault.wgt";
const CONFIG: &str = include_str!("../../../../resources/tv/tizen/config.xml");

pub async fn device(http: &reqwest::Client, ip: Ipv4Addr) -> Option<Value> {
    let info: Value =
        serde_json::from_str(&discover::get(http, ip, API, "/api/v2/").await?).ok()?;
    let device = info.get("device")?;
    (device["OS"] == "Tizen").then(|| device.clone())
}

pub async fn deploy(
    session: &Session,
    ip: Ipv4Addr,
    sync: Option<&SyncConfig>,
) -> Result<(), String> {
    let host = discover::local_ip(ip).ok_or("This computer has no address on the TV's network.")?;
    let developer = format!(
        "On the TV: Apps, type 1 2 3 4 5, set Developer mode On with Host PC IP {host}, then restart the TV."
    );
    wait_until(&developer, || async move {
        developer_mode(&session.lan, ip, host).await.then_some(())
    })
    .await;
    let public = bundle::build(session).await?;
    let mut tv = wait_until(WAITING, || async { Sdb::connect(ip).await.ok() }).await;
    let duid = tv.shell("0 getduid").await.map_err(lost)?;
    let certificate = Certificate::of(duid.trim());
    let app = between(CONFIG, "<tizen:application id=\"", "\"")
        .ok_or("config.xml has no application id.")?;
    let stage = session.out.join("tizen");
    remove(&stage);
    bundle::stage(&public, &stage, sync)?;
    let widget = format!("<widget version=\"{VERSION}\" ");
    write(
        &stage.join("config.xml"),
        CONFIG.replacen("<widget ", &widget, 1),
    )?;
    write(&stage.join("icon.png"), ICON)?;
    tv.shell(&format!("0 was_kill {app}")).await.map_err(lost)?;
    let stored = certificate.load().unwrap_or_else(|e| {
        say(e);
        None
    });
    let outcome = match &stored {
        Some(signers) => install(session, &mut tv, app, &stage, signers).await?,
        None => String::new(),
    };
    if !outcome.contains(INSTALLED) {
        if stored.is_some() && !outcome.to_lowercase().contains("certificate") {
            return Err(refused(&outcome));
        }
        let signers = certificate.create(session).await?;
        let outcome = install(session, &mut tv, app, &stage, &signers).await?;
        if !outcome.contains(INSTALLED) {
            return Err(refused(&outcome));
        }
    }
    tv.shell(&format!("0 was_execute {app}"))
        .await
        .map_err(lost)?;
    Ok(())
}

async fn install(
    session: &Session,
    tv: &mut Sdb,
    app: &str,
    stage: &Path,
    signers: &Signers,
) -> Result<String, String> {
    let wgt = widget::package(stage, &signers.author, &signers.distributor)?;
    write(&session.out.join("StreamVault.wgt"), &wgt)?;
    tv.push(PACKAGE, &wgt).await.map_err(lost)?;
    let outcome = tv
        .shell(&format!("0 vd_appinstall {app} {PACKAGE}"))
        .await
        .map_err(lost)?;
    session.append(&outcome)?;
    Ok(outcome)
}

async fn developer_mode(http: &reqwest::Client, ip: Ipv4Addr, host: Ipv4Addr) -> bool {
    device(http, ip).await.is_some_and(|device| {
        device["developerMode"] == "1" && device["developerIP"] == host.to_string()
    })
}

fn lost(error: std::io::Error) -> String {
    format!("The connection to the TV failed: {error}")
}

fn refused(outcome: &str) -> String {
    let reason = outcome
        .lines()
        .find(|line| {
            ["fail", "error"]
                .iter()
                .any(|word| line.to_lowercase().contains(word))
        })
        .unwrap_or(outcome);
    format!("The TV refused the app: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_shows_the_failing_line() {
        assert_eq!(
            refused(
                "app_id[A.B] install start\napp_id[A.B] install failed[118, -14], reason: Signature error\nspend time for wascmd is [340]ms"
            ),
            "The TV refused the app: app_id[A.B] install failed[118, -14], reason: Signature error"
        );
        assert_eq!(refused("odd"), "The TV refused the app: odd");
    }
}
