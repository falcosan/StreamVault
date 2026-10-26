mod android;
mod bundle;
mod discover;
#[cfg(not(windows))]
mod macos;
mod tizen;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use macos as os;
#[cfg(windows)]
use windows as os;

use super::{Session, VERSION, ask, say};
use crate::folders;
use crate::settings::SyncConfig;
use discover::Target;

const ICON: &[u8] = include_bytes!("../../../assets/img/raster/logo_raster_small.png");

pub async fn deploy() -> Result<(), String> {
    let session = Session::new("tv")?;
    let tv = discover::choose(&session).await?;
    let sync = sync();
    match &tv.target {
        Target::Tizen(ip) => tizen::deploy(&session, *ip, sync.as_ref()).await?,
        Target::Android(target) => android::deploy(&session, target, sync.as_ref()).await?,
    }
    let state = if sync.is_some() { "on" } else { "off" };
    say(format!(
        "StreamVault {VERSION} installed on {} (sync {state}).",
        tv.address
    ));
    Ok(())
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = &text[text.find(start)? + start.len()..];
    Some(&rest[..rest.find(end)?])
}

fn sync() -> Option<SyncConfig> {
    let saved = std::fs::read_to_string(folders::sync_file()).ok();
    let config = sync_config(saved.as_deref(), |name| std::env::var(name).ok())?;
    let answer = std::env::var("SYNC").ok().or_else(|| {
        ask("Sync this TV with your Continue Watching and preferences? Answer n on someone else's TV. [Y/n]")
    });
    wanted(answer.as_deref().unwrap_or_default()).then_some(config)
}

fn sync_config(saved: Option<&str>, env: impl Fn(&str) -> Option<String>) -> Option<SyncConfig> {
    let saved = saved
        .and_then(|text| serde_json::from_str::<SyncConfig>(text).ok())
        .filter(SyncConfig::is_active)
        .unwrap_or_default();
    let config = SyncConfig {
        enabled: true,
        endpoint: env("STREAMVAULT_SYNC_URL").unwrap_or(saved.endpoint),
        token: env("STREAMVAULT_SYNC_TOKEN").unwrap_or(saved.token),
    };
    config.is_active().then_some(config)
}

fn wanted(answer: &str) -> bool {
    !(answer.starts_with(['n', 'N']) || answer == "off")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn between_reads_the_text_inside_two_markers() {
        assert_eq!(between("a <b>c</b> d", "<b>", "</b>"), Some("c"));
        assert_eq!(between("a <b>c", "<b>", "</b>"), None);
        assert_eq!(between("a", "<b>", "</b>"), None);
    }

    const SAVED: &str =
        r#"{"enabled":true,"endpoint":"https://script.example/exec","token":"t0k"}"#;

    #[test]
    fn an_active_sync_file_gives_the_endpoint_and_token() {
        let config = sync_config(Some(SAVED), |_| None).unwrap();
        assert_eq!(
            (config.endpoint.as_str(), config.token.as_str()),
            ("https://script.example/exec", "t0k")
        );
        assert!(sync_config(Some(&SAVED.replace("true", "false")), |_| None).is_none());
        assert!(sync_config(None, |_| None).is_none());
    }

    #[test]
    fn the_environment_overrides_the_sync_file() {
        let env = |name: &str| {
            (name == "STREAMVAULT_SYNC_URL").then(|| "https://other.example".to_owned())
        };
        let config = sync_config(Some(SAVED), env).unwrap();
        assert_eq!(
            (config.endpoint.as_str(), config.token.as_str()),
            ("https://other.example", "t0k")
        );
        assert!(sync_config(None, env).is_none());
    }

    #[test]
    fn only_no_or_off_leaves_sync_out() {
        assert!(wanted("") && wanted("y") && wanted("on"));
        assert!(!wanted("n") && !wanted("No") && !wanted("off"));
    }
}
