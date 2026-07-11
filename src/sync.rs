use crate::config::{
    player_prefs_path, watch_items_path, write_json_unlocked, AppConfig, PlayerPrefs, WatchItem,
    FS_LOCK,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

const TICK_SECS: u64 = 10;
const PULL_INTERVAL_MS: u64 = 60_000;
const MAX_ITEMS: usize = 100;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub token: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncDoc {
    pub rev: u64,
    pub prefs_updated_at: u64,
    #[serde(deserialize_with = "lenient_items")]
    pub watch: Vec<WatchItem>,
    pub stamps: HashMap<String, u64>,
    pub prefs: Option<PlayerPrefs>,
}

fn lenient_items<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WatchItem>, D::Error> {
    let raw: Vec<serde_json::Value> = Vec::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Snapshot {
    doc: SyncDoc,
    ledger: HashMap<String, LedgerEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LedgerEntry {
    stamp: u64,
    item: WatchItem,
}

#[derive(Debug)]
pub struct MergedState {
    pub watch: Vec<WatchItem>,
    pub prefs: PlayerPrefs,
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    conflict: bool,
    #[serde(default)]
    doc: Option<SyncDoc>,
}

#[derive(Serialize)]
struct PushBody<'a> {
    token: &'a str,
    base_rev: u64,
    watch: &'a [WatchItem],
    stamps: &'a HashMap<String, u64>,
    prefs: &'a PlayerPrefs,
    prefs_updated_at: u64,
}

enum PushOutcome {
    Accepted(SyncDoc),
    Conflict(SyncDoc),
    Failed,
}

enum FileState<T> {
    Data(T),
    Missing,
    Unreadable,
}

struct LocalState {
    watch: Vec<WatchItem>,
    prefs: PlayerPrefs,
    watch_mtime: u64,
    prefs_mtime: u64,
    missing: bool,
}

static STARTED: AtomicBool = AtomicBool::new(false);

pub fn start() -> Option<UnboundedReceiver<MergedState>> {
    let cfg: SyncConfig = read_json(&AppConfig::config_dir().join("sync.json"))?;
    if !cfg.enabled || cfg.endpoint.trim().is_empty() || cfg.token.trim().is_empty() {
        return None;
    }
    if STARTED.swap(true, Ordering::SeqCst) {
        return None;
    }
    let (tx, rx) = unbounded_channel();
    std::thread::Builder::new()
        .name("streamvault-sync".into())
        .spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                rt.block_on(run(cfg, tx));
            }
        })
        .ok()?;
    Some(rx)
}

fn snapshot_path() -> PathBuf {
    AppConfig::config_dir().join("sync_snapshot.json")
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    match load_state(path) {
        FileState::Data(t) => Some(t),
        _ => None,
    }
}

fn load_state<T: serde::de::DeserializeOwned>(path: &Path) -> FileState<T> {
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_or(FileState::Unreadable, FileState::Data),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => FileState::Missing,
        Err(_) => FileState::Unreadable,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn mtime_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_millis() as u64).min(now_ms()))
        .unwrap_or_else(now_ms)
}

fn read_local(base: &SyncDoc) -> Option<LocalState> {
    let (watch, watch_missing) = match load_state::<Vec<WatchItem>>(&watch_items_path()) {
        FileState::Data(w) => (w, false),
        FileState::Missing if base.watch.is_empty() => (Vec::new(), true),
        _ => return None,
    };
    let (prefs, prefs_missing) = match load_state::<PlayerPrefs>(&player_prefs_path()) {
        FileState::Data(p) => (p, false),
        FileState::Missing => (base.prefs.clone().unwrap_or_default(), true),
        FileState::Unreadable => return None,
    };
    Some(LocalState {
        watch,
        prefs,
        watch_mtime: mtime_ms(&watch_items_path()),
        prefs_mtime: mtime_ms(&player_prefs_path()),
        missing: watch_missing || prefs_missing,
    })
}

async fn run(cfg: SyncConfig, tx: UnboundedSender<MergedState>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut snap: Snapshot = read_json(&snapshot_path()).unwrap_or_default();
    let mut last_pull = 0u64;
    let mut cooldown_until = 0u64;
    let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let now = now_ms();
        if now < cooldown_until {
            continue;
        }
        let Some(local) = read_local(&snap.doc) else {
            continue;
        };
        let local_changed = local.watch != snap.doc.watch
            || snap.doc.prefs.as_ref() != Some(&local.prefs)
            || local.missing;
        if !local_changed {
            if now.saturating_sub(last_pull) < PULL_INTERVAL_MS {
                continue;
            }
            last_pull = now;
        }
        let (next, ok) = sync_once(&cfg, &client, snap, local, local_changed, &tx).await;
        snap = next;
        if !ok {
            cooldown_until = now_ms() + PULL_INTERVAL_MS;
        }
    }
}

async fn sync_once(
    cfg: &SyncConfig,
    client: &reqwest::Client,
    mut snap: Snapshot,
    local: LocalState,
    local_changed: bool,
    tx: &UnboundedSender<MergedState>,
) -> (Snapshot, bool) {
    let mut remote = if local_changed && snap.doc.rev > 0 {
        snap.doc.clone()
    } else {
        match fetch_remote(cfg, client).await {
            Some(r) => r,
            None => return (snap, false),
        }
    };
    let fresh_base = SyncDoc::default();
    let mut ledger_dirty = false;
    for _ in 0..2 {
        let eff_base = if remote.rev < snap.doc.rev {
            &fresh_base
        } else {
            &snap.doc
        };
        let (local_stamps, dirty) = stamp_local(eff_base, &local, &mut snap.ledger);
        ledger_dirty |= dirty;
        let prefs_stamp = local_prefs_stamp(eff_base, &local);
        let (watch, stamps) = merge_watch(eff_base, &local.watch, &local_stamps, &remote);
        let (prefs, prefs_updated_at) = merge_prefs(eff_base, &local.prefs, prefs_stamp, &remote);
        let remote_dirty = watch != remote.watch
            || stamps != remote.stamps
            || remote.prefs.as_ref() != Some(&prefs);
        let new_doc = if remote_dirty {
            match push_remote(
                cfg,
                client,
                remote.rev,
                &watch,
                &stamps,
                &prefs,
                prefs_updated_at,
            )
            .await
            {
                PushOutcome::Accepted(doc) => doc,
                PushOutcome::Conflict(doc) => {
                    remote = doc;
                    continue;
                }
                PushOutcome::Failed => return (persist_ledger(snap, ledger_dirty), false),
            }
        } else {
            remote
        };
        return (
            apply_local(snap, new_doc, &local, watch, prefs, ledger_dirty, tx),
            true,
        );
    }
    (persist_ledger(snap, ledger_dirty), true)
}

fn persist_ledger(snap: Snapshot, dirty: bool) -> Snapshot {
    if dirty {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        write_json_unlocked(&snapshot_path(), &snap, "sync snapshot");
    }
    snap
}

fn apply_local(
    mut snap: Snapshot,
    new_doc: SyncDoc,
    local: &LocalState,
    watch: Vec<WatchItem>,
    prefs: PlayerPrefs,
    ledger_dirty: bool,
    tx: &UnboundedSender<MergedState>,
) -> Snapshot {
    let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(current) = read_local(&snap.doc) else {
        return snap;
    };
    if current.watch != local.watch || current.prefs != local.prefs {
        return snap;
    }
    let changed = watch != local.watch || prefs != local.prefs;
    if changed || local.missing {
        if !(write_json_unlocked(&watch_items_path(), &watch, "sync watch")
            && write_json_unlocked(&player_prefs_path(), &prefs, "sync prefs"))
        {
            return snap;
        }
        if changed {
            let _ = tx.send(MergedState { watch, prefs });
        }
    }
    if new_doc.rev != snap.doc.rev || ledger_dirty {
        let old_doc = std::mem::replace(&mut snap.doc, new_doc);
        if !write_json_unlocked(&snapshot_path(), &snap, "sync snapshot") {
            snap.doc = old_doc;
        }
    } else {
        snap.doc = new_doc;
    }
    snap
}

async fn api_json(req: reqwest::RequestBuilder) -> Option<ApiResponse> {
    req.send().await.ok()?.json().await.ok()
}

async fn fetch_remote(cfg: &SyncConfig, client: &reqwest::Client) -> Option<SyncDoc> {
    let api = api_json(
        client
            .get(&cfg.endpoint)
            .query(&[("token", cfg.token.as_str())]),
    )
    .await?;
    if api.ok {
        api.doc
    } else {
        None
    }
}

async fn push_remote(
    cfg: &SyncConfig,
    client: &reqwest::Client,
    base_rev: u64,
    watch: &[WatchItem],
    stamps: &HashMap<String, u64>,
    prefs: &PlayerPrefs,
    prefs_updated_at: u64,
) -> PushOutcome {
    let Ok(body) = serde_json::to_string(&PushBody {
        token: &cfg.token,
        base_rev,
        watch,
        stamps,
        prefs,
        prefs_updated_at,
    }) else {
        return PushOutcome::Failed;
    };
    let Some(api) = api_json(
        client
            .post(&cfg.endpoint)
            .header("content-type", "text/plain;charset=utf-8")
            .body(body),
    )
    .await
    else {
        return PushOutcome::Failed;
    };
    match (api.ok, api.conflict, api.doc) {
        (true, _, Some(doc)) => PushOutcome::Accepted(doc),
        (false, true, Some(doc)) => PushOutcome::Conflict(doc),
        _ => PushOutcome::Failed,
    }
}

fn item_key(i: &WatchItem) -> String {
    format!("{}:{}", i.entry.provider, i.entry.id)
}

fn stamp_local(
    base: &SyncDoc,
    local: &LocalState,
    ledger: &mut HashMap<String, LedgerEntry>,
) -> (HashMap<String, u64>, bool) {
    let base_items: HashMap<String, &WatchItem> =
        base.watch.iter().map(|i| (item_key(i), i)).collect();
    let fresh = if base.rev == 0 { 0 } else { local.watch_mtime };
    let mut dirty = false;
    let mut stamps = HashMap::new();
    for (pos, i) in local.watch.iter().enumerate() {
        let k = item_key(i);
        let stamp = match base_items.get(&k) {
            Some(b) if **b == *i => {
                dirty |= ledger.remove(&k).is_some();
                base.stamps.get(&k).copied().unwrap_or(0)
            }
            _ => match ledger.get(&k) {
                Some(e) if e.item == *i => e.stamp,
                _ => {
                    let s = fresh.saturating_sub(pos as u64);
                    ledger.insert(
                        k.clone(),
                        LedgerEntry {
                            stamp: s,
                            item: i.clone(),
                        },
                    );
                    dirty = true;
                    s
                }
            },
        };
        stamps.insert(k, stamp);
    }
    let before = ledger.len();
    ledger.retain(|k, _| stamps.contains_key(k));
    dirty |= ledger.len() != before;
    (stamps, dirty)
}

fn local_prefs_stamp(base: &SyncDoc, local: &LocalState) -> u64 {
    if base.prefs.as_ref() == Some(&local.prefs) {
        base.prefs_updated_at
    } else if base.rev == 0 {
        0
    } else {
        local.prefs_mtime
    }
}

fn merge_watch(
    base: &SyncDoc,
    local: &[WatchItem],
    local_stamps: &HashMap<String, u64>,
    remote: &SyncDoc,
) -> (Vec<WatchItem>, HashMap<String, u64>) {
    let base_keys: HashSet<String> = base.watch.iter().map(item_key).collect();
    let base_stamp = |k: &str| base.stamps.get(k).copied().unwrap_or(0);
    let local_map: HashMap<String, &WatchItem> = local.iter().map(|i| (item_key(i), i)).collect();
    let remote_map: HashMap<String, &WatchItem> =
        remote.watch.iter().map(|i| (item_key(i), i)).collect();
    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    for i in local.iter().chain(remote.watch.iter()) {
        let k = item_key(i);
        if seen.insert(k.clone()) {
            keys.push(k);
        }
    }
    let mut merged: Vec<(WatchItem, u64)> = Vec::new();
    for k in keys {
        let ls = local_stamps.get(&k).copied().unwrap_or(0);
        let rs = remote.stamps.get(&k).copied().unwrap_or(0);
        let pick = match (local_map.get(&k), remote_map.get(&k)) {
            (Some(l), Some(r)) => Some(if rs > ls {
                ((*r).clone(), rs)
            } else {
                ((*l).clone(), ls)
            }),
            (Some(l), None) => {
                (!base_keys.contains(&k) || ls > base_stamp(&k)).then(|| ((*l).clone(), ls))
            }
            (None, Some(r)) => {
                (!base_keys.contains(&k) || rs > base_stamp(&k)).then(|| ((*r).clone(), rs))
            }
            (None, None) => None,
        };
        if let Some(p) = pick {
            merged.push(p);
        }
    }
    merged.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
    merged.truncate(MAX_ITEMS);
    let stamps = merged.iter().map(|(i, s)| (item_key(i), *s)).collect();
    (merged.into_iter().map(|(i, _)| i).collect(), stamps)
}

fn merge_prefs(
    base: &SyncDoc,
    local: &PlayerPrefs,
    local_stamp: u64,
    remote: &SyncDoc,
) -> (PlayerPrefs, u64) {
    let base_prefs = base.prefs.clone().unwrap_or_default();
    let remote_prefs = remote.prefs.clone().unwrap_or_default();
    let local_changed = base.prefs.as_ref() != Some(local);
    let remote_changed =
        remote.prefs_updated_at > base.prefs_updated_at || remote_prefs != base_prefs;
    match (local_changed, remote_changed) {
        (true, true) => {
            if remote.prefs_updated_at > local_stamp {
                (remote_prefs, remote.prefs_updated_at)
            } else {
                (local.clone(), local_stamp)
            }
        }
        (true, false) => (local.clone(), local_stamp.max(base.prefs_updated_at)),
        (false, true) => (remote_prefs, remote.prefs_updated_at),
        (false, false) => (local.clone(), base.prefs_updated_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{MediaEntry, MediaType};

    fn make_entry(id: u64) -> MediaEntry {
        MediaEntry {
            id,
            name: format!("Title {id}"),
            slug: String::new(),
            provider: 0,
            provider_name: "StreamingCommunity".into(),
            language: "it".into(),
            media_type: MediaType::Movie,
            alternative_names: Vec::new(),
            year: None,
            score: None,
            image_url: None,
            description: None,
        }
    }

    fn make_item(id: u64, time: f64) -> WatchItem {
        WatchItem {
            entry: make_entry(id),
            current_time: time,
            duration: 100.0,
            season: None,
            episode: None,
        }
    }

    fn make_doc(rev: u64, items: Vec<(WatchItem, u64)>) -> SyncDoc {
        let stamps = items.iter().map(|(i, s)| (item_key(i), *s)).collect();
        SyncDoc {
            rev,
            prefs_updated_at: 0,
            watch: items.into_iter().map(|(i, _)| i).collect(),
            stamps,
            prefs: None,
        }
    }

    fn local_state(watch: Vec<WatchItem>, watch_mtime: u64) -> LocalState {
        LocalState {
            watch,
            prefs: PlayerPrefs::default(),
            watch_mtime,
            prefs_mtime: 0,
            missing: false,
        }
    }

    fn stamps_of(
        base: &SyncDoc,
        local: &LocalState,
        ledger: &mut HashMap<String, LedgerEntry>,
    ) -> HashMap<String, u64> {
        stamp_local(base, local, ledger).0
    }

    #[test]
    fn stamp_unchanged_item_keeps_base_stamp() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let stamps = stamps_of(
            &base,
            &local_state(vec![make_item(1, 10.0)], 900),
            &mut HashMap::new(),
        );
        assert_eq!(stamps.get("0:1"), Some(&5));
    }

    #[test]
    fn stamp_changed_item_uses_file_mtime() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let stamps = stamps_of(
            &base,
            &local_state(vec![make_item(1, 42.0)], 900),
            &mut HashMap::new(),
        );
        assert_eq!(stamps.get("0:1"), Some(&900));
    }

    #[test]
    fn stamp_fresh_base_is_zero() {
        let base = make_doc(0, vec![]);
        let stamps = stamps_of(
            &base,
            &local_state(vec![make_item(1, 42.0)], 900),
            &mut HashMap::new(),
        );
        assert_eq!(stamps.get("0:1"), Some(&0));
    }

    #[test]
    fn stamp_new_items_preserve_list_order() {
        let base = make_doc(1, vec![]);
        let stamps = stamps_of(
            &base,
            &local_state(vec![make_item(1, 1.0), make_item(2, 2.0)], 900),
            &mut HashMap::new(),
        );
        assert!(stamps.get("0:1").unwrap() > stamps.get("0:2").unwrap());
    }

    #[test]
    fn ledger_preserves_first_change_stamp_across_ticks() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let mut ledger = HashMap::new();
        let (s1, d1) = stamp_local(
            &base,
            &local_state(vec![make_item(1, 42.0)], 900),
            &mut ledger,
        );
        let (s2, d2) = stamp_local(
            &base,
            &local_state(vec![make_item(1, 42.0)], 2000),
            &mut ledger,
        );
        assert!(d1);
        assert!(!d2);
        assert_eq!(s1.get("0:1"), Some(&900));
        assert_eq!(s2.get("0:1"), Some(&900));
    }

    #[test]
    fn ledger_refreshes_stamp_when_item_changes_again() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let mut ledger = HashMap::new();
        stamp_local(
            &base,
            &local_state(vec![make_item(1, 42.0)], 900),
            &mut ledger,
        );
        let (s2, d2) = stamp_local(
            &base,
            &local_state(vec![make_item(1, 60.0)], 2000),
            &mut ledger,
        );
        assert!(d2);
        assert_eq!(s2.get("0:1"), Some(&2000));
    }

    #[test]
    fn ledger_pruned_when_item_matches_base_again() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let mut ledger = HashMap::new();
        stamp_local(
            &base,
            &local_state(vec![make_item(1, 42.0)], 900),
            &mut ledger,
        );
        let (s2, d2) = stamp_local(
            &base,
            &local_state(vec![make_item(1, 10.0)], 2000),
            &mut ledger,
        );
        assert!(d2);
        assert!(ledger.is_empty());
        assert_eq!(s2.get("0:1"), Some(&5));
    }

    #[test]
    fn snapshot_serde_round_trip() {
        let mut ledger = HashMap::new();
        ledger.insert(
            "0:1".to_string(),
            LedgerEntry {
                stamp: 42,
                item: make_item(1, 7.0),
            },
        );
        let snap = Snapshot {
            doc: make_doc(3, vec![(make_item(1, 7.0), 42)]),
            ledger,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let loaded: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.doc.rev, 3);
        assert_eq!(loaded.ledger.get("0:1").unwrap().stamp, 42);
    }

    #[test]
    fn legacy_snapshot_parses_as_fresh() {
        let legacy = serde_json::to_string(&make_doc(9, vec![(make_item(1, 1.0), 4)])).unwrap();
        let loaded: Snapshot = serde_json::from_str(&legacy).unwrap();
        assert_eq!(loaded.doc.rev, 0);
        assert!(loaded.ledger.is_empty());
    }

    #[test]
    fn merge_takes_newer_remote() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let remote = make_doc(2, vec![(make_item(1, 50.0), 9)]);
        let local = vec![make_item(1, 10.0)];
        let stamps = stamps_of(&base, &local_state(local.clone(), 900), &mut HashMap::new());
        let (m, s) = merge_watch(&base, &local, &stamps, &remote);
        assert_eq!(m.len(), 1);
        assert!((m[0].current_time - 50.0).abs() < 0.01);
        assert_eq!(s.get("0:1"), Some(&9));
    }

    #[test]
    fn merge_takes_newer_local() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let remote = make_doc(2, vec![(make_item(1, 50.0), 9)]);
        let local = vec![make_item(1, 70.0)];
        let stamps = stamps_of(&base, &local_state(local.clone(), 900), &mut HashMap::new());
        let (m, _) = merge_watch(&base, &local, &stamps, &remote);
        assert_eq!(m.len(), 1);
        assert!((m[0].current_time - 70.0).abs() < 0.01);
    }

    #[test]
    fn merge_stale_local_change_loses_to_newer_remote_stamp() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let remote = make_doc(2, vec![(make_item(1, 50.0), 2000)]);
        let local = vec![make_item(1, 30.0)];
        let stamps = stamps_of(
            &base,
            &local_state(local.clone(), 1000),
            &mut HashMap::new(),
        );
        let (m, _) = merge_watch(&base, &local, &stamps, &remote);
        assert_eq!(m.len(), 1);
        assert!((m[0].current_time - 50.0).abs() < 0.01);
    }

    #[test]
    fn merge_local_delete_wins_when_remote_unchanged() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let remote = make_doc(2, vec![(make_item(1, 10.0), 5)]);
        let (m, s) = merge_watch(&base, &[], &HashMap::new(), &remote);
        assert!(m.is_empty());
        assert!(s.is_empty());
    }

    #[test]
    fn merge_remote_delete_wins_when_local_unchanged() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let remote = make_doc(2, vec![]);
        let local = vec![make_item(1, 10.0)];
        let stamps = stamps_of(&base, &local_state(local.clone(), 900), &mut HashMap::new());
        let (m, _) = merge_watch(&base, &local, &stamps, &remote);
        assert!(m.is_empty());
    }

    #[test]
    fn merge_remote_update_survives_local_delete() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let remote = make_doc(2, vec![(make_item(1, 60.0), 8)]);
        let (m, _) = merge_watch(&base, &[], &HashMap::new(), &remote);
        assert_eq!(m.len(), 1);
        assert!((m[0].current_time - 60.0).abs() < 0.01);
    }

    #[test]
    fn merge_local_update_survives_remote_delete() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let remote = make_doc(2, vec![]);
        let local = vec![make_item(1, 60.0)];
        let stamps = stamps_of(&base, &local_state(local.clone(), 900), &mut HashMap::new());
        let (m, _) = merge_watch(&base, &local, &stamps, &remote);
        assert_eq!(m.len(), 1);
        assert!((m[0].current_time - 60.0).abs() < 0.01);
    }

    #[test]
    fn merge_keeps_additions_from_both_sides() {
        let base = make_doc(1, vec![]);
        let remote = make_doc(2, vec![(make_item(2, 20.0), 6)]);
        let local = vec![make_item(1, 10.0)];
        let stamps: HashMap<String, u64> = [("0:1".to_string(), 3u64)].into();
        let (m, _) = merge_watch(&base, &local, &stamps, &remote);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].entry.id, 2);
        assert_eq!(m[1].entry.id, 1);
    }

    #[test]
    fn merge_sorts_most_recent_first() {
        let base = make_doc(1, vec![]);
        let remote = make_doc(2, vec![(make_item(3, 3.0), 5)]);
        let local = vec![make_item(1, 1.0), make_item(2, 2.0)];
        let stamps: HashMap<String, u64> =
            [("0:1".to_string(), 3u64), ("0:2".to_string(), 7u64)].into();
        let (m, _) = merge_watch(&base, &local, &stamps, &remote);
        let ids: Vec<u64> = m.iter().map(|i| i.entry.id).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn merge_caps_item_count() {
        let base = make_doc(1, vec![]);
        let remote = make_doc(2, vec![]);
        let local: Vec<WatchItem> = (0..150).map(|i| make_item(i, 1.0)).collect();
        let stamps: HashMap<String, u64> = local
            .iter()
            .enumerate()
            .map(|(pos, i)| (item_key(i), 10_000 - pos as u64))
            .collect();
        let (m, s) = merge_watch(&base, &local, &stamps, &remote);
        assert_eq!(m.len(), MAX_ITEMS);
        assert_eq!(s.len(), MAX_ITEMS);
        assert_eq!(m[0].entry.id, 0);
    }

    #[test]
    fn lenient_deserialization_drops_bad_items() {
        let json = r#"{
            "rev": 3,
            "watch": [
                {"entry": {"id": null}, "current_time": 1.0},
                {
                    "entry": {
                        "id": 7, "name": "Ok", "slug": "", "provider": 0,
                        "provider_name": "StreamingCommunity", "language": "it",
                        "media_type": "Movie", "alternative_names": [],
                        "year": null, "score": null, "image_url": null, "description": null
                    },
                    "current_time": 5.0, "duration": 10.0, "season": null, "episode": null
                }
            ],
            "stamps": {"0:7": 9}
        }"#;
        let doc: SyncDoc = serde_json::from_str(json).unwrap();
        assert_eq!(doc.rev, 3);
        assert_eq!(doc.watch.len(), 1);
        assert_eq!(doc.watch[0].entry.id, 7);
    }

    fn prefs(lang: &str) -> PlayerPrefs {
        PlayerPrefs {
            audio_lang: lang.into(),
            subtitle_lang: String::new(),
            subtitles_on: false,
            speed: 1.0,
        }
    }

    fn prefs_doc(p: Option<PlayerPrefs>, stamp: u64) -> SyncDoc {
        SyncDoc {
            rev: 1,
            prefs_updated_at: stamp,
            watch: Vec::new(),
            stamps: HashMap::new(),
            prefs: p,
        }
    }

    #[test]
    fn prefs_local_only_change_wins() {
        let base = prefs_doc(Some(prefs("it")), 5);
        let remote = prefs_doc(Some(prefs("it")), 5);
        let (p, ts) = merge_prefs(&base, &prefs("en"), 9, &remote);
        assert_eq!(p.audio_lang, "en");
        assert_eq!(ts, 9);
    }

    #[test]
    fn prefs_remote_only_change_wins() {
        let base = prefs_doc(Some(prefs("it")), 5);
        let remote = prefs_doc(Some(prefs("es")), 8);
        let (p, ts) = merge_prefs(&base, &prefs("it"), 5, &remote);
        assert_eq!(p.audio_lang, "es");
        assert_eq!(ts, 8);
    }

    #[test]
    fn prefs_conflict_newer_remote_wins() {
        let base = prefs_doc(Some(prefs("it")), 5);
        let remote = prefs_doc(Some(prefs("es")), 20);
        let (p, _) = merge_prefs(&base, &prefs("en"), 10, &remote);
        assert_eq!(p.audio_lang, "es");
    }

    #[test]
    fn prefs_conflict_newer_local_wins() {
        let base = prefs_doc(Some(prefs("it")), 5);
        let remote = prefs_doc(Some(prefs("es")), 8);
        let (p, ts) = merge_prefs(&base, &prefs("en"), 30, &remote);
        assert_eq!(p.audio_lang, "en");
        assert_eq!(ts, 30);
    }

    #[test]
    fn prefs_no_changes_keep_local() {
        let base = prefs_doc(Some(prefs("it")), 5);
        let remote = prefs_doc(Some(prefs("it")), 5);
        let (p, ts) = merge_prefs(&base, &prefs("it"), 5, &remote);
        assert_eq!(p.audio_lang, "it");
        assert_eq!(ts, 5);
    }

    #[test]
    fn prefs_fresh_base_defaults_lose_to_remote() {
        let base = prefs_doc(None, 0);
        let remote = prefs_doc(Some(prefs("es")), 8);
        let fresh = SyncDoc {
            rev: 0,
            ..prefs_doc(None, 0)
        };
        let stamp = local_prefs_stamp(&fresh, &local_state(vec![], 0));
        let (p, ts) = merge_prefs(&base, &PlayerPrefs::default(), stamp, &remote);
        assert_eq!(p.audio_lang, "es");
        assert_eq!(ts, 8);
    }
}
