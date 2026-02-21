# Findings

## CRITICAL

### F1: Platform-coupling panic in app.rs
- **File:** `src/gui/app.rs:298`
- **Category:** SECURITY / DESIGN
- **Impact:** CRITICAL
- **Problem:** `MainThreadMarker::new().expect("update() runs on main thread")` will panic on non-macOS. The `NativeVideoPlayer` import and all playback logic in `app.rs` has no `#[cfg(target_os = "macos")]` guard, but `playback/mod.rs` conditionally compiles the module.
- **Proposed:** Gate all playback code paths in `app.rs` with `#[cfg(target_os = "macos")]` and provide a graceful "not supported" error on other platforms.
- **Dependencies:** None

## HIGH

### F2: GOD FILE — gui/app.rs (661 lines)
- **File:** `src/gui/app.rs`
- **Category:** SOLID VIOLATIONS (SRP)
- **Impact:** HIGH
- **Problem:** `App` struct holds 16 fields spanning 5 unrelated concerns. `Message` enum has 35+ variants. `update()` is a ~400-line single match. Mixes navigation, provider orchestration, download management, settings mutation, and player control.
- **Proposed:** Extract `Message` and `App` update logic into sub-modules per domain: `messages.rs` for the enum, update handlers grouped by concern. Keep `App` struct but delegate to focused methods.
- **Dependencies:** None

### F3: Unused config fields (dead config)
- **File:** `src/config/settings.rs`, `src/download/engine.rs`
- **Category:** DEAD CODE
- **Impact:** HIGH
- **Problem:** Several config fields are stored but never used:
  - `ProcessConfig.use_gpu` — never passed to N_m3u8DL-RE
  - `ProcessConfig.merge_audio` / `merge_subtitle` — never passed to N_m3u8DL-RE
  - `RequestsConfig.timeout` — never used in reqwest client configuration
  - `RequestsConfig.max_retry` — never used anywhere
  - `Season.episodes` — always empty (episodes loaded separately via `get_episodes`)
- **Proposed:** Wire `use_gpu`, `merge_audio`, `merge_subtitle` to download engine args. Wire `timeout` and `max_retry` to reqwest client. Remove `Season.episodes` field.
- **Dependencies:** F2 (settings wiring touches app.rs)

### F4: Silent error swallowing
- **File:** Multiple (`settings.rs:118-121`, `engine.rs:38,108,130`, `app.rs:600+`)
- **Category:** READABILITY / SECURITY
- **Impact:** HIGH
- **Problem:** `let _ = fs::write(...)`, `let _ = progress_tx.send(...)`, `let _ = fs::create_dir_all(...)` throughout. Failures are silently ignored.
- **Proposed:** Use `eprintln!` or a logging crate for critical I/O failures. For channel sends, keep `let _` (receiver may be dropped intentionally).
- **Dependencies:** None

### F5: Streaming community provider is too large (489 lines)
- **File:** `src/provider/streaming_community.rs`
- **Category:** FILE ORGANIZATION / SRP
- **Impact:** HIGH
- **Problem:** Single file handles HTTP client setup, Inertia version fetching, search across languages, HTML parsing, iframe resolution, and VixCloud stream extraction. 6 static regex compilations.
- **Proposed:** Extract stream extraction logic (`extract_stream_from_vixcloud`, `fetch_iframe_url`, regex helpers) into `src/provider/vixcloud.rs`. Keep search + seasons/episodes in `streaming_community.rs`.
- **Dependencies:** None

## MEDIUM

### F6: Redundant clones
- **File:** `src/gui/app.rs` (multiple locations)
- **Category:** PERFORMANCE
- **Impact:** MEDIUM
- **Problem:** Frequent `.clone()` on `self.selected_entry`, `self.provider`, `self.config` inside `update()`. Some clones are necessary for async tasks, but several are avoidable.
- **Proposed:** Use references where possible; for async tasks, clone only what's needed rather than entire structs.
- **Dependencies:** F2

### F7: Magic numbers
- **File:** Multiple files
- **Category:** READABILITY
- **Impact:** MEDIUM
- **Problem:** Hardcoded values: `960.0, 540.0` (player window size), `500` (tick interval ms), `160` (sidebar width), various font sizes (36, 24, 20, 18, 16, 14, 13, 12, 11), padding values. 
- **Proposed:** Extract to named constants in `style.rs` or relevant modules.
- **Dependencies:** None

### F8: No doc comments
- **File:** All files
- **Category:** READABILITY
- **Impact:** MEDIUM
- **Problem:** Zero doc comments on any public type, function, or module. The `Provider` trait methods have no documentation.
- **Proposed:** Add `///` doc comments to all public items, focusing on traits, error types, and config structs.
- **Dependencies:** None

### F9: `ProviderError` manual impl
- **File:** `src/provider/traits.rs`
- **Category:** STACK STANDARD VIOLATIONS
- **Impact:** MEDIUM
- **Problem:** Manual `Display` and `Error` impl. Idiomatic Rust uses `thiserror` for library errors.
- **Proposed:** Replace with `thiserror::Error` derive. Would simplify code and follow Rust conventions.
- **Dependencies:** Adds dependency

### F10: Duplicate stream URL resolution logic
- **File:** `src/gui/app.rs`
- **Category:** DUPLICATION
- **Impact:** MEDIUM
- **Problem:** `PlayMovie`, `PlayEpisode`, `PlayEntry` all contain near-identical async blocks calling `provider.get_stream_url()`. `DownloadMovie` and `DownloadEpisode` also duplicate this pattern with slight variations.
- **Proposed:** Extract a shared `resolve_stream()` helper method on `App`.
- **Dependencies:** F2

## LOW

### F11: `nav_button` function is a free function
- **File:** `src/gui/app.rs:648`
- **Category:** FILE ORGANIZATION
- **Impact:** LOW
- **Problem:** `nav_button()` and `check_provider()` are free functions at the bottom of `app.rs`, not associated with `App`.
- **Proposed:** Move `nav_button` to `style.rs` or a `widgets.rs` module. Move `check_provider` to provider module.
- **Dependencies:** F2

### F12: `search_view` takes individual fields instead of a view model
- **File:** `src/gui/screens/search.rs`
- **Category:** READABILITY
- **Impact:** LOW
- **Problem:** View functions take 3-5 individual parameters. This will grow as features are added.
- **Proposed:** Introduce lightweight view-model structs passed to screen functions.
- **Dependencies:** F2

### F13: `eprintln!` used as logging
- **File:** `src/gui/app.rs`
- **Category:** STACK STANDARD VIOLATIONS
- **Impact:** LOW
- **Problem:** Debug output via `eprintln!` instead of a logging framework.
- **Proposed:** Add `tracing` or `log` crate for structured logging. Low priority — not blocking.
- **Dependencies:** None
