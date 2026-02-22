# Baseline Tests — StreamVault

## Test Command
```
cargo test
```

## Results
```
running 32 tests

config::tests::default_config_has_expected_values ... ok
config::tests::config_path_ends_with_json ... ok
config::tests::default_request_no_proxy ... ok
config::tests::config_dir_ends_with_streamvault ... ok
config::tests::default_process_merge_flags ... ok
config::tests::download_dir_uses_root_path ... ok
config::tests::movie_dir_appends_movie_folder ... ok
config::tests::serie_dir_appends_serie_folder ... ok
providers::tests::display_title_with_year ... ok
providers::tests::display_title_without_year ... ok
config::tests::serde_round_trip ... ok
providers::tests::episode_serde_roundtrip ... ok
providers::tests::fallback_url_is_https ... ok
providers::tests::is_movie_false ... ok
providers::tests::is_movie_true ... ok
providers::tests::media_entry_serde_roundtrip ... ok
providers::tests::provider_error_display ... ok
providers::tests::season_serde_roundtrip ... ok
providers::tests::year_display_with ... ok
providers::tests::year_display_without ... ok
util::tests::build_output_path_movie ... ok
util::tests::build_output_path_series ... ok
util::tests::bundled_bin_dir_returns_option ... ok
util::tests::find_binary_returns_name_for_missing ... ok
util::tests::format_episode_name_replaces_placeholders ... ok
util::tests::format_episode_name_with_show_name ... ok
util::tests::new_progress_starts_queued ... ok
util::tests::sanitize_preserves_unicode ... ok
util::tests::sanitize_replaces_illegal_chars ... ok
util::tests::parse_no_match_leaves_defaults ... ok
util::tests::parse_combined_line ... ok
util::tests::parse_percent ... ok

test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Coverage Breakdown
- **config.rs:** 9 tests — covers Default, paths, serde round-trip
- **providers.rs:** 11 tests — covers data model (MediaEntry, Season, Episode, ProviderError), serde, display. No tests for actual provider logic (network-dependent)
- **util.rs:** 12 tests — covers sanitize_filename, format_episode_name, build_output_path, DownloadProgress parsing. No tests for download execution (requires external binary)
- **app.rs:** 0 tests — no unit tests (UI component)
- **gui.rs:** 0 tests — no unit tests (UI components)
- **main.rs:** 0 tests — entry point only

## Clippy Results
2 warnings (needless borrows in providers.rs:190 and providers.rs:577)
