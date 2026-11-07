use crate::i18n::Lang;
use crate::settings::{SYNC_FILE, SyncConfig};
use crate::sync::Replica;
use dioxus::core::spawn_forever;
use dioxus::logger::tracing::warn;
use wasm_bindgen::prelude::*;

const REPLICA_KEY: &str = "replica";
const LANGUAGE_KEY: &str = "language";

#[wasm_bindgen(module = "/assets/js/storage.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn load(key: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    async fn save(key: &str, value: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    async fn replace(key: &str, value: &str) -> Result<JsValue, JsValue>;

    async fn packaged(name: &str) -> JsValue;
}

pub async fn sync_config() -> SyncConfig {
    let Some(json) = packaged(SYNC_FILE).await.as_string() else {
        return SyncConfig::default();
    };
    serde_json::from_str(&json)
        .inspect_err(|e| warn!("ignoring unreadable {SYNC_FILE}: {e}"))
        .unwrap_or_default()
}

pub async fn load_replica() -> Result<Option<Replica>, String> {
    let saved = load(REPLICA_KEY).await.map_err(|e| format!("{e:?}"))?;
    Ok(saved.as_string().and_then(|json| {
        serde_json::from_str(&json)
            .inspect_err(|e| warn!("discarding unreadable saved state: {e}"))
            .ok()
    }))
}

pub fn save_replica(replica: &Replica) {
    match serde_json::to_string(replica) {
        Ok(json) => {
            spawn_forever(async move { report(REPLICA_KEY, save(REPLICA_KEY, &json).await) });
        }
        Err(e) => warn!("cannot encode the state: {e}"),
    }
}

pub async fn load_language() -> Option<Lang> {
    Lang::parse(&stored(LANGUAGE_KEY).await?)
}

pub fn save_language(lang: Lang) {
    spawn_forever(async move { report(LANGUAGE_KEY, replace(LANGUAGE_KEY, lang.code()).await) });
}

async fn stored(key: &str) -> Option<String> {
    load(key)
        .await
        .inspect_err(|e| warn!("cannot read the saved {key}: {e:?}"))
        .ok()?
        .as_string()
}

fn report(key: &str, written: Result<JsValue, JsValue>) {
    if let Err(e) = written {
        warn!("cannot save the {key}: {e:?}");
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use crate::check::run_js;
    use serde_json::json;

    const STORAGE: &str = include_str!("../../assets/js/storage.js");

    #[test]
    fn a_store_that_failed_to_open_is_opened_again_on_the_next_use() {
        let result = run_js(&format!(
            r#"
            var later = setTimeout;
            setTimeout = (run) => later(run, 0);
            var failures = 20;
            var opens = 0;
            var request = (outcome) => {{
              const request = {{}};
              later(() => outcome(request), 0);
              return request;
            }};
            var connection = {{
              transaction: () => ({{
                objectStore: () => ({{
                  get: () => request((get) => {{
                    get.result = "saved";
                    get.onsuccess();
                  }}),
                }}),
              }}),
            }};
            var indexedDB = {{
              open: () => {{
                opens++;
                return request((open) => {{
                  if (failures-- > 0) {{
                    open.error = new Error("Internal error opening backing store");
                    open.onerror();
                  }} else {{
                    open.result = connection;
                    open.onsuccess();
                  }}
                }});
              }},
            }};
            {storage}
            (async () => {{
              const first = await load("replica").then(() => "loaded", () => "failed");
              const second = await load("replica");
              return JSON.stringify({{ first, second, opens }});
            }})()
            "#,
            storage = STORAGE.replace("export ", ""),
        ));
        assert_eq!(
            result,
            json!({ "first": "failed", "second": "saved", "opens": 21 })
        );
    }

    #[test]
    fn a_failed_read_holds_back_the_state_but_not_a_choice() {
        let result = run_js(&format!(
            r#"
            var later = setTimeout;
            setTimeout = (run) => later(run, 0);
            var saved = "saved";
            var locked = true;
            var request = (outcome) => {{
              const request = {{}};
              later(() => outcome(request), 0);
              return request;
            }};
            var connection = {{
              transaction: () => {{
                const transaction = {{
                  objectStore: () => ({{
                    get: () => request((get) => {{
                      get.result = saved;
                      get.onsuccess();
                    }}),
                    put: (value) => {{
                      saved = value;
                      later(() => transaction.oncomplete(), 0);
                    }},
                  }}),
                }};
                return transaction;
              }},
            }};
            var indexedDB = {{
              open: () => request((open) => {{
                if (locked) {{
                  open.error = new Error("locked");
                  open.onerror();
                }} else {{
                  open.result = connection;
                  open.onsuccess();
                }}
              }}),
            }};
            {storage}
            (async () => {{
              const first = await load("replica").then(() => "loaded", () => "failed");
              locked = false;
              const refused = await save("replica", "fresh").then(() => "written", () => "refused");
              const chosen = await replace("language", "it").then(() => saved, () => "refused");
              await load("replica");
              await save("replica", "newer");
              return JSON.stringify({{ first, refused, chosen, saved }});
            }})()
            "#,
            storage = STORAGE.replace("export ", ""),
        ));
        assert_eq!(
            result,
            json!({ "first": "failed", "refused": "refused", "chosen": "it", "saved": "newer" })
        );
    }
}
