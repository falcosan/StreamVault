# StreamVault TV

TV build of StreamVault: the same crate as the macOS app, compiled to WASM with
the `tv` feature. One web bundle serves every TV: Samsung (Tizen) installs it
as a signed `.wgt`, Android TV, Google TV and Fire TV as an `.apk` that hosts it
in the system WebView. It runs entirely on the TV: the providers fetch
catalogue, search results, episodes and streams straight from the sites, and
sync talks directly to the Apps Script endpoint. Nothing needs to run on the
Mac.

## Layout

Every device compiles the same files; only what a device alone has lives in its
own module.

- `src/` — app state, views, player, providers, search, Continue Watching,
  preferences, sync and the update flow, shared with macOS.
- `src/tv/` — the TV module: remote key handling and the spatial focus engine
  (`remote.rs`), IndexedDB storage (`storage.rs`), and the update check and
  install through `boot.js` (`update.rs`).
- `assets/` — shared stylesheet and player; `style/tv.css` holds the TV-only
  overrides, `js/hls.min.js` and `js/storage.js` are loaded only by the TV.
- `index.html` — boot page: 960×540 layout viewport (scaled to the screen by
  the TV) and the dark background shown while the WASM loads.
- `resources/tv/` — Tizen manifest (`tizen/config.xml`), the Android host
  (`android/AndroidManifest.xml`, `android/MainActivity.java`) and `boot.js`,
  the loader that starts the installed app or a downloaded update. The
  launcher icon and the Android banner are rasterised from
  `assets/img/vector/logo.svg` at build time.
- `scripts/tv/deploy.sh` — finds the TV, installs what the build needs, then
  builds, signs, installs and launches the app.
- `scripts/tv/bundle.sh` and `.github/workflows/deploy.yml` — publish each
  version as a signed update the TV installs by itself (see
  [Updates](#updates)).

Tizen does not apply CORS to an installed widget, so the providers call the
sites directly. On Android the page is served from
`https://appassets.androidplatform.net` and `MainActivity` performs every
cross-site request natively, answering CORS itself; a small script added to
`index.html` hands request bodies to it, since the WebView does not expose
them. What a browser still forbids also applies on both: requests cannot set
`Cookie`, `Referer` or `User-Agent`, and responses do not expose `Set-Cookie`.
StreamingCommunity, RaiPlay and Aurora do not need them; AnimeUnity's login
flow needs them, so AnimeUnity works on macOS only.

## TVs

| TV                                  | Installs          | Needs on the TV, once                                                    |
| ----------------------------------- | ----------------- | ------------------------------------------------------------------------ |
| Samsung (Tizen)                     | `.wgt` over `sdb` | Developer Mode pointed at the Mac; Samsung sign-in for the certificate   |
| Android TV, Google TV               | `.apk` over `adb` | Developer options with USB, network or wireless debugging; allow the Mac |
| Fire TV (Fire OS)                   | `.apk` over `adb` | Developer options with ADB debugging; allow the Mac                      |
| LG (webOS)                          | no                | installed apps cannot load module scripts or make cross-site requests    |
| Hisense (VIDAA), Philips (Titan OS) | no                | no installs outside the vendor store                                     |
| Roku                                | no                | runs only BrightScript channels, no web engine                           |

The WASM uses reference types (`wasm-bindgen` keeps JS values in an `externref`
table), so the TV's web engine must be Chromium 96 or later: Samsung models
running Tizen 8 or later (2024 on, and earlier ones updated to it); on Android
the system WebView, which the script checks before installing (Android TV
updates it from the Play Store, Fire TV with system updates).

## Build and install

```sh
scripts/tv/deploy.sh
```

or, without a clone,

```sh
curl -fsSL "https://raw.githubusercontent.com/falcosan/StreamVault/main/scripts/tv/deploy.sh" | bash
```

The second form unpacks the latest `main` into
`~/Library/Caches/StreamVault/source` (keeping its `target/` between runs) and
runs it from there. It:

- scans the local network (up to 1022 addresses per interface) and the Android
  devices already connected to `adb`, lists the TVs it finds and asks which one
  when there is more than one; `TV=<address>` picks one without asking;
- when the macOS `sync.json` exists and is enabled (see
  [CONFIGURATION.md](CONFIGURATION.md#sync)), asks whether this TV syncs with
  it: Enter builds it with that endpoint and token, `n` leaves them out, so a
  TV in someone else's home keeps its state locally; `SYNC=on` or `SYNC=off`
  answers in advance;
- installs what is missing: the Command Line Tools; into
  `~/Library/Caches/StreamVault` Rust with the `wasm32-unknown-unknown`
  target, the `dx` of the `dioxus` version in `Cargo.lock`, a JDK; for
  Android the SDK build tools (into `ANDROID_HOME`, default
  `~/Library/Android/sdk`); for Samsung Tizen Studio (about 3 GB, into
  `TIZEN_HOME`, default `~/tizen-studio`);
- waits, with instructions, until the TV is ready: Developer Mode on Samsung,
  debugging allowed on Android (it asks for the pairing code when the TV shows
  one);
- builds and signs `target/tv/StreamVault.wgt` (with the Samsung certificate
  profile below) or `target/tv/StreamVault.apk` (with `~/.android/debug.keystore`,
  created if missing, so updates keep the app data), both with the version of
  `Cargo.toml`, then installs and launches it.

Samsung TVs only install apps signed with a Samsung certificate issued to a
Samsung account for that TV's device id. The script uses the profile
`streamvault-<device id>`, else the active one; when there is none, or the TV
rejects the active one's certificate, it creates the certificate as
Certificate Manager does: it opens the Samsung sign-in in the browser, takes
the answer on `localhost:4794`, reads the device id over `sdb`, sends the
author and distributor requests to Samsung, stores the result in
`~/SamsungCertificate/streamvault-<device id>`, registers the profile, allows
the TV to install with it and retries. Build output goes to
`target/tv/deploy.log`.

Downloaded installers stay in `~/Library/Caches/StreamVault` for the next run,
and Tizen Studio's installer unpacks its Java runtime into `~/.package-manager`.

The sync settings go into the package as `sync.json`, never into the WASM, so
run it again when `sync.json` changes. `STREAMVAULT_SYNC_URL`,
`STREAMVAULT_SYNC_TOKEN`, `SYNC`, `TV`, `ANDROID_HOME`, `TIZEN_HOME` and
`JAVA_HOME` override the defaults.

## Updates

Once installed, the TV updates itself. When a newer version is published, the
update icon appears in the navbar, as on the Mac; OK on it downloads the new
app, checks it and restarts on it. This works on Samsung, Android TV, Google
TV and Fire TV alike, because what changes between versions is the web bundle
(JS, WASM and hls.js), not the native package around it.

- Publishing: pushing a new `version` in `Cargo.toml` to `main` runs
  `.github/workflows/deploy.yml`. It opens a draft GitHub release `v<version>`,
  runs one job per device (today `tv`) that attaches its files, and publishes
  the release once every job has finished, so a TV never sees a half-uploaded
  version; another device is another job. The `tv` job builds the bundle and
  runs `scripts/tv/bundle.sh`, which writes `tv.json` (version, files and their
  SHA-256) and signs it as `tv.sig` with the ECDSA P-256 key in the repository
  secret `TV_UPDATE_KEY`, as Tauri and Sparkle sign their updates.
- Who needs what: every installed TV updates from this repository's releases,
  with nothing to configure. Only the publisher needs the private key; a fork
  that publishes its own releases sets its own key in `TV_UPDATE_KEY` and its
  public key and repository in `PUBLIC_KEY` and `RELEASES` of `boot.js`.
- Installing: the app compares its version with the latest release when it
  starts and every hour after, since a TV resumes the app instead of
  restarting it. The update is accepted only when the manifest's signature
  matches the public key in `boot.js` and every file matches its hash; it is
  kept in IndexedDB.
- Starting: `boot.js` starts the downloaded version and, when it does not
  confirm that it started, falls back to the installed one on the next launch
  and discards it, as it does for an update that no longer verifies. When its
  store cannot be read, the installed version starts. A new installation from
  the Mac replaces any downloaded update.
- Reinstalling: `SHELL` in `boot.js` is the version of the native package. A
  release that needs a newer one is not installed; the app asks to run
  `scripts/tv/deploy.sh` again instead.

## Remote

| Key                     | Browsing                                        | Player                                                                            |
| ----------------------- | ----------------------------------------------- | --------------------------------------------------------------------------------- |
| Arrows                  | move focus                                      | Left/Right on the progress bar seek 10 s; Down to the buttons, Up back to the bar |
| OK                      | open the focused item                           | play/pause on the progress bar, otherwise press the focused button                |
| Return / Back           | back; on Home back to the top first, then exits | close the track menu, otherwise stop                                              |
| Play/Pause, Play, Pause |                                                 | play/pause                                                                        |
| Rewind, Fast forward    |                                                 | seek 30 s                                                                         |
| Stop                    |                                                 | stop                                                                              |

Search: OK on the magnifier opens the TV keyboard, _Done_ searches, _Cancel_
or Return closes it.

Player: the progress bar is focused when the controls appear; below it are
Play/Pause, Audio, Subtitles and Next episode. Seeking moves the bar and the
time at once on every press; playback jumps there 0.6 s after the last press,
keeping its play or pause state, or right away with OK, which also toggles
play/pause. Holding the key, or pressing it again and again, goes further: the
step doubles for every second, up to 16 times (10 s to 160 s on the progress
bar, 30 s to 8 min with Rewind and Fast forward), and a held key moves at most
five steps a second on any remote. The distance from where the seek started
shows in the middle of the screen while it moves (on macOS also while dragging
the bar). In the last 20 seconds of an episode the next one starts by itself,
and an episode or movie played from its page resumes where Continue Watching
left it. The spinner only shows while the first frames load or while playback
is stalled, never while paused. The audio/subtitle choice is saved as the
player preference (the same one the macOS app uses), applied to every title and
synced.

## Notes

- Continue Watching and preferences live on the TV in IndexedDB with strict
  durability, so they survive the app being killed and reinstalled.
- Playback uses hls.js 1.7.3 (`assets/js/hls.min.js`, Apache-2.0, see
  `assets/js/hls.LICENSE`) because Tizen's native HLS stalls on these streams.
  Progressive URLs (`.mp4`, …) play natively.
- Downloads are a macOS feature; the TV app streams only.
