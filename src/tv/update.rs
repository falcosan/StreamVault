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
            var outcome;
            var state;
            var location = {{ reload: () => (outcome = "reloaded") }};
            var atob = (text) => text;
            var TextEncoder = class {{ encode(text) {{ return text; }} }};
            var TextDecoder = class {{ decode() {{ outcome = "started the bundle"; state = stored.get("status").state; return ""; }} }};
            var crypto = {{
              subtle: {{
                importKey: async () => ({{}}),
                verify: async (_, key, signature) => signature === "sig",
                digest: async (_, data) => Uint8Array.of(data === "code" ? 0xab : 0xcd).buffer,
              }},
            }};
            var releases = {{}};
            var fetch = async (url) => {{
              const body = releases[url.split("/").pop()];
              return {{
                ok: body !== undefined,
                status: body === undefined ? 404 : 200,
                text: async () => body,
                arrayBuffer: async () => body,
              }};
            }};
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
              return JSON.stringify({{ opens, warnings, stored: Object.fromEntries(stored), outcome, state }});
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

    const MANIFEST: &str = r#"{"version":"1.1.2","shell":1,"entry":"e.js","wasm":"w.wasm","scripts":[],"files":[{"name":"e.js","sha256":"ab"}]}"#;

    fn install(name: &str, release: &str) -> Value {
        let then = format!(
            r#"releases["tv.json"] = {manifest};
            releases["tv.sig"] = "sig";
            releases["e.js"] = "code";
            {release}
            try {{ await streamvault.install(); }} catch (error) {{ outcome = error; }}"#,
            manifest = Value::from(MANIFEST),
        );
        boot(name, "0", &json!({}), &then)
    }

    #[test]
    fn an_update_is_kept_only_when_its_signature_files_and_shell_check_out() {
        let installed = install("install_signed", "");
        assert_eq!(installed["outcome"], "reloaded");
        assert_eq!(
            installed["stored"]["status"],
            json!({ "version": "1.1.2", "installed": "1.1.1", "state": "new" })
        );
        for (name, release, refusal) in [
            (
                "install_forged",
                r#"releases["tv.sig"] = "forged";"#,
                "the update signature is not valid",
            ),
            (
                "install_altered",
                r#"releases["e.js"] = "altered";"#,
                "e.js does not match the signed update",
            ),
            (
                "install_newer_shell",
                r#"releases["tv.json"] = releases["tv.json"].replace('"shell":1', '"shell":2');"#,
                "needs a new installation",
            ),
        ] {
            let refused = install(name, release);
            let outcome = refused["outcome"].as_str().unwrap_or_default();
            assert!(outcome.contains(refusal), "{name}: {outcome}");
            assert_eq!(refused["stored"], json!({}), "{name}");
        }
    }

    #[test]
    fn a_new_update_starts_as_a_trial_so_a_crash_rolls_it_back() {
        let stored = json!({
            "status": { "version": "1.1.2", "installed": "1.1.1", "state": "new" },
            "bundle": { "manifest": MANIFEST, "signature": "sig", "files": { "e.js": "code" } },
        });
        let started = boot("boot_trial", "0", &stored, "");
        assert_eq!(started["outcome"], "started the bundle");
        assert_eq!(started["state"], "trying");
    }

    #[test]
    fn an_update_stored_for_another_installation_is_discarded() {
        let stored = json!({
            "status": { "version": "1.1.2", "installed": "1.1.0", "state": "good" },
            "bundle": { "manifest": MANIFEST, "signature": "sig", "files": { "e.js": "code" } },
        });
        assert_eq!(
            boot("boot_other_install", "0", &stored, ""),
            json!({ "opens": 1, "warnings": [], "stored": {} })
        );
    }

    #[test]
    fn a_stored_update_that_no_longer_matches_starts_the_installed_version() {
        let stored = json!({
            "status": { "version": "1.1.2", "installed": "1.1.1", "state": "good" },
            "bundle": { "manifest": MANIFEST, "signature": "sig", "files": { "e.js": "altered" } },
        });
        assert_eq!(
            boot("boot_altered", "0", &stored, ""),
            json!({
                "opens": 1,
                "warnings": ["StreamVault: the downloaded update cannot start"],
                "stored": {},
            })
        );
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
    fn a_store_still_locked_at_start_is_opened_again_when_needed() {
        assert_eq!(
            boot(
                "boot_reopen",
                "20",
                &json!({ "status": trial() }),
                "await streamvault.confirm();"
            ),
            json!({
                "opens": 21,
                "warnings": ["StreamVault: the downloaded update cannot be read"],
                "stored": { "status": { "version": "1.1.2", "installed": "1.1.1", "state": "good" } },
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
