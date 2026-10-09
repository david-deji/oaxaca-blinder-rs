# Changelog

## [Unreleased]

### Changed (0119-MERIDIAN S1, 2026-10-09)
- The WASM build recipe lives once in `scripts/wasm-recipe.sh`, sourced by `scripts/build-wasm.sh` and by CI.
  The sequential pass maps the optional `rust-src` location back to `/rustc/<commit>`, so the raw blob no
  longer depends on whether that component is installed. Cause found: CI (no `rust-src`) embedded
  `/rustc/1159e78c.../library/...` std paths, the author's machine (with `rust-src`) embedded
  `/rustup/toolchains/1.90.0-.../lib/rustlib/src/rust/library/...`; 33 paths and the `.llvm.<hash>` symbol
  suffixes differed, so the sequential hash never matched.
- Every cargo build, run, test and clippy in CI takes `--locked`. `build-wasm.sh` has a preflight that fails
  on a missing toolchain, component, target or wasm-bindgen version and prints host triple and toolchain dirs.
- Raw sequential and threaded blobs, their sha256 and the toolchain description are uploaded as the `wasm-raw`
  artifact before any baseline compare, and the compares no longer stop each other.
- Baselines re-recorded from the proven recipe (2026-10-09). Old: sequential `32b41510563c...`, threaded
  `59988ec0d431...` (engine `cf7a2af`). New: sequential `e70dcee6bb56...`, threaded `a80a9d807963...`; both
  changed because the S4 dependency bumps changed the bytes, the sequential one also because of the recipe.

## [0.2.2] - 2025-12-18

### Added
- Added `allow(dead_code)` to various structs (`OaxacaBuilder`, `ProbitResult`, `LogitResult`, `OlsResult`) to reduce noise in API usage.
- Added `diagnostics` module (VIF calculation) with `polars` integration.
- Added `report` command to CLI for generating HTML summaries.

### Fixed
- Fixed lint warnings for `unused_mut`, deprecated functions, and unused imports across the codebase.
- Resolved `clippy::useless_vec` warnings in tests.
- Fixed dependency configurations.
