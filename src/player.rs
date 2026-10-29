use crate::app::{AppState, cancel, restart};
use crate::i18n::{Lang, Text, text};
use crate::platform::focus;
use crate::prefs::PlayerPrefs;
use crate::time::{now_ms, sleep};
use crate::watch;
use dioxus::core::Task;
use dioxus::document::{Eval, EvalError};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use web_time::Instant;

const SCRIPT: &str = concat!(
    include_str!("../assets/js/player.js"),
    "\nawait control(dioxus);"
);
const PROGRESS_SECONDS: f64 = 10.0;
const SCRUB_DELAY: Duration = Duration::from_millis(600);
const SCRUB_TOLERANCE: f64 = 1.0;
const STEP_INTERVAL: Duration = Duration::from_millis(200);
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
            Self::Audio => "control-audio",
            Self::Subtitles => "control-subtitles",
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
        self.get(menu, self.list(menu).1)
    }

    fn get(&self, menu: Menu, index: i32) -> Option<&Track> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.list(menu).0.get(i))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Track {
    language: String,
    label: String,
}

impl Track {
    pub fn name(&self, t: &Text) -> String {
        let forced = self.language.strip_prefix("forced-");
        let localized = Lang::parse(forced.unwrap_or(&self.language)).and_then(|lang| {
            let rest = if self.label.is_empty() || self.label == self.language {
                ""
            } else {
                self.label
                    .strip_prefix(Lang::En.text().name_of(lang))
                    .filter(|rest| rest.is_empty() || rest.starts_with(' '))?
            };
            let name = t.name_of(lang);
            Some(match forced {
                Some(_) => format!("{name} [{}]", t.forced),
                None => format!("{name}{rest}"),
            })
        });
        localized.unwrap_or_else(|| {
            if self.label.is_empty() {
                self.language.to_uppercase()
            } else {
                self.label.clone()
            }
        })
    }

    fn lang(&self) -> String {
        if self.language.is_empty() {
            self.label.clone()
        } else {
            self.language.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
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
        languages: [&'static [&'static str]; 2],
        generation: u32,
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

#[derive(Deserialize)]
struct Message {
    generation: u32,
    #[serde(flatten)]
    event: Event,
}

impl Message {
    fn current(self, generation: u32) -> Option<Event> {
        (self.generation == generation).then_some(self.event)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scrub {
    pub from: f64,
    pub to: f64,
}

impl Scrub {
    fn next(current: Option<Self>, playing: f64, to: f64) -> Self {
        Self {
            from: current.map_or(playing, |scrub| scrub.from),
            to,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Streak {
    forward: bool,
    since: Instant,
    last: Instant,
}

impl Streak {
    fn next(previous: Option<Self>, forward: bool, now: Instant) -> Option<Self> {
        match previous.filter(|streak| streak.forward == forward) {
            Some(streak) if now.duration_since(streak.last) < STEP_INTERVAL => None,
            Some(streak) => Some(Self {
                last: now,
                ..streak
            }),
            None => Some(Self {
                forward,
                since: now,
                last: now,
            }),
        }
    }

    fn boost(self) -> f64 {
        match self.last.duration_since(self.since).as_secs() {
            0 => 1.0,
            1 => 2.0,
            2 => 4.0,
            3 => 8.0,
            _ => 16.0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Player {
    pub osd: Signal<bool>,
    pub menu: Signal<Option<Menu>>,
    pub status: Signal<Status>,
    pub tracks: Signal<Tracks>,
    pub scrub: Signal<Option<Scrub>>,
    pending: CopyValue<Option<Task>>,
    streak: CopyValue<Option<Streak>>,
    generation: CopyValue<u32>,
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
            pending: CopyValue::default(),
            streak: CopyValue::default(),
            generation: CopyValue::default(),
            last_input: CopyValue::default(),
            channel: CopyValue::default(),
        }
    }

    pub async fn run(mut self, app: AppState) {
        let mut channel = document::eval(SCRIPT);
        self.channel.set(Some(channel));
        let mut saved = None;
        loop {
            let message = match channel.recv::<Message>().await {
                Ok(message) => message,
                Err(EvalError::Serialization(e)) => {
                    warn!("unreadable player event: {e}");
                    continue;
                }
                Err(e) => return warn!("player channel closed: {e}"),
            };
            let Some(event) = message.current(*self.generation.peek()) else {
                continue;
            };
            match event {
                Event::Status(status) => {
                    let scrub = *self.scrub.peek();
                    if self.pending.peek().is_none()
                        && scrub.is_some_and(|scrub| settled(scrub.to, status.time))
                    {
                        self.scrub.set(None);
                    }
                    self.status.set(status);
                    let tick = (status.time / PROGRESS_SECONDS) as u64;
                    if status.paused || saved != Some(tick) {
                        saved = Some(tick);
                        app.progress(status.time, status.duration);
                    }
                    if !status.paused
                        && watch::ending(status.time, status.duration)
                        && app.has_next()
                    {
                        app.play_next();
                    }
                }
                Event::Tracks(tracks) => self.tracks.set(tracks),
                Event::Ended => {
                    app.finished();
                    focus("#control-next");
                }
                Event::Failed { reason } => app.report(text().playback_failed, reason),
            }
        }
    }

    pub fn attach(mut self, url: &str, start: f64, prefs: &PlayerPrefs) {
        self.last_input.set(now_ms());
        self.send(Command::Attach {
            url,
            start,
            prefs,
            languages: [Lang::current().codes(), Lang::En.codes()],
            generation: *self.generation.peek(),
        });
    }

    pub fn detach(self) {
        self.send(Command::Detach);
    }

    pub fn reset(mut self) {
        self.generation += 1;
        cancel(self.pending);
        self.scrub.set(None);
        self.menu.set(None);
        self.status.set(Status::default());
        self.tracks.set(Tracks::default());
    }

    pub fn wake(mut self) -> bool {
        self.osd.toggle();
        let now = now_ms();
        let last = std::mem::replace(&mut *self.last_input.write(), now);
        now.saturating_sub(last) > OSD_SECONDS * 1000
    }

    pub fn toggle(self) {
        self.flush();
        self.send(Command::Toggle);
    }

    pub fn seek(mut self, by: f64) {
        let previous = self
            .pending
            .peek()
            .is_some()
            .then(|| *self.streak.peek())
            .flatten();
        let Some(streak) = Streak::next(previous, by > 0.0, Instant::now()) else {
            return;
        };
        self.streak.set(Some(streak));
        let from = self
            .scrub
            .peek()
            .map_or(self.status.peek().time, |scrub| scrub.to);
        self.scrub_to(from + by * streak.boost());
    }

    pub fn scrub_to(mut self, time: f64) {
        let status = *self.status.peek();
        let Some(to) = scrub_target(time, status.duration) else {
            return;
        };
        let scrub = Scrub::next(*self.scrub.peek(), status.time, to);
        self.scrub.set(Some(scrub));
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
        if let Some(scrub) = *self.scrub.peek() {
            self.send(Command::SeekTo { time: scrub.to });
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
        focus(&format!("#{}", menu.id()));
        self.menu.set(None);
    }

    pub fn choose(self, app: AppState, menu: Menu, index: i32) {
        self.send(Command::Select { menu, index });
        let prefs = with_choice(
            app.prefs.peek().clone(),
            menu,
            self.tracks.peek().get(menu, index),
        );
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

fn with_choice(mut prefs: PlayerPrefs, menu: Menu, track: Option<&Track>) -> PlayerPrefs {
    match (menu, track) {
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
    prefs
}

fn scrub_target(time: f64, duration: f64) -> Option<f64> {
    (duration > 0.0).then(|| time.clamp(0.0, duration))
}

fn settled(target: f64, time: f64) -> bool {
    (time - target).abs() < SCRUB_TOLERANCE
}

pub fn offset_label(target: f64, from: f64) -> Option<String> {
    if settled(target, from) {
        return None;
    }
    let sign = if target > from { '+' } else { '−' };
    let seconds = (target - from).abs().round();
    Some(if seconds < 60.0 {
        format!("{sign}{seconds} s")
    } else {
        format!("{sign}{}", clock(seconds))
    })
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
    fn a_scrub_measures_from_where_it_started_while_playback_moves_on() {
        let first = Scrub::next(None, 12.0, 22.0);
        assert_eq!(
            first,
            Scrub {
                from: 12.0,
                to: 22.0
            }
        );
        let further = Scrub::next(Some(first), 13.5, 42.0);
        assert_eq!(
            further,
            Scrub {
                from: 12.0,
                to: 42.0
            }
        );
        assert_eq!(
            offset_label(further.to, further.from).as_deref(),
            Some("+30 s")
        );
    }

    #[test]
    fn a_held_key_steps_at_most_every_interval() {
        let start = Instant::now();
        let first = Streak::next(None, true, start).unwrap();
        assert_eq!(
            Streak::next(Some(first), true, start + STEP_INTERVAL / 2),
            None
        );
        let second = Streak::next(Some(first), true, start + STEP_INTERVAL).unwrap();
        assert_eq!((second.since, second.last), (start, start + STEP_INTERVAL));
        let back = Streak::next(Some(second), false, start + STEP_INTERVAL).unwrap();
        assert_eq!((back.forward, back.since), (false, start + STEP_INTERVAL));
    }

    #[test]
    fn the_step_doubles_for_every_second_of_the_streak() {
        let since = Instant::now();
        let boost = |held: u64| {
            Streak {
                forward: true,
                since,
                last: since + Duration::from_millis(held),
            }
            .boost()
        };
        assert_eq!(
            [0, 999, 1_000, 2_000, 3_000, 4_000, 60_000].map(boost),
            [1.0, 1.0, 2.0, 4.0, 8.0, 16.0, 16.0]
        );
    }

    #[test]
    fn the_offset_reads_in_seconds_then_minutes() {
        assert_eq!(offset_label(40.0, 10.3).as_deref(), Some("+30 s"));
        assert_eq!(offset_label(10.0, 100.0).as_deref(), Some("−1:30"));
        assert_eq!(offset_label(200.0, 10.0).as_deref(), Some("+3:10"));
        assert_eq!(offset_label(10.4, 10.0), None);
    }

    #[test]
    fn a_scrub_settles_once_playback_reaches_its_target() {
        assert!(settled(40.0, 40.0));
        assert!(settled(40.0, 40.6));
        assert!(!settled(40.0, 12.0));
    }

    #[test]
    fn track_names_and_languages_fall_back_to_each_other() {
        let en = Lang::En.text();
        let (labelled, bare) = (track("", "Japanese"), track("jpn", ""));
        assert_eq!(
            (labelled.name(en), labelled.lang()),
            ("Japanese".into(), "Japanese".into())
        );
        assert_eq!((bare.name(en), bare.lang()), ("JPN".into(), "jpn".into()));
    }

    #[test]
    fn track_names_speak_the_interface_language() {
        let it = Lang::It.text();
        assert_eq!(track("ita", "Italian").name(it), "Italiano");
        assert_eq!(track("eng", "English [CC]").name(it), "Inglese [CC]");
        assert_eq!(
            track("forced-ita", "Italian [Forced]").name(it),
            "Italiano [Forzati]"
        );
        assert_eq!(track("en", "").name(it), "Inglese");
        assert_eq!(
            track("forced-ita", "forced-ita").name(it),
            "Italiano [Forzati]"
        );
        assert_eq!(track("eng", "Commentary").name(it), "Commentary");
        assert_eq!(track("ita", "Italiano").name(Lang::En.text()), "Italiano");
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
    fn only_events_of_the_current_playback_are_kept() {
        let message = |generation| {
            let status = json!({"type": "status", "time": 5, "duration": 60, "paused": false, "waiting": false, "fullscreen": false, "generation": generation});
            serde_json::from_value::<Message>(status).unwrap()
        };
        assert!(matches!(
            message(3).current(3),
            Some(Event::Status(Status { duration: 60.0, .. }))
        ));
        assert!(message(2).current(3).is_none());
    }

    #[test]
    fn choosing_a_track_becomes_the_preference() {
        let prefs = PlayerPrefs {
            audio_lang: "eng".into(),
            subtitle_lang: "ita".into(),
            subtitles_on: true,
            ..PlayerPrefs::default()
        };
        let ita = track("ita", "Italian");
        let audio = with_choice(prefs.clone(), Menu::Audio, Some(&ita));
        assert_eq!(audio.audio_lang, "ita");
        assert_eq!(with_choice(prefs.clone(), Menu::Audio, None), prefs);
        let off = with_choice(prefs.clone(), Menu::Subtitles, None);
        assert_eq!((off.subtitles_on, off.subtitle_lang.as_str()), (false, ""));
        let on = with_choice(off, Menu::Subtitles, Some(&track("eng", "")));
        assert_eq!((on.subtitles_on, on.subtitle_lang.as_str()), (true, "eng"));
    }

    #[test]
    fn commands_serialize_for_the_player_script() {
        let prefs = PlayerPrefs::default();
        let attach = Command::Attach {
            url: "u",
            start: 3.0,
            prefs: &prefs,
            languages: [Lang::It.codes(), Lang::En.codes()],
            generation: 4,
        };
        let attach = serde_json::to_value(&attach).unwrap();
        assert_eq!(
            (&attach["type"], &attach["generation"]),
            (&json!("attach"), &json!(4))
        );
        assert_eq!(attach["languages"], json!([["it", "ita"], ["en", "eng"]]));
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

    #[cfg(target_os = "macos")]
    mod script {
        use crate::check::run_js;
        use serde_json::{Value, json};

        const TRACKS: &str = r#"[{ language: "eng", label: "English [CC]" }, { language: "forced-ita", label: "Italian [Forced]" }, { language: "ita", label: "Italian" }]"#;

        fn eval(expression: &str) -> Value {
            run_js(&format!(
                "var HTMLMediaElement = {{ HAVE_FUTURE_DATA: 3 }};{}JSON.stringify({expression});",
                include_str!("../assets/js/player.js")
            ))
        }

        #[test]
        fn pick_prefers_the_exact_track_over_a_partial_match() {
            assert_eq!(
                eval(&format!(
                    r#"["ita", "forced-ita", "Italian"].map((lang) => pick({TRACKS}, lang))"#
                )),
                json!([2, 1, 2])
            );
        }

        #[test]
        fn without_a_choice_the_audio_speaks_the_interface_then_english() {
            let spoken = |tracks: &str| {
                eval(&format!(
                    r#"spoken({tracks}, [["it", "ita"], ["en", "eng"]])"#
                ))
            };
            assert_eq!(
                spoken(r#"[{ language: "eng" }, { language: "ita" }]"#),
                json!(1)
            );
            assert_eq!(
                spoken(r#"[{ language: "jpn" }, { language: "en-US" }]"#),
                json!(1)
            );
            assert_eq!(spoken(r#"[{ language: "jpn" }]"#), json!(-1));
        }

        #[test]
        fn a_track_the_viewer_picked_survives_a_track_list_update() {
            assert_eq!(
                eval(
                    r#"(() => {
                        const listeners = {};
                        const target = (object) => Object.assign(object, { addEventListener: (name, listener) => (listeners[name] = listener), removeEventListener() {} });
                        globalThis.document = { addEventListener() {}, removeEventListener() {} };
                        const audioTracks = target([{ language: "ita", enabled: true }, { language: "eng", enabled: false }]);
                        const video = target({ audioTracks, play: async () => {}, removeAttribute() {}, load() {} });
                        const prefs = { audio_lang: "", subtitle_lang: "", subtitles_on: false, speed: 1 };
                        const session = attach(video, "movie.mp4", 0, prefs, [["es", "spa"], ["en", "eng"]], () => {});
                        const enabled = () => audioTracks.findIndex((t) => t.enabled);
                        listeners.addtrack();
                        const automatic = enabled();
                        session.select("audio", 0);
                        listeners.addtrack();
                        return [automatic, enabled()];
                    })()"#
                ),
                json!([1, 0])
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
                json!([true, false, true, true, false])
            );
        }

        #[test]
        fn pick_falls_back_to_a_partial_match_then_the_first_track() {
            assert_eq!(
                eval(&format!(
                    r#"["ital", "xyz", ""].map((lang) => pick({TRACKS}, lang))"#
                )),
                json!([1, 0, 0])
            );
        }
    }
}
