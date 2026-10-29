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

pub async fn load_replica() -> Option<Replica> {
    let json = stored(REPLICA_KEY).await?;
    serde_json::from_str(&json)
        .inspect_err(|e| warn!("discarding unreadable saved state: {e}"))
        .ok()
}

pub fn save_replica(replica: &Replica) {
    match serde_json::to_string(replica) {
        Ok(json) => store(REPLICA_KEY, json),
        Err(e) => warn!("cannot encode the state: {e}"),
    }
}

pub async fn load_language() -> Option<Lang> {
    Lang::parse(&stored(LANGUAGE_KEY).await?)
}

pub fn save_language(lang: Lang) {
    store(LANGUAGE_KEY, lang.code().to_owned());
}

async fn stored(key: &str) -> Option<String> {
    load(key)
        .await
        .inspect_err(|e| warn!("cannot read the saved {key}: {e:?}"))
        .ok()?
        .as_string()
}

fn store(key: &'static str, value: String) {
    spawn_forever(async move {
        if let Err(e) = save(key, &value).await {
            warn!("cannot save the {key}: {e:?}");
        }
    });
}
