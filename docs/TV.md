# StreamVault TV

Samsung Tizen TV build of StreamVault: the same crate as the macOS app,
compiled to WASM with the `tv` feature and packaged as a signed `.wgt`. It runs
entirely on the TV: the providers fetch catalogue, search results, episodes and
streams straight from the sites, and sync talks directly to the Apps Script
endpoint. Nothing needs to run on the Mac.

## Layout

Every device compiles the same files; only what a device alone has lives in its
own module.

- `src/` — app state, views, player, providers, search, Continue Watching,
  preferences and sync, shared with macOS.
- `src/tv/` — the TV module: remote key handling and the spatial focus engine
  (`remote.rs`), IndexedDB storage (`storage.rs`).
- `assets/` — shared stylesheet and player; `style/tv.css` holds the TV-only
  overrides, `js/hls.min.js` and `js/storage.js` are loaded only by the TV.
- `index.html` — boot page: 960×540 layout viewport (scaled ×2 by the TV) and
  the dark background shown while the WASM loads.
- `resources/tv/` — Tizen manifest (`config.xml`) and launcher icon.
- `scripts/deploy-tv.sh` — builds, signs, installs and launches the app.

The TV webview does not apply CORS to an installed widget, so the providers
can call the sites directly. What a browser still forbids also applies here:
requests cannot set `Cookie`, `Referer` or `User-Agent`, and responses do not
expose `Set-Cookie`. StreamingCommunity, RaiPlay and Aurora work without them;
AnimeUnity's login flow needs them (and its search currently fails on macOS as
well).

## Build and install

Requires `dx` 0.7, the `wasm32-unknown-unknown` target, the Tizen Studio CLI
with the `streamvault-samsung` signing profile and the TV in Developer Mode.

```sh
scripts/deploy-tv.sh
```

With no arguments it:

- enables sync with the endpoint and token of the macOS `sync.json` when that
  file exists and is enabled (see the root README); otherwise the TV keeps its
  state locally;
- uses the TV already connected to `sdb`, or finds it on the LAN by its MAC
  address and connects;
- builds and signs `target/tv/StreamVault.wgt`, then installs and launches it.

The sync settings are compiled into the WASM, so run it again when `sync.json`
changes. `STREAMVAULT_SYNC_URL`, `STREAMVAULT_SYNC_TOKEN`, `TV_IP`, `TV_MAC`,
`TIZEN`, `SDB` and `PROFILE` override the defaults.

## Remote

| Key | Browsing | Player |
|---|---|---|
| Arrows | move focus | Left/Right on the progress bar seek 10 s; Down to the buttons, Up back to the bar |
| OK | open the focused item | play/pause on the progress bar, otherwise press the focused button |
| Return | back; exits on Home | close the track menu, otherwise stop |
| Play/Pause, Play, Pause | | play/pause |
| Rewind, Fast forward | | seek 30 s |
| Stop | | stop |

Search: OK on the magnifier opens the TV keyboard, *Done* searches, *Cancel*
or Return closes it.

Player: the progress bar is focused when the controls appear; below it are
Play/Pause, Audio, Subtitles and Next episode. Seeking moves the bar and the
time at once on every press; playback jumps there 0.6 s after the last press,
keeping its play or pause state, or right away with OK, which also toggles
play/pause. The spinner only shows while the first frames
load or while playback is stalled, never while paused. The audio/subtitle choice is
saved as the player preference (the same one the macOS app uses), applied to
every title and synced.

## Notes

- Continue Watching and preferences live on the TV in IndexedDB with strict
  durability, so they survive the app being killed and reinstalled.
- Playback uses hls.js 1.7.3 (`assets/js/hls.min.js`, Apache-2.0, see
  `assets/js/hls.LICENSE`) because Tizen's native HLS stalls on these streams.
  Progressive URLs (`.mp4`, …) play natively.
- Downloads are a macOS feature; the TV app streams only.
