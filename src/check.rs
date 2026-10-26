use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[cfg(all(feature = "app", target_os = "macos"))]
const JSC: &str =
    "/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc";

#[cfg(any(feature = "desktop", feature = "deploy", target_os = "macos"))]
pub fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sv_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub async fn serve(responses: &'static [(u16, &'static str)]) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    tokio::spawn(async move {
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let read = socket.read(&mut request).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&request[..read]).into_owned();
            log.lock().unwrap().push(request);
            let response = format!(
                "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (url, requests)
}

#[cfg(all(feature = "app", target_os = "macos"))]
pub fn run_js(script: &str) -> serde_json::Value {
    let runner = format!(
        "Promise.resolve((0, eval)({})).then(print, (e) => print(e?.stack ?? e));",
        serde_json::Value::from(script)
    );
    let output = std::process::Command::new(JSC)
        .args(["-e", &runner])
        .output()
        .unwrap();
    let printed = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&printed)
        .unwrap_or_else(|_| panic!("{printed}{}", String::from_utf8_lossy(&output.stderr)))
}
