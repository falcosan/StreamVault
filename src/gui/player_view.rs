use super::{BackButton, Choice, PLAY_ICON, Picker};
use crate::app::{AppState, Playback};
use crate::i18n::text;
use crate::platform::focus;
use crate::player::{Menu, OSD_SECONDS, SEEK_SECONDS, clock, offset_label};
use dioxus::prelude::*;

const PAUSE_ICON: &str = include_str!("../../assets/img/vector/pause.svg");
const FULLSCREEN_ICON: &str = include_str!("../../assets/img/vector/fullscreen.svg");
const FULLSCREEN_EXIT_ICON: &str = include_str!("../../assets/img/vector/fullscreen-exit.svg");

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
    let scrub = *player.scrub.read();
    let time = scrub.map_or(status.time, |scrub| scrub.to);
    let offset = scrub.and_then(|scrub| offset_label(scrub.to, scrub.from));
    let loading = url.is_none() || status.waiting;
    let percent = if status.duration > 0.0 {
        time / status.duration * 100.0
    } else {
        0.0
    };
    let phase = if *player.osd.read() { "b" } else { "a" };
    let menu = *player.menu.read();
    let pinned = status.paused || menu.is_some();
    let list = player.tracks.read();
    let t = text();
    let audio = list
        .current(Menu::Audio)
        .map_or_else(|| t.default_track.into(), |track| track.name(t));
    let subtitles = list
        .current(Menu::Subtitles)
        .map_or_else(|| t.off.into(), |track| track.name(t));

    rsx! {
        div {
            class: "player-screen",
            tabindex: "-1",
            onmounted: move |e| async move {
                let _ = e.set_focus(true).await;
            },
            onmousemove: move |_| {
                player.wake();
            },
            onkeydown: move |e: KeyboardEvent| {
                player.wake();
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
                div { class: "player-spinner", role: "progressbar", aria_label: t.loading }
            }
            if let Some(offset) = offset {
                div { class: "player-offset", aria_hidden: true, "{offset}" }
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
                        TrackMenu { menu }
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
                            aria_label: t.seek,
                            aria_valuetext: (t.time_of)(&clock(time), &clock(status.duration)),
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
                            aria_label: if status.paused { t.play } else { t.pause },
                            onclick: move |_| player.toggle(),
                            span {
                                class: "control-icon",
                                dangerous_inner_html: if status.paused { PLAY_ICON } else { PAUSE_ICON },
                            }
                        }
                        if list.audio.len() > 1 {
                            button {
                                id: Menu::Audio.id(),
                                class: "player-control",
                                aria_expanded: menu == Some(Menu::Audio),
                                onclick: move |_| player.toggle_menu(Menu::Audio),
                                "{(t.audio_track)(&audio)}"
                            }
                        }
                        if !list.subtitles.is_empty() {
                            button {
                                id: Menu::Subtitles.id(),
                                class: "player-control",
                                aria_expanded: menu == Some(Menu::Subtitles),
                                onclick: move |_| player.toggle_menu(Menu::Subtitles),
                                "{(t.subtitles_track)(&subtitles)}"
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
                                "{t.next_episode}"
                            }
                        }
                        button {
                            id: "control-fullscreen",
                            class: "player-control icon",
                            aria_label: if status.fullscreen { t.exit_fullscreen } else { t.fullscreen },
                            onclick: move |_| player.fullscreen(),
                            span {
                                class: "control-icon",
                                dangerous_inner_html: if status.fullscreen { FULLSCREEN_EXIT_ICON } else { FULLSCREEN_ICON },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TrackMenu(menu: Menu) -> Element {
    let app = use_context::<AppState>();
    let tracks = app.player.tracks.read();
    let t = text();
    let (title, selected) = match menu {
        Menu::Audio => (t.audio, tracks.audio_index.max(0)),
        Menu::Subtitles => (t.subtitles, tracks.subtitle_index),
    };
    let choice = |value, name| Choice {
        value,
        name,
        lang: None,
    };
    let mut options: Vec<Choice<i32>> = tracks
        .list(menu)
        .0
        .iter()
        .enumerate()
        .map(|(i, track)| choice(i32::try_from(i).unwrap_or(i32::MAX), track.name(t)))
        .collect();
    if menu == Menu::Subtitles {
        options.insert(0, choice(-1, t.off.into()));
    }
    rsx! {
        Picker {
            class: "player-menu",
            title,
            options,
            selected,
            onchoose: move |index| app.player.choose(app, menu, index),
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

    #[test]
    fn only_plain_language_codes_go_without_a_label() {
        assert!(is_language_code("ita"));
        assert!(is_language_code("en"));
        assert!(!is_language_code("forced-ita"));
        assert!(!is_language_code(""));
    }
}
