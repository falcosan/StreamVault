# Findings — StreamVault

## CRITICAL

### F1. SOLID: God Component app.rs (SRP violation)
- **File:** src/app.rs:10-433
- **Problem:** Single App() function is 424 lines with 30+ signals, 10+ event handler closures, catalog loading, search logic, download logic, play logic, and rendering — all in one function
- **Category:** SOLID Violations
- **Proposed Transformation:** Extract state into logical groups: AppState (screen/history), MediaState (catalog/search/selection), PlayerState, DownloadState. Extract event handlers into separate functions or a controller module.

### F2. FILE ORGANIZATION: providers.rs is 1095 lines with mixed concerns
- **File:** src/providers.rs:1-1095
- **Problem:** Contains data models (MediaEntry, Season, Episode, StreamUrl), error types, Provider trait, AND two full provider implementations (StreamingCommunity ~340 lines, RaiPlay ~340 lines). Violates SRP.
- **Category:** File Organization
- **Proposed Transformation:** Split into:
  - `src/providers/mod.rs` — trait, error types, re-exports
  - `src/providers/models.rs` — MediaEntry, Season, Episode, StreamUrl, MediaType
  - `src/providers/streaming_community.rs` — StreamingCommunityProvider
  - `src/providers/raiplay.rs` — RaiPlayProvider

### F3. FILE ORGANIZATION: gui.rs mixes 250-line CSS with UI components
- **File:** src/gui.rs:45-251 (CSS), 253-653 (components)
- **Problem:** 250 lines of CSS as an inline string constant mixed with 9 components. Hard to maintain.
- **Category:** File Organization
- **Proposed Transformation:** Extract CSS to `src/style.rs` (or a const module). Keep components in gui.rs.

## HIGH

### F4. DUPLICATION: `.map_err()` for error conversion repeated 27+ times
- **File:** src/providers.rs — lines 193, 197, 220, 224, 329, 333, 356, 360, 384, 388, 529, 533, 580, 584, 700, 704, 713, 716, 735, 739, 742, 789, 793, 821, 825, 885, 889
- **Problem:** `.map_err(|e| ProviderError::Network(e.to_string()))` (18 instances) and `.map_err(|e| ProviderError::Parse(e.to_string()))` (9 instances) repeated throughout
- **Category:** Duplication
- **Proposed Transformation:** Implement `From<reqwest::Error>` for `ProviderError` (maps to Network) and `From<serde_json::Error>` for `ProviderError` (maps to Parse). Then use `?` directly.

### F5. DUPLICATION: Identical user-agent string in both providers
- **File:** src/providers.rs:136, 646
- **Problem:** Same 120-char UA string copy-pasted in StreamingCommunityProvider::with_config() and RaiPlayProvider::with_config()
- **Category:** Duplication
- **Proposed Transformation:** Extract to a shared constant `const USER_AGENT: &str = "..."`.

### F6. DUPLICATION: HashSet dedup pattern repeated 3 times
- **File:** src/providers.rs:468-477, 587-610, 798-804
- **Problem:** `let mut seen = HashSet::new(); ... if seen.insert(e.id) { vec.push(e); }` repeated 3 times with slight variations
- **Category:** Duplication
- **Proposed Transformation:** Extract `fn dedup_by_id(entries: impl IntoIterator<Item = MediaEntry>) -> Vec<MediaEntry>` utility function.

### F7. DESIGN PATTERN: Manual async trait boilerplate
- **File:** src/providers.rs:92-118 (trait), 455-614 (SC impl), 766-978 (RP impl)
- **Problem:** Every trait method returns `Pin<Box<dyn Future<Output = ...> + Send + '_>>` and every impl wraps in `Box::pin(async move { ... })`. ~50 lines of pure boilerplate.
- **Category:** Design Pattern Opportunities
- **Proposed Transformation:** Use the `async_trait` crate (already in dependency tree via other crates) to eliminate Pin/Box boilerplate.

### F8. STACK STANDARDS: Clippy warnings unfixed
- **File:** src/providers.rs:190, 577
- **Problem:** `the borrowed expression implements the required traits` — unnecessary `&` before `self.base_url()`
- **Category:** Stack Standard Violations
- **Proposed Transformation:** Remove unnecessary `&` → `self.client.get(self.base_url())`

## MEDIUM

### F9. READABILITY: extract_stream_url is 82 lines
- **File:** src/providers.rs:371-452
- **Problem:** Long function doing script extraction + 4 regex captures + URL reconstruction
- **Category:** Readability
- **Proposed Transformation:** Extract regex capture logic into a helper.

### F10. READABILITY: resolve_stream is 77 lines
- **File:** src/providers.rs:687-763
- **Problem:** Long function with multi-step JSON fetch + URL transformation
- **Category:** Readability
- **Proposed Transformation:** Extract JSON URL resolution and relinker call into helpers.

### F11. STACK STANDARDS: AppConfig Default impl delegates manually
- **File:** src/config.rs:12-23
- **Problem:** `AppConfig::default()` just calls `SubConfig::default()` for each field — could derive Default
- **Category:** Stack Standard Violations
- **Proposed Transformation:** Add `#[derive(Default)]` to AppConfig and remove manual impl.

### F12. STACK STANDARDS: Unnecessary #[inline] annotations
- **File:** src/config.rs:14,112,156; src/util.rs:10,251; src/providers.rs:58
- **Problem:** `#[inline]` on functions that perform allocations, I/O, or multiple operations — compiler won't benefit
- **Category:** Stack Standard Violations
- **Proposed Transformation:** Remove `#[inline]` from non-trivial functions. Keep only on genuinely trivial ones.

### F13. DUPLICATION: Hash functions for names
- **File:** src/gui.rs:33-36 (name_hash), src/providers.rs:624-627 (raiplay_hash)
- **Problem:** Two separate hash functions with nearly identical logic (wrapping multiply + add), differing only in multiplier (37 vs 31) and output type (usize vs u64)
- **Category:** Duplication
- **Proposed Transformation:** These serve different purposes (color selection vs ID generation), so this is acceptable. No change needed.

## LOW

### F14. READABILITY: Unused _season parameters
- **File:** src/providers.rs:558, 954
- **Problem:** `_season: Option<u32>` parameter declared but unused in both get_stream_url impls
- **Category:** Readability
- **Proposed Transformation:** Acceptable — required by trait signature. No change needed.

### F15. STACK STANDARDS: Inconsistent LazyLock vs OnceLock
- **File:** src/providers.rs:174, 361, 372 (LazyLock) vs 374-377, 750 (OnceLock)
- **Problem:** Two different lazy-init patterns used in the same file
- **Category:** Stack Standard Violations
- **Proposed Transformation:** Both are used correctly for their purposes (LazyLock for computed values, OnceLock for get_or_init pattern). Standardize on LazyLock for both where possible.
