# Scout Report — Mission B: Reference-Absent Bootstrap Edge, Publish Freshness, Residual Gaps

## 1. Reference-Absent Edge (headline question)

### 1a. Architecture: why the literal "reference group vanishes from a resample" case is structurally impossible

Both bootstrap loops (mean-path `run()` and RIF quantile-path `decompose_quantile()`) split into per-group frames **once**, before any resampling, then resample **each group's own frame separately** at a **fixed size equal to that group's original row count**:

- `oaxaca_blinder/src/builder.rs:828-830` — `let groups = self.split_groups(&df)?; let df_a_global = groups.df_a; let df_b_global = groups.df_b;` (mean path)
- `oaxaca_blinder/src/builder.rs:863-866` — inside the bootstrap closure: `df_a_global.take(&resample_indices(&mut rng_a, df_a_global.height()))` and the same for `df_b_global` (mean path)
- `oaxaca_blinder/src/builder.rs:806-830` — identical group-split-then-per-group-resample structure for `decompose_quantile` (RIF path), reusing the same `df_a_global`/`df_b_global`
- `oaxaca_blinder/src/quantile_decomposition.rs:390-419` — the deprecated Machado-Mata module uses the identical stratified-by-group, fixed-size, per-group `take()` pattern

`resample_indices` (`oaxaca_blinder/src/rng.rs:54-58`) always draws exactly `n` indices in `0..n` — so a resample of a group's own frame always returns exactly that group's original row count, every replicate, forever. Since `split_groups` (`builder.rs:103-144`) already requires ≥2 distinct group values to exist in the *original* data (erroring with `InvalidGroupVariable` at `builder.rs:109-113` otherwise), both `n_a ≥ 1` and `n_b ≥ 1` are guaranteed before any bootstrap rep runs, and neither can change per rep. **A comparison group cannot vanish or shrink in any single bootstrap replicate under the current design** — this is a structural property of stratified-by-group resampling, not a check that happens to catch it.

### 1b. The real degenerate-resample surface: a categorical predictor level, not the comparison group

Dummy columns for categorical predictors are built **once**, from the full (unresampled, both-groups) dataset — `create_dummies_manual` (`builder.rs:458-496`) — and hstacked onto `df` *before* the group split. `all_dummy_names`/`category_counts`/`base_categories` are then reused unchanged across the point estimate and every bootstrap replicate (`builder.rs:809-826`, `secs 845-848` comment). If a categorical level is rare enough that a group's resample happens to draw zero rows carrying it, that dummy column becomes an **exact all-zero column** in that replicate's design matrix (`prepare_data`, `builder.rs:411-422`, falls through to reusing the existing all-zero-valued column — it does not need the "column missing" branch since the column exists, just constant).

Each group is fit with its **own separate OLS** (`estimation.rs:52-53`: `ols_a = ols(ctx.y_a, ctx.x_a, ...)`, `ols_b = ols(...)`), via `ols()` in `oaxaca_blinder/src/math/ols.rs:96-101`, which forms `X'X` and requires a **Cholesky decomposition** (`ols.rs:100-105`). An identically-zero column makes that row/column of `X'X` exactly zero, so `X'X` is exactly singular — Cholesky returns `None` and `ols()` returns `Err(OaxacaError::NalgebraError("Failed to perform Cholesky decomposition. Matrix may be singular or not positive definite due to multicollinearity."))` (`ols.rs:101-105`). This propagates through `run_single_pass` (`builder.rs:498-542`) as a hard `Err` — **no NaN, no silently-wrong estimate**.

- **Inside a bootstrap replicate**, that `Err` is caught by the closure at `builder.rs:862-877`/`1033-1045` and converted to `RepOutcome::Failed` (`builder.rs:881`/`1053`), incrementing `discarded` (`builder.rs:889`/`1061`). This is the correct, already-guarded fail-safe path for a categorical level that vanishes **only** from a specific replicate's resample.
- **At the point estimate**, the same `run_single_pass` call (`builder.rs:836` for quantile, `builder.rs:986` for mean path) is **not** wrapped in a discard-catching closure — it uses a bare `?`. If the categorical level is confined to one group in the **original, unresampled data** (not a resampling artifact at all), the entire `.run()`/`.decompose_quantile()` call fails outright with the same opaque Cholesky message, before any bootstrap rep executes.

**Empirically verified** (ran the currently-`#[ignore]`d test, see §1c): this point-estimate failure is real and reproducible today, not theoretical.

### 1c. Existing test coverage: found and is currently disabled

`grep` for `absent|empty group|degenerate|reference|rare categ` across `oaxaca_blinder/tests/` surfaces exactly one directly-on-point test:

- **`oaxaca_blinder/tests/integration_test.rs:332-334`**: `#[test]` `#[ignore]` `fn test_detailed_components_with_rare_category()` — builds a 20-row fixture where sector `"B"` appears in exactly 1 row, that row belonging to the reference group ("F"), and asserts (with `bootstrap_reps(5)`) that the `sector_B` detailed component is present and its bootstrap CI is finite. This is **precisely** the scenario Mission B asks about, and it is **not run** in the default `cargo test` suite (`#[ignore]` at line 333).
- Ran it directly with `--ignored`: it **panics**, not on a bootstrap discard, but at the `.run().expect(...)` call itself (`integration_test.rs:73`):
  ```
  thread 'test_detailed_components_with_rare_category' panicked at oaxaca_blinder/tests/integration_test.rs:73:10:
  Oaxaca run failed: NalgebraError("Failed to perform Cholesky decomposition. Matrix may be singular or not positive definite due to multicollinearity.")
  ```
  This confirms §1b precisely: because sector `"B"` in this fixture is 100% absent from group M's *original* rows (not merely absent by resampling chance), the M-group OLS's `sector_B` column is constant-zero even at the point estimate, so the whole call errors out before any bootstrap replicate runs — the test's own premise ("might have issues if not present in **all bootstrap samples**") does not match what actually breaks.
- `decomposition_properties_test.rs` (`oaxaca_blinder/tests/decomposition_properties_test.rs:126-139`, `stress_returns_err`) proves the engine returns `Err` (never panics) on undersized/constant designs, but uses only continuous predictors and `bootstrap_reps(1)` (`:36`) — it does not exercise categorical-level bootstrap degeneracy at all.
- No test in the suite exercises a categorical level that is present in **both** groups in the full data but rare enough to plausibly vanish from a **specific bootstrap replicate's** resample (the scenario the D5/`RepOutcome::Failed` machinery was actually built for).

### 1d. The RIF `n<2` fail-loud guard (commit `3858b43`) — what it covers, and what it doesn't

`git show 3858b43 -- oaxaca_blinder/src/math/rif.rs` replaced a silent `Ok(series.clone())` (returning the raw outcome as its own "RIF") with a hard `Err` when `n<2` — `oaxaca_blinder/src/math/rif.rs:14-34`. Per the commit message and `_specify-wasm-rayon-multithreading-2026-07-17/.build-state/FOLLOWUPS.md:61-70` (item D):

- The guard fires on **whole-group row count** (`n = y_vec.len()`, `rif.rs:15-16`), because "sample variance and the KDE bandwidth are both undefined for n<2" (`rif.rs:24`).
- It is reachable because `split_groups` enforces ≥2 *distinct group values* but **no per-group row-count minimum** (`FOLLOWUPS.md:62-64`).
- Given §1a (group size is fixed per replicate = original group size), this guard's bootstrap-replicate branch can only ever fire if the **original, full** group already has `n<2` — in which case `decompose_quantile`'s point-estimate call (`builder.rs:833-835`, bare `?`, no discard-catching closure) fails the **entire call**, before any bootstrap rep executes.
- **Discrepancy worth flagging to the spec**: `FOLLOWUPS.md:68` states "On the bootstrap path a stray singleton resample degrades to one discarded replicate" — this describes a resample dynamically shrinking a group to `n=1`, which is not possible under the current fixed-size-per-group resampling architecture. The only way this guard's error is actually observed is as a whole-call `Err` at the point estimate (same failure shape verified in §1c), not as a per-replicate discard. Treat the FOLLOWUPS.md characterization as **[UNVERIFIED]** against the actual resampling code, not as ground truth for the new spec.
- The `n<2` guard covers *density-estimation* degeneracy (whole group too small), **not** the reference/level-absence scenario at all — that's covered (correctly, for the per-replicate case) by the Cholesky-singularity path in §1b, and (with a hard-fail, opaque-message, whole-run-abort behavior) for the full-data case.
- Separately, `rif.rs:65-71` and `:88-89` already contain zero-spread / zero-density floors (`min_spread < 1e-8 → 1.0`, `density < 1e-8 → 1e-8`) that specifically guard the "all resampled values are ties" case the ruling-4 per-replicate RIF recomputation (`builder.rs:793-797, 867-869`) newly introduced — verified no NaN risk there.

### 1e. UX consequence (ties into residual-gaps section)

The frontend surfaces the raw Rust error string verbatim with no translation — `pay-equity-app/frontend/src/stores/__tests__/dashboard.store.spec.js:172,180` and `analysisResults.store.spec.js:91-129` pin `decompositionError` to literally `'Nalgebra error: Failed to perform Cholesky decomposition'` / `'Rust panic: singular matrix'`. An analyst who hits the categorical-level-confined-to-one-group case (a completely explainable, common real-world data situation — e.g., "only one employee in category X, and they're in the reference group") sees an opaque linear-algebra error with no indication of which column or why.

---

## 2. Publish Freshness

**Current — confirmed by direct evidence, not inference.**

- Engine HEAD: `f81feca` @ `2026-08-07 05:09:01 -0400` (`fix(engine): csv_data must deserialize through #[serde(flatten)]`) — `git log -1 f81feca`.
- Consuming-app commit `pay-equity-app` `a1c9b451` @ `2026-08-07 05:09:32 -0400` (32 seconds after the engine fix) is the publish commit, and its own message states explicitly: *"Fixed engine-side (oaxaca-blinder-rs f81feca, serde_bytes on csv_data)... **Both blobs rebuilt through scripts/build-wasm.sh, sha256-verified.**"* This is a first-party, self-documenting confirmation that the published blobs correspond exactly to engine HEAD `f81feca`.
- File listing confirms both artifact sets were written together: `frontend/src/wasm/pay_equity_engine_bg.wasm` and `frontend/src/wasm-threaded/pay_equity_engine_bg.wasm` both timestamped `Aug 7 05:03` on disk, consistent with the `a1c9b451` publish. (`frontend/src/wasm/analysis.worker.js` shows a later `Aug 19` mtime, but per `scripts/build-wasm.sh`'s own comment and `CLAUDE.md`, `analysis.worker.js`/`thread-cap.js`/`.gitignore` are **frontend-owned** files the publish script never touches — their later edits are unrelated to engine freshness.)
- No engine commit after `f81feca` exists (`f81feca` is HEAD).

**Verdict: current.** Last published engine commit = `f81feca`, matching engine HEAD exactly.

---

## 3. Residual Gaps

- **`run_metadata` (bootstrap discard count, seed, RNG algorithm/version, `fixed_rif` flag) is computed, serialized, and shipped across the WASM boundary — and then dies unread on the frontend.**
  - It's a first-class, always-present field on the top-level decomposition result: `engine/src/types.rs:66-67` (`pub run_metadata: RunMetadata`), populated at `engine/src/analysis.rs:183,256,342,358`.
  - `grep -rniE "run_metadata|runMetadata|bootstrap_reps_discarded|rng_algorithm|fixed_rif"` across `pay-equity-app/frontend/src` returns **zero matches**. The exact `bootstrap_reps_discarded` count this epic's headline question is about — the authoritative, deterministic record of how many replicates were thrown away (`rng.rs:87-88`: "the authoritative record of failed reps") — reaches the Meridian UI's JSON payload and is never displayed, logged, or surfaced anywhere in the app. Even a perfectly-correct discard mechanism is invisible to the analyst using the tool today.

- **Two `defer/out-of-scope` items from the 0014-MERIDIAN spec that remain open, neither closed by any follow-up commit through `f81feca`:**
  - **Two-stage DFL-reweighting quantile detail** — explicitly "documented v2, explicitly out of scope" — `_specify-wasm-rayon-multithreading-2026-07-17/phase4-final-engine-parallel-surface.md:30,143` and `phase4-final-statistical-trust-layer.md:81`. No shipped surface implements DFL reweighting; the documented limitation "(ii) no double-robustness without reweighting" (`phase4-final-engine-parallel-surface.md:143`) still applies to the shipped RIF path.
  - **Item E (bootstrap-SE golden consuming test) — BLOCKED**, and **item G (quantile SE / density-floor tail at τ=0.1/0.9) — BLOCKED** per `FOLLOWUPS.md:85-114`. Both need R-machine golden regeneration that has not happened between `3858b43` and `f81feca`. Item G is specifically about the *same* density-floor mechanism examined in §1d (`rif.rs:65-71`) — its SE behavior at the tails is not covered by any oracle-verified test yet.

- **Item D's stated reachability path (§1d) is only half-verified against the real resampling architecture.** The `n<2` fail-loud fix is real and correct for the case it actually guards (whole small groups), but its bootstrap-path narrative in `FOLLOWUPS.md` doesn't match the fixed-size-per-group resampling code — worth the new spec correcting this explicitly so it isn't re-cited as-is.

- **The disabled `test_detailed_components_with_rare_category` test (§1c) is itself an unclosed residual gap** — it predates 0014-MERIDIAN (added around commit `82de6d9`/`10f7563`, well before the 0014 spec dir existed) and was never revisited by the 0014-MERIDIAN stages or the `3858b43` follow-ups, despite `3858b43` doing a full statistical-trust-layer pass. It currently fails if un-ignored.

---

## Verdicts for the spec

1. The literal "a bootstrap resample contains zero rows of the reference/comparison group" scenario is **structurally impossible** in the current architecture (`builder.rs:828-830,863-866`; `quantile_decomposition.rs:390-419`) — resampling is stratified per-group at fixed size, not a single-frame resample over the whole dataset. The new spec should not scope work to "catch" this; it cannot occur.
2. The scenario that actually matters is a **categorical predictor level vanishing from one group's resample** (or being absent from one group in the full data to begin with) — this produces an exact-zero design-matrix column, which Cholesky decomposition (`math/ols.rs:100-105`) deterministically and correctly rejects as `Err`, never a silent NaN or reindexed-coefficient result.
3. **Per-replicate** vanishing is already correctly handled: caught, converted to `RepOutcome::Failed`, and counted in the authoritative `discarded` total (`builder.rs:879-891,1051-1063`).
4. **Full-data-level** vanishing (level present in the dataset but confined to one group) is **not** gracefully handled: it hard-fails the entire `.run()`/`.decompose_quantile()` call with an opaque `"Failed to perform Cholesky decomposition... multicollinearity"` message that names neither the column nor the actual cause, and this is empirically confirmed (§1c test run), not theoretical.
5. `oaxaca_blinder/tests/integration_test.rs:332-334`'s `test_detailed_components_with_rare_category` is real, targeted coverage for exactly this class of bug, is currently `#[ignore]`d, and currently panics if un-ignored — the new spec should treat un-ignoring + fixing (or rewriting) this test as in-scope work, not assume it already passes.
6. The RIF `n<2` guard (`3858b43`, `math/rif.rs:14-34`) guards whole-group-too-small density estimation, not level-absence; its FOLLOWUPS.md bootstrap-path narrative should be corrected in the new spec rather than re-cited, since it describes a "singleton resample" that the fixed-size-per-group architecture cannot produce.
7. The zero-spread/zero-density floors in `rif.rs:65-71,88-89` already guard the ties-heavy resample case the ruling-4 per-replicate RIF recomputation introduced — no NaN risk found there.
8. Published WASM in `pay-equity-app/frontend/src/{wasm,wasm-threaded}` is **current** with engine HEAD `f81feca`, confirmed by the app's own publish commit `a1c9b451` (32s later) stating the rebuild+sha256-verify explicitly — no republish action needed before this spec's work begins.
9. `RunMetadata`/`run_metadata` (including `bootstrap_reps_discarded`) is fully computed and shipped in the WASM JSON payload but has zero consumers anywhere in the Meridian frontend — any spec that improves discard-handling correctness without also wiring this field to the UI leaves the improvement invisible to end users; treat UI wiring of `run_metadata` as a candidate in-scope item, not an assumed pre-existing capability.
10. Two known-open deferred items (DFL two-stage reweighting; bootstrap-SE and tail-quantile-SE golden verification, FOLLOWUPS.md items E/G) remain unclosed through engine HEAD and are adjacent risk surface the new spec should acknowledge as pre-existing, not conflate with the reference-absent work.