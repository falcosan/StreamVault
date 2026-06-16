use dioxus::prelude::*;

const AUDIO_SYNC_JS: &str = r#"
const v = document.querySelector('.player-video');
if (!v || !v.audioTracks || v.audioTracks.length === 0) {
    dioxus.send("");
} else {
    const ts = v.audioTracks;
    const tag = (t) => ((t.language || '') + ' ' + (t.label || '')).trim().toLowerCase();
    const enabledTag = () => {
        for (let i = 0; i < ts.length; i++) if (ts[i].enabled) return (ts[i].language || ts[i].label || '');
        return '';
    };
    if (v.__svApplied === undefined) {
        const want = __WANT__;
        if (want && ts.length > 1) {
            let idx = -1;
            for (let i = 0; i < ts.length; i++) { if (tag(ts[i]).includes(want)) { idx = i; break; } }
            if (idx < 0) idx = 0;
            for (let i = 0; i < ts.length; i++) ts[i].enabled = (i === idx);
        }
        const cur = enabledTag();
        v.__svApplied = cur;
        v.__svLast = cur;
        dioxus.send("");
    } else {
        const cur = enabledTag();
        if (cur !== v.__svLast) {
            v.__svLast = cur;
            dioxus.send(cur);
        } else {
            dioxus.send("");
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
    preferred_audio_lang: ReadSignal<Option<String>>,
    on_stop: EventHandler<()>,
    on_go_details: EventHandler<()>,
    on_next_episode: EventHandler<()>,
    on_time_update: EventHandler<(f64, f64)>,
    on_ended: EventHandler<()>,
    on_language_change: EventHandler<String>,
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
            let want = preferred_audio_lang().unwrap_or_default().to_lowercase();
            let want_js = serde_json::to_string(&want).unwrap_or_else(|_| "\"\"".to_string());
            let mut eval = document::eval(&AUDIO_SYNC_JS.replace("__WANT__", &want_js));
            if let Ok(lang) = eval.recv::<String>().await {
                let lang = lang.trim().to_string();
                if !lang.is_empty() {
                    on_language_change.call(lang);
                }
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
