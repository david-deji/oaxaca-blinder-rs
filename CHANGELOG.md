# Changelog

## [Unreleased]

### Changed, BREAKING (0122-MERIDIAN, the remedy says what it does, 2026-10-09)
- **`check_defensibility` reports every gap with `optimize`'s sign.** `original_gap`, `new_gap`,
  `original_unexplained_gap` and `new_unexplained_gap` are the compared group's figure minus the line (or the reference
  group's): negative while the compared group is underpaid, rising with money paid. They used to be the opposite sign on
  all four (the 0118 carry). A consumer that negated them must stop.
- **`adjust_both_groups` no longer credits reference raises to the compared group** (REM-3). `new_gap` is
  `original_gap + cost_target / n_compared - cost_reference / n_reference`, and `new_unexplained_gap` is the gap on the
  line REFITTED to the schedule's wages (the reference line moves when reference employees are raised; under `Pooled`
  the group coefficient moves with every dollar). One shared function prices both entry points, so a schedule has one
  `new_unexplained_gap`; it equals R `lm` refitted on the adjusted wages to 1e-6 on 62 cases. On the probe roster the
  compared group's remaining gap was +2,103 and is -434.
- **`required_budget` is the compared group's need in both entry points**, and `total_cost` stays the money spent on
  both groups. It is the same figure in both on the default basis (midpoint line, threshold 0); `optimize` honours
  `range_target` and `min_gap_pct`, `check_defensibility` always reads the midpoint at threshold 0 (`target_line`), so
  they differ when either is set (a 4 % threshold on the probe roster: 16,884 against 19,607). New: `cost_target`, `cost_reference`, `need_target`, `need_reference` (optimise), `target_line`.
- **`target_gap` is read.** It is a rule for the budget (no solver: on the reference line every dollar to a compared
  employee moves the compared group's gap by the same amount, so the cost of a gap is fixed; on the pooled line a dollar
  moves it by a weight of its own, which is NEGATIVE for a compared employee far beyond the reference group's
  characteristics), on the sign and scale of `original_unexplained_gap`. Already met pays nothing (a budget of 0 used to
  read as "no cap" and paid the full need); out of reach pays every eligible shortfall, or on a pooled roster where paying
  someone widens the gap, up to the budget where the gap is highest, and reports `target_gap_reachable: false`,
  `best_reachable_gap` (the highest the gap gets as the budget grows) and `shortfall_to_target`. Under `Pooled` the
  budget is found along the strategy's own order, at the first point the path reaches the target. New result fields:
  `best_reachable_gap`, `target_gap_reachable`, `shortfall_to_target`, `target_budget`, `budget_binding`,
  `unfunded_amount`, `unfunded_count`, `threshold_excluded_count`, `closure` (share of the compared group's need paid,
  0 to 1, monotone in the spend), `overshoot_mean`. Field definitions: `docs/DIAGNOSTICS.md`.
- **Refused by name:** a negative or non-finite `budget` (`INVALID_BUDGET`; it used to fund the full need), a non-finite
  `target_gap` (`INVALID_TARGET_GAP`), a negative or non-finite `min_gap_pct` (`INVALID_MIN_GAP_PCT`), and `target_gap`
  with `adjust_both_groups` (`TARGET_GAP_WITH_REFERENCE_RAISES`). `budget == 0` is still "no cap" and is documented as
  such on the request, the MCP schema and `docs/API.md`. `min_gap_pct` is documented as shortfall / current pay.
- **Under `range_target` `LowerBound` / `UpperBound`** `original_unexplained_gap` and `new_unexplained_gap` are on the
  midpoint line (the headline's), no longer measured from the bound, and reference employees raised with
  `adjust_both_groups` are raised to the same point of THEIR range (they went to the midpoint whatever the target said).
- **Where each employee stands.** Each adjustment carries `source` (`Compared` / `Reference`), `range_position` and
  `range_position_before` (`Below` / `Inside` / `Above` against its own prediction range; `Below` is exactly
  "not `is_defensible`", one cent of slack on each edge). `check_defensibility` adds `position_counts` over EVERY
  analysed compared employee, before and after (an unnamed row counts at adjustment 0, so a partial schedule cannot
  hide the people it left out; reference employees are in no count), and `group_test`: the exact pooled-indicator test
  on the schedule's wages (closes the "not built" note of 0120 T8; equal to the frontier at equal budget and to R `lm`).
  `group_test.line` is always `"Pooled"`: under `target: Reference` its `group_coefficient` is the group's gap on the
  pooled line, not `new_unexplained_gap` (the reference line), and the two can differ in sign.
- **The frontier follows the remedy.** `EfficientFrontierRequest` gains `strategy`, `target`, `range_target`,
  `min_gap_pct`, `adjust_both_groups` (absent: the only remedy it could describe before). `Equitable` is recomputed per
  step (the sweep is nested only for `Greedy`) and ties are paid in `optimize`'s order. The default budget axis ends at
  the remedy's full cost instead of 1.1 times it. The frontier's `confidence_level` is also the level of the interval a
  `LowerBound` / `UpperBound` remedy pays to (at 0.80 the lower bound is higher, so the axis ends further out). A
  negative or non-finite `max_budget` is refused (`INVALID_BUDGET`) and `steps: 0` too (`INVALID_STEPS`); both used to
  return a curve of nulls or a lone baseline point.
- **MCP:** `simulate_remediation`, `check_defensibility` and `generate_efficient_frontier` refuse an unknown `target`,
  `strategy` or `range_target` by name (`UNKNOWN_TARGET`, `UNKNOWN_STRATEGY`, `UNKNOWN_RANGE_TARGET`); a misspelt
  `"equitable"` used to run Greedy. The schemas list every field and say what the amounts do.
- **Library:** `OaxacaResults::optimize_budget` is `#[deprecated]` (two allocators with opposite group conventions; its
  `target_gap` is the raw `total_gap`; a non-finite `budget` or `target_gap` now pays nothing, a NaN budget used to fund
  the full need); the READMEs no longer sell it as a solver. Stale solver text removed from
  `ARCHITECTURE.md`, `CLAUDE.md`. The engine enums `OptimizationTarget`, `AllocationStrategy` and `RangeTarget` now
  derive `Serialize`, `Clone`, `Copy` and `PartialEq`.
- Result serialisation: an empty sum no longer serialises as `-0.0`.
- Oracles: `verification/gen_remedy_goldens.R` (R `lm`, `predict.lm`, a REFIT of the line on every schedule, `uniroot` on
  that refit for the budget) and `verification/gen_remedy_bruteforce.py` (every allocation on a 250 dollar grid), with
  sha256 of generator and fixtures checked on load. `null_free_regression_test` stays on the pre-0118 golden except one
  regenerated line (`optimize/noisy/forensic_both`, REM-3) and named comparisons for the keys, the defensibility sign and
  the range-target gap.

### Changed, BREAKING (0120-MERIDIAN T8, optimiser Pooled target, 2026-10-09)
- **The optimiser's `Pooled` target is the decomposition's `Pooled` line.** `OptimizationTarget::Pooled` used to stack
  both groups with no group column, which is `PooledNoIndicator` (Neumark), so the remedy was priced against one line
  while the headline beside it was measured against another. It now fits the pooled regression WITH a target-group
  indicator, drops the indicator, and reads every fair wage at indicator 0. At the midpoint `original_unexplained_gap`
  equals the indicator's coefficient, which is the decomposition's `Pooled` unexplained gap (R `lm` and `oaxaca` weight
  -2, 1e-9; `optimize` against `decompose` on the same file, 1e-9). `model_coefficients` and each row's `contributions`
  are the pooled terms of the model's own columns, without the indicator. **Every Pooled-target dollar changes**: fair
  wages, payments, `total_cost`, `required_budget`, `new_gap` and both unexplained gaps. The Reference target is
  byte-identical.
- **The interval for that target comes from the same fit.** Bounds are `predict.lm(pooled_fit, newdata = row at
  indicator 0, interval = "prediction")`: sigma squared, `(Z'Z)^-1` and the residual df (`n_reference + n_target - k -
  1`) are the pooled regression's, so `interval.degrees_of_freedom` and `critical_value` report that fit. The
  "approximation until T8" note is removed from `IntervalBasis` and `docs/DIAGNOSTICS.md`.
- **`extrapolated` and `few_residual_df` judge the pooled fit under this target.** A row is extrapolated when its
  leverage at indicator 0 exceeds the largest leverage among the reference rows of the pooled design (ordinal sets
  equal R's `hatvalues` row by row). `few_residual_df` carries subject `pooled` and the pooled df; a pooled fit with no
  residual df is refused as `INSUFFICIENT_RESIDUAL_DF: group=pooled`, and a baseline group too small to carry its own
  line is no longer a reason to refuse under Pooled (it still is under Reference).
- Oracle `verification/gen_pooled_target_goldens.R` writes `oaxaca_blinder/tests/fixtures/pooled_target_goldens_r.json`
  (sha256 of the script and of seven fixtures checked on load). `null_free_regression_test` no longer compares
  `optimize/noisy/pooled_target` with the pre-0118 text (it pinned the no-indicator fit); that case is held to a pooled
  `lm` fitted from the fixture's own cells, and every Reference-target case still equals the golden at 1e-9.
- **`check_defensibility` takes the same `target`** (0120-MERIDIAN review N8, "E2-c" of the Track A plan). WASM and
  MCP requests may carry `target: "Reference" | "Pooled"`; absent means Reference, byte-identical to before (an explicit
  Reference equals the default byte for byte). Under Pooled the check reads fair wages, bounds, `extrapolated` and
  `few_residual_df` (subject `pooled`) off the pooled fit, so the remedy's rows and the check's rows mark the same
  people. Held to the same R golden as the optimiser: bounds at 0.90 / 0.95 / 0.99 against `predict.lm` (1e-9),
  critical value and df, extrapolated ordinals against `hatvalues`, and equal to the remedy's set. New type
  `DefensibilityRequest` (a flattened `VerificationRequest` plus `target`); `verify_adjustments` is unchanged.
  `check_defensibility_inner` is `check_defensibility_on(req, &Reference)`.
- Still not built: E2-e / E2-f (coefficients and group shares for a redo-by-hand table). The exact `group_test` on the
  defensibility run was built by 0122-MERIDIAN.

### Changed, BREAKING (0120-MERIDIAN S1-S4, Track E core, 2026-10-09)
- **Per-level driver rows no longer depend on which level sorts first.** The engine (WASM `decompose` and
  `verify_adjustments`, MCP) and the CLI (`run`, `report`) normalise every categorical predictor on every run:
  each level, the alphabetically first one included, is a deviation from the pooled-sample share-weighted average of
  the levels (population-share, founder decision D1). All k levels are emitted; the intercept absorbs the average.
  The decomposition aggregates and the remedy's level are unchanged by S1-S4 alone; the numbers that do move are
  listed under S6-S9 below (Student t intervals and the range-target payments built on them). The library keeps `normalize()`
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

### Changed, BREAKING (0120-MERIDIAN S6-S9, Track E diagnostics, 2026-10-09)
- **A weights column needs a kind.** `OaxacaBuilder::weights_kind(WeightsKind::Frequency | Relative)` and CLI
  `--weights-kind frequency|relative`, each required with `--weights` (a run without it fails with
  `WEIGHTS_KIND_REQUIRED`). `frequency`: whole-number counts, `w = 2` is the row twice; a fractional, negative or
  non-finite value fails with `INVALID_WEIGHT: column=, row=, value=` (the original 0-based data-row ordinal).
  `relative`: FTE / design weights, rescaled so each regression's weights sum to its row count; the RIF percentile
  is `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)`; uniform weights change nothing.
  `oaxaca_blinder::weighted_quantile(values, weights, tau, kind)` is public. An integer-typed weights column (a
  headcount read from CSV) is accepted. Under `frequency` the RIF density bandwidth now reads `sum(w)` instead of
  Kish's effective n, so a run with `w = 2` equals the run on the row twice, density included.
- **Frequency weights bootstrap the expanded sample.** A replicate used to draw one row per ROW and carry the weight,
  so `w = 2` matched the repeated rows for point estimates only: on `norm_skewed_fixture.csv` the standard errors came
  out 1.32x (explained) and 1.45x (unexplained) those of the repeated rows and Department_Admin's unexplained p-value
  was 0.084 against 0.000. Under `weights_kind = frequency` a replicate now draws `sum(w)` employees per group
  (multinomial, probability `w_i / sum(w)`) and the draw counts become that replicate's weights, on the mean and the
  percentile path. `relative` weights and unweighted runs keep the row-level draw (byte baselines unchanged).
- **Prediction intervals and the frontier p-value are Student t**, on the baseline regression's residual degrees of
  freedom (`predict.lm(interval = "prediction")`, `pt`), not Normal. **Remedy dollars move under `range_target`
  `LowerBound` / `UpperBound`**: the payment is the interval bound, so `adjustments[].adjustment`, `new_wage`,
  `total_cost`, `required_budget`, `new_gap`, `original_unexplained_gap` and `new_unexplained_gap` are t-based. The
  half-width grows by t/z, about +2% at 58 residual df and +14% at 10, so a saved scenario re-run on this engine
  returns different dollars. `Midpoint` payments do not touch the interval and are unchanged. The bound is checked
  against `predict.lm` in `intervals_test` (`v7_range_target_payments_equal_the_predict_lm_bound_minus_the_wage`).
  `is_defensible` allows one cent below the
  interval floor, not one dollar. `confidence_level` is now read by `check_defensibility` and
  `calculate_efficient_frontier` (default 0.95, clamped to [0.50, 0.999]; for the frontier it sets
  `is_significant`). MCP `check_defensibility` and `generate_efficient_frontier` accept it.
- **A fitted group with no residual degrees of freedom is refused** on every entry point:
  `INSUFFICIENT_RESIDUAL_DF: group=reference|target, rows=, model_columns=, residual_df=` (it was a zero-width
  interval, an anonymous estimator error, or the frontier's `t = 0, p = 1` sentinel).
- `optimize` no longer fits a regression to the compared group to obtain `original_gap`; it is the difference of
  the analysed group means (what `total_gap` is). Numbers are unchanged.

### Added (0120-MERIDIAN S6-S8)
- Result fields (documented in `engine/src/types.rs`, mapped in `docs/DIAGNOSTICS.md`), all additive: `support`
  and `warnings` on decompose, verify, optimize and defensibility results; `adjustments[].extrapolated` and
  `interval` on optimize and defensibility; `quantile_report` on a percentile decompose (omitted otherwise);
  `group_coefficient` and `degrees_of_freedom` on every frontier point.
- `warnings[]` is `{code, subject, value, threshold}` with codes `outside_range` (> 5% of compared rows beyond the
  baseline range), `normalised_difference` (|Imbens-Rubin| > 0.25), `few_residual_df` (< 10), `tie_share` (> 5%)
  and `ecdf_offset` (|F_n(q) - tau| > max(0.01, 1/n): a tie-free group of n rows is off by up to 1/n by discreteness
  alone, so a 23-person roster with 23 different salaries is silent; `tie_share` is the step-grid signal). The
  engine carries no wording.
- `verification/gen_diag_goldens.R` -> `diag_goldens_r.json` (base R `lm` / `predict.lm` / `quantile`, `ddecompose`,
  `Hmisc`), fixtures `diag_*.csv`; sha256 of the generator and every fixture are recorded and checked.

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
