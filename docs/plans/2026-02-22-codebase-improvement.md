# Codebase Improvement Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform StreamVault from a working prototype into a well-structured, idiomatic Rust codebase following SOLID principles, DRY, and Dioxus best practices.

**Architecture:** Split the monolithic providers.rs (1095 lines) into a providers module with separate files per provider. Extract CSS from gui.rs. Eliminate error-handling boilerplate via `From` impls. Add `async_trait` to remove Pin/Box noise. Extract shared constants.

**Tech Stack:** Rust 2021, Dioxus 0.6 (desktop), Tokio, reqwest, scraper, serde_json, regex, async-trait (new dep)

**Worktree:** `/Users/danielefalchetti/Desktop/streaming/StreamVault/.worktrees/codebase-improvement`
**Branch:** `improve/codebase-overhaul`
**Baseline:** 32 tests passing, 2 clippy warnings

---

## Task Group A: Stack Standards / Quick Fixes (Scope: S)

### Task A1: Fix clippy warnings

**Files:**
- Modify: `src/providers.rs:190`
- Modify: `src/providers.rs:577`

**Step 1:** Remove unnecessary `&` on lines 190 and 577:
```rust
// Line 190: change
.get(&self.base_url())
// to
.get(self.base_url())

// Line 577: change
.get(&self.base_url())
// to
.get(self.base_url())
```

**Step 2:** Run `cargo clippy` — expected: 0 warnings

**Step 3:** Run `cargo test` — expected: 32 tests pass

**Step 4:** Commit: `fix: resolve clippy needless-borrow warnings`

---

### Task A2: Derive Default for AppConfig

**Files:**
- Modify: `src/config.rs:5-23`

**Step 1:** Replace manual Default impl with derive:
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub output: OutputConfig,
    pub download: DownloadConfig,
    pub process: ProcessConfig,
    pub requests: RequestsConfig,
}
```
Remove the manual `impl Default for AppConfig` block (lines 12-23).

**Step 2:** Run `cargo test` — expected: 32 tests pass

**Step 3:** Commit: `refactor: derive Default for AppConfig`

---

### Task A3: Remove unnecessary #[inline] annotations

**Files:**
- Modify: `src/config.rs:14,112,156`
- Modify: `src/providers.rs:58`
- Modify: `src/util.rs:10,251`

**Step 1:** Remove `#[inline]` from these functions:
- `config.rs`: `AppConfig::default()` (line 14, removed in A2), `config_dir()` (line 112), `download_dir()` (line 156)
- `providers.rs`: `display_title()` (line 58)
- `util.rs`: `bundled_bin_dir()` (line 10), `format_episode_name()` (line 251)

Keep `#[inline]` on genuinely trivial functions: `is_movie()`, `year_display()`, `config_path()`, `movie_dir()`, `serie_dir()`, `DownloadProgress::new()`, `DownloadEngine::new()`, `sanitize_filename()`.

**Step 2:** Run `cargo test` — expected: 32 tests pass

**Step 3:** Commit: `refactor: remove unnecessary inline annotations`

---

## Task Group B: Structural Changes — Split providers.rs (Scope: L)

### Task B1: Create providers module directory structure

**Files:**
- Create: `src/providers/mod.rs`
- Create: `src/providers/models.rs`
- Create: `src/providers/streaming_community.rs`
- Create: `src/providers/raiplay.rs`
- Delete: `src/providers.rs`

**Step 1:** Create `src/providers/models.rs` containing:
- `MediaType` enum (lines 12-15)
- `MediaEntry` struct (lines 17-29)
- `Season` struct (lines 31-36)
- `Episode` struct (lines 38-44)
- `StreamUrl` struct (lines 46-49)
- `MediaEntry` impl block (lines 52-70)

**Step 2:** Create `src/providers/mod.rs` containing:
- `mod models;` + `mod streaming_community;` + `mod raiplay;`
- Re-exports: `pub use models::*;`
- Shared constant: `pub(crate) const USER_AGENT: &str = "Mozilla/5.0 ...";`
- `ProviderError` enum + `Display` + `Error` impls
- `ProviderResult<T>` alias
- `From<reqwest::Error>` impl for ProviderError (maps to Network)
- `From<serde_json::Error>` impl for ProviderError (maps to Parse)
- `Provider` trait definition (using `async_trait`)
- `pub(crate) fn dedup_by_id(entries: impl IntoIterator<Item = MediaEntry>) -> Vec<MediaEntry>`
- Re-exports: `pub use streaming_community::StreamingCommunityProvider;` + `pub use raiplay::RaiPlayProvider;`
- Tests module (existing tests from providers.rs:981-1095)

**Step 3:** Create `src/providers/streaming_community.rs` containing:
- `StreamingCommunityProvider` struct and all its methods
- Uses `super::USER_AGENT`, `super::ProviderError`, `super::dedup_by_id`
- Uses `?` instead of `.map_err()` where From impls cover it

**Step 4:** Create `src/providers/raiplay.rs` containing:
- `RaiPlayProvider` struct and all its methods
- `raiplay_hash()` and `raiplay_abs_url()` helper functions (private to this module)
- Uses `super::USER_AGENT`, `super::ProviderError`, `super::dedup_by_id`
- Uses `?` instead of `.map_err()` where From impls cover it

**Step 5:** Delete `src/providers.rs`

**Step 6:** Run `cargo test` — expected: 32 tests pass

**Step 7:** Run `cargo clippy` — expected: 0 warnings

**Step 8:** Commit: `refactor: split providers.rs into module with separate files per provider`

---

## Task Group C: Structural Changes — Extract CSS (Scope: S)

### Task C1: Extract CSS to separate module

**Files:**
- Create: `src/style.rs`
- Modify: `src/gui.rs`
- Modify: `src/main.rs`

**Step 1:** Create `src/style.rs` containing:
- `pub const GLOBAL_CSS: &str = r#"..."#;` (moved from gui.rs:45-251)
- `pub const LOGO_SVG: &str = r##"..."##;` (moved from gui.rs:43)

**Step 2:** Update `src/gui.rs`:
- Remove `GLOBAL_CSS` and `LOGO_SVG` constants
- Add `use crate::style::{GLOBAL_CSS, LOGO_SVG};` (only if used directly — LOGO_SVG is used in gui.rs)
- Actually: keep importing via `crate::style`

**Step 3:** Update `src/main.rs`:
- Add `mod style;`

**Step 4:** Update `src/app.rs`:
- Change `gui::GLOBAL_CSS` to `crate::style::GLOBAL_CSS`

**Step 5:** Run `cargo test` — expected: 32 tests pass

**Step 6:** Commit: `refactor: extract CSS and SVG assets to style module`

---

## Task Group D: DRY — Eliminate Error Boilerplate (Scope: M)

This is done as part of Task B1 (the From impls and `?` operator usage). If B1 is already complete, verify:

### Task D1: Verify From impls eliminate map_err

**Step 1:** Grep for remaining `.map_err(|e| ProviderError::Network` and `.map_err(|e| ProviderError::Parse` in `src/providers/`.

Expected: Only a few remaining cases where the error source isn't `reqwest::Error` or `serde_json::Error` (e.g., `url::ParseError` in `extract_stream_url`).

**Step 2:** If any `reqwest::Error` or `serde_json::Error` map_err calls remain, replace with `?`.

**Step 3:** Run `cargo test` — expected: 32 tests pass

**Step 4:** Commit if changes made: `refactor: replace remaining manual error conversions with ?`

---

## Task Group E: Design Pattern — async_trait (Scope: M)

### Task E1: Add async_trait dependency and refactor Provider trait

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/providers/mod.rs`
- Modify: `src/providers/streaming_community.rs`
- Modify: `src/providers/raiplay.rs`

**Step 1:** Add to Cargo.toml dependencies:
```toml
async-trait = "0.1"
```

**Step 2:** Rewrite Provider trait in `src/providers/mod.rs`:
```rust
use async_trait::async_trait;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn init(&self) {}
    async fn search(&self, query: &str) -> ProviderResult<Vec<MediaEntry>>;
    async fn get_seasons(&self, entry: &MediaEntry) -> ProviderResult<Vec<Season>>;
    async fn get_episodes(&self, entry: &MediaEntry, season: u32) -> ProviderResult<Vec<Episode>>;
    async fn get_stream_url(&self, entry: &MediaEntry, episode: Option<&Episode>, season: Option<u32>) -> ProviderResult<StreamUrl>;
    async fn get_catalog(&self) -> ProviderResult<Vec<MediaEntry>>;
}
```

**Step 3:** Update both provider impls to use `#[async_trait]` and regular `async fn` instead of `Pin<Box<...>>`.

**Step 4:** Run `cargo test` — expected: 32 tests pass

**Step 5:** Run `cargo clippy` — expected: 0 warnings

**Step 6:** Commit: `refactor: replace manual Pin/Box async with async_trait`

---

## Task Group F: DRY — Extract Shared Constants (Scope: S)

### Task F1: Extract shared user-agent constant

This is already handled in Task B1 via `USER_AGENT` constant in `src/providers/mod.rs`.

Verify it's used in both `streaming_community.rs` and `raiplay.rs`.

---

## Execution Order

1. **A1** → Fix clippy (S)
2. **A2** → Derive Default (S)
3. **A3** → Remove #[inline] (S)
4. **B1** → Split providers module (L) — includes From impls, dedup helper, USER_AGENT constant
5. **E1** → Add async_trait (M)
6. **C1** → Extract CSS to style module (S)
7. **D1** → Verify error elimination (S)

Total: 7 tasks, estimated scope S-L.
