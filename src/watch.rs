use crate::providers::{Episode, MediaEntry, MediaType, next_episode};
use serde::{Deserialize, Serialize};

const END_MARGIN: f64 = 20.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchItem {
    pub entry: MediaEntry,
    pub current_time: f64,
    pub duration: f64,
    pub season: Option<u32>,
    pub episode: Option<Episode>,
}

impl WatchItem {
    pub fn progress_pct(&self) -> f64 {
        if self.duration > 0.0 {
            (self.current_time / self.duration * 100.0).min(100.0)
        } else {
            0.0
        }
    }

    pub fn started(&self) -> bool {
        self.current_time > 0.0
    }

    fn episode_number(&self) -> Option<u32> {
        self.episode.as_ref().map(|e| e.number)
    }
}

pub fn upsert(items: &mut Vec<WatchItem>, item: WatchItem) {
    remove(items, item.entry.key());
    items.insert(0, item);
}

pub fn remove(items: &mut Vec<WatchItem>, key: (usize, u64)) {
    items.retain(|i| i.entry.key() != key);
}

pub fn record(items: &mut Vec<WatchItem>, item: WatchItem, episodes: &[Episode]) -> bool {
    if item.current_time < 1.0 || item.duration <= 0.0 {
        return false;
    }
    if item.duration - item.current_time > END_MARGIN {
        upsert(items, item);
        return true;
    }
    let same_episode = items
        .iter()
        .find(|i| i.entry.key() == item.entry.key())
        .is_some_and(|i| i.episode_number() == item.episode_number());
    same_episode && finish(items, &item, episodes)
}

pub fn finish(items: &mut Vec<WatchItem>, item: &WatchItem, episodes: &[Episode]) -> bool {
    let (MediaType::Series, Some(season), Some(current)) =
        (item.entry.media_type, item.season, &item.episode)
    else {
        remove(items, item.entry.key());
        return true;
    };
    let Some(next) = next_episode(episodes, current.number) else {
        return false;
    };
    upsert(items, start_of(item.entry.clone(), season, next.clone()));
    true
}

pub fn start_of(entry: MediaEntry, season: u32, episode: Episode) -> WatchItem {
    WatchItem {
        entry,
        current_time: 0.0,
        duration: 0.0,
        season: Some(season),
        episode: Some(episode),
    }
}

pub fn next_season(seasons: &[u32], after: u32) -> Option<u32> {
    seasons.iter().copied().filter(|s| *s > after).min()
}

pub fn first_episode(episodes: &[Episode]) -> Option<&Episode> {
    episodes.iter().min_by_key(|e| e.number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, media_type: MediaType) -> MediaEntry {
        MediaEntry {
            id,
            media_type,
            ..Default::default()
        }
    }

    fn movie_at(id: u64, time: f64) -> WatchItem {
        WatchItem {
            entry: entry(id, MediaType::Movie),
            current_time: time,
            duration: 100.0,
            season: None,
            episode: None,
        }
    }

    fn series_at(number: u32, time: f64) -> WatchItem {
        WatchItem {
            entry: entry(1, MediaType::Series),
            current_time: time,
            duration: 100.0,
            season: Some(1),
            episode: Some(Episode::numbered(number, "")),
        }
    }

    fn ids(items: &[WatchItem]) -> Vec<u64> {
        items.iter().map(|i| i.entry.id).collect()
    }

    #[test]
    fn progress_pct_is_clamped_and_safe() {
        assert!((movie_at(1, 50.0).progress_pct() - 50.0).abs() < 1e-9);
        assert_eq!(movie_at(1, 200.0).progress_pct(), 100.0);
        let unknown = WatchItem {
            duration: 0.0,
            ..movie_at(1, 5.0)
        };
        assert_eq!(unknown.progress_pct(), 0.0);
    }

    #[test]
    fn upsert_moves_item_to_front() {
        let mut items = vec![movie_at(1, 1.0), movie_at(2, 2.0), movie_at(3, 3.0)];
        upsert(&mut items, movie_at(3, 55.0));
        assert_eq!(ids(&items), [3, 1, 2]);
        assert_eq!(items[0].current_time, 55.0);
    }

    #[test]
    fn items_are_keyed_by_provider_and_id() {
        let mut items = vec![movie_at(1, 1.0)];
        let mut other = movie_at(1, 2.0);
        other.entry.provider = 1;
        upsert(&mut items, other);
        assert_eq!(items.len(), 2);
        remove(&mut items, (0, 1));
        assert_eq!(items[0].entry.provider, 1);
    }

    #[test]
    fn record_saves_progress_before_the_end() {
        let mut items = Vec::new();
        assert!(record(&mut items, movie_at(1, 30.0), &[]));
        assert_eq!(items[0].current_time, 30.0);
    }

    #[test]
    fn record_ignores_startup_and_unknown_duration() {
        let mut items = Vec::new();
        assert!(!record(&mut items, movie_at(1, 0.5), &[]));
        let unknown = WatchItem {
            duration: 0.0,
            ..movie_at(1, 30.0)
        };
        assert!(!record(&mut items, unknown, &[]));
        assert_eq!(items, []);
    }

    #[test]
    fn record_near_end_removes_finished_movie() {
        let mut items = vec![movie_at(1, 30.0)];
        assert!(record(&mut items, movie_at(1, 85.0), &[]));
        assert_eq!(items, []);
    }

    #[test]
    fn record_near_end_advances_series_once() {
        let episodes = [Episode::numbered(1, ""), Episode::numbered(2, "")];
        let mut items = vec![series_at(1, 30.0)];
        assert!(record(&mut items, series_at(1, 90.0), &episodes));
        assert_eq!(items[0].episode.as_ref().unwrap().number, 2);
        assert_eq!(items[0].current_time, 0.0);
        assert!(!record(&mut items, series_at(1, 95.0), &episodes));
        assert_eq!(items[0].episode.as_ref().unwrap().number, 2);
    }

    #[test]
    fn record_near_end_without_saved_item_is_ignored() {
        let mut items = Vec::new();
        assert!(!record(&mut items, movie_at(1, 90.0), &[]));
        assert_eq!(items, []);
    }

    #[test]
    fn finish_reports_when_next_season_is_needed() {
        let mut items = vec![series_at(2, 99.0)];
        assert!(!finish(
            &mut items,
            &series_at(2, 99.0),
            &[Episode::numbered(1, ""), Episode::numbered(2, "")]
        ));
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn finish_advances_once_the_season_episodes_are_known() {
        let mut items = vec![series_at(1, 99.0)];
        assert!(!finish(&mut items, &series_at(1, 99.0), &[]));
        assert!(finish(
            &mut items,
            &series_at(1, 99.0),
            &[Episode::numbered(1, ""), Episode::numbered(2, "")]
        ));
        assert_eq!(items[0].episode.as_ref().unwrap().number, 2);
    }

    #[test]
    fn next_season_is_the_lowest_later_one() {
        assert_eq!(next_season(&[3, 1, 2], 1), Some(2));
        assert_eq!(next_season(&[1, 2], 2), None);
    }

    #[test]
    fn first_episode_is_the_lowest_number() {
        let episodes = [
            Episode::numbered(3, ""),
            Episode::numbered(0, ""),
            Episode::numbered(1, ""),
        ];
        assert_eq!(first_episode(&episodes).map(|e| e.number), Some(0));
        assert!(first_episode(&[]).is_none());
    }

    #[test]
    fn start_of_resets_progress() {
        let item = start_of(entry(1, MediaType::Series), 2, Episode::numbered(1, ""));
        assert_eq!(
            (item.current_time, item.duration, item.season),
            (0.0, 0.0, Some(2))
        );
    }

    #[test]
    fn watch_item_round_trips_through_json() {
        let item = series_at(5, 123.5);
        let json = serde_json::to_string(&item).unwrap();
        assert_eq!(serde_json::from_str::<WatchItem>(&json).unwrap(), item);
    }
}
