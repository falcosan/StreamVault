# StreamVault Codebase Improvement Report

## Project Summary
**StreamVault** — macOS desktop app for streaming, downloading, and watching media from StreamingCommunity. Built with Rust + Iced GUI + AVPlayer, using N_m3u8DL-RE for HLS downloads.

**Codebase:** 20 source files, 2,491 → 2,765 lines (net +274 from tests and extracted modules)

## Stack Standards Applied
- Rust 2021 edition conventions
- Inline `#[cfg(test)] mod tests` for unit testing
- Zero clippy warnings
- Module-per-concern file organization
- Named constants over magic numbers

## Test Results

| Metric | Before | After |
|--------|--------|-------|
| Tests | 0 | 27 |
| Passing | — | 27 |
| Clippy warnings | 0 | 0 |

### Test Coverage Added
- `config::settings` — 7 tests (defaults, paths, serde round-trip)
- `download::progress` — 6 tests (regex parsing, combined output)
- `download::engine` — 7 tests (sanitize, episode naming, output paths)
- `provider::models` — 6 tests (type detection, display helpers)
- `util::binary` — 1 test (fallback behavior)

## Transformations Performed

### Dead Code Removed
- `Season.episodes: Vec<Episode>` field (always empty)
- Inlined `DEFAULT_BASE_URL` usage, removed unused `with_base_url()` wrapper

### Files Split
| Original | New Module | Lines Moved |
|----------|-----------|-------------|
| `streaming_community.rs` (489→354) | `vixcloud.rs` (147) | ~135 |
| `app.rs` (661→572) | `messages.rs` (58) | ~89 |

### Config Wiring Fixed
- `merge_audio` / `merge_subtitle` → N_m3u8DL-RE `--mux-import` args
- `timeout` → reqwest client builder
- Config I/O errors → logged via `eprintln!` instead of silently swallowed

### DRY Consolidation
- 3 play handlers (`PlayEntry`, `PlayMovie`, `PlayEpisode`) → single `resolve_stream_for_play()` method
- `nav_button()` moved from `app.rs` free function to `style.rs` alongside other UI helpers

### Magic Numbers → Constants
- `TICK_INTERVAL_MS`, `SIDEBAR_WIDTH`, `PLAYER_DEFAULT_WIDTH/HEIGHT`
- `BG_SIDEBAR`, `BORDER_CARD`, `CARD_BORDER_RADIUS`, `CARD_BORDER_WIDTH`

## Architecture Changes
- Provider module now has 4 files with clear responsibilities: models, traits, streaming_community (search/seasons/episodes), vixcloud (stream extraction)
- GUI module has 4 files: app (state + update + view), messages (types), style (constants + helpers), screens/*

## Findings Not Fixed
| Finding | Reason |
|---------|--------|
| F1: Platform cfg guards | App is macOS-only; invasive guards for no practical benefit |
| F8: Doc comments | Skipped per user request |
| F9: thiserror adoption | Adds dependency; manual impl is adequate |
| F12: View-model structs | Low impact at current codebase size |
| F13: Structured logging | eprintln sufficient for desktop app |

## Risks and Notes
- The app only compiles on macOS due to objc2/AVPlayer dependencies
- N_m3u8DL-RE must be available at runtime for downloads to work
- `--mux-import audio/subtitle` flags added — verify they match the N_m3u8DL-RE version in use
