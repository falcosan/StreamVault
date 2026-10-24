<p align="center">
  <img src="assets/img/raster/logo_raster_small.png" alt="StreamVault" width="120" />
</p>

<h1 align="center">StreamVault</h1>

<p align="center">
  <strong>Your streaming catalogue, on your Mac and on your TV.</strong><br />
  Browse, watch and download in one native app, then pick up on the big screen right where you left off.
</p>

<p align="center">
  <img alt="macOS" src="https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white" />
  <img alt="Samsung TV" src="https://img.shields.io/badge/Samsung_TV-1428A0?logo=samsung&logoColor=white" />
  <img alt="Android TV" src="https://img.shields.io/badge/Android_TV-3DDC84?logo=android&logoColor=white" />
  <img alt="Fire TV" src="https://img.shields.io/badge/Fire_TV-FF9900" />
  <img alt="Rust" src="https://img.shields.io/badge/Rust-Dioxus-B7410E?logo=rust&logoColor=white" />
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-blue" /></a>
</p>

---

## Install in two commands

**Mac**

```bash
curl -fsSL "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main/scripts/package.sh" | bash
```

`StreamVault.app` appears in the current folder, ready to drag into Applications.

**TV**

```bash
curl -fsSL "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main/scripts/deploy-tv.sh" | bash
```

Run it on a Mac on the same network as the TV. It finds the TVs, asks which one if there are several, installs every tool it needs and puts StreamVault on the TV. When the TV needs something only you can do, such as switching on Developer Mode, it tells you exactly what and carries on by itself.

No Rust, SDK or Java to set up beforehand: the scripts fetch what is missing. Run either command again to update.

## Why StreamVault

- **Everything in one place.** StreamingCommunity, RaiPlay and more in a single catalogue and a single search.
- **Built for the remote.** Every screen works with the D-pad; seeking follows your thumb instantly, audio and subtitles are one press away.
- **Continue Watching everywhere.** Progress and track choices follow you across Mac, TV and browser, stored in your own Google Drive.
- **Downloads on the Mac.** Save movies and episodes with the tracks you want, merged into MP4 by [N_m3u8DL-RE](https://github.com/nilaoda/N_m3u8DL-RE) and [FFmpeg](https://ffmpeg.org/).
- **The TV stands alone.** Catalogue, playback and sync run on the TV itself: no server, and no Mac once it is installed.
- **Native and light.** One Rust codebase built with [Dioxus](https://dioxuslabs.com/) for every screen; the Mac app updates itself.

## Supported TVs

| TV                    | Works on                                              |
| --------------------- | ----------------------------------------------------- |
| Samsung (Tizen)       | Tizen 8 or later: 2024 models, and older ones updated |
| Android TV, Google TV | Android 5 or later with an up-to-date System WebView  |
| Fire TV               | Fire OS models (not the newer Vega OS)                |

LG webOS, Hisense VIDAA, Philips Titan OS and Roku cannot run it: [docs/TV.md](docs/TV.md) explains why, along with the remote controls and how the TV build works.

## Sync

Optional and private: a small Google Apps Script keeps one JSON file in your Drive, protected by your own token. Put its address and token in `sync.json` on the Mac and every device you install from it stays in step. Setup takes four steps: [docs/CONFIGURATION.md](docs/CONFIGURATION.md#sync).

## Build from source

```bash
cargo run                  # run the Mac app
./scripts/package.sh       # build dist/StreamVault.app
./scripts/deploy-tv.sh     # build and install on a TV
```

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
