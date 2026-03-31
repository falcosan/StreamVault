use dioxus::prelude::*;

#[component]
pub fn PlayerView(
    stream_url: ReadSignal<Option<String>>,
    playing_title: ReadSignal<String>,
    has_next_episode: ReadSignal<bool>,
    start_time: ReadSignal<Option<f64>>,
    on_stop: EventHandler<()>,
    on_go_details: EventHandler<()>,
    on_next_episode: EventHandler<()>,
    on_time_update: EventHandler<(f64, f64)>,
    on_ended: EventHandler<()>,
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
