use crate::config::{SNAPSHOT_FILE, config_dir, read_json, state_dir, write_json};
use crate::state::{PlayerPrefs, WatchItem, parse_items, prefs_path, watch_path};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TICK: Duration = Duration::from_secs(10);
const PULL_INTERVAL_MS: u64 = 60_000;
const MAX_ITEMS: usize = 100;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    enabled: bool,
    endpoint: String,
    token: String,
}

impl SyncConfig {
    pub fn load() -> Option<Self> {
        let config: Self = read_json(&config_dir().join("sync.json")).ok()?;
        (config.enabled && !config.endpoint.trim().is_empty() && !config.token.trim().is_empty())
            .then_some(config)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct SyncDoc {
    rev: u64,
    prefs_updated_at: u64,
    #[serde(deserialize_with = "lenient_items")]
    watch: Vec<WatchItem>,
    stamps: HashMap<String, u64>,
    prefs: Option<PlayerPrefs>,
}

fn lenient_items<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WatchItem>, D::Error> {
    Vec::deserialize(d).map(parse_items)
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

struct LocalState {
    watch: Vec<WatchItem>,
    prefs: PlayerPrefs,
    watch_mtime: u64,
    prefs_mtime: u64,
    missing: bool,
}

struct Remote<'a> {
    config: &'a SyncConfig,
    client: reqwest::Client,
}

pub async fn run(config: SyncConfig, mut on_merge: impl FnMut(Vec<WatchItem>, PlayerPrefs)) {
    let remote = Remote {
        config: &config,
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default(),
    };
    let mut snap: Snapshot = read_json(&snapshot_path()).unwrap_or_default();
    let mut last_pull = 0;
    let mut cooldown_until = 0;
    let mut ticker = tokio::time::interval(TICK);
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
        let (next, ok) = sync_once(&remote, snap, local, local_changed, &mut on_merge).await;
        snap = next;
        if !ok {
            cooldown_until = now_ms() + PULL_INTERVAL_MS;
        }
    }
}

fn snapshot_path() -> PathBuf {
    state_dir().join(SNAPSHOT_FILE)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| millis(d.as_millis()))
}

fn millis(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn mtime_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or_else(now_ms, |d| millis(d.as_millis()).min(now_ms()))
}

fn read_local(base: &SyncDoc) -> Option<LocalState> {
    let (watch, watch_missing) = match read_json::<Vec<WatchItem>>(&watch_path()) {
        Ok(watch) => (watch, false),
        Err(e) if e.kind() == io::ErrorKind::NotFound && base.watch.is_empty() => {
            (Vec::new(), true)
        }
        Err(_) => return None,
    };
    let (prefs, prefs_missing) = match read_json::<PlayerPrefs>(&prefs_path()) {
        Ok(prefs) => (prefs, false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            (base.prefs.clone().unwrap_or_default(), true)
        }
        Err(_) => return None,
    };
    Some(LocalState {
        watch,
        prefs,
        watch_mtime: mtime_ms(&watch_path()),
        prefs_mtime: mtime_ms(&prefs_path()),
        missing: watch_missing || prefs_missing,
    })
}

async fn sync_once(
    remote: &Remote<'_>,
    mut snap: Snapshot,
    local: LocalState,
    local_changed: bool,
    on_merge: &mut impl FnMut(Vec<WatchItem>, PlayerPrefs),
) -> (Snapshot, bool) {
    let mut doc = if local_changed && snap.doc.rev > 0 {
        snap.doc.clone()
    } else {
        match remote.fetch().await {
            Some(doc) => doc,
            None => return (snap, false),
        }
    };
    let fresh_base = SyncDoc::default();
    let mut ledger_dirty = false;
    for _ in 0..2 {
        let base = if doc.rev < snap.doc.rev {
            &fresh_base
        } else {
            &snap.doc
        };
        let (local_stamps, dirty) = stamp_local(base, &local, &mut snap.ledger);
        ledger_dirty |= dirty;
        let prefs_stamp = local_prefs_stamp(base, &local);
        let (watch, stamps) = merge_watch(base, &local.watch, &local_stamps, &doc);
        let (prefs, prefs_updated_at) = merge_prefs(base, &local.prefs, prefs_stamp, &doc);
        let remote_dirty =
            watch != doc.watch || stamps != doc.stamps || doc.prefs.as_ref() != Some(&prefs);
        let merged = if remote_dirty {
            let body = PushBody {
                token: &remote.config.token,
                base_rev: doc.rev,
                watch: &watch,
                stamps: &stamps,
                prefs: &prefs,
                prefs_updated_at,
            };
            match remote.push(&body).await {
                PushOutcome::Accepted(accepted) => accepted,
                PushOutcome::Conflict(current) => {
                    doc = current;
                    continue;
                }
                PushOutcome::Failed => return (persist_ledger(snap, ledger_dirty), false),
            }
        } else {
            doc
        };
        return (
            apply_local(snap, merged, &local, watch, prefs, ledger_dirty, on_merge),
            true,
        );
    }
    (persist_ledger(snap, ledger_dirty), true)
}

fn persist_ledger(snap: Snapshot, dirty: bool) -> Snapshot {
    if dirty {
        write_logged(&snapshot_path(), &snap);
    }
    snap
}

fn write_logged(path: &Path, value: &(impl Serialize + ?Sized)) -> bool {
    write_json(path, value)
        .inspect_err(|e| eprintln!("[StreamVault] sync write error {}: {e}", path.display()))
        .is_ok()
}

fn apply_local(
    mut snap: Snapshot,
    merged: SyncDoc,
    local: &LocalState,
    watch: Vec<WatchItem>,
    prefs: PlayerPrefs,
    ledger_dirty: bool,
    on_merge: &mut impl FnMut(Vec<WatchItem>, PlayerPrefs),
) -> Snapshot {
    let Some(current) = read_local(&snap.doc) else {
        return snap;
    };
    if current.watch != local.watch || current.prefs != local.prefs {
        return snap;
    }
    let changed = watch != local.watch || prefs != local.prefs;
    if changed || local.missing {
        if !(write_logged(&watch_path(), &watch) && write_logged(&prefs_path(), &prefs)) {
            return snap;
        }
        if changed {
            on_merge(watch, prefs);
        }
    }
    if merged.rev != snap.doc.rev || ledger_dirty {
        let previous = std::mem::replace(&mut snap.doc, merged);
        if !write_logged(&snapshot_path(), &snap) {
            snap.doc = previous;
        }
    } else {
        snap.doc = merged;
    }
    snap
}

impl Remote<'_> {
    async fn call(request: reqwest::RequestBuilder) -> Option<ApiResponse> {
        request.send().await.ok()?.json().await.ok()
    }

    async fn fetch(&self) -> Option<SyncDoc> {
        let request = self
            .client
            .get(&self.config.endpoint)
            .query(&[("token", self.config.token.as_str())]);
        let response = Self::call(request).await?;
        response.ok.then_some(response.doc).flatten()
    }

    async fn push(&self, body: &PushBody<'_>) -> PushOutcome {
        let Ok(body) = serde_json::to_string(body) else {
            return PushOutcome::Failed;
        };
        let request = self
            .client
            .post(&self.config.endpoint)
            .header("content-type", "text/plain;charset=utf-8")
            .body(body);
        match Self::call(request).await {
            Some(ApiResponse {
                ok: true,
                doc: Some(doc),
                ..
            }) => PushOutcome::Accepted(doc),
            Some(ApiResponse {
                ok: false,
                conflict: true,
                doc: Some(doc),
            }) => PushOutcome::Conflict(doc),
            _ => PushOutcome::Failed,
        }
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
        assert_eq!(m, []);
        assert_eq!(s, HashMap::new());
    }

    #[test]
    fn merge_remote_delete_wins_when_local_unchanged() {
        let base = make_doc(1, vec![(make_item(1, 10.0), 5)]);
        let remote = make_doc(2, vec![]);
        let local = vec![make_item(1, 10.0)];
        let stamps = stamps_of(&base, &local_state(local.clone(), 900), &mut HashMap::new());
        let (m, _) = merge_watch(&base, &local, &stamps, &remote);
        assert_eq!(m, []);
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
