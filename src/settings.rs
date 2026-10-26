use serde::{Deserialize, Serialize};

pub const SYNC_FILE: &str = "sync.json";

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub token: String,
}

impl SyncConfig {
    pub fn is_active(&self) -> bool {
        self.enabled && !self.endpoint.trim().is_empty() && !self.token.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
