use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerPrefs {
    pub audio_lang: String,
    pub subtitle_lang: String,
    pub subtitles_on: bool,
    pub speed: f64,
}

impl Default for PlayerPrefs {
    fn default() -> Self {
        Self {
            audio_lang: String::new(),
            subtitle_lang: String::new(),
            subtitles_on: false,
            speed: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_deserialize_partial_with_defaults() {
        let prefs: PlayerPrefs = serde_json::from_str(r#"{"audio_lang":"it"}"#).unwrap();
        assert_eq!(prefs.audio_lang, "it");
        assert_eq!(prefs.speed, 1.0);
    }
}
