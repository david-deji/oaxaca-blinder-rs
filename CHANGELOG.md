# Changelog

## [Unreleased]

### Changed, BREAKING (0120-MERIDIAN S1-S4, Track E core, 2026-10-09)
- **Per-level driver rows no longer depend on which level sorts first.** The engine (WASM `decompose` and
  `verify_adjustments`, MCP) and the CLI (`run`, `report`) normalise every categorical predictor on every run:
  each level, the alphabetically first one included, is a deviation from the pooled-sample share-weighted average of
  the levels (population-share, founder decision D1). All k levels are emitted; the intercept absorbs the average.
  Aggregates, remedy, verify, defensibility and frontier numbers are unchanged. The library keeps `normalize()`
  opt-in (raw by default); `normalize_all_categoricals()` and `normalization_convention(PopulationShare |
  EqualShare)` are new, `EqualShare` being what Stata `categorical()`, R `oaxaca` and `ddecompose` print.
  `oaxaca-cli --normalization population-share|equal-share|none`. Surface table: `docs/NORMALIZATION.md`.
- **One restriction per run.** The share vector is keyed by variable and level name and built once per pass from the
  pooled analysed rows (sum of weights when a weights column is set); it is applied to beta_A, beta_B, the pooled fit
  and the weighted mix alike. A bootstrap replicate builds its own from its pooled resample. The unused `_x_mean`
  parameter of `normalize_categorical_coefficients` and the `category_counts` plumbing are gone. With a Heckman
  selection model normalisation is skipped on A, B and pooled together and the run says so.
- **Three-fold is computed from the treatment-coded vectors**, in the point estimate and in every replicate. Under
  normalisation it used to sum to 79% of the gap and move with the base level; it now equals R `oaxaca`
  `threefold$overall` (E, C, I) to 1e-10.
- **`reference_coefficients` is required and exact** at WASM `decompose` / `verify_adjustments` and the MCP tools
  `forensic_decomposition` / `verify_adjustments`: `GroupA`, `GroupB`, `Pooled`, `PooledNoIndicator`, `Weighted`.
  Absent, `"pooled"`, `"Neumark"` or anything else is an error (`UNKNOWN_REFERENCE_COEFFICIENTS`); the silent
  fallback to `Pooled` is removed. The MCP schema requires the field and describes each counterfactual.
  The frontier and defensibility entry points still ignore the field.
- **New `ReferenceCoefficients::PooledNoIndicator`** (Neumark 1988, Stata `omega`, R `oaxaca` weight -1).
  `ReferenceCoefficients::Neumark` is `#[deprecated]` and keeps computing `Pooled` (pooled WITH a group indicator, whose
  coefficient is the unexplained gap; Jann 2008 `pooled`), so its number is unchanged. `GroupA` / `GroupB` doc
  comments now say which group is which (A = the compared, non-reference group).
- **The intercept has one name.** `oaxaca_blinder::INTERCEPT_NAME` (`"__ob_intercept__"`) replaces ten string
  literals; `pay_equity_engine::intercept_token()` (and the WASM function of the same name) returns it. The
  unreachable `"Base Rate (Intercept)"` branch in `optimize_inner` and `check_defensibility_inner` is removed.
  `model_coefficients` and per-employee `contributions` are documented as RAW treatment-coded model terms, never
  drivers.
- **`run_metadata` gains** `normalization` (convention, share basis, applied, the point sample's shares),
  `reference_coefficients_used`, `engine_version`, `method` (`oaxaca-blinder-mean` | `rif-quantile`) and
  `bootstrap_discard_levels` (`variable=level` entries for levels that cost replicates). All are omitted when unset, so a
  raw library run serializes to the bytes it always did. `reference_coefficients_used`, `engine_version` and `method` are stamped by the engine layer only.
- New library method `OaxacaBuilder::rif_outcome_frame(tau)` and example `emit_rif_fixture` export the per-group RIF
  outcome so an external package can check the quantile path's normalisation.
- Crate versions: `oaxaca_blinder` 0.3.0, `pay-equity-engine` 0.2.0.

### Added (0120-MERIDIAN oracles)
- `verification/gen_norm_goldens.R` + `regen_norm_goldens.sh`: base-R `lm()` weighted-effect-coding refit (population
  share), `ddecompose(normalize_factors = TRUE)` and R `oaxaca` (equal share) goldens in `norm_goldens_r.json`,
  with the sha256 of the generator and of every fixture; the Rust tests refuse a stale golden. Fixtures:
  `norm_skewed_fixture.csv` (levels 60/30/9/1 percent, mixes differing by 20+ points between the groups, an 8-person
  department, integer weights, blank Tenure cells), `norm_balanced_fixture.csv` (every factor balanced in the pooled
  sample, unbalanced inside each group), `norm_skewed_rif.csv` (the engine's RIF columns).
- `trust_goldens_r.json` gains the block `quantile_detail_normalized`; every existing block is byte-identical (the
  generator was rerun and diffed). `0118-null-free-golden.txt` is NOT regenerated: its comparator is field-scoped.

### Added (0119-MERIDIAN S7 + S6, 2026-10-09)
- `scripts/build-wasm.sh --verify` builds both raw blobs and compares them with `git show HEAD:engine/*.sha256`;
  it never writes a baseline, never publishes, and exits 1 on a mismatch (result in `target/wasm-verify.json`).
  `--record` is now the only mode that writes `engine/*.sha256`; the default build no longer rewrites them.
- Publishing writes `engine-manifest.json` beside each shipped blob: raw sha256, shipped `_bg.wasm` sha256,
  engine commit, dirty flag. wasm-bindgen output is not deterministic (two runs over one raw blob differ in
  123 bytes of the `_bg.wasm`; the glue JS is identical), so the shipped blob can only be checked against its
  manifest, never against a committed hash. The app's freshness gate compares content instead of mtimes.
- The publish list no longer copies a stale `pay_equity_engine_bg.js` from an old bundler build, and the
  sequential `package.json` is written by the script instead of existing only in the gitignored `engine/pkg/`.
- `scripts/ground.sh` (deterministic probes, exit 0 always, `errors[]` plus a stderr banner) and
  `scripts/verify-live.sh <epic>` (refuses a dirty tree, writes `ground/receipts/<epic>-live.json`) for the dev loop;
  `scripts/test-loop-scripts.py` tests both, including `gh` and `cargo` removed from PATH.

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
