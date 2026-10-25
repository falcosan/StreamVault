use super::config::{
    DOWNLOADS_FILE, PREFS_FILE, REPLICA_FILE, WATCH_FILE, missing_ok, read_json, state_dir,
    write_json,
};
use super::download::Download;
use crate::prefs::PlayerPrefs;
use crate::sync::Replica;
use crate::time::{epoch_ms, now_ms};
use crate::watch::WatchItem;
use dioxus::logger::tracing::warn;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::Path;

pub async fn load_replica() -> Option<Replica> {
    migrate(state_dir())
}

pub fn save_replica(replica: &Replica) {
    save(&state_dir().join(REPLICA_FILE), replica);
}

pub fn load_downloads() -> Vec<Download> {
    load::<Vec<Download>>(&state_dir().join(DOWNLOADS_FILE))
        .into_iter()
        .filter_map(Download::restored)
        .collect()
}

pub fn save_downloads(downloads: &[Download]) {
    save(&state_dir().join(DOWNLOADS_FILE), downloads);
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

pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    read_json(path).unwrap_or_else(|e| {
        if e.kind() != io::ErrorKind::NotFound {
            warn!("{} read error: {e}", path.display());
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
