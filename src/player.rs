use crate::app::{AppState, cancel, restart};
use crate::gui::focus;
use crate::prefs::PlayerPrefs;
use crate::time::{now_ms, sleep};
use dioxus::core::Task;
use dioxus::document::{Eval, EvalError};
use dioxus::logger::tracing::warn;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const SCRIPT: &str = concat!(
    include_str!("../assets/js/player.js"),
    "\nawait control(dioxus);"
);
const PROGRESS_SECONDS: f64 = 10.0;
const SCRUB_DELAY: Duration = Duration::from_millis(600);
const SCRUB_TOLERANCE: f64 = 1.0;
pub const OSD_SECONDS: u64 = 4;
pub const SEEK_SECONDS: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Menu {
    Audio,
    Subtitles,
}

impl Menu {
    pub fn id(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Subtitles => "subtitles",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Tracks {
    pub audio: Vec<Track>,
    pub audio_index: i32,
    pub subtitles: Vec<Track>,
    pub subtitle_index: i32,
}

impl Tracks {
    pub fn list(&self, menu: Menu) -> (&[Track], i32) {
        match menu {
            Menu::Audio => (&self.audio, self.audio_index),
            Menu::Subtitles => (&self.subtitles, self.subtitle_index),
        }
    }

    pub fn current(&self, menu: Menu) -> Option<&Track> {
        let (list, index) = self.list(menu);
        usize::try_from(index).ok().and_then(|i| list.get(i))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Track {
    language: String,
    label: String,
}

impl Track {
    pub fn name(&self) -> String {
        if self.label.is_empty() {
            self.language.to_uppercase()
        } else {
            self.label.clone()
        }
    }

    fn lang(&self) -> String {
        if self.language.is_empty() {
            self.label.clone()
        } else {
            self.language.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct Status {
    pub time: f64,
    pub duration: f64,
    pub paused: bool,
    pub waiting: bool,
    pub fullscreen: bool,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Command<'a> {
    Attach {
        url: &'a str,
        start: f64,
        prefs: &'a PlayerPrefs,
    },
    Detach,
    Toggle,
    SeekTo {
        time: f64,
    },
    Select {
        menu: Menu,
        index: i32,
    },
    Fullscreen,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    Status(Status),
    Tracks(Tracks),
    Ended,
    Failed { reason: String },
}

#[derive(Clone, Copy)]
pub struct Player {
    pub osd: Signal<u32>,
    pub menu: Signal<Option<Menu>>,
    pub status: Signal<Status>,
    pub tracks: Signal<Tracks>,
    pub scrub: Signal<Option<f64>>,
    pending: Signal<Option<Task>>,
    last_input: CopyValue<u64>,
    channel: CopyValue<Option<Eval>>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            osd: Signal::default(),
            menu: Signal::default(),
            scrub: Signal::default(),
            status: Signal::default(),
            tracks: Signal::default(),
            pending: Signal::default(),
            last_input: CopyValue::new(0),
            channel: CopyValue::new(None),
        }
    }

    pub async fn run(mut self, app: AppState) {
        let mut channel = document::eval(SCRIPT);
        self.channel.set(Some(channel));
        let mut saved = None;
        loop {
            let event = match channel.recv::<Event>().await {
                Ok(event) => event,
                Err(EvalError::Serialization(e)) => {
                    warn!("unreadable player event: {e}");
                    continue;
                }
                Err(e) => return warn!("player channel closed: {e}"),
            };
            match event {
                Event::Status(status) => {
                    let target = *self.scrub.peek();
                    if self.pending.peek().is_none()
                        && target.is_some_and(|target| settled(target, status.time))
                    {
                        self.scrub.set(None);
                    }
                    self.status.set(status);
                    let tick = (status.time / PROGRESS_SECONDS) as u64;
                    if status.paused || saved != Some(tick) {
                        saved = Some(tick);
                        app.progress(status.time, status.duration);
                    }
                }
                Event::Tracks(tracks) => self.tracks.set(tracks),
                Event::Ended => {
                    app.finished();
                    focus("#control-next");
                }
                Event::Failed { reason } => app.report("Playback failed", reason),
            }
        }
    }

    pub fn attach(mut self, url: &str, start: f64, prefs: &PlayerPrefs) {
        self.last_input.set(now_ms());
        self.send(Command::Attach { url, start, prefs });
    }

    pub fn detach(self) {
        self.send(Command::Detach);
    }

    pub fn reset(mut self) {
        cancel(self.pending);
        self.scrub.set(None);
        self.menu.set(None);
        self.status.set(Status::default());
        self.tracks.set(Tracks::default());
    }

    pub fn wake(mut self) -> bool {
        self.osd += 1;
        let now = now_ms();
        let last = std::mem::replace(&mut *self.last_input.write(), now);
        now.saturating_sub(last) > OSD_SECONDS * 1000
    }

    pub fn toggle(self) {
        self.flush();
        self.send(Command::Toggle);
    }

    pub fn seek(self, by: f64) {
        let from = self.scrub.peek().unwrap_or(self.status.peek().time);
        self.scrub_to(from + by);
    }

    pub fn scrub_to(mut self, time: f64) {
        let Some(target) = scrub_target(time, self.status.peek().duration) else {
            return;
        };
        self.scrub.set(Some(target));
        restart(self.pending, async move {
            sleep(SCRUB_DELAY).await;
            self.pending.set(None);
            self.commit();
        });
    }

    pub fn seek_to(self, time: f64) {
        self.scrub_to(time);
        self.flush();
    }

    fn flush(mut self) {
        if let Some(task) = self.pending.take() {
            task.cancel();
            self.commit();
        }
    }

    fn commit(self) {
        if let Some(time) = *self.scrub.peek() {
            self.send(Command::SeekTo { time });
        }
    }

    pub fn fullscreen(self) {
        self.send(Command::Fullscreen);
    }

    pub fn toggle_menu(mut self, menu: Menu) {
        if *self.menu.peek() == Some(menu) {
            self.close(menu);
        } else {
            self.menu.set(Some(menu));
        }
    }

    pub fn close(mut self, menu: Menu) {
        focus(&format!("#control-{}", menu.id()));
        self.menu.set(None);
    }

    pub fn choose(self, app: AppState, menu: Menu, index: i32) {
        self.send(Command::Select { menu, index });
        let mut prefs = app.prefs.peek().clone();
        let chosen = usize::try_from(index)
            .ok()
            .and_then(|i| self.tracks.peek().list(menu).0.get(i).cloned());
        match (menu, chosen) {
            (Menu::Audio, Some(track)) => prefs.audio_lang = track.lang(),
            (Menu::Subtitles, Some(track)) => {
                prefs.subtitles_on = true;
                prefs.subtitle_lang = track.lang();
            }
            (Menu::Subtitles, None) => {
                prefs.subtitles_on = false;
                prefs.subtitle_lang = String::new();
            }
            (Menu::Audio, None) => {}
        }
        app.change_prefs(prefs);
        self.close(menu);
    }

    fn send(self, command: Command) {
        if let Some(channel) = *self.channel.peek()
            && let Err(e) = channel.send(&command)
        {
            warn!("player command failed: {e}");
        }
    }
}

fn scrub_target(time: f64, duration: f64) -> Option<f64> {
    (duration > 0.0).then(|| time.clamp(0.0, duration))
}

fn settled(target: f64, time: f64) -> bool {
    (time - target).abs() < SCRUB_TOLERANCE
}

pub fn clock(seconds: f64) -> String {
    let total = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn track(language: &str, label: &str) -> Track {
        Track {
            language: language.into(),
            label: label.into(),
        }
    }

    #[test]
    fn clock_formats_minutes_and_hours() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(65.9), "1:05");
        assert_eq!(clock(3_725.0), "1:02:05");
        assert_eq!(clock(f64::NAN), "0:00");
        assert_eq!(clock(-4.0), "0:00");
    }

    #[test]
    fn scrubbing_stays_inside_a_known_duration() {
        assert_eq!(scrub_target(-5.0, 100.0), Some(0.0));
        assert_eq!(scrub_target(40.0, 100.0), Some(40.0));
        assert_eq!(scrub_target(130.0, 100.0), Some(100.0));
        assert_eq!(scrub_target(40.0, 0.0), None);
    }

    #[test]
    fn a_scrub_settles_once_playback_reaches_its_target() {
        assert!(settled(40.0, 40.0));
        assert!(settled(40.0, 40.6));
        assert!(!settled(40.0, 12.0));
    }

    #[test]
    fn track_names_and_languages_fall_back_to_each_other() {
        let (labelled, bare) = (track("ita", "Italian"), track("eng", ""));
        assert_eq!(
            (labelled.name(), labelled.lang()),
            ("Italian".into(), "ita".into())
        );
        assert_eq!((bare.name(), bare.lang()), ("ENG".into(), "eng".into()));
    }

    #[test]
    fn current_track_follows_the_selected_index() {
        let tracks = Tracks {
            audio: vec![track("ita", ""), track("eng", "")],
            audio_index: 1,
            subtitles: vec![track("ita", "")],
            subtitle_index: -1,
        };
        assert_eq!(tracks.current(Menu::Audio), Some(&track("eng", "")));
        assert_eq!(tracks.current(Menu::Subtitles), None);
        assert_eq!(tracks.list(Menu::Subtitles).1, -1);
    }

    #[test]
    fn events_parse_the_player_script_messages() {
        let parse = |value| serde_json::from_value::<Event>(value).unwrap();
        let status = json!({"type": "status", "time": 5.5, "duration": 60, "paused": false, "waiting": true, "fullscreen": true});
        assert!(matches!(
            parse(status),
            Event::Status(Status {
                time: 5.5,
                duration: 60.0,
                paused: false,
                waiting: true,
                fullscreen: true
            })
        ));
        let tracks = json!({"type": "tracks", "audio": [{"language": "ita", "label": "Italian"}], "audio_index": 0, "subtitles": [], "subtitle_index": -1});
        assert!(matches!(parse(tracks), Event::Tracks(t) if t.audio.len() == 1));
        assert!(matches!(parse(json!({"type": "ended"})), Event::Ended));
        assert!(
            matches!(parse(json!({"type": "failed", "reason": "x"})), Event::Failed { reason } if reason == "x")
        );
    }

    #[test]
    fn commands_serialize_for_the_player_script() {
        let prefs = PlayerPrefs::default();
        let attach = Command::Attach {
            url: "u",
            start: 3.0,
            prefs: &prefs,
        };
        assert_eq!(
            serde_json::to_value(&attach).unwrap()["type"],
            json!("attach")
        );
        assert_eq!(
            serde_json::to_value(Command::SeekTo { time: 1.0 }).unwrap(),
            json!({"type": "seek_to", "time": 1.0})
        );
        assert_eq!(
            serde_json::to_value(Command::Select {
                menu: Menu::Subtitles,
                index: -1
            })
            .unwrap(),
            json!({"type": "select", "menu": "subtitles", "index": -1})
        );
    }
}
