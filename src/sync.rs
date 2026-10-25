use crate::prefs::PlayerPrefs;
use crate::time::sleep;
use crate::watch::WatchItem;
use dioxus::prelude::*;
use futures::future::{Either, select};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::pin::pin;
use std::time::Duration;
use uuid::Uuid;

const MAX_WATCH: usize = 100;
const MAX_CHANGES: usize = 500;
const TOMBSTONE_TTL_MS: u64 = 90 * 24 * 60 * 60 * 1000;
const WATCH_PREFIX: &str = "watch:";
const PREFS_KEY: &str = "prefs";
const TICK: Duration = Duration::from_secs(10);
const PULL_TICKS: u32 = 6;
const BACKOFF: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(30);
const VISIBLE: &str = "document.addEventListener('visibilitychange', () => document.hidden || dioxus.send(true)); await new Promise(() => {});";

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub token: String,
}

impl SyncConfig {
    fn is_active(&self) -> bool {
        self.enabled && !self.endpoint.trim().is_empty() && !self.token.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Change {
    key: String,
    value: Option<Value>,
    at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Record {
    key: String,
    value: Option<Value>,
    at: u64,
    device: String,
}

#[derive(Serialize)]
struct Request<'a> {
    token: &'a str,
    device: &'a str,
    since: u64,
    changes: &'a [Change],
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Response {
    ok: bool,
    error: Option<String>,
    id: Option<String>,
    seq: u64,
    reset: bool,
    entries: Vec<Record>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Replica {
    device: String,
    document: Option<String>,
    since: u64,
    entries: BTreeMap<String, Record>,
    pending: BTreeMap<String, Change>,
}

impl Replica {
    fn new(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            ..Self::default()
        }
    }

    pub fn fresh() -> Self {
        Self::new(Uuid::new_v4().to_string())
    }

    pub fn is_dirty(&self) -> bool {
        !self.pending.is_empty()
    }

    fn changes(&self) -> Vec<Change> {
        self.pending.values().take(MAX_CHANGES).cloned().collect()
    }

    fn request<'a>(&'a self, token: &'a str, changes: &'a [Change]) -> Request<'a> {
        Request {
            token,
            device: &self.device,
            since: self.since,
            changes,
        }
    }

    fn apply(&mut self, sent: &[Change], response: Response) -> Result<(), String> {
        if !response.ok {
            return Err(response.error.unwrap_or_else(|| "sync rejected".into()));
        }
        let wiped =
            matches!((&self.document, &response.id), (Some(known), Some(id)) if known != id);
        if wiped {
            for record in self.entries.values() {
                self.pending
                    .entry(record.key.clone())
                    .or_insert_with(|| Change {
                        key: record.key.clone(),
                        value: record.value.clone(),
                        at: record.at,
                    });
            }
        } else if response.reset {
            let pending = &self.pending;
            self.entries.retain(|key, _| pending.contains_key(key));
        }
        if response.id.is_some() {
            self.document = response.id;
        }
        for record in response.entries {
            if let Some(change) = self.pending.get(&record.key) {
                let processed = sent.iter().any(|s| s == change);
                if !processed && wins((change.at, &self.device), (record.at, &record.device)) {
                    continue;
                }
                self.pending.remove(&record.key);
            }
            self.entries.insert(record.key.clone(), record);
        }
        self.since = if wiped { 0 } else { response.seq };
        Ok(())
    }

    pub fn watch(&self) -> Vec<WatchItem> {
        self.live_watch().take(MAX_WATCH).collect()
    }

    fn live_watch(&self) -> impl Iterator<Item = WatchItem> {
        let mut live: Vec<&Record> = self
            .entries
            .values()
            .filter(|r| r.key.starts_with(WATCH_PREFIX) && r.value.is_some())
            .collect();
        live.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.key.cmp(&b.key)));
        live.into_iter()
            .filter_map(|r| WatchItem::deserialize(r.value.as_ref()?).ok())
    }

    pub fn prefs(&self) -> Option<PlayerPrefs> {
        PlayerPrefs::deserialize(self.entries.get(PREFS_KEY)?.value.as_ref()?).ok()
    }

    pub fn record_watch(&mut self, after: &[WatchItem], at: u64) {
        let previous: HashMap<String, WatchItem> = self
            .live_watch()
            .map(|item| (watch_key(&item), item))
            .collect();
        let mut kept = HashSet::new();
        for (index, item) in after.iter().take(MAX_WATCH).enumerate() {
            let key = watch_key(item);
            if previous.get(&key) != Some(item) {
                let value = serde_json::to_value(item).ok();
                self.write(key.clone(), value, at.saturating_sub(index as u64));
            }
            kept.insert(key);
        }
        for key in previous.into_keys().filter(|key| !kept.contains(key)) {
            self.write(key, None, at);
        }
        let pending = &self.pending;
        self.entries.retain(|key, record| {
            record.value.is_some()
                || pending.contains_key(key)
                || at.saturating_sub(record.at) <= TOMBSTONE_TTL_MS
        });
    }

    pub fn record_prefs(&mut self, prefs: &PlayerPrefs, at: u64) -> bool {
        if self.prefs().unwrap_or_default() == *prefs {
            return false;
        }
        let value = serde_json::to_value(prefs).ok();
        self.write(PREFS_KEY.into(), value, at);
        true
    }

    fn write(&mut self, key: String, value: Option<Value>, at: u64) {
        let at = self
            .entries
            .get(&key)
            .map_or(at, |current| at.max(current.at.saturating_add(1)));
        let record = Record {
            key: key.clone(),
            value: value.clone(),
            at,
            device: self.device.clone(),
        };
        self.entries.insert(key.clone(), record);
        self.pending.insert(key.clone(), Change { key, value, at });
    }
}

fn wins(a: (u64, &str), b: (u64, &str)) -> bool {
    a > b
}

fn watch_key(item: &WatchItem) -> String {
    format!("{WATCH_PREFIX}{}:{}", item.entry.provider, item.entry.id)
}

pub async fn run(mut replica: CopyValue<Replica>, config: SyncConfig, mut publish: impl FnMut()) {
    if !config.is_active() {
        return;
    }
    let client = Client::new();
    let mut visible = document::eval(VISIBLE);
    let mut idle = PULL_TICKS;
    loop {
        if idle >= PULL_TICKS || replica.peek().is_dirty() {
            idle = 0;
            let (sent, body) = {
                let replica = replica.peek();
                let changes = replica.changes();
                let body = serde_json::to_string(&replica.request(&config.token, &changes));
                (changes, body.unwrap_or_default())
            };
            let result = exchange(&client, &config, body)
                .await
                .and_then(|response| replica.write().apply(&sent, response));
            match result {
                Ok(()) => publish(),
                Err(e) => {
                    warn!("sync failed: {e}");
                    sleep(BACKOFF).await;
                }
            }
        }
        idle += 1;
        match select(pin!(sleep(TICK)), pin!(visible.recv::<bool>())).await {
            Either::Right((Ok(_), _)) => idle = PULL_TICKS,
            Either::Right((Err(_), tick)) => tick.await,
            Either::Left(_) => {}
        }
    }
}

async fn exchange(client: &Client, config: &SyncConfig, body: String) -> Result<Response, String> {
    client
        .post(&config.endpoint)
        .header("content-type", "text/plain;charset=utf-8")
        .body(body)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::serve;
    use crate::providers::MediaEntry;
    use serde_json::json;

    fn item(id: u64, time: f64) -> WatchItem {
        WatchItem {
            entry: MediaEntry {
                id,
                name: format!("Title {id}"),
                ..Default::default()
            },
            current_time: time,
            duration: 100.0,
            season: None,
            episode: None,
        }
    }

    fn ids(items: &[WatchItem]) -> Vec<u64> {
        items.iter().map(|i| i.entry.id).collect()
    }

    fn record(key: &str, value: Option<Value>, at: u64, device: &str) -> Record {
        Record {
            key: key.into(),
            value,
            at,
            device: device.into(),
        }
    }

    fn accepted(seq: u64, entries: Vec<Record>) -> Response {
        Response {
            ok: true,
            seq,
            entries,
            ..Response::default()
        }
    }

    #[test]
    fn record_watch_writes_only_what_changed() {
        let mut replica = Replica::new("tv");
        replica.record_watch(&[item(1, 5.0), item(2, 5.0)], 1_000);
        replica.pending.clear();
        replica.record_watch(&[item(2, 9.0), item(3, 1.0)], 2_000);
        let changes: Vec<_> = replica
            .changes()
            .into_iter()
            .map(|c| (c.key, c.value.is_some(), c.at))
            .collect();
        assert_eq!(
            changes,
            [
                ("watch:0:1".into(), false, 2_000),
                ("watch:0:2".into(), true, 2_000),
                ("watch:0:3".into(), true, 1_999),
            ]
        );
        assert_eq!(ids(&replica.watch()), [2, 3]);
    }

    #[test]
    fn record_watch_keeps_at_most_max_watch_titles_live() {
        let mut replica = Replica::new("tv");
        let items: Vec<_> = (0..MAX_WATCH as u64).map(|id| item(id, 1.0)).collect();
        replica.record_watch(&items, 1_000);
        let mut after = replica.watch();
        after.insert(0, item(999, 1.0));
        replica.record_watch(&after, 2_000);
        let live = replica.entries.values().filter(|r| r.value.is_some());
        assert_eq!(live.count(), MAX_WATCH);
        assert_eq!(replica.watch()[0].entry.id, 999);
        assert_eq!(
            replica.entries[&format!("watch:0:{}", MAX_WATCH - 1)].value,
            None
        );
    }

    #[test]
    fn acknowledged_tombstones_expire_with_the_server_ttl() {
        let mut replica = Replica::new("tv");
        replica.record_watch(&[item(1, 5.0), item(2, 5.0)], 1_000);
        replica.record_watch(&[], 2_000);
        let acknowledged: Vec<_> = replica
            .changes()
            .into_iter()
            .filter(|c| c.key == "watch:0:1")
            .collect();
        let echo = record(&acknowledged[0].key, None, acknowledged[0].at, "tv");
        replica
            .apply(&acknowledged, accepted(1, vec![echo]))
            .unwrap();
        replica.record_watch(&[], 2_000 + TOMBSTONE_TTL_MS);
        assert!(replica.entries.contains_key("watch:0:1"));
        replica.record_watch(&[], 2_001 + TOMBSTONE_TTL_MS);
        assert_eq!(replica.entries.keys().collect::<Vec<_>>(), ["watch:0:2"]);
    }

    #[test]
    fn local_writes_supersede_the_version_they_replace() {
        let mut replica = Replica::new("tv");
        let value = serde_json::to_value(item(1, 5.0)).ok();
        replica
            .apply(
                &[],
                accepted(1, vec![record("watch:0:1", value, 9_000, "mac")]),
            )
            .unwrap();
        replica.record_watch(&[item(1, 7.0)], 4_000);
        assert_eq!(replica.changes()[0].at, 9_001);
    }

    #[test]
    fn apply_takes_remote_winners_and_acknowledges_pending() {
        let mut replica = Replica::new("tv");
        replica.record_watch(&[item(1, 5.0), item(2, 5.0)], 1_000);
        let mine = replica.changes();
        let theirs = serde_json::to_value(item(2, 50.0)).ok();
        replica
            .apply(
                &mine,
                accepted(
                    7,
                    vec![
                        record("watch:0:1", mine[0].value.clone(), mine[0].at, "tv"),
                        record("watch:0:2", theirs, 5_000, "mac"),
                    ],
                ),
            )
            .unwrap();
        assert!(!replica.is_dirty());
        assert_eq!(replica.since, 7);
        assert_eq!(replica.watch()[0].current_time, 50.0);
    }

    #[test]
    fn newer_local_changes_survive_an_older_echo() {
        let mut replica = Replica::new("tv");
        replica.record_watch(&[item(1, 5.0)], 1_000);
        let sent = replica.changes();
        replica.record_watch(&[item(1, 9.0)], 2_000);
        replica
            .apply(
                &sent,
                accepted(
                    1,
                    vec![record("watch:0:1", sent[0].value.clone(), sent[0].at, "tv")],
                ),
            )
            .unwrap();
        assert!(replica.is_dirty());
        assert_eq!(replica.watch()[0].current_time, 9.0);
    }

    #[test]
    fn reset_drops_entries_the_server_no_longer_has() {
        let mut replica = Replica::new("tv");
        let value = serde_json::to_value(item(1, 5.0)).ok();
        replica
            .apply(
                &[],
                accepted(1, vec![record("watch:0:1", value, 1_000, "mac")]),
            )
            .unwrap();
        replica.record_watch(&[item(2, 5.0), item(1, 5.0)], 2_000);
        replica
            .apply(
                &[],
                Response {
                    reset: true,
                    ..accepted(9, Vec::new())
                },
            )
            .unwrap();
        assert_eq!(ids(&replica.watch()), [2]);
    }

    #[test]
    fn responses_without_an_id_only_reset() {
        let mut replica = Replica::new("tv");
        let value = serde_json::to_value(item(1, 5.0)).ok();
        replica
            .apply(
                &[],
                Response {
                    id: Some("a".into()),
                    ..accepted(5, vec![record("watch:0:1", value, 1_000, "mac")])
                },
            )
            .unwrap();
        replica
            .apply(
                &[],
                Response {
                    reset: true,
                    ..accepted(0, Vec::new())
                },
            )
            .unwrap();
        assert!(replica.watch().is_empty() && !replica.is_dirty());
        assert_eq!((replica.document.as_deref(), replica.since), (Some("a"), 0));
    }

    #[test]
    fn rejected_responses_leave_the_replica_untouched() {
        let mut replica = Replica::new("tv");
        replica.record_watch(&[item(1, 5.0)], 1_000);
        let before = replica.clone();
        let error = replica.apply(
            &replica.changes(),
            Response {
                error: Some("unauthorized".into()),
                ..Response::default()
            },
        );
        assert_eq!(error, Err("unauthorized".into()));
        assert_eq!(replica, before);
    }

    #[test]
    fn watch_is_newest_first_capped_and_lenient() {
        let mut replica = Replica::new("tv");
        let items: Vec<_> = (0..MAX_WATCH as u64 + 5).map(|id| item(id, 1.0)).collect();
        replica.record_watch(&items, 10_000);
        replica
            .apply(
                &[],
                accepted(
                    1,
                    vec![record(
                        "watch:9:9",
                        Some(json!({ "entry": null })),
                        20_000,
                        "mac",
                    )],
                ),
            )
            .unwrap();
        let watch = replica.watch();
        assert_eq!(watch.len(), MAX_WATCH);
        assert_eq!(ids(&watch)[..3], [0, 1, 2]);
    }

    #[test]
    fn prefs_are_recorded_only_when_they_change() {
        let mut replica = Replica::new("tv");
        assert!(!replica.record_prefs(&PlayerPrefs::default(), 500));
        let prefs = PlayerPrefs {
            audio_lang: "ita".into(),
            ..PlayerPrefs::default()
        };
        assert!(replica.record_prefs(&prefs, 1_000));
        replica.pending.clear();
        assert!(!replica.record_prefs(&prefs, 2_000));
        assert!(!replica.is_dirty());
        assert_eq!(replica.prefs(), Some(prefs));
    }

    #[test]
    fn config_needs_the_flag_an_endpoint_and_a_token() {
        let config = |enabled, endpoint: &str, token: &str| SyncConfig {
            enabled,
            endpoint: endpoint.into(),
            token: token.into(),
        };
        assert!(config(true, "https://x", "t").is_active());
        assert!(!config(false, "https://x", "t").is_active());
        assert!(!config(true, " ", "t").is_active());
        assert!(!config(true, "https://x", "").is_active());
    }

    #[tokio::test]
    async fn exchange_posts_pending_changes_and_applies_the_answer() {
        let (endpoint, requests) = serve(&[(
            200,
            r#"{"ok":true,"seq":3,"reset":false,"entries":[{"key":"watch:0:9","value":{"entry":{"id":9,"name":"","slug":"","provider":0,"provider_name":"","language":"","media_type":"Movie","alternative_names":[],"year":null,"score":null,"image_url":null,"description":null},"current_time":1,"duration":2,"season":null,"episode":null},"at":5,"device":"tv"}]}"#,
        )])
        .await;
        let config = SyncConfig {
            enabled: true,
            endpoint,
            token: "secret".into(),
        };
        let mut replica = Replica::new("mac");
        replica.record_watch(&[item(1, 5.0)], 1_000);
        let changes = replica.changes();
        let body = serde_json::to_string(&replica.request(&config.token, &changes)).unwrap();
        let response = exchange(&Client::new(), &config, body).await.unwrap();
        replica.apply(&changes, response).unwrap();
        let request = &requests.lock().unwrap()[0];
        assert!(request.contains("content-type: text/plain;charset=utf-8"));
        assert!(request.contains(r#""token":"secret""#));
        assert!(request.contains(r#""key":"watch:0:1""#));
        assert_eq!(
            replica
                .watch()
                .iter()
                .map(|i| i.entry.id)
                .collect::<Vec<_>>(),
            [1, 9]
        );
    }

    #[test]
    fn fresh_replicas_get_distinct_devices() {
        assert_ne!(Replica::fresh(), Replica::fresh());
    }

    #[test]
    fn replica_round_trips_through_json() {
        let mut replica = Replica::new("tv");
        replica.record_watch(&[item(1, 5.0)], 1_000);
        let json = serde_json::to_string(&replica).unwrap();
        assert_eq!(serde_json::from_str::<Replica>(&json).unwrap(), replica);
    }

    #[test]
    fn replicas_saved_without_a_document_id_still_load() {
        let json = r#"{"device":"tv","since":3,"entries":{},"pending":{}}"#;
        assert_eq!(
            serde_json::from_str::<Replica>(json).unwrap(),
            Replica {
                since: 3,
                ..Replica::new("tv")
            }
        );
    }

    #[cfg(target_os = "macos")]
    mod server {
        use super::*;
        use crate::check::run_js;

        const SCRIPT: &str = include_str!("../assets/js/google_apps_script.gs");
        const DAY: u64 = 24 * 60 * 60 * 1000;

        struct Server {
            content: String,
            now: u64,
            readable: bool,
        }

        impl Server {
            fn new(content: Value) -> Self {
                Self {
                    content: content.to_string(),
                    now: 1_000_000,
                    readable: true,
                }
            }

            fn post(&mut self, body: &str) -> Response {
                let result = run_js(&format!(
                    r#"{SCRIPT}
                    var store = {{ content: {content}, props: {{ TOKEN: "token", FILE_ID: "f" }} }};
                    var PropertiesService = {{ getScriptProperties: () => ({{
                      getProperty: (k) => store.props[k] || null,
                      setProperty: (k, v) => {{ store.props[k] = v; }},
                    }}) }};
                    var DriveApp = {{ getFileById: () => ({{
                      isTrashed: () => false,
                      getBlob: () => ({{ getDataAsString: () => {{
                        if (!{readable}) throw new Error("Drive unavailable");
                        return store.content;
                      }} }}),
                      setContent: (c) => {{ store.content = c; }},
                    }}) }};
                    var Utilities = {{ getUuid: () => String(Math.random()) }};
                    var LockService = {{ getScriptLock: () => ({{ tryLock: () => true, releaseLock: () => {{}} }}) }};
                    var ContentService = {{ MimeType: {{ JSON: "json" }}, createTextOutput: (s) => ({{ setMimeType: () => s }}) }};
                    Date.now = () => {now};
                    let response;
                    try {{
                      response = JSON.parse(doPost({{ postData: {{ contents: {body} }} }}));
                    }} catch (e) {{
                      response = {{ ok: false, error: String(e) }};
                    }}
                    JSON.stringify({{ response, content: store.content }});"#,
                    content = serde_json::to_string(&self.content).unwrap(),
                    now = self.now,
                    readable = self.readable,
                    body = serde_json::to_string(body).unwrap(),
                ));
                self.content = result["content"].as_str().unwrap().to_owned();
                serde_json::from_value(result["response"].clone()).unwrap()
            }

            fn sync(&mut self, replica: &mut Replica) {
                let changes = replica.changes();
                let body = serde_json::to_string(&replica.request("token", &changes)).unwrap();
                replica.apply(&changes, self.post(&body)).unwrap();
            }
        }

        #[test]
        fn limits_match_the_script() {
            assert_eq!(
                run_js(&format!(
                    "{SCRIPT}\nJSON.stringify([MAX_CHANGES, TOMBSTONE_TTL_MS]);"
                )),
                json!([MAX_CHANGES, TOMBSTONE_TTL_MS])
            );
        }

        #[test]
        fn legacy_documents_migrate_on_first_sync() {
            let legacy = json!({
                "rev": 4, "updated_at": 50,
                "watch": [item(1, 10.0), item(2, 20.0)],
                "stamps": { "0:1": 40 },
                "prefs": { "audio_lang": "ita" }, "prefs_updated_at": 30
            });
            let mut server = Server::new(legacy);
            let mut tv = Replica::new("tv");
            server.sync(&mut tv);
            assert_eq!(ids(&tv.watch()), [2, 1]);
            assert_eq!(tv.prefs().unwrap().audio_lang, "ita");
            assert_eq!(
                serde_json::from_str::<Value>(&server.content).unwrap()["v"],
                2
            );
        }

        #[test]
        fn devices_converge_on_the_latest_write() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            tv.record_watch(&[item(1, 10.0)], 1_000);
            mac.record_watch(&[item(1, 50.0), item(2, 5.0)], 2_000);
            server.sync(&mut tv);
            server.sync(&mut mac);
            server.sync(&mut tv);
            assert_eq!(tv.watch(), mac.watch());
            assert_eq!(tv.watch()[0].current_time, 50.0);
            assert!(!tv.is_dirty() && !mac.is_dirty());
        }

        #[test]
        fn deletions_beat_older_updates() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            mac.record_watch(&[item(1, 10.0)], 1_000);
            server.sync(&mut mac);
            server.sync(&mut tv);
            tv.record_watch(&[item(1, 30.0)], 2_000);
            mac.record_watch(&[], 3_000);
            server.sync(&mut mac);
            server.sync(&mut tv);
            assert!(tv.watch().is_empty());
        }

        #[test]
        fn future_timestamps_are_clamped() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            tv.record_watch(&[item(1, 10.0)], server.now + DAY);
            server.sync(&mut tv);
            server.now += 120_000;
            mac.record_watch(&[item(1, 90.0)], server.now);
            server.sync(&mut mac);
            server.sync(&mut tv);
            assert_eq!(tv.watch()[0].current_time, 90.0);
        }

        #[test]
        fn pruned_tombstones_reset_stale_replicas() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac, mut web) =
                (Replica::new("tv"), Replica::new("mac"), Replica::new("web"));
            mac.record_watch(&[item(1, 10.0)], server.now);
            server.sync(&mut mac);
            server.sync(&mut tv);
            mac.record_watch(&[], server.now);
            server.sync(&mut mac);
            server.now += 91 * DAY;
            server.sync(&mut web);
            server.sync(&mut tv);
            assert!(tv.watch().is_empty());
            assert!(!server.content.contains("watch:0:1"));
        }

        #[test]
        fn a_wiped_document_is_restored_from_the_devices() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            mac.record_watch(&[item(1, 10.0)], 1_000);
            server.sync(&mut mac);
            server.sync(&mut tv);
            server.content = Value::Null.to_string();
            server.sync(&mut tv);
            server.sync(&mut tv);
            server.sync(&mut mac);
            assert_eq!(ids(&tv.watch()), [1]);
            assert_eq!(ids(&mac.watch()), [1]);
            assert!(server.content.contains("watch:0:1"));
        }

        #[test]
        fn after_a_wipe_devices_pull_the_newer_values_others_restored() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            mac.record_watch(&[item(1, 10.0), item(2, 5.0)], 1_000);
            server.sync(&mut mac);
            server.sync(&mut tv);
            tv.record_watch(&[item(1, 50.0), item(2, 5.0)], 2_000);
            server.sync(&mut tv);
            assert_eq!((mac.since, tv.since), (2, 3));
            server.content = Value::Null.to_string();
            server.sync(&mut tv);
            server.sync(&mut tv);
            server.sync(&mut mac);
            server.sync(&mut mac);
            assert_eq!(mac.watch(), tv.watch());
            assert_eq!(mac.watch()[0].current_time, 50.0);
        }

        #[test]
        fn after_a_wipe_devices_behind_the_new_seq_upload_what_was_lost() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            mac.record_watch(&[item(9, 10.0)], 1_000);
            server.sync(&mut mac);
            server.content = Value::Null.to_string();
            tv.record_watch(&[item(1, 5.0), item(2, 5.0)], 2_000);
            server.sync(&mut tv);
            assert!(mac.since < tv.since);
            server.sync(&mut mac);
            server.sync(&mut mac);
            server.sync(&mut tv);
            assert_eq!(ids(&mac.watch()), [1, 2, 9]);
            assert_eq!(ids(&tv.watch()), [1, 2, 9]);
        }

        #[test]
        fn documents_without_an_id_get_one_that_lasts() {
            let old = json!({
                "v": 2, "seq": 1, "floor": 0,
                "entries": { "watch:0:1": { "value": item(1, 10.0), "at": 1_000, "device": "mac", "seq": 1 } }
            });
            let mut server = Server::new(old);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            server.sync(&mut tv);
            mac.record_watch(&[item(2, 5.0)], 2_000);
            server.sync(&mut mac);
            server.sync(&mut tv);
            assert!(!tv.is_dirty());
            assert_eq!(ids(&tv.watch()), [2, 1]);
            let stored: Value = serde_json::from_str(&server.content).unwrap();
            assert_eq!(tv.document.as_deref(), Some(stored["id"].as_str().unwrap()));
            assert_eq!(mac.document, tv.document);
        }

        #[test]
        fn changes_beyond_the_request_limit_wait_for_the_next_sync() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            let late = |time| WatchItem {
                entry: MediaEntry {
                    provider: 1,
                    ..item(1, time).entry
                },
                ..item(1, time)
            };
            mac.record_watch(&[late(10.0)], 1_000);
            server.sync(&mut mac);
            for id in 0..MAX_CHANGES {
                tv.write(format!("watch:0:{id}"), None, 2_000);
            }
            tv.record_watch(&[late(90.0)], 2_000);
            server.sync(&mut tv);
            assert!(tv.is_dirty());
            assert_eq!(tv.watch()[0].current_time, 90.0);
            server.sync(&mut tv);
            server.sync(&mut mac);
            assert!(!tv.is_dirty());
            assert_eq!(mac.watch()[0].current_time, 90.0);
        }

        #[test]
        fn unreadable_storage_is_never_overwritten() {
            let mut server = Server::new(Value::Null);
            let (mut tv, mut mac) = (Replica::new("tv"), Replica::new("mac"));
            mac.record_watch(&[item(1, 10.0)], 1_000);
            server.sync(&mut mac);
            let stored = server.content.clone();
            server.readable = false;
            tv.record_watch(&[item(2, 5.0)], 2_000);
            let changes = tv.changes();
            let body = serde_json::to_string(&tv.request("token", &changes)).unwrap();
            assert!(tv.apply(&changes, server.post(&body)).is_err());
            assert_eq!(server.content, stored);
            assert!(tv.is_dirty());
        }

        #[test]
        fn unparseable_storage_is_never_overwritten() {
            let mut server = Server::new(Value::Null);
            server.content = r#"{"v":2,"seq":"#.into();
            let mut tv = Replica::new("tv");
            tv.record_watch(&[item(1, 5.0)], 1_000);
            let changes = tv.changes();
            let body = serde_json::to_string(&tv.request("token", &changes)).unwrap();
            assert!(tv.apply(&changes, server.post(&body)).is_err());
            assert_eq!(server.content, r#"{"v":2,"seq":"#);
            assert!(tv.is_dirty());
        }

        #[test]
        fn requests_without_the_token_are_rejected() {
            let mut server = Server::new(Value::Null);
            let mut tv = Replica::new("tv");
            let body = serde_json::to_string(&tv.request("wrong", &[])).unwrap();
            assert_eq!(
                tv.apply(&[], server.post(&body)),
                Err("unauthorized".into())
            );
            assert_eq!(
                tv.apply(&[], server.post("null")),
                Err("unauthorized".into())
            );
        }
    }
}
