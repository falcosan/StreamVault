<p align="center">
  <img src="assets/logo/logo_raster_small.png" alt="StreamVault Logo" width="120" />
</p>

<h1 align="center">StreamVault</h1>

<p align="center">
  A native macOS desktop application for browsing and downloading streaming content, built with Rust and <a href="https://dioxuslabs.com/">Dioxus</a>.
</p>

---

## Overview

StreamVault is a lightweight, native desktop app that aggregates content from multiple streaming providers into a single interface. It lets you search, browse catalogs, view details, and download media — all from one place.

## Features

- **Unified Search** — Query multiple providers simultaneously and view aggregated results.
- **Catalog Browsing** — Browse trending/popular content from each provider on the home screen.
- **Detail View** — View metadata including year, score, description, and poster art.
- **Season & Episode Navigation** — Browse seasons and episodes for TV series.
- **Integrated Downloads** — Download streams via [N_m3u8DL-RE](https://github.com/nilaoda/N_m3u8DL-RE) with progress tracking.
- **Post-Processing** — Automatic muxing with [FFmpeg](https://ffmpeg.org/) (GPU acceleration optional).
- **Configurable** — JSON-based configuration for output paths, download threads, video/audio/subtitle track selection, proxy settings, and more.
- **Dark UI** — Custom-styled dark interface with a Netflix-inspired layout.

## Requirements

- **macOS** (the app uses a native WebView via Dioxus Desktop)
- **Rust** toolchain (1.75+ recommended)
- **FFmpeg** — for post-download muxing
- **N_m3u8DL-RE** — for HLS/DASH stream downloading

## Installation

### Quick Install (macOS)

Run a single command to install `StreamVault.app` into `~/Applications`:

```bash
curl -fsSL "https://raw.githubusercontent.com/falcosan/StreamVault/refs/heads/main/scripts/package.sh" | bash
```

### Building from Source

#### Development

```bash
cargo build
cargo run
```

#### Release

```bash
cargo build --release
```

#### Packaging (.app bundle)

The included packaging script builds a release binary, downloads FFmpeg and N_m3u8DL-RE, and assembles a self-contained `.app` bundle:

```bash
./scripts/package.sh
```

The resulting `StreamVault.app` will be in the `dist/` directory with all dependencies bundled under `Contents/Resources/bin/`.

## Configuration

StreamVault stores its configuration in a JSON file. On first launch, a default config is created automatically.

### Config Sections

| Section            | Key Options                                                                                             |
| ------------------ | ------------------------------------------------------------------------------------------------------- |
| **output**   | `root_path`, `movie_folder_name`, `serie_folder_name`, `map_episode_name`                       |
| **download** | `thread_count` (default: 8), `retry_count`, `concurrent_download`, `max_speed`, track selection |
| **process**  | `use_gpu`, `merge_audio`, `merge_subtitle`, `extension` (default: mp4)                          |
| **requests** | `timeout` (default: 30s), `max_retry`, `use_proxy`, `proxy_url`                                 |

## Project Structure

| Directory          | Description                                                                                                              |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------ |
| `src/`           | Application source code — UI components, state management, configuration, download engine, and provider implementations |
| `src/providers/` | Pluggable streaming provider modules with a shared trait interface and data models                                       |
| `resources/`     | macOS app bundle metadata (`Info.plist`)                                                                               |
| `scripts/`       | Build, packaging, and update automation scripts                                                                          |
| `assets/`        | Logo and branding assets                                                                                                 |

## Tech Stack

- **Language:** Rust (2021 edition)
- **UI Framework:** [Dioxus](https://dioxuslabs.com/) 0.7 (Desktop / WebView)
- **HTTP Client:** [reqwest](https://docs.rs/reqwest) with cookies and JSON support
- **Async Runtime:** [Tokio](https://tokio.rs/)
- **HTML Parsing:** [scraper](https://docs.rs/scraper)
- **Serialization:** [serde](https://serde.rs/) + serde_json

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
