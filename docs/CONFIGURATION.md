# Configuration

Config is stored in JSON. On first launch, StreamVault creates a default config automatically.

| Data                                                   | With iCloud Drive                                 | Without iCloud Drive                        |
| ------------------------------------------------------ | ------------------------------------------------- | ------------------------------------------- |
| **Settings** (`config.json`, `sync.json`)              | `iCloud Drive/Contents/Video/StreamVault`         | `~/Library/Application Support/StreamVault` |
| **State** (watch progress, player prefs, sync replica) | `~/Library/Application Support/StreamVault/state` | same                                        |
| **Downloads** (`root_path`)                            | `iCloud Drive/Contents/Video/StreamVault`         | `~`                                         |

State is per device and never stored in iCloud Drive; enable `sync.json` to share it across devices. Downloads are assembled in `~/Library/Caches/StreamVault` and only the finished file is moved into place.

Main sections:

| Section      | Key Options                                                                                     |
| ------------ | ----------------------------------------------------------------------------------------------- |
| **output**   | `root_path`, `movie_folder_name`, `serie_folder_name`, `map_episode_name`                       |
| **download** | `thread_count` (default: 8), `retry_count`, `concurrent_download`, `max_speed`, track selection |
| **process**  | `merge_audio`, `merge_subtitle`, `extension` (default: mp4)                                     |
| **requests** | `timeout` (default: 30s)                                                                        |

## Sync

Continue Watching and player preferences sync between the macOS app, the TV app and the browser userscript (`assets/js/streamvault.user.js`) through a Google Apps Script web app (`assets/js/google_apps_script.gs`) that stores one JSON file in your Drive.

1. Create an Apps Script project, paste `google_apps_script.gs`, set the script property `TOKEN` to a random secret and deploy it as a web app (execute as you, access: anyone).
2. macOS: `sync.json` → `{ "enabled": true, "endpoint": "<web app URL>", "token": "<TOKEN>" }`.
3. TV: `scripts/deploy-tv.sh` builds with the endpoint and token from `sync.json` (see [TV.md](TV.md)).
4. Userscript: set `ENDPOINT` and `TOKEN` at the top of the script.

Every value (one per title, plus the preferences) is a last-writer-wins record with a timestamp; deletions are tombstones. Devices push only their pending changes and pull what changed since their last sync, and the script resolves conflicts, so all devices converge. A document written by an earlier version is migrated on first use.

The endpoint and token are yours alone: every Mac reads its own `sync.json`, so each person syncs only with their own Drive. A TV package carries the token of the Mac that built it, so install TVs from your own Mac and do not share the built `.wgt` or `.apk`.
