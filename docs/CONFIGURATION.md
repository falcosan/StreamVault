# Configuration

Config is stored in JSON. On first launch, StreamVault creates a default config automatically.

| Data                                                                  | With iCloud Drive                                 | Without iCloud Drive                        |
| --------------------------------------------------------------------- | ------------------------------------------------- | ------------------------------------------- |
| **Settings** (`config.json`, `sync.json`)                             | `iCloud Drive/Contents/Video/StreamVault`         | `~/Library/Application Support/StreamVault` |
| **State** (watch progress, player prefs, sync replica, download list) | `~/Library/Application Support/StreamVault/state` | same                                        |
| **Downloads** (`root_path`)                                           | `iCloud Drive/Contents/Video/StreamVault/Medias`  | `~/Medias`                                  |

`root_path` (default `Medias`) is relative to that base, with movies and series in `movie_folder_name` and `serie_folder_name` (`Movie`, `Serie`). State is per device and never stored in iCloud Drive; enable `sync.json` to share it across devices. Downloads run two at a time (the rest wait their turn), are assembled in `~/Library/Caches/StreamVault`, and only the finished file is moved into place.

Main sections:

| Section      | Key Options                                                                                                                    |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| **output**   | `root_path`, `movie_folder_name`, `serie_folder_name`, `map_episode_name`                                                      |
| **download** | `thread_count` (default: 8), `retry_count`, `concurrent_download`, `max_speed`, track selection                                |
| **process**  | `merge_audio` (false drops every audio track), `merge_subtitle` (false drops every subtitle track), `extension` (default: mp4) |
| **requests** | `timeout` (default: 30s)                                                                                                       |

## Sync

Continue Watching and player preferences sync between the macOS app, the TV app and the browser userscript (`assets/js/streamvault.user.js`), which runs on the StreamingCommunity site and syncs its titles, through a Google Apps Script web app (`assets/js/google_apps_script.gs`) that stores one JSON file in your Drive.

1. Create an Apps Script project, paste `google_apps_script.gs`, set the script property `TOKEN` to a random secret and deploy it as a web app (execute as you, access: anyone).
2. macOS: `sync.json` → `{ "enabled": true, "endpoint": "<web app URL>", "token": "<TOKEN>" }`.
3. TV: `scripts/tv/deploy.sh` puts the endpoint and token from `sync.json` into the TV package (see [TV.md](TV.md)).
4. Userscript: set `ENDPOINT` and `TOKEN` at the top of the script.

Every value (one per title, plus the preferences) is a last-writer-wins record with a timestamp; deletions are tombstones. Devices push only their pending changes and pull what changed since their last sync, and the script resolves conflicts, so all devices converge. A document written by an earlier version is migrated on first use. If the JSON file in Drive is deleted, the script starts a new one with a new id; on its next sync each macOS app, TV and userscript sees the id change, uploads everything it holds again and downloads the whole new file, so the file is rebuilt from the devices and the newest value of each title wins. This needs the current `google_apps_script.gs` deployed (paste it and deploy a new version of the existing deployment, which keeps the web app URL): an older script sends no id, and the devices cannot rebuild a deleted file. If the file cannot be read or parsed, the script answers with an error and leaves it as it is, so delete it to start over. When you update the deployed Apps Script, update the macOS app, the TVs and the userscript too: older clients stop syncing until they are updated, without damaging the stored data.

To check the userscript, load a StreamingCommunity page with `#svdebug` at the end of the address: an alert says whether the endpoint answered, with the sync sequence and the number of titles, or shows its error. On a watch page a second alert, 10 seconds later, shows the title (and episode) the script detected, the player messages it received and the progress pushes that succeeded, or the last error.

The endpoint and token are yours alone: every Mac reads its own `sync.json`, so each person syncs only with their own Drive. A TV package carries the token of the Mac that built it, so install TVs from your own Mac and do not share the built `.wgt` or `.apk`. To install StreamVault on someone else's TV from your Mac, answer `n` when `scripts/tv/deploy.sh` asks about sync: that TV gets no endpoint or token and keeps its state locally.
