<p align="center">
  <img src="assets/img/raster/logo_raster_small.png" alt="StreamVault" width="120" />
</p>

<h1 align="center">StreamVault</h1>

<p align="center">
  <strong>Your streaming catalogue, on your computer and on your TV.</strong><br />
  Browse, watch and download in one native app, then pick up on the big screen right where you left off.
</p>

<p align="center">
  <img alt="macOS" src="https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white" />
  <img alt="Windows" src="https://img.shields.io/badge/Windows-0078D4" />
  <img alt="Samsung TV" src="https://img.shields.io/badge/Samsung_TV-1428A0?logo=samsung&logoColor=white" />
  <img alt="Android TV" src="https://img.shields.io/badge/Android_TV-3DDC84?logo=android&logoColor=white" />
  <img alt="Fire TV" src="https://img.shields.io/badge/Fire_TV-FF9900" />
  <img alt="Rust" src="https://img.shields.io/badge/Rust-Dioxus-B7410E?logo=rust&logoColor=white" />
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-blue" /></a>
</p>

---

## Install in two commands

One script, `scripts/install`, for both computers: it takes `desktop` or `tv` and works out the system by itself. A Mac runs it in Terminal, Windows in PowerShell.

**Desktop app**

```bash
curl -fsSL https://raw.githubusercontent.com/falcosan/StreamVault/main/scripts/install | bash -s desktop
```

```powershell
iex "& { $(irm https://raw.githubusercontent.com/falcosan/StreamVault/main/scripts/install) } desktop"
```

On a Mac, `StreamVault.app` comes ready-built from the latest release, with FFmpeg and N_m3u8DL-RE inside, and appears in the current folder, ready to drag into Applications.

On Windows, StreamVault installs for your user, without administrator rights, and appears in the Start menu; Settings › Apps uninstalls it. The app comes ready-built from the latest release, with FFmpeg and N_m3u8DL-RE next to it.

**TV**

```bash
curl -fsSL https://raw.githubusercontent.com/falcosan/StreamVault/main/scripts/install | bash -s tv
```

```powershell
iex "& { $(irm https://raw.githubusercontent.com/falcosan/StreamVault/main/scripts/install) } tv"
```

Run it on a computer on the same network as the TV. It finds the TVs, asks which one if there are several, installs every tool it needs and puts StreamVault on the TV. When the TV needs something only you can do, such as switching on Developer Mode, it tells you exactly what and carries on by itself. Whatever it installed for the run is removed when it ends.

Nothing to set up beforehand and nothing to compile: the script downloads a ready-built installer from the latest release, which fetches whatever the TV needs and removes it at the end. The apps then update themselves from their update icon in seconds; running a command again also updates.

## Why StreamVault

- **Everything in one place.** StreamingCommunity's catalogue, and one search across StreamingCommunity, RaiPlay, the Discovery channels and, on the computer, AnimeUnity.
- **Built for the remote.** Every screen works with the D-pad; seeking follows your thumb instantly, and the audio and subtitle menus sit right under the progress bar.
- **Continue Watching everywhere.** Progress and track choices follow you across computer, TV and the StreamingCommunity site in your browser, stored in your own Google Drive.
- **Downloads on the computer.** Save movies and episodes with the tracks you want: [N_m3u8DL-RE](https://github.com/nilaoda/N_m3u8DL-RE) fetches them, [FFmpeg](https://ffmpeg.org/) merges video and audio into MP4, subtitles sit alongside as `.vtt`.
- **The TV stands alone.** Catalogue, playback and sync run on the TV itself: no server, and no computer once it is installed.
- **Native and light.** One Rust codebase built with [Dioxus](https://dioxuslabs.com/) for every screen; the desktop and TV apps update themselves.

## Supported TVs

| TV                    | Works on                                                                        |
| --------------------- | ------------------------------------------------------------------------------- |
| Samsung (Tizen)       | Tizen 8 or later: 2024 models, and older ones updated                           |
| Android TV, Google TV | Android 5 or later with System WebView 96+ (tested on Android 16)               |
| Fire TV               | Fire OS models (not the newer Vega OS); the same APK, not yet tried on a device |

LG webOS, Hisense VIDAA, Philips Titan OS and Roku cannot run it: [docs/TV.md](docs/TV.md) explains why, along with the remote controls and how the TV build works.

## Sync

Optional and private: a small Google Apps Script keeps one JSON file in your Drive, protected by your own token. Put its address and token in `sync.json` on your computer and every device you install from it stays in step; installing on someone else's TV, you answer no and it stays on its own. Setup takes four steps: [docs/CONFIGURATION.md](docs/CONFIGURATION.md#sync).

## Build from source

```bash
cargo run                          # run the desktop app
scripts/install desktop            # build StreamVault.app from this clone into the current folder
scripts/install tv                 # build this clone and install it on a TV
./scripts/update.sh                # upgrade the dependencies in Cargo.toml and Cargo.lock
```

The Mac app needs macOS 13 or later, the Windows app 64-bit Windows 10 or 11. With `cargo run`, downloads use the `N_m3u8DL-RE` and `ffmpeg` found in `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin` or `~/.local/bin` (on Windows, in `PATH`); the packaged app ships its own.

Settings, download options and file locations: [docs/CONFIGURATION.md](docs/CONFIGURATION.md).

## License

This project is licensed under the [MIT License](LICENSE).

---

## Disclaimer

> This software is provided strictly for **educational and research purposes only**. The author and contributors:
>
> - **DO NOT** assume any responsibility for illegal or unauthorized use of this software
> - **DO NOT** encourage, promote, or support the download of copyrighted content without proper authorization
> - **DO NOT** provide, include, or facilitate obtaining any DRM circumvention tools, CDM modules, or decryption keys
> - **DO NOT** endorse piracy or copyright infringement in any form
>
> ### User Responsibilities
>
> By using this software, you agree that:
>
> 1. **You are solely responsible** for ensuring your use complies with all applicable local, national, and international laws and regulations
> 2. **You must have legal rights** to access and download any content you process with this software
> 3. **You will not use** this software to circumvent DRM, access unauthorized content, or violate copyright laws
> 4. **You understand** that downloading copyrighted content without permission is illegal in most jurisdictions
>
> ### No Warranty
>
> This software is provided "as is", without warranty of any kind, express or implied, including but not limited to the warranties of merchantability, fitness for a particular purpose, and noninfringement. In no event shall the authors or copyright holders be liable for any claim, damages, or other liability, whether in an action of contract, tort, or otherwise, arising from, out of, or in connection with the software or the use or other dealings in the software.
>
> **If you do not agree with these terms, do not use this software.**

---
