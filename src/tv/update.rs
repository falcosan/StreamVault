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

pub async fn check(device: Device) {
    device
        .update
        .check(async {
            latest_version()
                .await
                .map_err(|e| message(&e))?
                .as_string()
                .ok_or_else(|| "the release has no version".to_owned())
        })
        .await;
}

pub fn install(app: AppState) {
    app.device.update.install(app, async {
        install_latest().await.map(drop).map_err(|e| message(&e))
    });
}

fn message(error: &JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}
