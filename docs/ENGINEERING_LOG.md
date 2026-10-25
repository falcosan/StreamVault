# StreamVault for Samsung Tizen and Android TV — Engineering Log

A complete, reproducible account of porting StreamVault (a Rust/Dioxus desktop
application) to a Samsung Crystal UHD **UE50CU8072UXXH** running **Tizen 9.0**
(`armv7l`, build `T-X22UDCDEUC-0090-REL-202603190249`). Written as both a
narrative of what was tried and a reference for reproducing the result.

---

## 1. Objective

Ship a TV-native build of StreamVault that:

1. Appears as its own tile — name **StreamVault**, custom logo — in the Smart Hub
   app grid.
2. Opens into a UI that is visually and behaviourally identical to the desktop
   app (same component tree, same CSS, same providers).
3. Is driven entirely by the TV remote (D-pad + OK + Return), not a pointer.
4. Serves real catalogue/search data from the project's existing provider stack.

The desktop app is a Rust crate built on **Dioxus 0.7** (`dioxus-desktop`,
`wry`/`tao`). The key realisation that made this tractable: Dioxus compiles the
_same_ RSX component tree to a **web (WASM)** target. A Tizen "web app" is just
an HTML/JS/WASM bundle wrapped in a signed `.wgt` package. So the desktop UI can
be re-rendered inside the TV's webview with no rewrite — only a change of
platform feature (`dioxus-desktop` → `dioxus-web`) and a networked data source.

---

## 2. Architecture

```
StreamVault/            one crate, one device feature per build
├── src/
│   ├── main.rs         picks the device module (`desktop` or `tv`) as `platform`
│   ├── app.rs          AppState + App root, shared by every device
│   ├── player.rs       player controller (state + channel to player.js)
│   ├── gui/            home, search, details, player, navbar, poster
│   ├── providers/      StreamingCommunity, RaiPlay, Aurora, AnimeUnity, models
│   ├── search.rs  watch.rs  prefs.rs  time.rs
│   ├── sync.rs         replica + sync loop against the Apps Script endpoint
│   ├── desktop/        macOS only: window, files, downloads, local media,
│   │                   self-update, Downloads view
│   └── tv/             TVs only: remote focus engine, IndexedDB storage,
│                       remote key handling
├── assets/             style.css and player.js for every device; tv.css
│                       (10-foot overlay), storage.js and hls.js for the TV
├── resources/          packaging: macos/ (Info.plist, icon), tv/
│                       (tizen/config.xml, android/ manifest + WebView host)
├── scripts/            package.sh (macOS app), deploy-tv.sh (find the TV,
│                       install the tools, build, sign, install, launch)
└── index.html          boot page of the web build (viewport 960, dark background)
```

Each device module exposes the same small surface that the shared code calls:
`launch`, `Device` (request timeout, local playback lookup), `use_device`
(device hooks), `load_replica`/`save_replica`, `sync_config`, focus
bookkeeping, `NAV` and three slots (`Head`, `NavTools`, `EntryActions`). The
first port copied `src/providers/` and the models into `tv/` and then into a
separate `core` crate; both are gone, so every device compiles the same files.

Runtime topology:

```
┌──────────────── Samsung TV (Tizen 9 webview) ────────────────┐
│  StreamVault.wgt                                             │
│    index.html → WASM (Dioxus + the shared providers)         │
│         │ fetch (reqwest), no CORS for an installed widget   │
└─────────┼────────────────────────────────────────────────────┘
          ▼ Internet
   StreamingCommunity · RaiPlay · Aurora · AnimeUnity · vixcloud/CDNs
   Apps Script sync endpoint (Continue Watching + preferences)
```

Everything runs on the TV; the Mac is only used to build and install.

---

## 3. Toolchain setup

### 3.1 Tizen Studio 6.1 (headless install)

The macOS installer ships as `installer.app`, a Java/SWT GUI wrapper around
`installer.jar`. The jar also contains a **CLI entry point**
(`org.tizen.installer.cli.CLIMain`) that installs unattended — no display
required:

```bash
# from the mounted web-ide_Tizen_Studio_6.1_macos-64.dmg
BIN=/Volumes/Tizen_Studio_6.1/installer.app/Contents/Resources/Java
"$BIN/unzip" -qq -a "$BIN/tizen-sdk.zip" "jdk/*" -d "$HOME/.package-manager"   # bootstrap JDK
JAVA="$HOME/.package-manager/jdk/Contents/Home/bin/java"
"$JAVA" -cp "$BIN/installer.jar" org.tizen.installer.cli.CLIMain \
        --accept-license --no-java-check "$HOME/tizen-studio"
```

Result: `~/tizen-studio` (≈925 MB) with `tizen` (CLI 2.5.25), `sdb`, the
Certificate Manager and Device Manager apps.

### 3.2 TV extensions + certificate tooling

```bash
~/tizen-studio/package-manager/package-manager-cli.bin \
    install --accept-license \
    cert-add-on TV-SAMSUNG-Public TV-SAMSUNG-Extension-Tools
```

> **Gotcha.** Without `--accept-license` the `cert-add-on` dependency aborts
> with _"required license agreement due to dependencies"_ and installs nothing.

This adds the **TV-SAMSUNG** platform (`platforms/tizen-10.0/tv-samsung`) and the
Samsung certificate generator (`KeyCertGeneratorApi.jar`).

### 3.3 dioxus-cli (`dx`) — the OpenSSL build wall

`cargo install dioxus-cli --locked` failed repeatedly. The real error was buried
far above the final line (it only surfaced once the full build output was
captured instead of `… | tail -5`):

```
error: failed to run custom build command for `openssl-sys v0.9.114`
  Could not find openssl via pkg-config:
  The pkg-config command could not be found.
  …
  OpenSSL library directory does not exist: ["/opt/homebrew/opt/openssl@3/lib"]
```

Two distinct causes, fixed in order:

1. **No `pkg-config`** on the machine → `brew install pkgconf`.
2. **`openssl@3` present in the Cellar but its `opt` symlink/`lib` dir not
   linked** → `brew install openssl@3 && brew link openssl@3 --force`.

Then, with the environment pointed at the linked OpenSSL:

```bash
export OPENSSL_DIR=/opt/homebrew/opt/openssl@3
export PKG_CONFIG_PATH=/opt/homebrew/opt/openssl@3/lib/pkgconfig
cargo install dioxus-cli --locked      # → dx 0.7.10
```

> **Lesson.** When a long `cargo install` fails, never diagnose from the final
> line — it only says "failed to compile X". Capture the whole log; the build
> script's stderr is where the actionable message lives.

### 3.4 Rust targets

```bash
rustup target add wasm32-unknown-unknown            # TV UI
```

---

## 4. Certificates

A Samsung TV accepts only packages signed with a developer **author** +
**distributor** certificate pair, where the distributor certificate is locked to
that specific TV's device id. Created in **Certificate Manager** as profile
`streamvault-samsung`:

| Certificate | Issued by                    | Notes                           |
| ----------- | ---------------------------- | ------------------------------- |
| author      | Samsung VD Author CA         | identity; expires 2027/10/23    |
| distributor | VD DEVELOPER Public CA Class | DUID-locked; expires 2027/10/23 |

Inspecting the distributor certificate (modern OpenSSL needs `-legacy` for the
RC2-40 PKCS#12 encryption Samsung still uses) shows the binding:

```bash
openssl pkcs12 -legacy -in distributor.p12 -nokeys -passin pass:*** \
  | openssl x509 -noout -text | grep -A1 'Alternative Name'
#   X509v3 Subject Alternative Name:
#       URI:URN:tizen:packageid=, URI:URN:tizen:deviceid=7XCI7KNS2VUXI
```

Key points that matter later:

- `deviceid=7XCI7KNS2VUXI` — the raw device id this TV reports; the certificate
  is useless on any other set.
- `packageid=` is **empty** for a _Public_ distributor certificate. Empty means
  "any package id is acceptable", which is why a 10-character constraint (not a
  specific value) is what ultimately governs install — see §8.

The CLI picks the profile up automatically from
`~/tizen-studio-data/profile/profiles.xml`:

```bash
tizen security-profiles list
#   streamvault-samsung   O     (active)
```

---

## 5. Building the UI (Dioxus desktop → web)

The TV build is the same crate compiled with the `tv` feature, which turns on
`dioxus/web` and the Tizen module. `dx` selects it from `--platform web`
(it always passes `--no-default-features` plus the feature that enables the
platform), while `cargo build` keeps the default `desktop` feature:

```toml
[features]
default = ["desktop"]
desktop = ["dioxus/desktop", "dep:dioxus-desktop", "dep:dirs", "tokio/process", …]
tv = ["dioxus/web", "dep:wasm-bindgen", "dep:wasm-bindgen-futures", "dep:web-sys", …]
```

Both devices load `assets/style/style.css`; the TV adds `assets/style/tv.css`
(no transitions, no pointer-only buttons, neutral buttons until focused). The TV renders the desktop layout at
960×540 CSS pixels (`<meta name="viewport" content="width=960">`), which the
webview scales ×2 to 1920×1080, so one set of sizes serves both screens.

Build:

```bash
dx build --release --platform web --debug-symbols false
# → target/dx/streamvault/release/web/public/
#     index.html + assets/<hash>.wasm (≈3 MB) + <hash>.js (≈55 KB)
#     + hls.min-<hash>.js (≈620 KB, vendored hls.js, §11)
```

`dx` 0.7.10 keeps DWARF debug sections in release WASM unless
`--debug-symbols false` is passed (4.7 MB instead of 3 MB).

The sync endpoint and token are **compile-time** constants
(`STREAMVAULT_SYNC_URL`, `STREAMVAULT_SYNC_TOKEN`); `deploy-tv.sh` reads them from
the macOS `sync.json`.

> **Gotcha.** `dx` emits web-absolute asset paths (`/./assets/…`). The TV loads
> the widget as `file:///index.html`, so root paths already resolve inside the
> package and need no rewrite. The `<title>` comes from `Dioxus.toml`;
> `index.html` sets the viewport and the dark background shown before the WASM
> starts.
>
> **Gotcha.** `dx` does **not** clean old content-hashed bundles between builds;
> `public/assets/` accumulates stale `.wasm`/`.js`. `deploy-tv.sh` deletes the
> output directory before every build and stages from scratch.

---

## 6. Packaging the web app

`config.xml` is the Tizen application manifest. The final, working manifest
(distilled by comparing against a known-good TV web app — see §8):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<widget xmlns:tizen="http://tizen.org/ns/widgets"
        xmlns="http://www.w3.org/ns/widgets"
        id="https://streamvault.local/sv" viewmodes="maximized">
  <access origin="*" subdomains="true"></access>
  <tizen:app-control>
    <tizen:src name="index.html" reload="disable"/>
    <tizen:operation name="http://samsung.com/appcontrol/operation/eden_resume"/>
  </tizen:app-control>
  <tizen:application id="AR6J1YmstG.StreamVault" package="AR6J1YmstG" required_version="8.0"/>
  <author href="https://streamvault.local">StreamVault</author>
  <content src="index.html"/>
  <feature name="http://tizen.org/feature/screen.size.all"/>
  <icon src="icon.png"/>
  <tizen:metadata key="http://samsung.com/tv/metadata/html5.4k.support" value="true"/>
  <tizen:metadata key="http://samsung.com/tv/metadata/use.uwe" value="true"/>
  <name>StreamVault</name>
  <tizen:privilege name="http://tizen.org/privilege/internet"/>
  <tizen:privilege name="http://tizen.org/privilege/tv.inputdevice"/>
  <tizen:profile name="tv-samsung"/>
  <tizen:setting screen-orientation="landscape" context-menu="disable"
    background-support="disable" encryption="disable"
    install-location="auto" hwkey-event="enable"/>
</widget>
```

Changes from the first working manifest: `prelaunch.support` is gone (it kept a
resident instance serving the previous build, §10.6), `background-support` is
disabled so playback pauses when the TV goes Home, the context menu is off, and
the unused `unlimitedstorage` / `application.launch` privileges are dropped,
as are `web.service` (there is no `<tizen:service>`) and `network.public`
(nothing calls `webapis`). `required_version` is 8.0: the WASM needs
Chromium 96 (§13.4), which Samsung ships from Tizen 8.

The build adds the `version` attribute from `Cargo.toml`, the only place the
version is written, as the macOS and Android packages do, and rasterises the
icon from the vector logo with the native tool, so the app logo stays its only
source:

```bash
sips -s format png --resampleHeightWidth 512 512 assets/img/vector/logo.svg \
     --out <stage>/icon.png
```

Package + sign:

```bash
# stages dx output + resources/tv/tizen/config.xml + the icon, then signs
scripts/deploy-tv.sh   # → target/tv/StreamVault.wgt, then installs it
```

This produces `author-signature.xml` + `signature1.xml` inside the `.wgt` and
leaves a ~1.4 MB signed package.

---

## 7. Problem #1 — `sdb` cannot connect

### 7.1 Symptom

```
$ sdb connect 192.168.68.X:26101
error: failed to connect to remote target '192.168.68.X'
```

…even though the TV was powered on, on the network, and Developer Mode was
enabled. This persisted across multiple reboots and consumed the most time of
any single issue.

### 7.2 What the evidence said

The TV's public control API (port **8001**, no auth) is a reliable source of
truth for its developer state:

```bash
curl -s http://<tv>:8001/api/v2/ | python3 -m json.tool
#   "developerMode": "1",
#   "developerIP":   "<the dev host IP>",
#   "PowerState":    "on"
```

So Developer Mode was on and pointed at the right host. A raw TCP probe showed
port **26101 accepted the connection but never answered the protocol
handshake** (`host:version` got no reply) — the listener was up but the session
layer was dead. The capability banner, once a connection _did_ succeed, confirms
this is a secure-protocol daemon:

```
secure_protocol:enabled
intershell_support:disabled
filesync_support:pushpull
cpu_arch:armv7   platform_version:9.0
```

### 7.3 Root cause — competing sdb servers on the host

The breakthrough came from `lsof`:

```bash
$ lsof -i :26099
java  …  localhost:56469->localhost:26099 (ESTABLISHED)   # Device Manager's sdb
sdb   …  *:26099 (LISTEN)                                  # another sdb server
sdb   …  localhost:26099->… (ESTABLISHED)
```

Three different sdb servers were live at once: the one embedded in **Device
Manager**, the one from **Tizen Studio**, and ad-hoc ones started by hand. They
fought over the single client channel; the winner left the TV-side session in a
state where new clients connect at TCP level but get no protocol response. This
matches the Apps2Samsung project's own fix (`ClearCompetingSdbServer`, PR #739):
Tizen tooling silently restarts its sdb server, so a one-time `kill-server` isn't
enough.

### 7.4 Fix

Reduce the host to **exactly one** sdb client, then connect once:

```bash
pkill -9 -f sdb
pkill -9 -f device-manager          # Device Manager keeps its own sdb alive
pkill -9 -f Certificate-manager
# one client only:
~/…/bin/sdb start-server
~/…/bin/sdb connect <tv>:26101
#   connected to <tv>:26101
~/…/bin/sdb devices
#   <tv>:26101   device   UE50CU8072UXXH
```

Connection succeeded instantly. The problem was never the TV.

### 7.5 Secondary facts worth recording

- **sdb client key** lives at `~/.tizen/sdbkey(.pub)`, not `~/.sdb/`. Toggling
  Developer Mode on the TV resets the key-authorisation whitelist, so the host's
  key has to be re-accepted after a toggle.
- **The daemon's enablement flag is read at boot.** Changing Developer Mode and
  then _re-opening the dialog_ can re-arm the broken state; the reliable recovery
  is: set it once, cold-boot, and connect without touching the dialog again
  (corroborated by Apps2Samsung issue #751).
- **DHCP lease rotation.** Across reboots the TV's IP moved (…71 → …69 → …66).
  Track it by MAC instead of guessing:
  ```bash
  arp -a | grep -i 'e0:3:6b:70:4e:bd'    # the set's wired NIC
  ```

---

## 8. Problem #2 — `tizen install` → _Load archive info fail_

### 8.1 Symptom

```
app_id[STREAMVLTsv.StreamVault] install start
app_id[STREAMVLTsv.StreamVault] install failed[118, -4],
       reason: Operation not allowed : :Load archive info fail
```

Identical failure whether driven through `tizen install` or the lower-level
package tool. Changing `config.xml` (profile name, `required_version`, adding
`<author>`, privileges) made **no** difference. "Load archive info" fails _before_
content is laid down — it is a manifest/packaging gate, not a runtime problem.

### 8.2 The decisive isolation test

Rather than keep mutating our package blind, borrow a **known-good** reference.
TizenBrew is an open-source (GPL-3.0) Tizen TV web app that is widely installed
on this exact firmware. Download its `.wgt`, **re-sign it with our own
certificate**, and install it:

```bash
# fetch the published .wgt, strip its signatures, re-sign with our profile
tizen package -t wgt -s streamvault-samsung      # over the reference content
tizen install -t UE50CU8072UXXH -n TizenBrewNextGeneration.wgt
#   install completed
#   Installed the package: Id(xvvl3S1bvH.TizenBrewStandalone)
```

**It installed.** That single result eliminated three whole hypotheses at once:
our certificate is valid, the firmware is not blocking third-party packages, and
the signing pipeline is correct. The defect was purely in **our** package.

### 8.3 Root cause — package id length

Diffing the two manifests, the operative difference was the identifier:

|                    | package id    | length | installs? |
| ------------------ | ------------- | ------ | --------- |
| reference          | `xvvl3S1bvH`  | **10** | yes       |
| StreamVault (orig) | `STREAMVLTsv` | 11     | no        |

Tizen requires the **package id to be exactly 10 alphanumeric characters**. An
11-character id is rejected at archive-info time with the misleading
"Operation not allowed" message. Regenerating with a conforming id fixed it:

```bash
python3 -c "import secrets,string; \
  print(''.join(secrets.choice(string.ascii_letters+string.digits) for _ in range(10)))"
#   AR6J1YmstG
```

```
app_id[AR6J1YmstG.StreamVault] install completed
Installed the package: Id(AR6J1YmstG.StreamVault)
Tizen application is successfully installed.
```

### 8.4 Manifest conventions adopted from the reference

Beyond the id length, the reference manifest also demonstrated the TV-specific
pieces that a plain Tizen web manifest omits, and these were folded into the
final `config.xml` (§6):

- `<tizen:app-control>` with the Samsung `eden_resume` operation (correct
  resume/launch semantics on the TV home).
- `<tizen:profile name="tv-samsung"/>` (not the generic `tv`).
- `<feature screen.size.all>`.
- `html5.4k.support` and `use.uwe` metadata (`prelaunch.support` was tried and dropped, §10.6).

### 8.5 Takeaways

- A failing custom package + a passing re-signed reference package is the fastest
  possible triangulation: it partitions "my artifact" from "my credentials /
  the device" in one step.
- "Operation not allowed / Load archive info fail" is Tizen's generic archive
  rejection. Treat it as "the manifest or packaging is malformed", not as a
  permissions problem.

---

## 9. The data layer: providers on the TV

The providers (StreamingCommunity, RaiPlay, the Aurora family, AnimeUnity) run
inside the TV app, compiled from `src/providers/` to WASM. The first versions ran them in
`tv/service`, an axum JSON service on the Mac that the TV called over the LAN
(and that macOS's application firewall blocked until the unsigned binary was
allowed with `socketfilterfw`). That made the TV useless without the Mac, so
the service was removed.

### 9.1 Why the providers can run in the webview

A CDP test from inside the installed widget fetched sites that send no CORS
headers: StreamingCommunity returned its 324 KB page, AnimeUnity and RaiPlay
answered 200, and Aurora accepted an `Authorization` header. The Tizen web
runtime does not apply CORS to an installed widget. Browser rules on headers
still apply: requests cannot set `Cookie`, `Referer` or `User-Agent`, and
responses do not expose `Set-Cookie`. StreamingCommunity, RaiPlay and Aurora
do not need them; AnimeUnity's login flow does, so it works on macOS only.

### 9.2 Making the providers build for WASM

- `JoinSet` → `futures::join_all`: there is no tokio runtime in the browser,
  and results now keep provider order.
- Retry back-off sleeps with `tokio::time::sleep` natively and `gloo-timers`
  on WASM; AnimeUnity's auth cache uses `web_time::Instant` (std's panics on
  `wasm32-unknown-unknown`).
- A small `Http` wrapper applies the request timeout per request, because WASM
  `reqwest` has no client-level timeout. All providers share one client without
  a cookie store: StreamingCommunity serves its catalogue, search, title pages
  and player without cookies.
- `#[async_trait(?Send)]`: fetch futures are not `Send`, and nothing needed
  `Send` once the axum service was gone.

Verified natively against the live sites (catalogue, paging, search, seasons,
episodes, stream) and on the TV with nothing running on the Mac: catalogue (100
titles), multi-provider search (29 ranked results), a StreamingCommunity
episode at 1280×720 and a RaiPlay programme over HLS. The WASM grew from ~750 KB
to ~2.9 MB (HTML parser, regex) and loads from the package.

---

## 10. Remote navigation (10-foot UX)

The first installed build launched but was effectively **pointer-only**: the TV
remote's D-pad reached the header, OK did nothing, and the content grid was
unreachable. A TV app needs an explicit focus model. It lives in
`src/tv/remote.rs`, installed once when the app mounts. It listens in the
capture phase and stops the keys it handles, so the shared views' own keyboard
handlers (desktop shortcuts) never see a remote key twice.

### 10.1 Register the Samsung keys

Media keys are only delivered to the webview after the app registers them. The
bindings are typed `wasm-bindgen` imports (`tizen.tvinputdevice.registerKey`),
so outside Tizen they fail quietly instead of throwing:

```rust
for name in ["MediaPlayPause", "MediaPlay", "MediaPause",
             "MediaStop", "MediaRewind", "MediaFastForward"] {
    let _ = register_key(name);
}
```

Arrows, OK and Return are always delivered and cannot be registered.

### 10.2 Handle keys by **code**, not by name

On this Tizen webview the remote's **OK button arrives as `keyCode == 13` but
with an empty `key` string**, so a handler that matched on `event.key ==
"Enter"` silently did nothing. Keys are mapped code-first, name second (for
desktop-browser testing):

| Remote button                 | keyCode                | Browsing                       | Player                                                             |
| ----------------------------- | ---------------------- | ------------------------------ | ------------------------------------------------------------------ |
| Up / Down / Left / Right      | 38 / 40 / 37 / 39      | spatial focus move             | seek 10 s on the progress bar, otherwise move between controls     |
| OK                            | 13                     | click focused element          | play/pause on the progress bar, otherwise press the focused button |
| Return                        | 10009                  | history back, exit on Home     | close the track menu, otherwise stop                               |
| Play/Pause, Play, Pause, Stop | 10252 / 415 / 19 / 413 |                                | transport                                                          |
| Rewind / Fast forward         | 412 / 417              |                                | seek 30 s                                                          |
| IME Done / Cancel             | 65376 / 65385          | submit / close search keyboard |                                                                    |

### 10.3 Spatial focus engine

There is no DOM tab-order that matches a 2-D grid, so focus moves
geometrically. A candidate must lie beyond the current element's centre in the
pressed direction; the distance along that axis is the edge gap (or the centre
distance when the boxes overlap, which keeps the Continue Watching ✕ reachable
with Up but skipped by Right). Candidates that overlap on the cross axis win; ties
fall back to `gap × 2 + cross gap`, then centre offset. This matches Chromium's
own spatial navigation closely enough that Up from the right edge of the grid
reaches the search button, while Up from the left reaches Continue Watching. The
ranking is pure Rust with unit tests (`remote.rs`).

Focus survives screen changes: navigating stores the focused element's
`data-focus` key in the history, Return restores it, and a `MutationObserver`
gives focus to the first `[data-focus]` element whenever nothing is focused
(first poster at start-up, _Play_ on a details page, first result after a search).

### 10.4 Scroll the focus into view

Focus uses `preventScroll` and is followed by an instant
`scrollIntoView({ block: "center" })`, so the focused tile is always centred.
When focus enters the navbar the content scrolls back to the top. Smooth
scrolling was measured and dropped (§12.1).

### 10.5 Visible focus ring

At 3 m, a 3 px outline is invisible. Cards get a 3 px accent ring (6 px on the
TV after the ×2 viewport scale), a 6 % lift and a shadow; buttons and rows
invert to the accent colour, and primary buttons, already accent, get the
ring. The rules use `:focus-visible` in the shared
stylesheet: the TV's programmatic focus matches it, a mouse click on macOS does
not, so keyboard users on the desktop get the same feedback. The Continue
Watching card uses `:focus-within`, because a selector list containing `:has()`
is dropped whole by Chromium before 105.
Focus changes are not animated (`transition: none`, §12.1).
On the TV only the focused element is ever filled with the accent colour
(`tv.css`): primary buttons are neutral until focused, the active season is a
white chip and the chosen track carries a check mark, so exactly one element on
screen reads as "selected by the remote". Sideways moves stay in the
focused row, so Right at the end of a row no longer jumps to the search button.

### 10.6 App-lifecycle gotcha

`prelaunch.support=true` kept the web app **resident** between opens, so
re-opening the tile could show the previous build. The flag is now removed and
`deploy-tv.sh` kills the running instance (`was_kill`) before installing and
launches with `was_execute`. This was _not_ why the remote failed (§10.7).

### 10.7 What actually broke — found with DevTools on the TV

`sdb shell 0 debug <app-id>` launches the widget with a Chrome DevTools port;
after `sdb forward tcp:<port> tcp:<port>` a small CDP client can read the DOM,
take screenshots and dispatch real key events. That showed two code defects:

1. **Clicks never changed the screen.** `App` called `AppState::new()` on every
   render and passed the result to `use_context_provider(|| state)`. The
   provider kept the first state, but `App` read its `screen` from a freshly
   created one (always `Home`). The navbar highlighted _Downloads_ while the
   content stayed on Home. Fix: `use_context_provider(AppState::new)`.
2. **Focus bounced back to the header.** `tv.css` added `position: relative`
   on `:focus`. The Continue Watching button is `position: absolute; inset: 0`,
   so focusing it collapsed it to 0×0; the engine then saw no current element
   and jumped to the first focusable (the logo). Fix: focus styles change only
   visual properties (ring, transform), never layout.

---

## 11. Playback and persistence on Tizen

### 11.1 HLS needs hls.js

StreamingCommunity/AnimeUnity streams are extension-less master playlists with
alternate audio groups and AES-128 segments served as `.html` / `text/html`.
Tizen's native `<video>` reports `canPlayType` "maybe" but either fails with
`MEDIA_ERR_SRC_NOT_SUPPORTED` or stalls at `readyState 0`. hls.js over MSE plays
them (verified: 1280×720, Italian/English audio, 37 subtitle tracks), because
every host involved answers `access-control-allow-origin: *`.

hls.js 1.7.3 is vendored (`assets/js/hls.min.js`, verified against the npm
`sha512` integrity, Apache-2.0) so playback never depends on a CDN; only the TV
loads it. `assets/js/player.js` is the one player for every device: Rust talks
to it through a long-lived `document::eval` channel (commands in, status and
track events out), which works the same in the macOS webview and in the TV's
WASM. It attaches hls.js when present and plain `src` otherwise (macOS plays
HLS natively, and progressive files everywhere), starts at the resume position,
applies the audio/subtitle preference and retries a fatal media error once
before reporting it. Each `<video>` is its own Dioxus component keyed by URL,
so the session is detached exactly when the episode changes or the player
closes, and the last position is saved at that moment.

CDP screenshots show the video area white: the TV composites video on a
hardware plane underneath the web layer.

### 11.2 Continue Watching must use IndexedDB

`localStorage` on this webview is flushed only on a clean exit: a value written
15 s before `was_kill` was gone on the next launch, while the same value
survived Return → `application.exit()`. A TV can terminate a backgrounded app,
so Continue Watching is stored in IndexedDB with
`transaction(…, "readwrite", { durability: "strict" })`, which survived the
same forced kill. `tizen.preference` is not available to this widget.
Progress is written every 10 s of playback, on Return/Stop and when the app is
hidden. The IndexedDB data also survives `tizen install` of a new build.
Relaunching right after the app closes fails `indexedDB.open` with "Internal
error opening backing store" while the previous process still holds the
database lock, which left that session without Continue Watching;
`storage.js` retries the open for up to 5 s.

### 11.3 Tooling quirks

- `tizen run -p <pkg>` throws a `NullPointerException` against this TV
  (`appcmd_support:disabled`); `sdb shell 0 was_execute <app-id>` launches it.
- `sdb shell 0 debug` answers `closed` while the app is running; kill it first.
- The sdb channel can wedge mid-session (every `sdb shell` hangs). The §7.4
  recovery works: `sdb kill-server`, `start-server`, `connect`.
- The DevTools endpoint accepts one client at a time; a hung client blocks the
  rest.

---

## 12. Scrolling, tracks and sync

### 12.1 Catalogue scrolling: measure, then fix

Holding Down on the grid felt slow. A CDP harness on the TV (`requestAnimationFrame`
frame times + long tasks while sending Down every 120 ms) measured **15 fps, p90
133 ms, ~38 frames over 50 ms**. A Chromium trace showed the JS main thread idle
(the focus engine costs ~13 ms per key) and the compositor thread saturated
(`Draw`, `UpdateDrawProperties`, `PrepareTiles`). Only 4 layers existed, and the
posters are 200×300 WebP of ~10 KB, so neither layer count nor images were the
cause. Isolating one variable at a time, from the same start position:

| Variant                                                  | fps    | p90       |
| -------------------------------------------------------- | ------ | --------- |
| smooth scroll + focus transition (original)              | 15     | 133 ms    |
| instant scroll only                                      | 24     | 133 ms    |
| no focus transition only                                 | 15     | 133 ms    |
| instant scroll + transform-only transition               | 24     | 133 ms    |
| **instant scroll, no transitions (blurred shadow kept)** | **58** | **17 ms** |
| smooth scroll, any focus variant                         | 15     | 133 ms    |

The fix is instant `scrollIntoView` plus `* { transition: none !important }`
(the desktop stylesheet also animates cards). Verified after the change: 57–60
fps, p90 17 ms, no long tasks. `content-visibility: auto` was tried as well: it
cuts the cost of appending a catalogue page (style + layout 109 → 44 ms) but
drops navigation to 28 fps because every step renders new rows, so it was not
adopted. Appending a page still costs ~100 ms once per page.

### 12.2 Audio and subtitle tracks

The player controls (Down from the progress bar) are Play/Pause, Audio,
Subtitles and Next episode; Audio and Subtitles open a menu of the stream's
tracks. The progress bar is the default focus and grows a scrubber thumb when
focused, so every focusable element in the player has visible feedback. No
emoji are used anywhere in the UI; the icons are SVG files in
`assets/img/vector/`, besides the score star and the menu check mark, which are text. Selection is stored as `PlayerPrefs` and applied to every
title by `pick` in `player.js`; macOS uses the same player and the same menus,
so both devices choose tracks identically. hls.js re-selects forced subtitles when the audio track
changes; `player.js` re-applies the chosen subtitle track after every audio
switch, which was verified on the TV (English audio + Italian subtitles from a
preference written by another device).

Seeking scrubs before it seeks. Each Left/Right (or Rewind/Fast forward) moves
`Player::scrub`, which the bar and the clock render in the same frame, and
restarts a 0.6 s timer; only when it fires does `player.js` set
`currentTime`, once. OK (play/pause) commits a pending scrub first, so playback
resumes from the chosen time; on macOS dragging the bar scrubs and releasing or
clicking it seeks at once. The scrub stays on screen until a status from the video
reports the new position, so a status already in flight cannot pull the bar
back. Measured on the TV: four presses 150 ms apart moved the bar 10 s each
within 40 ms while the video kept playing at 0:13, then the video jumped to
0:53 in one seek. The spinner follows `buffering` in `player.js`: before the
first `canplay` it is on, while paused it is off, and while playing it is on
only when the video is seeking or has less than `HAVE_FUTURE_DATA`. Before,
it was set by `waiting` and cleared only by `playing`, so a seek while paused
left it spinning until playback resumed.

### 12.3 Sync between macOS, TV and the browser userscript

The previous protocol stored one document (`watch`, `stamps`, `prefs`, `rev`)
and every client re-implemented a three-way merge with a snapshot, a ledger and
timestamps guessed from file mtimes (≈860 lines in Rust plus a JS copy). It is
replaced by a per-key last-writer-wins map:

- Each title (`watch:<provider>:<id>`) and the preferences (`prefs`) are
  records `{ value | null, at, device }`; `null` is a tombstone.
- A device sends its pending changes and the cursor of its last sync; the Apps
  Script applies them under its lock (newer `at` wins, ties by device id,
  timestamps clamped to server time + 60 s), assigns sequence numbers, and
  returns everything newer than the cursor plus the winners for the keys it
  received. Tombstones older than 90 days are pruned; a device whose cursor is
  older than the pruning answers with a full reset.
- The client engine is `sync::Replica`, shared by macOS and TV. A pending
  change the server has processed is resolved by the server's answer, so a
  clamped echo cannot win forever (a bug found by the conformance tests: JS
  serialises `10.0` as `10`, so comparing values could not detect the echo).
- A replica keeps at most 100 titles live (older ones become tombstones) and
  drops tombstones the server has acknowledged once they are older than the
  script's 90 days. It sends at most 500 changes per request, the script's
  limit, and keeps the rest pending for the next round. The script gives every
  document a random `id` (a document stored without one gets it on first use)
  and returns it with each answer. A replica remembers the last `id`; a
  different one means the document was wiped: the replica marks everything it
  holds as pending and restarts from sequence 0, so it uploads what it has and
  pulls the whole new document. Comparing sequences missed a wipe whenever the
  device's cursor was at or below the rebuilt document's sequence. An answer
  without an `id` (an older script) only resets. The userscript's replica
  follows the same rules.
- macOS and TV run the same loop (`sync::run`): the replica is the store
  itself (a JSON file on macOS, IndexedDB on the TV), pushed within 10 s when
  dirty, pulled every 60 s and whenever the app becomes visible. On first run
  the macOS app folds its old `continue_watching.json`/`player_prefs.json` into
  the replica and removes them. Userscript: the same replica in `localStorage`.
- The server code is tested from Rust: `sync.rs` runs `google_apps_script.gs`
  under JavaScriptCore (`osascript`) and drives real replicas through it
  (migration, convergence, deletions, clock skew, pruning, a wiped document,
  the request limit, auth, and a Drive read failure or an unparseable file that
  must not overwrite the stored state).
- Verified end to end against a local server running the same `.gs`: a change
  from a simulated Mac reached the TV, the TV's progress and subtitle choice
  reached the server, and the userscript's store (run in Node) pulled both and
  pushed its own change back to the TV.

The deployed Apps Script must be updated to the new file. Older clients then
fail to sync without corrupting data, so update the macOS app and the
userscript at the same time. The old document is migrated on first use.

---

## 13. Android TV, Google TV, Fire TV and the autonomous deploy

The same web bundle runs on Android in a WebView, wrapped by one Java class
(`resources/tv/android/MainActivity.java`) and built without Gradle: `aapt2`
links the manifest, icon and banner (both rasterised from the logo SVG with
`sips`, the banner padded to 320×180) and the
bundle as assets, `javac` + `d8` compile the activity (D8 backports `Map.of`
and `String.join` down to API 21), `zipalign` and `apksigner` finish it with
`~/.android/debug.keystore`, so every build updates the previous one in place.
Verified on Android TV emulators (Android 16 / WebView 143 and Android 12 /
WebView 91) driven with `adb shell input keyevent`.

### 13.1 Serving the bundle

`fetch()` refuses `file://` URLs whatever the WebView settings, and the
bundle loads its WASM with `fetch`. The page is therefore served from
`https://appassets.androidplatform.net` (the origin reserved for this by
`WebViewAssetLoader`) by `shouldInterceptRequest`, with explicit MIME types
(`application/wasm` keeps streaming compilation). IndexedDB works on that
origin and survives `adb install -r`.

### 13.2 CORS

Unlike a Tizen widget, the page is subject to CORS, and the sites send no
CORS headers. A probe page showed that `shouldInterceptRequest` receives every
CORS-mode request (`Origin` header), preflights included, and that an
intercepted response carrying `Access-Control-Allow-Origin` satisfies both
`fetch` and XHR. So the activity answers preflights itself and performs the
requests natively (`HttpURLConnection`, redirects followed by hand, 303 and
POST-on-301/302 turning into GET as `fetch` does), returning the response
with its own CORS headers in place of the site's. The WebView never exposes request bodies, so a script
inserted into `index.html` before the app wraps `fetch`: a request with a body
hands it (base64) to the activity through a `@JavascriptInterface` and sends a
one-time id header instead. Verified from DevTools on a debuggable build:
JSON and binary bodies, 302 and 307 redirects, 404 status, custom response
headers, DNS failure (rejects like `fetch`) and `AbortSignal` timeouts.

### 13.3 Remote and lifecycle

D-pad, OK and the media keys arrive as the same DOM keys as on Tizen. Back
does not: from Android 13 with target SDK 36 it never reaches the page or
`onBackPressed`, and the default callback moves the task to the background.
An `OnBackInvokedCallback` (and `onBackPressed` below API 33) dispatches a
cancelable `GoBack` key to the page instead; when the page does not cancel it
(not loaded yet, or a WebView too old to run it) the activity finishes, so
Back always leaves a blank app. On Home, where Tizen's `application.exit()` is
absent, the page leaves the key uncancelled, so the activity finishes. The page
is hidden when the activity stops, so playback pauses like on Tizen.

### 13.4 Web engine floor

`wasm-bindgen` keeps JS values in an `externref` table, so the WASM needs
reference types (Chromium 96). On WebView 91 the page stays blank. Building
with `-C target-cpu=mvp` does not help on stable Rust: the prebuilt `std`
still marks the module as using reference types and `wasm-bindgen` then fails
(`failed to find __wbindgen_externref_table_dealloc`). The deploy script reads
the TV's WebView version (`dumpsys webviewupdate`) and says when it must be
updated from the Play Store.

### 13.5 Finding the TV and installing everything

`deploy-tv.sh` pings every address of the local networks (at most a /22 per
interface) so the ARP table lists the live hosts, then probes those in
parallel: Samsung's control API (port 8001, `OS: Tizen`), ADB (5555) or the
Android TV remote service (6466), Roku's ECP (8060), LG's SSAP (3000 + 3001)
and Hisense's MQTT (36669). Android devices already on `adb` (USB, emulators)
are listed too. One TV is used directly; with several, it asks.

Missing tools are installed without questions: Rust and `dx` (the release
binary, avoiding the OpenSSL build of §3.3) and a Temurin JDK into
`~/Library/Caches/StreamVault`, the Android command-line tools and packages
into `ANDROID_HOME`, Tizen Studio as in §3.1 with only `cert-add-on` and
`TV-SAMSUNG-Extension-Tools` (3.0 GB instead of 4.1: `TV-SAMSUNG-Public`
only adds the TV emulator; the certificate tools are identical and the CLI
signs with the same profile). Each path was run from an empty `HOME`. What only the owner can do is waited for, with instructions: Developer
Mode on Samsung (checked through port 8001, then `sdb`), debugging allowed on
Android (pairing code read from the terminal for wireless debugging), and the
Samsung account sign-in for a new TV's certificate.

The certificate is created the way Certificate Manager does it (its constants
and the files it left in `~/SamsungCertificate` match; TizenBrew Installer
automates the same flow): the browser opens Samsung's sign-in for the Tizen
SDK (`serviceID=v285zxnl3h`, redirect to `localhost:4794/signin/callback`),
which posts `code` = JSON with `access_token`, `userId` and `inputEmailID`;
`sdb shell 0 getduid` gives the device id; two 2048-bit keys and SHA-512 CSRs
(author `CN=StreamVault`; distributor `CN=TizenSDK`, the e-mail and the SAN
URIs `URN:tizen:packageid=` and `URN:tizen:deviceid=<DUID>`) go to
`svdca.samsungqbe.com/apis/v3/authors` and `/distributors` (`platform=VD`,
`privilege_level=Public`, `developer_type=Individual`); the distributor answer
is the device profile with the certificate inside. PKCS#12 files with
Samsung's CA from the Certificate Manager plugin (LibreSSL's defaults, which
the CLI's Java 8 reads) become profile `streamvault-<DUID>`, and the device
profile is pushed to `/home/owner/share/tmp/sdk_tools/`, as *Permit to install
applications* does. The script signs with the per-TV profile if it exists, else
with the active one, and creates a per-TV certificate only when there is no
profile or the TV rejects the active one's certificate. Checked
with a local CA standing in for Samsung (profile registered, `.wgt` signed,
the user's `profiles.xml` restored byte for byte); the real sign-in and
issuance need the account owner.

LG webOS, VIDAA and Roku are reported and skipped (Titan OS exposes nothing to
probe for, and cannot run it either): installed webOS
apps cannot load module scripts from `file://` or make cross-site requests
(LG's answer is a hosted app, which would need a server), VIDAA and Titan OS
have no installs outside their stores, and Roku has no web engine.

---

## 14. Gotchas & lessons — quick reference

| #   | Symptom                                                      | Root cause                                                                                                                                                          | Fix                                                                         |
| --- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| 1   | `cargo install dioxus-cli` fails                             | missing `pkg-config`; `openssl@3` unlinked                                                                                                                          | `brew install pkgconf openssl@3 && brew link openssl@3 --force`             |
| 2   | install fails, final log line unhelpful                      | real error is in the build-script stderr                                                                                                                            | capture full log, don't `tail -5`                                           |
| 3   | `sdb connect … failed` though Dev Mode on                    | multiple competing sdb servers on host                                                                                                                              | kill all sdb + Device Manager; keep exactly one                             |
| 4   | sdbd "dead" after Developer-Mode toggle                      | enable flag read at boot; dialog re-arms it                                                                                                                         | set once, cold-boot, connect without reopening                              |
| 5   | TV unreachable after reboot                                  | DHCP moved its IP                                                                                                                                                   | resolve by MAC via `arp -a`                                                 |
| 6   | `install failed[-4] Load archive info fail`                  | package id ≠ 10 chars                                                                                                                                               | use a 10-char alphanumeric package id                                       |
| 7   | couldn't tell if cert/firmware at fault                      | —                                                                                                                                                                   | re-sign a known-good reference `.wgt`, install it                           |
| 8   | TV can't reach the dev service                               | macOS app firewall blocks inbound                                                                                                                                   | allow the binary via `socketfilterfw` (service later removed, §9)           |
| 9   | OK button does nothing                                       | OK = `keyCode 13`, empty `key` string                                                                                                                               | match on keyCode, not `key` name                                            |
| 10  | D-pad down "freezes"                                         | `focus()` doesn't auto-scroll                                                                                                                                       | `scroll_into_view` after every focus move                                   |
| 11  | rebuild looks identical on TV                                | resident app (`prelaunch.support`)                                                                                                                                  | flag removed; `was_kill` before install                                     |
| 12  | `.wgt` bloated / stale bundles                               | `dx` keeps old content-hashed assets                                                                                                                                | stage packaging from a clean directory                                      |
| 13  | OK "does nothing", screen never changes                      | `AppState::new()` called on every render                                                                                                                            | `use_context_provider(AppState::new)`                                       |
| 14  | focus jumps back to the header                               | `position: relative` on `:focus` collapses absolute buttons                                                                                                         | focus styles change visuals only                                            |
| 15  | HLS stalls / `MEDIA_ERR_SRC_NOT_SUPPORTED`                   | native player can't handle these playlists                                                                                                                          | hls.js over MSE (vendored)                                                  |
| 16  | Continue Watching lost after a kill                          | `localStorage` flushed only on clean exit                                                                                                                           | IndexedDB with `durability: "strict"`                                       |
| 17  | `tizen run` NullPointerException                             | CLI vs `appcmd_support:disabled`                                                                                                                                    | `sdb shell 0 was_execute <app-id>`                                          |
| 18  | every `sdb shell` hangs mid-session                          | wedged sdb channel                                                                                                                                                  | `sdb kill-server`, `start-server`, `connect`                                |
| 19  | grid scrolling at 15 fps                                     | smooth scroll + animated focus re-raster every frame                                                                                                                | instant scroll, no transitions                                              |
| 20  | wrong subtitles after audio change                           | hls.js re-selects forced subtitles                                                                                                                                  | re-apply the chosen subtitle track                                          |
| 21  | every dialog freezes after Return                            | `if let Some(x) = *signal.peek()` keeps the read guard alive through the block; writing the signal inside panics (`AlreadyBorrowed`) and a wasm panic stops the app | copy the value out of the signal before branching                           |
| 22  | TV useless without the Mac                                   | providers ran in a LAN service                                                                                                                                      | run the providers in the TV app; the widget is exempt from CORS (§9)        |
| 23  | Continue Watching empty after a quick relaunch               | the closing process still holds the IndexedDB lock                                                                                                                  | retry `indexedDB.open` for up to 5 s                                        |
| 24  | bar lags behind the remote, stream re-buffers on every press | each press seeked the video and the bar waited for its status                                                                                                       | scrub in Rust at once, seek once 0.6 s after the last press                 |
| 25  | spinner stuck after seeking while paused                     | `waiting` was cleared only by `playing`                                                                                                                             | buffering = first load, or playing and seeking / short of data              |
| 26  | Android: WASM never loads from `file://`                     | `fetch` rejects the `file` scheme                                                                                                                                   | serve the bundle from `https://appassets.androidplatform.net`               |
| 27  | Android: every provider request fails                        | CORS applies to the WebView page                                                                                                                                    | answer preflights and forward requests natively in `shouldInterceptRequest` |
| 28  | Android: POST bodies missing                                 | the WebView does not expose them                                                                                                                                    | `fetch` wrapper hands the body to the activity, sends an id header          |
| 29  | Android: Back leaves the app                                 | Android 13+ back dispatch bypasses keys                                                                                                                             | `OnBackInvokedCallback` → `GoBack` key; Home leaves it uncancelled          |
| 30  | `$version…` unbound in the script                            | bash 3.2 reads the UTF-8 ellipsis into the name                                                                                                                     | write `${version}…`                                                         |

---

## 15. Current status

**Done**

- Tizen Studio 6.1 + TV extensions, headless, ~3 GB.
- `streamvault-samsung` certificate profile (author + distributor, DUID-locked).
- One crate for every device: shared app state, views, player, providers,
  sync and assets; device modules only add what the device alone has.
- The TV app is standalone: providers, playback and sync run on the TV (§9).
- Signed `.wgt` installs, the **StreamVault tile with its logo appears in Smart
  Hub**, and the whole flow works with the remote, verified on the TV over
  DevTools: Home (Continue Watching + catalogue with infinite paging), search
  with the TV keyboard, movie and series details (seasons, episodes), playback
  with seek / pause / _Next episode_, resume from Continue Watching, Return
  history with focus restore, exit from Home, error state with Retry.
- Grid navigation at 60 fps; audio/subtitle selection; Continue Watching and
  preferences synced with macOS and the userscript (§12).
- Android TV, Google TV and Fire TV build from the same bundle; catalogue,
  search, details, HLS playback, remote, Back/exit and persistence verified
  on Android TV emulators (§13).
- `scripts/deploy-tv.sh` finds the TV, installs every missing tool and waits
  for the TV-side steps (§13.5).

**Open**

- On the TV, AnimeUnity's cookie-based login cannot work under browser header
  rules (§9.1). Its search failed with HTTP 405 until `domains.json` followed the
  site's move to `animeunity.co` (the old domain's 301 turned the POST into a GET).
- Aurora accepts the realm token but answers 403 `access.denied.geoblocked`
  outside Italy, and its homepages carry no `X-REALM-DPLAY` token, so videos of
  that realm cannot play.
- Downloads stay a desktop feature.

---

## 16. Reproducible command reference

```bash
# ── toolchain ───────────────────────────────────────────────────────────────
brew install pkgconf openssl@3 && brew link openssl@3 --force
OPENSSL_DIR=/opt/homebrew/opt/openssl@3 \
  PKG_CONFIG_PATH=/opt/homebrew/opt/openssl@3/lib/pkgconfig \
  cargo install dioxus-cli --version 0.7.10 --locked
rustup target add wasm32-unknown-unknown

# ── one sdb client, connect by MAC-resolved IP ──────────────────────────────
pkill -9 -f sdb; pkill -9 -f device-manager
TV=$(arp -a | grep -i e0:3:6b:70:4e:bd | grep -oE '([0-9]+\.){3}[0-9]+' | head -1)
sdb start-server && sdb connect "$TV:26101" && sdb devices

# ── find the TV, install the tools, build, sign, install and launch ────────
scripts/deploy-tv.sh                 # asks when several TVs are found
TV=192.168.68.66 scripts/deploy-tv.sh

# ── Android: debug the page (debuggable builds only) ────────────────────────
adb forward tcp:9222 localabstract:webview_devtools_remote_$(adb shell pidof local.streamvault)

# ── debug on the TV (Chrome DevTools protocol) ──────────────────────────────
sdb shell 0 was_kill AR6J1YmstG.StreamVault
sdb shell 0 debug AR6J1YmstG.StreamVault      # → "… debug 1 port: <port>"
sdb forward tcp:<port> tcp:<port>             # then http://127.0.0.1:<port>/json
```

### Key facts

- **Device:** UE50CU8072UXXH · Tizen 9.0 · armv7l · device id `7XCI7KNS2VUXI`
- **sdb port:** 26101 · **control API:** 8001 (`/api/v2/`)
- **Package id:** `AR6J1YmstG` (exactly 10 chars — non-negotiable)
- **Certificate profile:** `streamvault-samsung`
