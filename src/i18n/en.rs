#[cfg(feature = "desktop")]
use super::Desktop;
use super::Text;

pub static TEXT: Text = Text {
    english: "English",
    italian: "Italian",
    language: "Language",
    forced: "Forced",
    decimal: ".",
    back: "Back",
    home: "Home",
    loading: "Loading",
    search: "Search",
    search_placeholder: "Search...",
    submit_search: "Submit search",
    updating: "Updating",
    install_update: "Install update",
    update_failed: "Update failed",
    dismiss_error: "Dismiss error",
    continue_watching: "Continue Watching",
    remove_from_continue_watching: |name| format!("Remove {name} from Continue Watching"),
    no_media: "No media available",
    retry: "Retry",
    loading_more: "Loading more...",
    searching: "Searching...",
    no_results: "No results found",
    results: "Results",
    result_count: |n| match n {
        1 => "1 result".into(),
        n => format!("{n} results"),
    },
    movie: "Movie",
    series: "Series",
    season_count: |n| match n {
        1 => "1 Season".into(),
        n => format!("{n} Seasons"),
    },
    seasons: "Seasons",
    season: |n| format!("Season {n}"),
    episodes: "Episodes",
    episode: |n| format!("Episode {n}"),
    no_episodes: "No episodes available",
    play: "Play",
    resume: "Resume",
    play_episode: |n, title| format!("Play episode {n}: {title}"),
    time_left: |time| format!("{time} left"),
    episodes_failed: "Failed to load episodes",
    next_season_failed: "Failed to load next season",
    stream_failed: "Failed to get stream",
    playback_failed: "Playback failed",
    pause: "Pause",
    seek: "Seek",
    time_of: |time, total| format!("{time} of {total}"),
    audio: "Audio",
    subtitles: "Subtitles",
    audio_track: |name| format!("Audio: {name}"),
    subtitles_track: |name| format!("Subtitles: {name}"),
    default_track: "Default",
    off: "Off",
    next_episode: "Next episode",
    fullscreen: "Fullscreen",
    exit_fullscreen: "Exit fullscreen",
    #[cfg(feature = "desktop")]
    desktop: Desktop {
        downloads: "Downloads",
        download: "Download",
        queued: "Queued",
        progress: |percent| format!("Downloading {percent}%"),
        finishing: "Finishing",
        downloaded: "Downloaded",
        retry: "Retry download",
        download_this: |episode| match episode {
            Some(n) => format!("Download episode {n}"),
            None => "Download this movie".into(),
        },
        play_this: |episode| match episode {
            Some(n) => format!("Play episode {n}"),
            None => "Play this movie".into(),
        },
        retry_this: |episode| match episode {
            Some(n) => format!("Retry the download of episode {n}"),
            None => "Retry the download of this movie".into(),
        },
        show_this: |episode| match episode {
            Some(n) => format!("Show the download of episode {n}"),
            None => "Show the download of this movie".into(),
        },
        empty: "No downloads yet",
        empty_hint: "Download a movie or an episode from its page and watch it here, even offline.",
        browse: "Browse titles",
        active: "Downloading",
        ready_to_watch: "Ready to watch",
        waiting: "Waiting to start",
        progress_of: |title| format!("{title} download progress"),
        finishing_of: |title| format!("{title} finishing"),
        failed: |error| format!("Download failed: {error}"),
        reveal: if cfg!(windows) {
            "Show in File Explorer"
        } else {
            "Show in Finder"
        },
        reveal_title: |title| {
            if cfg!(windows) {
                format!("Show {title} in File Explorer")
            } else {
                format!("Show {title} in Finder")
            }
        },
        reveal_failed: if cfg!(windows) {
            "Cannot show in File Explorer"
        } else {
            "Cannot show in Finder"
        },
        play_title: |title| format!("Play {title}"),
        delete: "Delete",
        delete_title: |title| format!("Delete {title}"),
        retry_title: |title| format!("Retry {title}"),
        remove: "Remove",
        remove_title: |title| format!("Remove {title}"),
        cancel: "Cancel",
        cancel_title: |title| format!("Cancel {title}"),
        ready: "Ready",
        running: |n| format!("{n} downloading"),
        failed_count: |n| format!("{n} failed"),
        titles_ready: |n| match n {
            1 => "1 title ready".into(),
            n => format!("{n} titles ready"),
        },
        delete_failed: "Cannot delete download",
        file_missing: "File not found",
        restart: "Update installed, restart StreamVault",
    },
};
