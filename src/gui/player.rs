use crate::config::PlayerPrefs;
use dioxus::prelude::*;

const PLAYER_PREFS_JS: &str = r#"
const v = document.querySelector('.player-video');
if (!v || v.readyState < 2) {
    dioxus.send(null);
} else {
    const want = __WANT__;
    const norm = (s) => (s || '').toString().trim().toLowerCase();
    const tag = (t) => norm((t.language || '') + ' ' + (t.label || ''));
    const subTracks = () => {
        const out = [];
        const tt = v.textTracks;
        if (tt) for (let i = 0; i < tt.length; i++) {
            if (tt[i].kind === 'subtitles' || tt[i].kind === 'captions') out.push(tt[i]);
        }
        return out;
    };
    const enabledAudio = () => {
        const ats = v.audioTracks;
        if (ats) for (let i = 0; i < ats.length; i++) if (ats[i].enabled) return (ats[i].language || ats[i].label || '');
        return '';
    };
    const showingSub = () => {
        const subs = subTracks();
        for (let i = 0; i < subs.length; i++) if (subs[i].mode === 'showing') return subs[i];
        return null;
    };
    const state = () => {
        const s = showingSub();
        return {
            audio_lang: enabledAudio(),
            subtitle_lang: s ? (s.language || s.label || '') : '',
            subtitles_on: s != null,
            speed: v.playbackRate || 1,
        };
    };
    const pickIndex = (tracks, wantLang) => {
        if (wantLang) for (let i = 0; i < tracks.length; i++) if (tag(tracks[i]).includes(norm(wantLang))) return i;
        return 0;
    };
    if (v.__svApplied === undefined) {
        const ats = v.audioTracks;
        if (want.audio_lang && ats && ats.length > 1) {
            const idx = pickIndex(ats, want.audio_lang);
            for (let i = 0; i < ats.length; i++) ats[i].enabled = (i === idx);
        }
        const subs = subTracks();
        if (subs.length) {
            const idx = want.subtitles_on ? pickIndex(subs, want.subtitle_lang) : -1;
            for (let i = 0; i < subs.length; i++) subs[i].mode = (i === idx) ? 'showing' : 'disabled';
        }
        if (want.speed && want.speed > 0) v.playbackRate = want.speed;
        v.__svApplied = JSON.stringify(state());
        dioxus.send(null);
    } else {
        const j = JSON.stringify(state());
        if (j !== v.__svApplied) {
            v.__svApplied = j;
            dioxus.send(state());
        } else {
            dioxus.send(null);
        }
    }
}
"#;

#[component]
pub fn PlayerView(
    stream_url: ReadSignal<Option<String>>,
    playing_title: ReadSignal<String>,
    has_next_episode: ReadSignal<bool>,
    start_time: ReadSignal<Option<f64>>,
    player_prefs: ReadSignal<PlayerPrefs>,
    on_stop: EventHandler<()>,
    on_go_details: EventHandler<()>,
    on_next_episode: EventHandler<()>,
    on_time_update: EventHandler<(f64, f64)>,
    on_ended: EventHandler<()>,
    on_prefs_change: EventHandler<PlayerPrefs>,
) -> Element {
    let title = playing_title();
    let url = stream_url();
    let show_next = has_next_episode();

    use_future(move || async move {
        let seek_time = start_time().filter(|&t| t > 0.0).unwrap_or(0.0);
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let mut eval = document::eval(&format!(
                r#"
                const v = document.querySelector('.player-video');
                if (v && v.readyState >= 2 && !isNaN(v.duration)) {{
                    const seek = {seek_time};
                    if (seek > 0) v.currentTime = seek;
                    v.play().catch(() => {{}});
                    dioxus.send(true);
                }} else {{
                    dioxus.send(false);
                }}
                "#
            ));
            if let Ok(true) = eval.recv::<bool>().await {
                break;
            }
        }
        let mut ended_sent = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let mut eval = document::eval(
                r#"
                const v = document.querySelector('.player-video');
                if (v && v.readyState >= 2 && !isNaN(v.duration)) {
                    dioxus.send([v.currentTime, v.duration, v.ended]);
                } else {
                    dioxus.send(null);
                }
                "#,
            );
            let Ok(val) = eval.recv::<serde_json::Value>().await else {
                continue;
            };
            let Some(arr) = val.as_array() else {
                continue;
            };
            let t = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
            let d = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let e = arr.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
            if e && !ended_sent {
                ended_sent = true;
                on_ended.call(());
            } else if !e {
                ended_sent = false;
                if t > 1.0 {
                    on_time_update.call((t, d));
                }
            }
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            let json = serde_json::to_string(&player_prefs()).unwrap_or_else(|_| "{}".to_string());
            let mut eval = document::eval(&PLAYER_PREFS_JS.replace("__WANT__", &json));
            if let Ok(Some(prefs)) = eval.recv::<Option<PlayerPrefs>>().await {
                on_prefs_change.call(prefs);
            }
        }
    });

    rsx! {
        div {
            tabindex: "0",
            autofocus: true,
            class: "player-screen",
            onkeydown: move |e: KeyboardEvent| {
                let js: Option<&str> = match e.key() {
                    Key::ArrowLeft => Some("document.querySelector('.player-video').currentTime -= 10;"),
                    Key::ArrowRight => Some("document.querySelector('.player-video').currentTime += 10;"),
                    Key::Character(c) if c == " " => Some("const v=document.querySelector('.player-video');v.paused?v.play():v.pause();"),
                    _ => None,
                };
                if let Some(js) = js {
                    e.prevent_default();
                    document::eval(js);
                }
            },
            div { class: "player-top-bar",
                button { class: "btn-ghost", onclick: move |_| on_stop.call(()), "← Stop" }
                div { class: "player-title-wrapper",
                    span { class: "player-title-link", onclick: move |_| on_go_details.call(()), "{title}" }
                }
                if show_next {
                    button { class: "btn-next-episode", onclick: move |_| on_next_episode.call(()), "Next →" }
                }
            }
            div { class: "player-video-container",
                if let Some(ref src) = url {
                    video {
                        src: "{src}",
                        controls: true,
                        autoplay: true,
                        class: "player-video",
                        oncontextmenu: |e: Event<MouseData>| e.prevent_default(),
                    }
                }
            }
        }
    }
}
