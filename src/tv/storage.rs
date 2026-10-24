use crate::sync::Replica;
use dioxus::core::spawn_forever;
use dioxus::logger::tracing::warn;
use wasm_bindgen::prelude::*;

const REPLICA_KEY: &str = "replica";

#[wasm_bindgen(module = "/assets/js/storage.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn load(key: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch)]
    async fn save(key: &str, value: &str) -> Result<JsValue, JsValue>;
}

pub async fn load_replica() -> Option<Replica> {
    let json = load(REPLICA_KEY)
        .await
        .inspect_err(|e| warn!("cannot read the saved state: {e:?}"))
        .ok()?
        .as_string()?;
    serde_json::from_str(&json)
        .inspect_err(|e| warn!("discarding unreadable saved state: {e}"))
        .ok()
}

pub fn save_replica(replica: &Replica) {
    let json = match serde_json::to_string(replica) {
        Ok(json) => json,
        Err(e) => return warn!("cannot encode the state: {e}"),
    };
    spawn_forever(async move {
        if let Err(e) = save(REPLICA_KEY, &json).await {
            warn!("cannot save the state: {e:?}");
        }
    });
}
