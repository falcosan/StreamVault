use super::{BackButton, PAUSE_ICON, PLAY_ICON, focus};
use crate::app::{AppState, Playback};
use crate::player::{Menu, OSD_SECONDS, SEEK_SECONDS, Tracks, clock};
use dioxus::prelude::*;

const FULLSCREEN_ICON: &str = include_str!("../../assets/img/vector/fullscreen.svg");

#[component]
pub fn PlayerView() -> Element {
    let app = use_context::<AppState>();
    let player = app.player;
    let playback = app.playback.read();
    let title = playback.as_ref().map(Playback::title).unwrap_or_default();
    let url = playback.as_ref().and_then(|p| p.url.clone());
    let start = playback.as_ref().map_or(0.0, |p| p.start);
    let tracks = playback.as_ref().map(Playback::tracks).unwrap_or_default();
    let status = *player.status.read();
    let time = player.scrub.read().unwrap_or(status.time);
    let loading = url.is_none() || status.waiting;
    let percent = if status.duration > 0.0 {
        time / status.duration * 100.0
    } else {
        0.0
    };
    let phase = if player.osd.read().is_multiple_of(2) {
        "a"
    } else {
        "b"
    };
    let menu = *player.menu.read();
    let pinned = status.paused || menu.is_some();
    let list = player.tracks.read().clone();
    let audio = list
        .current(Menu::Audio)
        .map_or_else(|| "Default".into(), |t| t.name());
    let subtitles = list
        .current(Menu::Subtitles)
        .map_or_else(|| "Off".into(), |t| t.name());

    rsx! {
        div {
            class: "player-screen",
            tabindex: "-1",
            autofocus: true,
            onmousemove: move |_| {
                player.wake();
            },
            onkeydown: move |e: KeyboardEvent| {
                match e.key() {
                    Key::Character(c) if c == " " => player.toggle(),
                    Key::ArrowLeft => player.seek(-SEEK_SECONDS),
                    Key::ArrowRight => player.seek(SEEK_SECONDS),
                    _ => return,
                }
                e.prevent_default();
            },
            if let Some(url) = url {
                Video { key: "{url}", url, start, tracks }
            }
            if loading {
                div { class: "player-spinner", role: "progressbar", aria_label: "Loading" }
            }
            div {
                class: "player-osd {phase}",
                class: if pinned { "pinned" },
                style: "animation-delay: {OSD_SECONDS}s;",
                div { class: "player-osd-top",
                    BackButton {}
                    button {
                        class: "player-title",
                        onclick: move |_| app.show_playing_details(),
                        "{title}"
                    }
                }
                div { class: "player-osd-bottom",
                    if let Some(menu) = menu {
                        TrackMenu { menu, tracks: list.clone() }
                    }
                    div { class: "player-timeline",
                        span { class: "player-time", "{clock(time)}" }
                        input {
                            id: "seek",
                            class: "player-seek",
                            r#type: "range",
                            min: "0",
                            max: "{status.duration}",
                            step: "any",
                            value: "{time}",
                            aria_label: "Seek",
                            style: "--progress: {percent}%;",
                            oninput: move |e| {
                                if let Ok(time) = e.value().parse() {
                                    player.scrub_to(time);
                                }
                            },
                            onchange: move |e| {
                                if let Ok(time) = e.value().parse() {
                                    player.seek_to(time);
                                }
                            },
                        }
                        span { class: "player-time", "{clock(status.duration)}" }
                    }
                    div { class: "player-controls",
                        button {
                            id: "control-play",
                            class: "player-control icon",
                            aria_label: if status.paused { "Play" } else { "Pause" },
                            onclick: move |_| player.toggle(),
                            span {
                                class: "control-icon",
                                dangerous_inner_html: if status.paused { PLAY_ICON } else { PAUSE_ICON },
                            }
                        }
                        if list.audio.len() > 1 {
                            button {
                                id: "control-audio",
                                class: "player-control",
                                onclick: move |_| player.toggle_menu(Menu::Audio),
                                "Audio: {audio}"
                            }
                        }
                        if !list.subtitles.is_empty() {
                            button {
                                id: "control-subtitles",
                                class: "player-control",
                                onclick: move |_| player.toggle_menu(Menu::Subtitles),
                                "Subtitles: {subtitles}"
                            }
                        }
                        if app.has_next() {
                            button {
                                id: "control-next",
                                class: "player-control next",
                                onclick: move |_| {
                                    focus("#seek");
                                    app.play_next();
                                },
                                "Next episode"
                            }
                        }
                        button {
                            id: "control-fullscreen",
                            class: "player-control icon",
                            aria_label: "Fullscreen",
                            onclick: move |_| player.fullscreen(),
                            span { class: "control-icon", dangerous_inner_html: FULLSCREEN_ICON }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TrackMenu(menu: Menu, tracks: Tracks) -> Element {
    let app = use_context::<AppState>();
    let (title, selected) = match menu {
        Menu::Audio => ("Audio", tracks.audio_index.max(0)),
        Menu::Subtitles => ("Subtitles", tracks.subtitle_index),
    };
    let mut options: Vec<(i32, String)> = tracks
        .list(menu)
        .0
        .iter()
        .enumerate()
        .map(|(i, track)| (i32::try_from(i).unwrap_or(i32::MAX), track.name()))
        .collect();
    if menu == Menu::Subtitles {
        options.insert(0, (-1, "Off".into()));
    }
    rsx! {
        div { class: "player-menu", role: "menu", aria_label: "{title}",
            span { class: "player-menu-title", "{title}" }
            for (index, name) in options {
                button {
                    key: "{index}",
                    class: if index == selected { "player-option active" } else { "player-option" },
                    role: "menuitemradio",
                    aria_checked: index == selected,
                    onmounted: move |e| async move {
                        if index == selected {
                            let _ = e.set_focus(true).await;
                        }
                    },
                    onclick: move |_| app.player.choose(app, menu, index),
                    "{name}"
                }
            }
        }
    }
}

#[component]
fn Video(url: String, start: f64, tracks: Vec<(String, String)>) -> Element {
    let app = use_context::<AppState>();
    use_drop(move || app.player.detach());
    rsx! {
        video {
            class: "player-video",
            autoplay: true,
            onmounted: move |_| app.player.attach(&url, start, &app.prefs.peek()),
            onclick: move |_| app.player.toggle(),
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

fn is_language_code(language: &str) -> bool {
    (2..=3).contains(&language.len()) && language.bytes().all(|b| b.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const TRACKS: &str = r#"[{ language: "eng", label: "English [CC]" }, { language: "forced-ita", label: "Italian [Forced]" }, { language: "ita", label: "Italian" }]"#;

    fn eval(expression: &str) -> String {
        let script = format!(
            "{}JSON.stringify({expression});",
            include_str!("../../assets/js/player.js")
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
        assert_eq!(
            eval(&format!(
                r#"["ita", "forced-ita", "Italian"].map((lang) => pick({TRACKS}, lang))"#
            )),
            "[2,1,2]"
        );
    }

    #[test]
    fn only_the_first_load_or_a_stalled_playback_is_buffering() {
        assert_eq!(
            eval(
                r#"[
                    buffering({ paused: true, seeking: false, readyState: 0 }, false),
                    buffering({ paused: true, seeking: true, readyState: 1 }, true),
                    buffering({ paused: false, seeking: false, readyState: 2 }, true),
                    buffering({ paused: false, seeking: true, readyState: 4 }, true),
                    buffering({ paused: false, seeking: false, readyState: 4 }, true),
                ]"#
            ),
            "[true,false,true,true,false]"
        );
    }

    #[test]
    fn pick_falls_back_to_a_partial_match_then_the_first_track() {
        assert_eq!(
            eval(&format!(
                r#"["ital", "xyz", ""].map((lang) => pick({TRACKS}, lang))"#
            )),
            "[1,0,0]"
        );
    }
}
