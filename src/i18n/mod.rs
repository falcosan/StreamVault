mod en;
mod it;

use dioxus::prelude::*;

static LANG: GlobalSignal<Lang> = Signal::global(|| preferred(sys_locale::get_locales()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    It,
}

impl Lang {
    pub const ALL: [Self; 2] = [Self::En, Self::It];

    pub fn current() -> Self {
        *LANG.read()
    }

    pub fn select(self) {
        if *LANG.peek() != self {
            *LANG.write() = self;
        }
    }

    pub fn parse(tag: &str) -> Option<Self> {
        let primary = tag.split(['-', '_']).next()?;
        Self::ALL.into_iter().find(|lang| {
            lang.codes()
                .iter()
                .any(|code| code.eq_ignore_ascii_case(primary))
        })
    }

    pub fn code(self) -> &'static str {
        self.codes()[0]
    }

    pub fn codes(self) -> &'static [&'static str] {
        match self {
            Self::En => &["en", "eng"],
            Self::It => &["it", "ita"],
        }
    }

    pub fn text(self) -> &'static Text {
        match self {
            Self::En => &en::TEXT,
            Self::It => &it::TEXT,
        }
    }
}

pub fn text() -> &'static Text {
    Lang::current().text()
}

fn preferred(tags: impl IntoIterator<Item = String>) -> Lang {
    tags.into_iter()
        .find_map(|tag| Lang::parse(&tag))
        .unwrap_or(Lang::En)
}

pub struct Text {
    pub english: &'static str,
    pub italian: &'static str,
    pub language: &'static str,
    pub forced: &'static str,
    pub decimal: &'static str,
    pub back: &'static str,
    pub home: &'static str,
    pub loading: &'static str,
    pub search: &'static str,
    pub search_placeholder: &'static str,
    pub submit_search: &'static str,
    pub updating: &'static str,
    pub install_update: &'static str,
    pub update_failed: &'static str,
    pub dismiss_error: &'static str,
    pub continue_watching: &'static str,
    pub remove_from_continue_watching: fn(&str) -> String,
    pub no_media: &'static str,
    pub retry: &'static str,
    pub loading_more: &'static str,
    pub searching: &'static str,
    pub no_results: &'static str,
    pub results: &'static str,
    pub result_count: fn(usize) -> String,
    pub movie: &'static str,
    pub series: &'static str,
    pub season_count: fn(usize) -> String,
    pub seasons: &'static str,
    pub season: fn(u32) -> String,
    pub episodes: &'static str,
    pub episode: fn(u32) -> String,
    pub no_episodes: &'static str,
    pub play: &'static str,
    pub resume: &'static str,
    pub play_episode: fn(u32, &str) -> String,
    pub time_left: fn(&str) -> String,
    pub episodes_failed: &'static str,
    pub next_season_failed: &'static str,
    pub stream_failed: &'static str,
    pub playback_failed: &'static str,
    pub pause: &'static str,
    pub seek: &'static str,
    pub time_of: fn(&str, &str) -> String,
    pub audio: &'static str,
    pub subtitles: &'static str,
    pub audio_track: fn(&str) -> String,
    pub subtitles_track: fn(&str) -> String,
    pub default_track: &'static str,
    pub off: &'static str,
    pub next_episode: &'static str,
    pub fullscreen: &'static str,
    pub exit_fullscreen: &'static str,
    #[cfg(feature = "desktop")]
    pub desktop: Desktop,
}

impl Text {
    pub fn name_of(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::En => self.english,
            Lang::It => self.italian,
        }
    }
}

#[cfg(feature = "desktop")]
pub struct Desktop {
    pub downloads: &'static str,
    pub download: &'static str,
    pub queued: &'static str,
    pub progress: fn(u8) -> String,
    pub finishing: &'static str,
    pub downloaded: &'static str,
    pub retry: &'static str,
    pub download_this: fn(Option<u32>) -> String,
    pub play_this: fn(Option<u32>) -> String,
    pub retry_this: fn(Option<u32>) -> String,
    pub show_this: fn(Option<u32>) -> String,
    pub empty: &'static str,
    pub empty_hint: &'static str,
    pub browse: &'static str,
    pub active: &'static str,
    pub ready_to_watch: &'static str,
    pub waiting: &'static str,
    pub progress_of: fn(&str) -> String,
    pub finishing_of: fn(&str) -> String,
    pub failed: fn(&str) -> String,
    pub reveal: &'static str,
    pub reveal_title: fn(&str) -> String,
    pub reveal_failed: &'static str,
    pub play_title: fn(&str) -> String,
    pub delete: &'static str,
    pub delete_title: fn(&str) -> String,
    pub retry_title: fn(&str) -> String,
    pub remove: &'static str,
    pub remove_title: fn(&str) -> String,
    pub cancel: &'static str,
    pub cancel_title: fn(&str) -> String,
    pub ready: &'static str,
    pub running: fn(usize) -> String,
    pub failed_count: fn(usize) -> String,
    pub titles_ready: fn(usize) -> String,
    pub delete_failed: &'static str,
    pub file_missing: &'static str,
    pub restart: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_supported_system_language_wins() {
        let tags = |tags: &[&str]| preferred(tags.iter().map(|tag| (*tag).to_owned()));
        assert_eq!(tags(&["pt-BR", "es-ES", "it-IT"]), Lang::It);
        assert_eq!(tags(&["en_GB"]), Lang::En);
        assert_eq!(tags(&["IT"]), Lang::It);
        assert_eq!(tags(&["zh-Hans-CN"]), Lang::En);
        assert_eq!(tags(&[]), Lang::En);
    }

    #[test]
    fn track_codes_name_the_same_languages() {
        assert_eq!(Lang::parse("ita"), Some(Lang::It));
        assert_eq!(Lang::parse("eng"), Some(Lang::En));
        assert_eq!(Lang::parse("jpn"), None);
        assert_eq!(Lang::parse(""), None);
    }

    #[test]
    fn every_language_calls_itself_by_its_own_name() {
        let names: Vec<_> = Lang::ALL
            .into_iter()
            .map(|lang| lang.text().name_of(lang))
            .collect();
        assert_eq!(names, ["English", "Italiano"]);
        assert_eq!(Lang::It.text().name_of(Lang::En), "Inglese");
    }

    #[test]
    fn counts_follow_the_cldr_plural_rules() {
        let (en, it) = (Lang::En.text(), Lang::It.text());
        let counts = |text: &Text| [0, 1, 2].map(text.result_count);
        assert_eq!(counts(en), ["0 results", "1 result", "2 results"]);
        assert_eq!(counts(it), ["0 risultati", "1 risultato", "2 risultati"]);
    }
}
