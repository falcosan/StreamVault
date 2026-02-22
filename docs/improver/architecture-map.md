# Architecture Map — StreamVault

## Overview
StreamVault is a macOS desktop app for browsing, streaming, and downloading media from multiple providers. Built with Dioxus (Rust desktop UI framework using WebView).

## Module Map

```
main.rs (17 lines)
  └─ Entry point: launches Dioxus desktop app with window config
  └─ Depends on: app

app.rs (433 lines) ★ GOD COMPONENT
  └─ App() component: ALL app state (30+ signals), ALL event handlers (10+ closures)
  └─ Depends on: config, gui, providers, util

config.rs (249 lines)
  └─ AppConfig + sub-configs (Output, Download, Process, Requests)
  └─ Load/save from JSON file in config directory
  └─ Depends on: serde, dirs, std::fs

gui.rs (653 lines)
  └─ GLOBAL_CSS constant (250 lines of CSS)
  └─ Screen enum
  └─ Components: Navbar, HomeView, SearchView, PosterCard, DetailsView, EpisodeRow, PlayerView, DownloadsView, DlCard
  └─ Helper functions: name_hash, poster_color, LOGO_SVG
  └─ Depends on: providers (MediaEntry), util (DownloadProgress, DownloadStatus)

providers.rs (1095 lines) ★ GOD FILE
  └─ Data models: MediaType, MediaEntry, Season, Episode, StreamUrl
  └─ Error types: ProviderError, ProviderResult
  └─ Provider trait (5 async methods via Pin<Box<dyn Future>>)
  └─ StreamingCommunityProvider (340 lines) — Italian streaming site scraper
  └─ RaiPlayProvider (340 lines) — RAI public TV scraper
  └─ Depends on: reqwest, scraper, serde_json, regex, url, std::sync

util.rs (367 lines)
  └─ Binary finder (bundled_bin_dir, find_binary)
  └─ Download types: DownloadStatus, DownloadProgress, DownloadRequest
  └─ DownloadEngine: builds N_m3u8DL-RE command, parses progress
  └─ sanitize_filename helper
  └─ Depends on: config, regex, tokio::process, tokio::io
```

## Dependency Graph
```
main → app → config
             gui → providers (MediaEntry)
                   util (DownloadProgress, DownloadStatus)
             providers
             util → config
```

No circular dependencies.

## Data Flow
1. App starts → init providers → resolve domains → fetch catalog
2. User searches → providers.search() → results displayed
3. User selects entry → providers.get_seasons/get_episodes()
4. User plays → providers.get_stream_url() → video element
5. User downloads → providers.get_stream_url() → DownloadEngine → N_m3u8DL-RE subprocess → progress updates via mpsc channel

## Problem Areas

### Critical: app.rs God Component
- 30+ signals managing all app state
- 10+ closures as event handlers
- All business logic mixed with UI wiring
- Impossible to test independently

### Critical: providers.rs God File (1095 lines)
- Two completely unrelated providers in one file
- Data model types mixed with provider implementations
- Should be split into: models, trait definition, streaming_community, raiplay

### High: gui.rs Mixed Concerns
- 250 lines of CSS as a string constant
- 9 components + helpers all in one file
- Should extract CSS, potentially split components

### High: Duplicated Patterns
- `.map_err(|e| ProviderError::Network(e.to_string()))` appears 15+ times
- `.map_err(|e| ProviderError::Parse(e.to_string()))` appears 10+ times
- Same user-agent string hardcoded in both providers
- Same dedup pattern (`seen.insert(id)`) in 4 places

### Medium: Manual Async Trait Boilerplate
- Every trait method uses `Pin<Box<dyn Future<Output = ...> + Send + '_>>`
- Every impl wraps body in `Box::pin(async move { ... })`
- Could use `async_trait` crate or RPITIT

### Medium: No From impls for Error
- Could implement `From<reqwest::Error>` and `From<serde_json::Error>` for ProviderError
- Would eliminate majority of `.map_err()` calls

### Low: Unnecessary #[inline] annotations
- On trivial one-liner methods that the compiler inlines anyway

### Low: Inconsistent Static Init
- `LazyLock` for CSS selectors, `OnceLock` for regexes
- Both serve the same purpose (lazy static init)
