# Codebase Improvement Plan — 2026-02-21

## Execution Order

Ordered: dead code/wiring first → structural changes → SOLID/DRY → stack standards.

---

### Task Group 1: Wire Unused Config + Fix Dead Code

**T1.1** Wire `ProcessConfig` fields to download engine (F3)
- **Before:** `use_gpu`, `merge_audio`, `merge_subtitle` stored but ignored
- **After:** Passed as N_m3u8DL-RE CLI args (`--use-gpu`, `--select-audio`, `--select-subtitle` merge flags)
- **Files:** `src/download/engine.rs`
- **Scope:** S
- **Verify:** `cargo test && cargo clippy`

**T1.2** Wire `RequestsConfig` to reqwest client and engine (F3)
- **Before:** `timeout` and `max_retry` stored but not used
- **After:** `timeout` used in reqwest client builder, `max_retry` passed to `--download-retry-count` where applicable
- **Files:** `src/provider/streaming_community.rs`, `src/download/engine.rs`
- **Scope:** S
- **Verify:** `cargo test && cargo clippy`

**T1.3** Remove `Season.episodes` field (F3)
- **Before:** `Season` has `episodes: Vec<Episode>` always empty
- **After:** Field removed, construction sites updated
- **Files:** `src/provider/models.rs`, `src/provider/streaming_community.rs`
- **Scope:** S
- **Verify:** `cargo test && cargo clippy`

**T1.4** Fix silent error swallowing for config I/O (F4)
- **Before:** `let _ = fs::write(...)`, `let _ = fs::create_dir_all(...)`
- **After:** Log errors via `eprintln!` for I/O operations
- **Files:** `src/config/settings.rs`
- **Scope:** S
- **Verify:** `cargo test && cargo clippy`

**Checkpoint:** commit "Wire unused config fields, remove dead code"

---

### Task Group 2: Platform Guard (F1)

**T2.1** Add cfg guards to app.rs playback code
- **Before:** Playback code compiles only on macOS but `app.rs` doesn't gate it
- **After:** All playback-related Message handling and imports wrapped in `#[cfg(target_os = "macos")]` with graceful fallback
- **Files:** `src/gui/app.rs`
- **Scope:** M
- **Verify:** `cargo test && cargo clippy`

**Checkpoint:** commit "Add platform guards for macOS-only playback"

---

### Task Group 3: Structural — Extract Provider VixCloud (F5)

**T3.1** Extract VixCloud stream extraction into `src/provider/vixcloud.rs`
- **Before:** `streaming_community.rs` 489 lines with mixed concerns
- **After:** ~200 lines moved to `vixcloud.rs` (iframe fetching, stream URL extraction, regex helpers). `streaming_community.rs` delegates to it.
- **Files:** `src/provider/streaming_community.rs` (split), `src/provider/vixcloud.rs` (new), `src/provider/mod.rs`
- **Scope:** M
- **Verify:** `cargo test && cargo clippy`

**Checkpoint:** commit "Extract VixCloud stream extraction module"

---

### Task Group 4: Structural — Decompose GOD FILE (F2, F10, F11)

**T4.1** Extract `Message` enum to `src/gui/messages.rs`
- **Before:** 35+ variant enum in `app.rs`
- **After:** Standalone `messages.rs` re-exported from `gui/mod.rs`
- **Files:** `src/gui/app.rs`, `src/gui/messages.rs` (new), `src/gui/mod.rs`, all screens
- **Scope:** M
- **Verify:** `cargo test && cargo clippy`

**T4.2** Extract duplicate stream resolution into helper method (F10)
- **Before:** 5 near-identical async blocks for stream resolution
- **After:** Single `resolve_stream_url()` method on `App`
- **Files:** `src/gui/app.rs`
- **Scope:** S
- **Verify:** `cargo test && cargo clippy`

**T4.3** Move `nav_button` to `style.rs`, `check_provider` to provider (F11)
- **Before:** Free functions at bottom of `app.rs`
- **After:** `nav_button` in `style.rs`, `check_provider` as associated function on provider
- **Files:** `src/gui/app.rs`, `src/gui/style.rs`, `src/provider/traits.rs`
- **Scope:** S
- **Verify:** `cargo test && cargo clippy`

**Checkpoint:** commit "Decompose app.rs: extract messages, helpers, widgets"

---

### Task Group 5: Magic Numbers + Constants (F7)

**T5.1** Extract magic numbers to named constants
- **Before:** Hardcoded dimensions, sizes, intervals scattered throughout
- **After:** Constants in `style.rs` (UI) and relevant modules (timers, player)
- **Files:** `src/gui/style.rs`, `src/gui/app.rs`, `src/playback/native_player.rs`
- **Scope:** S
- **Verify:** `cargo test && cargo clippy`

**Checkpoint:** commit "Extract magic numbers to named constants"

---

### Task Group 6: Doc Comments (F8)

**T6.1** Add doc comments to public API
- **Before:** Zero doc comments
- **After:** `///` docs on all public types, traits, and key methods
- **Files:** All public modules
- **Scope:** M
- **Verify:** `cargo test && cargo clippy`

**Checkpoint:** commit "Add documentation comments to public API"

---

## Skipped (LOW priority / future work)
- F9: `thiserror` adoption — adds a dependency, current impl works fine
- F12: View-model structs for screen functions — nice-to-have, low impact
- F13: Structured logging — `eprintln!` is adequate for a desktop app at this size
