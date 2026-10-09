# Changelog

## [Unreleased]

### Changed (0119-MERIDIAN S2 + S3, 2026-10-09)
- CI jobs can no longer silence each other. `wasm-verify` is split into `wasm-seq`, `wasm-threaded`, `wasm-repro`
  and `native-baseline`; `browser-parity` runs after `wasm-threaded` and `native-baseline` even when a hash
  compare failed (only a cancelled build skips it). A final `gate` job needs every gating job and fails unless
  each is `success`; skipped and cancelled count as failures. `scripts/check-ci-invariants.py` (run by `gate`)
  fails on `continue-on-error`, a job-level `if` on a gating job, a missing `timeout-minutes`, a `gate` whose
  `needs` drifted, an unpinned action, `npm install`, a download piped into a shell, or `paths`/`paths-ignore`.
- Workflows: top-level `permissions: contents: read` (release.yml raises it for its one job), every action pinned
  to a full commit SHA with the tag in a comment, `npm ci` instead of `npm install`, the release job installs a
  pinned, sha256-verified syft instead of piping an install script from a branch into a shell, and
  `.github/dependabot.yml` keeps the pins current.
- The CI artifact `wasm-raw` is now `wasm-raw-seq` and `wasm-raw-threaded`; the double-build blobs are `wasm-repro`.
- Browser parity compares numbers: the native/wasm comparator treats an absent key as equal only when the other
  side is literally `null` and only at `$.unexplained_standard_error` and `$.unresolved_row_keys` (the
  `serde_wasm_bindgen` drop of `None`); number-vs-absent stays a mismatch; every numeric leaf compares at 1e-6.
  It returns the numeric leaf count; the spec asserts a floor (18) and 18 named required paths. The comparator and
  the baseline stamp have `node --test` unit tests that run in CI before Playwright.
- The native baseline freshness check compares a hash written by the generator step (engine source files plus
  the baseline's sha256) instead of file mtimes. Generate with `node verification/browser-parity/gen-native-baseline.mjs`
  (`npm test` does it).

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
