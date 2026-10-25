use serde_json::Value;
use std::process::Command;

pub fn run(script: &str) -> Value {
    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", script])
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)))
}
