# Architecture Map

## Module Structure (2,491 LOC across 20 .rs files)

```
src/
├── main.rs (16)           Entry point — wires Iced app
├── config/
│   ├── mod.rs (3)         Re-exports AppConfig
│   └── settings.rs (143)  Config structs + load/save (JSON)
├── download/
│   ├── mod.rs (8)         Re-exports
│   ├── engine.rs (169)    Spawns N_m3u8DL-RE subprocess
│   └── progress.rs (64)   Download progress model + regex parsing
├── gui/
│   ├── mod.rs (5)         Re-exports App
│   ├── app.rs (661)       ★ GOD FILE — state, Message enum, update(), view(), sidebar
│   ├── style.rs (31)      Color constants + 2 style functions
│   └── screens/
│       ├── mod.rs (13)    Re-exports
│       ├── home.rs (73)   Home screen view
│       ├── search.rs (116) Search + result cards
│       ├── details.rs (147) Movie/series detail + episode cards
│       ├── player.rs (72) Player controls screen
│       ├── downloads.rs (85) Downloads list screen
│       └── settings.rs (132) Settings form screen
├── playback/
│   ├── mod.rs (6)         Conditional macOS re-export
│   └── native_player.rs (99) AVPlayer wrapper (macOS only)
├── provider/
│   ├── mod.rs (7)         Re-exports
│   ├── models.rs (57)     MediaEntry, Season, Episode, StreamUrl
│   ├── traits.rs (49)     Provider trait + ProviderError
│   └── streaming_community.rs (489) ★ LARGE — HTTP+parsing+stream extraction
└── util/
    ├── mod.rs (3)         Re-exports
    └── binary.rs (46)     Binary discovery (bundled + system paths)
```

## Dependency Graph
```
main.rs → gui::App
gui::app → config, download, playback, provider, gui::screens, gui::style
gui::screens/* → gui::app::Message, gui::style, config, download, provider::models
download::engine → download::progress, config, util::binary
provider::streaming_community → provider::models, provider::traits
playback::native_player → (macOS-only objc2 crates)
config::settings → (serde, dirs, fs)
util::binary → (dirs, std::path)
```

## Data Flow
1. User interacts with Iced GUI → `Message` dispatched
2. `App::update()` matches message → triggers async tasks or state updates
3. Provider searches/fetches via reqwest HTTP → returns models
4. Download engine spawns N_m3u8DL-RE subprocess → streams progress via mpsc channel
5. Playback creates native macOS AVPlayer window

## Problem Areas

### GOD FILE: gui/app.rs (661 lines)
- `App` struct holds ALL application state (16 fields)
- `Message` enum has 35+ variants spanning navigation, search, details, player, download, settings
- `update()` is a single ~400 line match expression
- Mixes navigation logic, provider orchestration, download management, settings mutation, and player control

### LARGE FILE: provider/streaming_community.rs (489 lines)
- Combines HTTP client management, HTML parsing, JSON extraction, iframe resolution, and stream URL construction
- 6 static regex compilations
- Could benefit from extracting parsing helpers

### PLATFORM COUPLING
- `app.rs` line 298: `MainThreadMarker::new().expect("update() runs on main thread")` — macOS-only, will panic on other platforms
- No `#[cfg(target_os = "macos")]` guard in app.rs for playback-related code

### NO CIRCULAR DEPENDENCIES
Module dependency graph is strictly hierarchical — no cycles detected.

### ORPHANED CODE
- `ProcessConfig.use_gpu` and `ProcessConfig.merge_audio`/`merge_subtitle` fields are stored but never passed to the download engine command
- `RequestsConfig.timeout` and `max_retry` are stored but never used in reqwest client or download engine
- `Season.episodes` field is always empty (episodes loaded separately)
