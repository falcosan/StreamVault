# Changelog

## Task Group 1: Wire Unused Config + Fix Dead Code
- Wired `ProcessConfig.merge_audio` and `merge_subtitle` to N_m3u8DL-RE `--mux-import` flags
- Wired `RequestsConfig.timeout` to reqwest client builder via `StreamingCommunityProvider::with_config`
- Removed dead `Season.episodes` field (always empty, episodes loaded separately)
- Replaced silent `let _ = fs::write(...)` / `let _ = fs::create_dir_all(...)` with `eprintln!` error logging in config save

## Task Group 3: Extract VixCloud Module
- Created `src/provider/vixcloud.rs` (147 lines) — iframe fetching, stream URL extraction, all regex helpers
- Reduced `streaming_community.rs` from 489 → 354 lines
- Removed duplicated regex statics and HTML selectors from `streaming_community.rs`

## Task Group 4: Decompose GOD FILE
- Created `src/gui/messages.rs` (58 lines) — `Screen` and `Message` enums extracted from `app.rs`
- Added `resolve_stream_for_play()` helper to deduplicate `PlayEntry`/`PlayMovie`/`PlayEpisode` handlers
- Moved `nav_button()` from `app.rs` free function to `style.rs` (co-located with other UI helpers)
- Reduced `app.rs` from 661 → 572 lines

## Task Group 5: Magic Numbers
- Extracted `TICK_INTERVAL_MS`, `SIDEBAR_WIDTH`, `BG_SIDEBAR`, `BORDER_CARD`, `CARD_BORDER_RADIUS`, `CARD_BORDER_WIDTH` as named constants
- Extracted `PLAYER_DEFAULT_WIDTH`/`PLAYER_DEFAULT_HEIGHT` in `native_player.rs`
- Replaced hardcoded values across `app.rs`, `style.rs`, `native_player.rs`, `player.rs`, `downloads.rs`

## Skipped (per user request)
- Doc comments (Task Group 6)
- Platform cfg guards (macOS-only app — guards would be invasive for no benefit)
- `thiserror` adoption (adds dependency, manual impl works fine)
- View-model structs (low impact)
- Structured logging (desktop app, eprintln is adequate)
