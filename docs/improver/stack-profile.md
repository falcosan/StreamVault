# Stack Profile

## Stack Identity
- **Language:** Rust (edition 2021)
- **Framework:** Iced 0.13 (GUI), Tokio (async runtime)
- **Package Manager:** Cargo
- **Test Runner:** `cargo test` (built-in)
- **Linter:** `cargo clippy`
- **Formatter:** `rustfmt`
- **Build Tool:** Cargo + custom `scripts/package.sh` (macOS .app bundle)
- **CI/CD:** None detected

## Key Dependencies
| Dependency | Version | Purpose |
|---|---|---|
| iced | 0.13 | GUI framework (Elm architecture) |
| tokio | 1 (full) | Async runtime |
| reqwest | 0.12 | HTTP client |
| scraper | 0.22 | HTML parsing |
| serde/serde_json | 1 | Serialization |
| regex | 1 | Regex parsing |
| url | 2 | URL manipulation |
| dirs | 6 | Platform directories |
| uuid | 1 | Unique IDs |
| objc2-* | 0.3-0.6 | macOS native playback (AVPlayer) |

## Official Rust Standards (Deviations)
1. **No tests** — Rust convention: modules have `#[cfg(test)] mod tests`
2. **No `thiserror`/`anyhow`** — Manual `Display`/`Error` impls for `ProviderError` instead of idiomatic crate usage
3. **God file: `app.rs` (661 lines)** — Elm-arch `update()` is a single 400+ line match. Should be decomposed.
4. **`streaming_community.rs` (489 lines)** — Large provider implementation with mixed concerns (HTTP, parsing, URL building)
5. **Magic numbers** — Hardcoded dimensions (960x540), timer intervals (500ms), UI sizes
6. **No `async_trait`** — Manual `Pin<Box<dyn Future>>` return types in `Provider` trait; acceptable but verbose
7. **Platform coupling** — `MainThreadMarker::new().expect()` in `app.rs` will panic on non-macOS; `playback` module is `#[cfg(target_os = "macos")]` but `app.rs` calls it unconditionally
8. **Silent error swallowing** — `let _ = fs::write(...)`, `let _ = progress_tx.send(...)` throughout
9. **No logging framework** — Uses `eprintln!` for debug output
10. **Redundant clones** — Multiple `.clone()` calls where borrows would suffice

## Current Conventions
- Module-per-file with `mod.rs` re-exports
- Snake_case naming throughout (correct)
- `Message` enum as central event type (Iced pattern)
- No doc comments anywhere
- No `#![deny(warnings)]` or `#![warn(clippy::all)]`
