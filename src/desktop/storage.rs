use super::config::{
    DOWNLOADS_FILE, DOWNLOADS_LOCK, LANGUAGE_FILE, PREFS_FILE, REPLICA_FILE, WATCH_FILE,
    missing_ok, read_json, state_dir, write_json,
};
use super::download::Download;
use crate::folders::sync_file;
use crate::i18n::Lang;
use crate::prefs::PlayerPrefs;
use crate::settings::SyncConfig;
use crate::sync::Replica;
use crate::time::{epoch_ms, now_ms, sleep};
use crate::watch::WatchItem;
use dioxus::logger::tracing::warn;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

const CLAIM_ATTEMPTS: u32 = 20;
const CLAIM_PAUSE: Duration = Duration::from_millis(500);

pub fn load_sync_config() -> SyncConfig {
    setting(&sync_file())
}

pub async fn load_replica() -> Option<Replica> {
    migrate(state_dir())
}

pub fn save_replica(replica: &Replica) {
    save(&state_dir().join(REPLICA_FILE), replica);
}

pub async fn load_language() -> Option<Lang> {
    language(state_dir())
}

pub fn save_language(lang: Lang) {
    save(&state_dir().join(LANGUAGE_FILE), lang.code());
}

pub fn load_downloads(root: &Path) -> Vec<Download> {
    load::<Vec<Download>>(&state_dir().join(DOWNLOADS_FILE))
        .into_iter()
        .filter_map(|download| download.restored(root))
        .collect()
}

pub fn save_downloads(downloads: &[Download]) {
    save(&state_dir().join(DOWNLOADS_FILE), downloads);
}

pub async fn claim_downloads() -> bool {
    claim_soon(&state_dir().join(DOWNLOADS_LOCK)).await
}

async fn claim_soon(path: &Path) -> bool {
    for _ in 0..CLAIM_ATTEMPTS {
        if claim(path) {
            return true;
        }
        sleep(CLAIM_PAUSE).await;
    }
    false
}

fn claim(path: &Path) -> bool {
    let Ok(lock) = fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
    else {
        return true;
    };
    let claimed = lock.try_lock().is_ok();
    if claimed {
        std::mem::forget(lock);
    }
    claimed
}

fn language(dir: &Path) -> Option<Lang> {
    Lang::parse(&load::<String>(&dir.join(LANGUAGE_FILE)))
}

fn migrate(dir: &Path) -> Option<Replica> {
    let path = dir.join(REPLICA_FILE);
    let saved = load::<Option<Replica>>(&path);
    let (watch, prefs) = (dir.join(WATCH_FILE), dir.join(PREFS_FILE));
    if !watch.is_file() && !prefs.is_file() {
        return saved;
    }
    let mut replica = saved.unwrap_or_else(Replica::fresh);
    if watch.is_file() {
        let items = parse_items(load(&watch));
        replica.record_watch(&items, modified(&watch));
    }
    if prefs.is_file() {
        replica.record_prefs(&load::<PlayerPrefs>(&prefs), modified(&prefs));
    }
    if save(&path, &replica) {
        for legacy in [watch, prefs] {
            if let Err(e) = missing_ok(fs::remove_file(&legacy)) {
                warn!("{} remove error: {e}", legacy.display());
            }
        }
    }
    Some(replica)
}

fn setting<T: DeserializeOwned + Default>(path: &Path) -> T {
    read_json(path).unwrap_or_else(|e| {
        if e.kind() != io::ErrorKind::NotFound {
            warn!("ignoring unreadable {}: {e}", path.display());
        }
        T::default()
    })
}

fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    read_json(path).unwrap_or_else(|e| {
        if e.kind() != io::ErrorKind::NotFound {
            warn!("{} read error: {e}", path.display());
            if let Err(e) = fs::rename(path, path.with_added_extension("unreadable")) {
                warn!("{} cannot be set aside: {e}", path.display());
            }
        }
        T::default()
    })
}

fn save(path: &Path, value: &(impl Serialize + ?Sized)) -> bool {
    write_json(path, value)
        .inspect_err(|e| warn!("{} write error: {e}", path.display()))
        .is_ok()
}

fn parse_items(values: Vec<Value>) -> Vec<WatchItem> {
    values
        .into_iter()
        .filter_map(|v| {
            serde_json::from_value(v)
                .inspect_err(|e| warn!("skipping watch item: {e}"))
                .ok()
        })
        .collect()
}

fn modified(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map_or(0, epoch_ms)
        .min(now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::scratch_dir;
    use crate::providers::MediaEntry;
    use serde_json::json;

    fn item(id: u64, time: f64) -> WatchItem {
        WatchItem {
            entry: MediaEntry {
                id,
                ..Default::default()
            },
            current_time: time,
            duration: 100.0,
            season: None,
            episode: None,
        }
    }

    #[test]
    fn parse_items_drops_invalid_entries() {
        let valid = serde_json::to_value(item(7, 5.0)).unwrap();
        let items = parse_items(vec![json!({ "entry": { "id": null } }), valid]);
        assert_eq!(items.iter().map(|i| i.entry.id).collect::<Vec<_>>(), [7]);
    }

    #[test]
    fn the_chosen_language_comes_back() {
        let dir = scratch_dir("language");
        assert_eq!(language(&dir), None);
        save(&dir.join(LANGUAGE_FILE), Lang::It.code());
        assert_eq!(language(&dir), Some(Lang::It));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_file_is_set_aside_instead_of_overwritten() {
        let dir = scratch_dir("unreadable");
        let path = dir.join(REPLICA_FILE);
        fs::write(&path, "{ not json").unwrap();
        assert!(load::<Option<Replica>>(&path).is_none());
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(path.with_added_extension("unreadable")).unwrap(),
            "{ not json"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hand_written_setting_that_does_not_parse_stays_where_it_is() {
        let dir = scratch_dir("setting");
        let path = dir.join("sync.json");
        fs::write(&path, r#"{ "enabled": true, }"#).unwrap();
        assert!(!setting::<SyncConfig>(&path).enabled);
        assert!(path.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_first_claim_of_the_downloads_succeeds() {
        let dir = scratch_dir("claim");
        let path = dir.join(DOWNLOADS_LOCK);
        assert!(claim(&path));
        assert!(!claim(&path));
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_downloads_pass_on_once_the_old_app_has_quit() {
        let dir = scratch_dir("claim_handover");
        let path = dir.join(DOWNLOADS_LOCK);
        let old_app = fs::File::create(&path).unwrap();
        old_app.try_lock().unwrap();
        let quits = tokio::spawn(async move {
            sleep(CLAIM_PAUSE / 2).await;
            drop(old_app);
        });
        assert!(claim_soon(&path).await);
        quits.await.unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn a_second_app_stops_asking_for_downloads_it_cannot_get() {
        let dir = scratch_dir("claim_refused");
        let path = dir.join(DOWNLOADS_LOCK);
        assert!(claim(&path));
        let started = tokio::time::Instant::now();
        assert!(!claim_soon(&path).await);
        assert_eq!(started.elapsed(), CLAIM_PAUSE * CLAIM_ATTEMPTS);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_folds_legacy_files_into_the_replica_once() {
        let dir = scratch_dir("migrate_legacy");
        let prefs = PlayerPrefs {
            audio_lang: "ita".into(),
            ..PlayerPrefs::default()
        };
        write_json(&dir.join(WATCH_FILE), &[item(1, 5.0)]).unwrap();
        write_json(&dir.join(PREFS_FILE), &prefs).unwrap();
        let replica = migrate(&dir).unwrap();
        assert_eq!(replica.watch(), [item(1, 5.0)]);
        assert_eq!(replica.prefs(), Some(prefs));
        assert!(replica.is_dirty());
        assert!(!dir.join(WATCH_FILE).exists() && !dir.join(PREFS_FILE).exists());
        assert_eq!(migrate(&dir), Some(replica));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_without_any_state_is_empty() {
        let dir = scratch_dir("migrate_empty");
        assert_eq!(migrate(&dir), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn modified_is_zero_for_missing_files() {
        assert_eq!(modified(Path::new("/nonexistent/streamvault")), 0);
    }
}
