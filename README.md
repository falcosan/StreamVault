<p align="center">
  <img src="assets/logo/logo_raster_small.png" alt="StreamVault Logo" width="120" />
</p>

<h1 align="center">StreamVault</h1>

<p align="center">
  Native macOS app to browse and download streaming content, built with Rust + <a href="https://dioxuslabs.com/">Dioxus</a>.
</p>

---

## Features

- Multi-provider search and catalog browsing
- Show details, seasons, and episodes
- Download streams with [N_m3u8DL-RE](https://github.com/nilaoda/N_m3u8DL-RE)
- Post-process with [FFmpeg](https://ffmpeg.org/) (optional GPU)
- JSON configuration for paths, download behavior, tracks, and requests
- In-app update flow (rebuilds from source)

## Requirements

- **macOS** (the app uses a native WebView via Dioxus Desktop)
- **Rust** toolchain
- **FFmpeg**
- **N_m3u8DL-RE**

## Installation

### Quick Install

Run a single command to install `StreamVault.app`:

```bash
curl -fsSL "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main/scripts/package.sh" | bash
```

### Build from Source

Run in development:

```bash
cargo run
```

Build release:

```bash
cargo build --release
```

Package `.app` bundle:

```bash
./scripts/package.sh
```

Output: `dist/StreamVault.app`

## Configuration

Config is stored in JSON. On first launch, StreamVault creates a default config automatically.

Main sections:

| Section            | Key Options                                                                                             |
| ------------------ | ------------------------------------------------------------------------------------------------------- |
| **output**   | `root_path`, `movie_folder_name`, `serie_folder_name`, `map_episode_name`                       |
| **download** | `thread_count` (default: 8), `retry_count`, `concurrent_download`, `max_speed`, track selection |
| **process**  | `use_gpu`, `merge_audio`, `merge_subtitle`, `extension` (default: mp4)                          |
| **requests** | `timeout` (default: 30s), `max_retry`, `use_proxy`, `proxy_url`                                 |

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
