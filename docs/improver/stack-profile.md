# Stack Profile — StreamVault

## Stack Identity
- **Language:** Rust (edition 2021)
- **Framework:** Dioxus 0.6 (desktop mode via WebView/Wry)
- **Async runtime:** Tokio (full features)
- **HTTP client:** reqwest 0.12 (json, cookies)
- **HTML parsing:** scraper 0.25
- **Regex:** regex 1
- **Serialization:** serde 1 + serde_json 1
- **URL handling:** url 2
- **UUID:** uuid 1 (v4)
- **Dirs:** dirs 6
- **Package manager:** Cargo
- **Test runner:** `cargo test` (32 tests, all passing)
- **Linter:** clippy (2 warnings: needless borrows)
- **Formatter:** rustfmt (clean)
- **Build tool:** Cargo
- **CI/CD:** None
- **Packaging:** Custom shell script (`scripts/package.sh`) for macOS .app bundle

## Official Rust/Dioxus Standards
- Modules should follow single-responsibility
- Prefer `async fn` in traits when possible (Rust 2024 stabilizes RPITIT)
- Use `thiserror` or `From` impls for error conversions instead of manual `.map_err()` chains
- Keep components small and focused (Dioxus best practice)
- CSS should be in separate files or scoped per-component, not monolithic strings
- Use `clippy::pedantic` for stricter code quality
- Idiomatic: derive Default when all fields have suitable defaults
- Idiomatic: use `impl From<T>` for error type conversions

## Current Conventions
- No rustfmt.toml or clippy.toml
- `#[inline]` annotations on trivial functions
- `eprintln!("[StreamVault]...")` for logging (no structured logging)
- Manual `Pin<Box<dyn Future<...>>>` for async trait methods
- Global static selectors using `LazyLock` and `OnceLock` (inconsistent)
- CSS as a massive `&str` constant in gui.rs
- All state management in a single App component with 30+ signals

## Deviation List
1. **God component:** `app.rs` App() has 30+ signals and 10+ closures in one function (433 lines)
2. **God file:** `providers.rs` has 1095 lines with two unrelated providers
3. **Mixed concerns in gui.rs:** 250-line CSS string + all UI components in one file
4. **Manual async trait:** Uses `Pin<Box<dyn Future>>` instead of async trait patterns
5. **No `From` impl for errors:** Repeated `.map_err(|e| ProviderError::Network(e.to_string()))` throughout
6. **Duplicated user-agent:** Same UA string hardcoded in both providers
7. **Inconsistent static init:** Mix of `LazyLock` and `OnceLock` for the same purpose
8. **Unnecessary `#[inline]`:** On functions that the compiler would inline anyway
9. **Clippy warnings:** 2 needless borrow warnings unfixed
10. **AppConfig Default impl delegates manually** instead of deriving

## Do Not Touch
- `Cargo.lock` (lock file)
- `target/` (build artifacts)
- `assets/` (binary assets)
- `resources/Info.plist` (app metadata)
- `resources/AppIcon.icns` (binary asset)
- `scripts/` (build/update scripts)
