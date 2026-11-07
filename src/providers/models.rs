use super::{MAX_SAFE_INTEGER, stable_hash};
use serde::{Deserialize, Serialize};

const JSON_ROUNDING: u64 = 1 << 11;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    Movie,
    #[default]
    Series,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaEntry {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub provider: usize,
    pub provider_name: String,
    pub language: String,
    pub media_type: MediaType,
    pub alternative_names: Vec<String>,
    pub year: Option<String>,
    pub score: Option<String>,
    pub image_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub description: Option<String>,
}

pub type EpisodeOrder = (u32, Option<u8>);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub id: u64,
    pub number: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<u8>,
    pub name: String,
    pub duration: Option<u32>,
    pub image_url: Option<String>,
    #[serde(skip)]
    pub description: Option<String>,
}

pub struct Details {
    pub entry: MediaEntry,
    pub seasons: Vec<u32>,
}

pub struct StreamUrl {
    pub url: String,
    #[cfg(feature = "desktop")]
    pub headers: Vec<(String, String)>,
}

impl StreamUrl {
    pub(super) fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            #[cfg(feature = "desktop")]
            headers: Vec::new(),
        }
    }
}

impl Episode {
    pub fn code(&self, season: u32) -> String {
        format!("S{season:02}E{:02}{}", self.number, self.fraction())
    }

    pub fn number_text(&self) -> String {
        format!("{}{}", self.number, self.fraction())
    }

    pub fn fraction(&self) -> String {
        self.part.map_or_else(String::new, |hundredths| {
            format!(".{}", format!("{hundredths:02}").trim_end_matches('0'))
        })
    }

    pub fn order(&self) -> EpisodeOrder {
        (self.number, self.part)
    }

    pub fn label(&self, season: u32) -> String {
        let code = self.code(season);
        if self.name.is_empty() {
            code
        } else {
            format!("{code} - {}", self.name)
        }
    }

    pub fn matches(&self, saved: &Self) -> bool {
        self.id == saved.id || saved.id > MAX_SAFE_INTEGER && self.was_saved_as(saved)
    }

    fn was_saved_as(&self, saved: &Self) -> bool {
        (saved.id & MAX_SAFE_INTEGER).abs_diff(self.id) <= JSON_ROUNDING
            || !saved.name.is_empty() && self.name == saved.name && self.order() == saved.order()
    }
}

impl MediaEntry {
    pub fn key(&self) -> (usize, u64) {
        (self.provider, self.safe_id())
    }

    pub(super) fn safe_id(&self) -> u64 {
        if self.id > MAX_SAFE_INTEGER {
            stable_hash(&self.slug)
        } else {
            self.id
        }
    }

    pub fn key_at(
        &self,
        season: Option<u32>,
        episode: Option<&Episode>,
    ) -> (usize, u64, Option<u32>, Option<EpisodeOrder>) {
        (
            self.provider,
            self.safe_id(),
            season,
            episode.map(Episode::order),
        )
    }

    pub fn is_movie(&self) -> bool {
        self.media_type == MediaType::Movie
    }

    pub fn display_title(&self) -> String {
        match &self.year {
            Some(year) => format!("{} ({year})", self.name),
            None => self.name.clone(),
        }
    }

    pub fn title(&self, season: Option<u32>, episode: Option<&Episode>) -> String {
        match (season, episode) {
            (Some(season), Some(episode)) => format!("{} {}", self.name, episode.label(season)),
            _ => self.display_title(),
        }
    }
}

pub fn next_episode<'a>(episodes: &'a [Episode], current: &Episode) -> Option<&'a Episode> {
    episodes
        .iter()
        .filter(|e| (e.order(), e.id) > (current.order(), current.id))
        .min_by_key(|e| (e.order(), e.id))
}

#[cfg(test)]
impl Episode {
    pub(crate) fn numbered(number: u32, name: &str) -> Self {
        Self {
            id: u64::from(number),
            number,
            part: None,
            name: name.into(),
            duration: None,
            image_url: None,
            description: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG_ID: u64 = 1_723_015_840_060_750_309;
    const ROUNDED_ID: u64 = 1_723_015_840_060_750_300;

    fn movie() -> MediaEntry {
        MediaEntry {
            name: "Inception".into(),
            media_type: MediaType::Movie,
            year: Some("2010".into()),
            ..Default::default()
        }
    }

    #[test]
    fn records_saved_before_backdrops_still_load() {
        let json = r#"{"id":1,"name":"x","slug":"","provider":0,"provider_name":"","language":"","media_type":"Movie","alternative_names":[],"year":null,"score":null,"image_url":null,"description":null}"#;
        let entry: MediaEntry = serde_json::from_str(json).unwrap();
        assert_eq!((entry.id, entry.backdrop_url), (1, None));
    }

    #[test]
    fn episode_synopses_are_never_persisted() {
        let episode = Episode {
            description: Some("Plot".into()),
            ..Episode::numbered(1, "Pilot")
        };
        let json = serde_json::to_value(&episode).unwrap();
        assert!(json.get("description").is_none());
        let restored: Episode = serde_json::from_value(json).unwrap();
        assert_eq!(restored.description, None);
    }

    #[test]
    fn display_title_appends_year_when_known() {
        assert_eq!(movie().display_title(), "Inception (2010)");
        let untitled = MediaEntry {
            year: None,
            ..movie()
        };
        assert_eq!(untitled.display_title(), "Inception");
    }

    #[test]
    fn is_movie_follows_media_type() {
        assert!(movie().is_movie());
        assert!(!MediaEntry::default().is_movie());
    }

    #[test]
    fn episode_label_adds_the_name_only_when_present() {
        assert_eq!(Episode::numbered(3, "").code(1), "S01E03");
        assert_eq!(Episode::numbered(3, "").label(1), "S01E03");
        assert_eq!(Episode::numbered(12, "Pilot").label(10), "S10E12 - Pilot");
    }

    #[test]
    fn episode_title_includes_name_only_when_present() {
        let show = MediaEntry {
            name: "Lost".into(),
            ..Default::default()
        };
        assert_eq!(
            show.title(Some(1), Some(&Episode::numbered(3, ""))),
            "Lost S01E03"
        );
        assert_eq!(
            show.title(Some(2), Some(&Episode::numbered(5, "Pilot"))),
            "Lost S02E05 - Pilot"
        );
        assert_eq!(show.title(Some(2), None), "Lost");
    }

    #[test]
    fn next_episode_picks_lowest_higher_number() {
        let episodes = [
            Episode::numbered(3, ""),
            Episode::numbered(1, ""),
            Episode::numbered(2, ""),
        ];
        let after =
            |number| next_episode(&episodes, &Episode::numbered(number, "")).map(|e| e.number);
        assert_eq!(after(1), Some(2));
        assert!(after(3).is_none());
    }

    #[test]
    fn episodes_sharing_a_number_are_all_visited() {
        let special = Episode {
            id: 99,
            ..Episode::numbered(4, "")
        };
        let episodes = [
            Episode::numbered(3, ""),
            special.clone(),
            Episode::numbered(4, ""),
            Episode::numbered(5, ""),
        ];
        let walk: Vec<u64> =
            std::iter::successors(episodes.first(), |e| next_episode(&episodes, e))
                .map(|e| e.id)
                .collect();
        assert_eq!(walk, [3, 4, 99, 5]);
    }

    #[test]
    fn a_fractional_episode_keeps_its_own_place_name_and_identity() {
        let half = Episode {
            id: 75,
            part: Some(50),
            ..Episode::numbered(7, "")
        };
        let episodes = [
            Episode::numbered(8, ""),
            half.clone(),
            Episode::numbered(7, ""),
        ];
        assert_eq!(next_episode(&episodes, &episodes[2]), Some(&half));
        assert_eq!(next_episode(&episodes, &half), Some(&episodes[0]));
        assert_eq!(
            (half.code(1), half.number_text()),
            ("S01E07.5".into(), "7.5".into())
        );
        let entry = MediaEntry::default();
        assert_ne!(
            entry.key_at(Some(1), Some(&half)),
            entry.key_at(Some(1), Some(&episodes[2]))
        );
        let whole = serde_json::to_value(&episodes[2]).unwrap();
        assert!(whole.get("part").is_none());
        let restored: Episode = serde_json::from_value(whole).unwrap();
        assert_eq!(restored.part, None);
    }

    #[test]
    fn a_title_saved_before_ids_were_cut_is_the_title_a_search_finds() {
        let slug = "/programmi/montalbano.json";
        let found = MediaEntry {
            id: stable_hash(slug),
            slug: slug.into(),
            ..Default::default()
        };
        let pilot = Episode::numbered(1, "");
        for id in [LONG_ID, ROUNDED_ID] {
            let saved = MediaEntry {
                id,
                ..found.clone()
            };
            assert_eq!(saved.key(), found.key());
            assert_eq!(
                saved.key_at(Some(1), Some(&pilot)),
                found.key_at(Some(1), Some(&pilot))
            );
        }
    }

    #[test]
    fn an_episode_saved_before_ids_were_cut_keeps_the_bits_of_its_id() {
        let current = Episode {
            id: stable_hash("/programmi/montalbano.json"),
            ..Episode::numbered(3, "")
        };
        for id in [LONG_ID, ROUNDED_ID] {
            let moved = Episode {
                id,
                ..Episode::numbered(9, "")
            };
            assert!(current.matches(&moved));
            assert!(!Episode::numbered(9, "").matches(&moved));
        }
    }

    #[test]
    fn an_episode_hashed_differently_is_found_by_its_place_and_title() {
        let saved = Episode {
            id: LONG_ID,
            ..Episode::numbered(3, "Finale")
        };
        assert!(Episode::numbered(3, "Finale").matches(&saved));
        assert!(!Episode::numbered(3, "Pilot").matches(&saved));
        assert!(!Episode::numbered(4, "Finale").matches(&saved));
    }

    #[test]
    fn an_episode_saved_with_a_current_id_is_found_by_that_id_alone() {
        let saved = Episode {
            id: 41,
            ..Episode::numbered(3, "")
        };
        assert!(saved.matches(&saved));
        assert!(!Episode::numbered(3, "").matches(&saved));
    }

    #[test]
    fn media_entry_json_shape_is_stable() {
        let json = serde_json::to_value(movie()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": 0, "name": "Inception", "slug": "", "provider": 0,
                "provider_name": "", "language": "", "media_type": "Movie",
                "alternative_names": [], "year": "2010", "score": null,
                "image_url": null, "backdrop_url": null, "description": null
            })
        );
    }
}
