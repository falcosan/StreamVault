# StreamVault TV

TV build of StreamVault: the same crate as the desktop app, compiled to WASM with
the `tv` feature. One web bundle serves every TV: Samsung (Tizen) installs it
as a signed `.wgt`, Android TV, Google TV and Fire TV as an `.apk` that hosts it
in the system WebView. It runs entirely on the TV: the providers fetch
catalogue, search results, episodes and streams straight from the sites, and
sync talks directly to the Apps Script endpoint. Nothing needs to run on the
computer.

## Layout

Every device compiles the same files; only what a device alone has lives in its
own module.

- `src/` — app state, views, player, providers, search, Continue Watching,
  preferences, sync and the update flow, shared with the desktop app.
- `src/tv/` — the TV module: remote key handling and the spatial focus engine
  (`remote.rs`), IndexedDB storage (`storage.rs`), and the update check and
  install through `boot.js` (`update.rs`).
- `assets/` — shared stylesheet and player; `style/tv.css` holds the TV-only
  overrides, `js/hls.min.js` and `js/storage.js` are loaded only by the TV, and
  `js/boot.js` is the loader that starts the installed app or a downloaded
  update.
- `index.html` — boot page: 960×540 layout viewport (scaled to the screen by
  the TV) and the dark background shown while the WASM loads.
- `resources/tv/` — Tizen manifest (`tizen/config.xml`) and the Android host
  (`android/AndroidManifest.xml`, `android/MainActivity.java`). The
  launcher icon is `assets/img/raster/logo_raster_small.png`; the Android
  banner (`android/banner.xml`) centres it on the app's background.
- `src/deploy/` — the installer, the same crate built with the `deploy`
  feature; `tv/` is the TV deploy: it finds the TV, installs the tools it
  needs, then packages, signs, installs and launches the app, on macOS and
  Windows alike. `tv/mod.rs` is the flow (pick the TV, ask about sync,
  install), `tv/discover.rs` finds the TVs on the network, `tv/bundle.rs`
  builds or downloads the web app every TV installs, `tv/tizen.rs` and
  `tv/android.rs` hold what each TV needs, `tv/macos.rs` and `tv/windows.rs`
  what each computer does differently. `scripts/install tv`, one file that
  runs in bash on macOS and in PowerShell on Windows, downloads the installer
  built for that computer from the latest release and runs it; run from a
  clone on macOS, it builds and runs it instead.
- `scripts/bundle.sh tv` and `.github/workflows/deploy.yml` — publish each
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
flow needs them, so AnimeUnity works in the desktop app only.

## TVs

| TV                                  | Installs          | Needs on the TV, once                                                         |
| ----------------------------------- | ----------------- | ----------------------------------------------------------------------------- |
| Samsung (Tizen)                     | `.wgt` over `sdb` | Developer Mode pointed at the computer; Samsung sign-in for the certificate   |
| Android TV, Google TV               | `.apk` over `adb` | Developer options with USB, network or wireless debugging; allow the computer |
| Fire TV (Fire OS)                   | `.apk` over `adb` | Developer options with ADB debugging; allow the computer                      |
| LG (webOS)                          | no                | installed apps cannot load module scripts or make cross-site requests         |
| Hisense (VIDAA), Philips (Titan OS) | no                | no installs outside the vendor store                                          |
| Roku                                | no                | runs only BrightScript channels, no web engine                                |

The WASM uses reference types (`wasm-bindgen` keeps JS values in an `externref`
table), so the TV's web engine must be Chromium 96 or later: Samsung models
running Tizen 8 or later (2024 on, and earlier ones updated to it); on Android
the system WebView, which the script checks before installing (Android TV
updates it from the Play Store, Fire TV with system updates).

## Build and install

On a Mac (from a clone, `scripts/install tv`):

```sh
curl -fsSL https://raw.githubusercontent.com/falcosan/StreamVault/main/scripts/install | bash -s tv
```

On Windows, in PowerShell (from a clone,
`cargo run --no-default-features --features deploy -- tv`, with the GNU
toolchain):

```powershell
iex "& { $(irm https://raw.githubusercontent.com/falcosan/StreamVault/main/scripts/install) } tv"
```

Without a clone, the script downloads the installer of the latest release and
installs the TV bundle published with it, so nothing is compiled. From a
clone, the installer builds the bundle from that source; on a Mac the script
also builds the installer, installing Rust in a temporary folder when it is
missing. It:

- scans the local networks (up to 1022 addresses per interface) and the
  Android devices already connected to `adb`, lists the TVs it finds and asks
  which one when there is more than one; `TV=<address>` checks only that
  address or `adb` serial, without scanning or asking;
- when the desktop app's `sync.json` is active (see
  [CONFIGURATION.md](CONFIGURATION.md#sync)), asks whether this TV syncs with
  it: Enter builds it with that endpoint and token, `n` leaves them out, so a
  TV in someone else's home keeps its state locally; `SYNC=on` or `SYNC=off`
  answers in advance;
- installs what is missing: from a clone, on macOS the Command Line Tools and
  for the build the `wasm32-unknown-unknown` target and the `dx` of the
  `dioxus` version in `Cargo.lock`; for Android a JDK and the SDK build tools (an SDK in
  `ANDROID_HOME`, default `~/Library/Android/sdk` or
  `%LOCALAPPDATA%\Android\Sdk`, is used when it already has them); for
  Samsung Tizen Studio (about 3 GB, into `TIZEN_HOME`, default
  `~/tizen-studio` or `C:\tizen-studio`);
- waits, with instructions, until the TV is ready: Developer Mode on Samsung,
  debugging allowed on Android (it asks for the pairing code when the TV shows
  one);
- packages and signs `StreamVault.wgt` (with the Samsung certificate profile
  below) or `StreamVault.apk` (with `~/.android/debug.keystore`, created if
  missing, so updates keep the app data), both with the version of
  `Cargo.toml`, then installs and launches it; from a clone the packages stay
  in `target/tv/`.

When it ends, successfully or not, it removes everything it installed: Rust,
the Rust target, `dx` and its tools, the JDK, the Android SDK and Tizen Studio
with its data. It keeps only the keys later installs need to update the app:
`~/.android/debug.keystore` and `~/SamsungCertificate`.

Samsung TVs only install apps signed with a Samsung certificate issued to a
Samsung account for that TV's device id. The deploy uses the profile
`streamvault-<device id>`, registers it again from
`~/SamsungCertificate/streamvault-<device id>` when Tizen Studio was installed
afresh, else uses the active one; when there is none, or the TV rejects the
active one's certificate, it creates the certificate as Certificate Manager
does: it opens the Samsung sign-in in the browser, takes the answer on
`localhost:4794`, reads the device id over `sdb`, makes the keys and requests
with Tizen Studio's `keytool`, sends them to Samsung, stores the result and its
password in `~/SamsungCertificate/streamvault-<device id>`, registers the
profile, allows the TV to install with it and retries. Build output goes to
`streamvault-tv.log` in the temporary folder.

The sync settings go into the package as `sync.json`, never into the WASM, so
run it again when `sync.json` changes. `STREAMVAULT_SYNC_URL`,
`STREAMVAULT_SYNC_TOKEN`, `SYNC`, `TV`, `ANDROID_HOME`, `TIZEN_HOME` and
`JAVA_HOME` override the defaults.

## Updates

Once installed, the TV updates itself. When a newer version is published, the
update icon appears in the navbar, as on the computer; OK on it downloads the new
app, checks it and restarts on it. This works on Samsung, Android TV, Google
TV and Fire TV alike, because what changes between versions is the web bundle
(JS, WASM and hls.js), not the native package around it.

- Publishing: pushing a new `version` in `Cargo.toml` to `main` runs
  `.github/workflows/deploy.yml`. It opens a draft GitHub release `v<version>`,
  runs one job per computer or device (`tv`, `windows` and `macos`) that
  attaches its files,
  and publishes the release once every job has finished, so a TV never sees a
  half-uploaded version; another device is another job. The `windows` and
  `macos` jobs attach the desktop app and the installer that `scripts/install`
  downloads. The `tv` job builds the bundle and
  runs `scripts/bundle.sh tv`, which writes `tv.json` (version, files and their
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
  the computer replaces any downloaded update.
- Reinstalling: `SHELL` in `boot.js` is the version of the native package. A
  release that needs a newer one is not installed; the app asks to run the
  TV install again instead.

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
shows in the middle of the screen while it moves (on the computer also while
dragging the bar). In the last 20 seconds of an episode the next one starts by
itself, and an episode or movie played from its page resumes where Continue
Watching left it. The spinner only shows while the first frames load or while playback
is stalled, never while paused. The audio/subtitle choice is saved as the
player preference (the same one the desktop app uses), applied to every title and
synced.

## Notes

- Continue Watching and preferences live on the TV in IndexedDB with strict
  durability, so they survive the app being killed and reinstalled.
- Playback uses hls.js 1.7.3 (`assets/js/hls.min.js`, Apache-2.0, see
  `assets/js/hls.LICENSE`) because Tizen's native HLS stalls on these streams.
  Progressive URLs (`.mp4`, …) play natively.
- Downloads are a desktop feature; the TV app streams only.
