use crate::app::{AppState, Playback};
use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::time::Duration;

const PLAYER_JS: &str = concat!(
    include_str!("../../assets/js/tracks.js"),
    include_str!("../../assets/js/player.js")
);
const TICK: Duration = Duration::from_secs(1);
const SEEK_SECONDS: u32 = 10;

#[derive(Deserialize)]
struct Tick {
    time: f64,
    duration: f64,
    ended: bool,
    changed: Map<String, Value>,
}

#[component]
pub fn PlayerView() -> Element {
    let app = use_context::<AppState>();

    use_future(move || async move {
        let mut ended = false;
        loop {
            tokio::time::sleep(TICK).await;
            let Some(start) = app.playback.peek().as_ref().map(|p| p.start) else {
                continue;
            };
            let script = document::eval(PLAYER_JS);
            if script.send((start, &*app.prefs.peek())).is_err() {
                continue;
            }
            let Ok(Some(tick)) = script.join::<Option<Tick>>().await else {
                continue;
            };
            if !tick.ended {
                ended = false;
                app.progress(tick.time, tick.duration);
            } else if !ended {
                ended = true;
                app.finished();
            }
            if !tick.changed.is_empty() {
                app.change_prefs(tick.changed);
            }
        }
    });

    let playback = app.playback.read();
    let title = playback.as_ref().map(Playback::title).unwrap_or_default();
    let url = playback.as_ref().and_then(|p| p.url.clone());
    let tracks = playback.as_ref().map(Playback::tracks).unwrap_or_default();

    rsx! {
        div {
            class: "player-screen",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |e: KeyboardEvent| {
                let action = match e.key() {
                    Key::ArrowLeft => format!("v.currentTime -= {SEEK_SECONDS}"),
                    Key::ArrowRight => format!("v.currentTime += {SEEK_SECONDS}"),
                    Key::Character(c) if c == " " => "v.paused ? v.play() : v.pause()".into(),
                    _ => return,
                };
                e.prevent_default();
                document::eval(&format!(
                    "const v = document.querySelector('.player-video'); if (v) {{ {action}; }}"
                ));
            },
            div { class: "player-top-bar",
                button { class: "btn-ghost", onclick: move |_| app.stop(), "← Stop" }
                div { class: "player-title-wrapper",
                    button {
                        class: "player-title-link",
                        onclick: move |_| app.show_playing_details(),
                        "{title}"
                    }
                }
                if app.has_next() {
                    button {
                        class: "btn-next-episode",
                        onclick: move |_| app.play_next(),
                        "Next →"
                    }
                }
            }
            div { class: "player-video-container",
                if let Some(url) = url {
                    video {
                        key: "{url}",
                        class: "player-video",
                        src: "{url}",
                        controls: true,
                        autoplay: true,
                        for (src, language) in tracks {
                            track {
                                key: "{src}",
                                kind: "subtitles",
                                src: "{src}",
                                srclang: "{language}",
                                label: (!is_language_code(&language)).then_some(language.clone()),
                            }
                        }
                    }
                }
            }
        }
    }
}

fn is_language_code(language: &str) -> bool {
    (2..=3).contains(&language.len()) && language.bytes().all(|b| b.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn eval(expression: &str) -> String {
        let script = format!(
            "{}JSON.stringify({expression});",
            include_str!("../../assets/js/tracks.js")
        );
        let output = Command::new("osascript")
            .args(["-l", "JavaScript", "-e", &script])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    #[test]
    fn only_plain_language_codes_go_without_a_label() {
        assert!(is_language_code("ita"));
        assert!(is_language_code("en"));
        assert!(!is_language_code("forced-ita"));
        assert!(!is_language_code(""));
    }

    #[test]
    fn pick_prefers_the_exact_track_over_a_partial_match() {
        let tracks = r#"[{ language: "eng", label: "English [CC]" }, { language: "forced-ita", label: "Italian [Forced]" }, { language: "ita", label: "Italian" }]"#;
        assert_eq!(
            eval(&format!(
                r#"["ita", "forced-ita", "Italian"].map((lang) => pick({tracks}, lang))"#
            )),
            "[2,1,2]"
        );
    }

    #[test]
    fn pick_falls_back_to_a_partial_match_then_the_first_track() {
        let tracks = r#"[{ language: "eng", label: "English [CC]" }, { language: "forced-ita", label: "Italian [Forced]" }, { language: "ita", label: "Italian" }]"#;
        assert_eq!(
            eval(&format!(
                r#"["ital", "xyz", ""].map((lang) => pick({tracks}, lang))"#
            )),
            "[1,0,0]"
        );
    }
}
