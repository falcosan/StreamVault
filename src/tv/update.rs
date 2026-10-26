use super::Device;
use crate::app::AppState;
use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = streamvault, js_name = latest, catch)]
    async fn latest_version() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = streamvault, js_name = install, catch)]
    async fn install_latest() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = streamvault, js_name = confirm, catch)]
    async fn confirm_start() -> Result<JsValue, JsValue>;
}

pub async fn confirm() {
    if let Err(e) = confirm_start().await {
        warn!("cannot confirm the start: {}", message(&e));
    }
}

pub async fn watch(device: Device) {
    device.update.watch(latest).await;
}

pub fn install(app: AppState) {
    app.device.update.install(app, async {
        install_latest().await.map(drop).map_err(|e| message(&e))
    });
}

async fn latest() -> Result<String, String> {
    latest_version()
        .await
        .map_err(|e| message(&e))?
        .as_string()
        .ok_or_else(|| "the release has no version".to_owned())
}

fn message(error: &JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use crate::check::{run_js, scratch_dir};
    use serde_json::{Value, json};

    const BOOT: &str = include_str!("../../assets/js/boot.js");

    fn boot(name: &str, failures: &str, stored: &Value, then: &str) -> Value {
        let dir = scratch_dir(name);
        let entry = dir.join("entry.mjs");
        std::fs::write(&entry, "globalThis.started();").unwrap();
        let result = run_js(&format!(
            r#"
            var window = globalThis;
            var warnings = [];
            var console = {{ warn: (message) => warnings.push(message) }};
            var later = setTimeout;
            setTimeout = (run) => later(run, 0);
            var stored = new Map(Object.entries({stored}));
            var opens = 0;
            var failures = {failures};
            var database = {{
              transaction: () => {{
                const transaction = {{
                  objectStore: () => ({{
                    get: (key) => {{
                      const request = {{}};
                      later(() => {{
                        request.result = stored.get(key);
                        request.onsuccess();
                      }}, 0);
                      return request;
                    }},
                    put: (value, key) => stored.set(key, value),
                    delete: (key) => stored.delete(key),
                  }}),
                }};
                later(() => transaction.oncomplete?.(), 0);
                return transaction;
              }},
            }};
            var indexedDB = {{
              open: () => {{
                const request = {{}};
                opens++;
                later(() => {{
                  if (failures-- > 0) {{
                    request.error = new Error("Internal error opening backing store");
                    request.onerror();
                  }} else {{
                    request.result = database;
                    request.onsuccess();
                  }}
                }}, 0);
                return request;
              }},
            }};
            var document = {{ currentScript: {{ dataset: {{ version: "1.1.1", entry: {entry} }} }} }};
            new Promise((started) => {{
              globalThis.started = started;
              {BOOT}
            }}).then(async () => {{
              {then}
              return JSON.stringify({{ opens, warnings, stored: Object.fromEntries(stored) }});
            }})
            "#,
            entry = Value::from(entry.to_string_lossy()),
        ));
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    fn trial() -> Value {
        json!({ "version": "1.1.2", "installed": "1.1.1", "state": "trying" })
    }

    #[test]
    fn a_locked_update_store_opens_once_the_closing_app_releases_it() {
        let stored = json!({ "status": trial(), "bundle": {} });
        assert_eq!(
            boot("boot_locked", "3", &stored, ""),
            json!({ "opens": 4, "warnings": [], "stored": {} })
        );
    }

    #[test]
    fn an_unreadable_update_store_still_starts_the_installed_version() {
        let stored = json!({ "status": trial() });
        assert_eq!(
            boot("boot_unreadable", "Infinity", &stored, ""),
            json!({
                "opens": 20,
                "warnings": ["StreamVault: the downloaded update cannot be read"],
                "stored": stored,
            })
        );
    }

    #[test]
    fn confirming_marks_only_a_trial_start_good() {
        let then = format!(
            "stored.set('status', {}); await streamvault.confirm();",
            trial()
        );
        assert_eq!(
            boot("boot_confirm", "0", &json!({}), &then)["stored"],
            json!({ "status": { "version": "1.1.2", "installed": "1.1.1", "state": "good" } })
        );
        assert_eq!(
            boot(
                "boot_unconfirmed",
                "0",
                &json!({}),
                "await streamvault.confirm();"
            ),
            json!({ "opens": 1, "warnings": [], "stored": {} })
        );
    }
}
