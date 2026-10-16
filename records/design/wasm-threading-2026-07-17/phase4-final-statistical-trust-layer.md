> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (statistical-trust-layer) — Phase 4 FINAL (buildable)
> Charter items: In-Scope 9 (statistical trust layer), In-Scope 12 (per-predictor quantile detail golden + identity), In-Scope 13 (Employers_data.csv fixture)
> Bound invariants: INV-01 (native byte-equivalent; existing method goldens frozen), INV-08 (engine math is f64; this suite runs entirely inside the f64 surface)

# Phase 4 Final Spec — Statistical Trust Layer (Golden Files, Property Tests, QR Validation, Per-Predictor Quantile Golden)

## Summary

This suite validates the engine's **methods** — that the numbers are statistically correct against independent references and satisfy the decomposition algebra. It is **threading-independent**: every test runs native `cargo test` and asserts mathematical identities or reference agreement. It is distinct from the MODES suite (native vs wasm-seq vs wasm-threaded byte-parity, INV-02) which lives in `phase4-final-verification-benchmark.md`; the two "parity"s are kept disambiguated throughout (see § Design, naming collision).

The engine has one method-golden today: `oaxaca_blinder/tests/parity_test.rs` reading `verification/gen_parity_golden.py` output (statsmodels oracle, seed-42 synthetic, two-fold point estimates). This spec **extends, never replaces** it, adding four independent trust assets:

1. A second R-based golden pipeline (`gen_trust_goldens.R`) using R `oaxaca` 0.1.5 + `quantreg`, plus a **bespoke manual-loop bootstrap** (W5 — `oaxaca(R=)` accepts no external index).
2. A `proptest` property suite for the adding-up identities on randomized designs.
3. Heteroskedastic location-scale QR designs with analytically-known **tau-varying** slopes + `quantreg::rq()` goldens (the current QR unit tests use perfectly-linear data where every quantile has the same slope — tau-insensitive).
4. A **new per-predictor quantile-detail golden** (In-Scope 12) via R `ddecompose::ob_decompose(..., reweighting=FALSE)`, with its own adding-up identity (per-predictor detail sums to the aggregate quantile effect).

Build order: this validation suite is **last** (determinism → memory profile → threading → validation). **Correction 2026-07-18 (build-readiness audit CRITICAL-1):** the RIF-OLS per-predictor detail already exists in `OaxacaBuilder::decompose_quantile()` (`builder.rs:720-766`), tested only as a smoke test by `rif_test.rs:38`. This golden therefore validates **existing** engine code and can be authored against the `oaxaca_blinder` crate API independently of the WASM wiring — it closes a real coverage gap on already-shipped math rather than gating on new math.

---

## In-Scope (this domain)

- **R golden pipeline** (`verification/gen_trust_goldens.R`) — regeneration-only tooling; Rust tests read committed JSON offline (no R, no network at test time), mirroring `parity_test.rs`.
- **Mean-path bootstrap golden** via a bespoke manual-loop R script over a committed shared resample-index matrix (W5).
- **proptest identity suite** — two-fold + three-fold adding-up, detailed-vs-aggregate, label-swap antisymmetry, scale equivariance.
- **QR location-scale designs** with analytic tau-varying slopes + `quantreg::rq()` goldens.
- **Per-predictor quantile-detail golden** (In-Scope 12) — `ddecompose::ob_decompose` golden + a per-predictor→aggregate adding-up identity test. Optional Stata `oaxaca_rif` cross-check.
- **PII handling** — strip `Employee_ID` + `Name` before any golden; commit only the stripped fixture + aggregate goldens.
- **Reconcile existing assets** — `gen_parity_golden.py`, `parity_test.rs`, `ground_truth_verification_test.rs` are KEPT and extended, not modified (INV-01).

Explicitly **not** in this domain: the byte-parity MODE suite, CI job wiring for wasm builds, memory ceiling, benchmark, reproducibility double-build (all in `phase4-final-verification-benchmark.md`); the engine RIF-OLS detail math itself (engine-parallel-surface domain — this suite validates it).

---

## Design & Decisions

### D-1 — Extend, not replace (recorded; INV-01)

KEEP `verification/gen_parity_golden.py` + `oaxaca_blinder/tests/parity_test.rs` as the statsmodels method-golden, unmodified. The R pipeline is **additive** — only new files. Two independent oracles (statsmodels + R `oaxaca`) agreeing to 1e-6 is strictly stronger than one. `ground_truth_verification_test.rs` (hand-computed three-fold + location-shift quantile cases, `ground_truth_verification_test.rs:16-74,76-132`) is KEPT and extended with the new proptest and per-predictor cases in separate files.

Verification: `git status` after build shows `gen_parity_golden.py`, `parity_test.rs`, `ground_truth_verification_test.rs`, `parity_fixture.csv`, `parity_golden.json` **unmodified** (bytes unchanged); only new files added under `verification/` and `oaxaca_blinder/tests/`.

### D-2 — Canonical model formula (design-matrix alignment)

One formula, factor coding, and reference levels fixed so R and the Rust `OaxacaBuilder` fit the same design matrix:

- Formula: `log(Salary) ~ Age + Experience_Years + Education_Level + Department + Location`; group = `Gender`; reference group = alphabetically-first `Gender` level present (matches `builder.rs::split_groups` reference convention; documented in `_meta`).
- `Job_Title` **excluded** (high-cardinality, near-collinear with Department) — recorded in `_meta.excluded_predictors` with reason; retained in the fixture CSV for provenance only.
- Categoricals use treatment contrasts (first level = reference, dropped). R uses `contr.treatment`; the Rust dummy expansion must produce the same columns in the same order.
- `Education_Level` = **3-level categorical** (`Master` 4930, `Bachelor` 3381, `PhD` 1689 — L6, confirmed by CSV scan; no missing values, no ordinal ranking encoded) → **2 dummy columns** under one-hot with a base category. This is the real 3-level categorical the Gardeazabal-Ugidos invariance tests exercise. This resolves the phase-2 R-TL-2a open question — encoding is fixed as treatment-coded dummies, recorded in `_meta.education_encoding`.
- Intercept: engine uses `__ob_intercept__` (`builder.rs:334`); the R JSON maps R's `(Intercept)` to `__ob_intercept__` for detailed rows — the same convention `gen_parity_golden.py:112` established.

### D-3 — Mean-path bootstrap golden is a bespoke manual-loop R script (W5, load-bearing)

R `oaxaca()` 0.1.5 exposes only a scalar `R` (replicate count); it accepts **no external resample-index argument**, uses its own internal `sample()`-style within-group loop, and is reproducible only via `set.seed()` (W5, `[CONSENSUS]`). Therefore the shared-resample-index cross-implementation strategy is **not** achievable through `oaxaca(R=)`. The mean-path bootstrap golden is a **manual-loop R script**: it reads the committed integer index matrix, and for each replicate row re-fits the two-fold/three-fold OB decomposition by hand via `lm()` on `data[indices[r,]+1, ]` (R is 1-based; the matrix stores 0-based indices). Rust computes each replicate on `df.take(&IdxCa)` over the same matrix row (`take` is bounds-checked gather, accepts duplicate/unsorted indices, returns exactly `idx.len()` rows — L1, `polars-core-0.44.2 src/frame/mod.rs:1841`). This makes bootstrap SEs compare **exactly** (same indices), not distributionally. Golden SEs = std-dev across replicate estimates, frozen in `trust_goldens_r.json.bootstrap.se_*`.

The R point-estimate goldens (group means, total_gap, two-fold explained/unexplained for GroupB + Pooled/Neumark schemes, three-fold endowments/coefficients/interaction, per-variable detailed rows) may still be produced by a direct `oaxaca()` call (no index needed for point estimates — deterministic single pass); only the bootstrap-SE path uses the manual loop.

### D-4 — proptest identities (native, randomized)

`proptest` added as a dev-dependency (absent today — `oaxaca_blinder/Cargo.toml:38-40` has only `assert_cmd`, `predicates`; `criterion` at :46). Generators produce only well-conditioned designs (group size ≥ 10; per-group column variance > 1e-6; per-group `X'X` condition number < 1e8 via nalgebra SVD σ_max/σ_min; predictors bounded finite range). Degenerate designs are `prop_assume`-filtered, not asserted. A separate `stress()` strategy asserts undersized/constant-column designs return a structured `Err` (never panic). Label-swap and scale cases derive a second design from the first so the identity is exact, not statistical.

### D-5 — QR location-scale (tau-varying, the antidote to tau-insensitive tests)

Design `Y = b0 + b1·X + (1 + c·X)·U`, `U ~ N(0,1)`, `X > 0` so `(1 + c·X) > 0`. True slope at quantile tau is `β1(tau) = b1 + c·z_tau`, `z_tau = Φ⁻¹(tau)` — analytically **tau-varying**. Includes a discrimination assertion (`β1(0.9) − β1(0.1) ≥ c·(z_0.9 − z_0.1)·0.8`) so a hypothetical tau-insensitive implementation fails. Plus `quantreg::rq(Y ~ X, tau=…)` goldens on the same committed synthetic sample; the Rust `solve_qr` must match to rel 1e-4 (LP-solver-to-LP-solver, consistent with the existing `1e-4` QR tolerance in `quantile_regression.rs`).

### D-6 — Per-predictor quantile-detail golden (In-Scope 12; W10)

The engine ALREADY computes per-predictor detailed quantile contributions via one-stage RIF-OLS: `OaxacaBuilder::decompose_quantile(τ)` (`builder.rs:720`) runs `run()` on RIF-transformed data, yielding `two_fold().detailed_explained()`/`detailed_unexplained()`. The WASM path returns empty detail (`analysis.rs:201-204`) only because it calls the MM builder; the engine-parallel-surface domain wires the RIF path in. This suite validates the `decompose_quantile` output (previously only smoke-tested) against an **independent golden**:

- Golden = R `ddecompose::ob_decompose(formula, data, group, rifreg_statistic="quantiles", rifreg_probs=c(0.10, 0.50, 0.90), reweighting=FALSE, bootstrap=TRUE, bootstrap_iterations=<N>)` (W10, signature verified Phase-3.5 from CRAN RDocumentation). `reweighting=FALSE` = one-stage RIF-OLS (FFL-2009 form) matching the engine's chosen route. Per-covariate effects are in the summary; reproducible via `set.seed()`. Companion `rifreg` package computes RIF variables.
- Optional Stata `oaxaca_rif` (Rios-Avila, Stata Journal 2020) cross-check — RIF-as-outcome into `oaxaca`'s detailed machinery.
- **Adding-up self-consistency guard** (council CV-2 correction): for each tau, `sum_k(per_predictor_endowment_k) == aggregate_characteristics_effect` and `sum_k(per_predictor_structure_k) == aggregate_coefficients_effect`, abs tol 1e-9, checked against **the RIF `run()`'s OWN aggregate output** (`two_fold().aggregate()` from the same `decompose_quantile` call) — NOT against the MM `quantile_decomposition.rs:267-271` aggregate, which is a different estimator. This is a **self-consistency regression guard** (the detail is the same OB algebra that produced the aggregate, so it must sum back), not an independent trust check. Independent validation of the explained/unexplained split comes from the `ddecompose` golden above.
- Categoricals route through the existing Gardeazabal-Ugidos normalization (base-category invariance carries over identically to RIF detail — W10). The 3-level `Education_Level` exercises it.
- **Density-estimator alignment (council MJ-3, MAJOR).** RIF detail ∝ `1/f_Y(q_τ)`, and the engine's inline density (`rif.rs`: Silverman `min(std, IQR/1.34)`, nearest-rank IQR, single-point kernel) structurally differs from `ddecompose`'s (`bw.nrd0`, 512-point FFT grid) — so a naive 1e-4 tolerance will fail or be silently loosened. **Two-track requirement:** (track 1, independent trust) pin the `ddecompose` golden but set its tolerance by MEASUREMENT — run `ddecompose` vs `decompose_quantile` on `employers_trust_fixture.csv`, record the actual per-predictor agreement, and pin the tolerance to it (expected ~1e-3 to 1e-2, documented with the measured value, NOT asserted at 1e-4 on faith); (track 2, tight cross-check) additionally author a **custom R RIF golden** replicating `rif.rs`'s exact Silverman-1.34 + nearest-rank-IQR + single-point-kernel density, giving a true ~1e-6 implementation-fidelity check. Track 1 proves "we agree with the field's reference method"; track 2 proves "our Rust matches our own spec'd math."
- **Bootstrap SE validation (council MJ-5 — RATIFIED: recompute RIF per replicate, ruling 4).** `decompose_quantile` now re-estimates the RIF inside each bootstrap replicate (`fixed_rif:false`), so its CIs capture density-estimation uncertainty. This golden compares the RIF-path per-predictor bootstrap SEs against `ddecompose`'s bootstrap SEs (which also recompute per rep) — expected to agree within the measured-then-pinned tolerance; assert the engine records `fixed_rif:false`.
- Two-stage DFL-reweighting (`reweighting=TRUE`, four-term doubly-robust) is **documented v2, out of scope**.

Do **not** extend the MM simulation for detail — MM per-covariate attribution is path-dependent and non-additive (W10). The MM/aggregate path stays as-is for the aggregate quantile effect.

### D-7 — Defensibility narrative citations (W9, primary-source)

The spec's claim — "TM's deterministic seeded bootstrap matches the standard reproducibility guarantee of R/Stata, and improves on it (bit-identical across thread counts, which R/Stata parallel bootstrap does NOT guarantee across `ncpus` changes)" — is backed by primary docs: R `boot`/`set.seed()` + `RNGkind()` fully determine `.Random.seed`; Stata `[R] set seed` fixes the global RNG before `[R] bootstrap` (W9, `[CONSENSUS]`). These citations go in the `gen_trust_goldens.R` header and the trust-suite README/module doc-comment.

### Naming collision (flagged, both specs)

`oaxaca_blinder/tests/parity_test.rs` is a METHOD golden (statsmodels) despite the word "parity". The MODE byte-compare suite uses the `mode_parity` prefix (verification-benchmark spec). New method files here use method-descriptive names (`trust_golden_r_test.rs`, `decomposition_properties_test.rs`, `qr_location_scale_test.rs`, `quantile_detail_golden_test.rs`) — never `parity`.

### Tolerance table

| Quantity | Comparison | Tolerance | Rationale / anchor |
|---|---|---|---|
| Group means, total_gap | engine vs golden | rel 1e-6 | matches `parity_test.rs:24` |
| Two/three-fold aggregate + detailed point estimates | engine vs golden | rel 1e-6, abs floor 1e-8 near zero | closed-form OLS; floor prevents rel-tol blowup on a near-zero channel (tenure-like) |
| Bootstrap SEs (shared indices) | engine vs golden | rel 1e-3 | LP/linear-algebra path divergence R↔Rust accumulates; still exact-index |
| QR coefficients (analytic) | engine vs true β1(tau) | abs, n-derived (≈0.05 @ n≥5000) | finite-sample sampling error, from QR asymptotic SE not a round number |
| QR coefficients (quantreg) | engine vs `rq()` | rel 1e-4 | LP-to-LP, matches existing QR tol |
| Per-predictor quantile detail (track 1: vs `ddecompose`) | engine vs `ddecompose` | **measured-then-pinned** (record actual agreement on the fixture; ~1e-3 to 1e-2 expected — do NOT assert 1e-4 on faith, council MJ-3) | RIF ∝ 1/f; engine density (`rif.rs`) ≠ `ddecompose` FFT-grid density |
| Per-predictor quantile detail (track 2: vs custom-R RIF) | engine vs custom R matching `rif.rs` density | rel 1e-6 | implementation fidelity to the spec'd density estimator |
| Quantile bootstrap SEs (track: vs `ddecompose`) | engine vs `ddecompose` SEs | measured-then-pinned; assert `fixed_rif:false` (ruling 4 — RIF recomputed per rep) | both recompute RIF per rep → tighter agreement expected |
| Internal identities (incl. detail→RIF-aggregate adding-up) | engine self-consistency | abs 1e-9 | matches `parity_test.rs:25`; adding-up checks vs the RIF run's own aggregate (CV-2) |

---

## Build Steps (ordered, buildable)

1. **Add dev-dependency** `proptest` under `[dev-dependencies]` in `oaxaca_blinder/Cargo.toml` (verified absent — currently only `assert_cmd`, `predicates`, `criterion`). No production-code dependency.
2. **PII-strip + commit the Employers fixture.** In `gen_trust_goldens.R`, read `/home/deji/Downloads/Employers_data.csv`, select `Age, Gender, Department, Job_Title, Experience_Years, Education_Level, Location, Salary` (drop `Employee_ID`, `Name`), assert `all(Salary > 0)`, write `oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv` with `float_format` preserving full f64 precision (mirror `gen_parity_golden.py:90` `%.17g`). Header comment: "Direct identifiers (Employee_ID, Name) stripped at generation; aggregate goldens only."
3. **Generate the shared resample-index matrix.** `gen_trust_goldens.R` writes `oaxaca_blinder/tests/fixtures/resample_indices.csv` (`reps` rows × `n` columns, 0-based indices) with `_meta` recording seed + generator algorithm + version.
4. **Write `verification/gen_trust_goldens.R`** (regeneration-only; pinned via committed `verification/renv.lock` OR `stopifnot(packageVersion("oaxaca") == "0.1.5")`). It produces `oaxaca_blinder/tests/fixtures/trust_goldens_r.json`: point estimates via `oaxaca()`; bootstrap SEs via the D-3 manual loop over the index matrix; QR goldens via `quantreg::rq()`; per-predictor quantile detail via `ddecompose::ob_decompose(..., reweighting=FALSE)`. `_meta` records R version, `oaxaca` version (== 0.1.5), `quantreg` version, `ddecompose` version, seed, `RNGkind`, n, formula, `excluded_predictors`, `design_columns` (ordered), `education_encoding`. Command exits 0.
5. **Write `oaxaca_blinder/tests/trust_golden_r_test.rs`** — offline: reads `trust_goldens_r.json`, asserts `_meta.oaxaca_version == "0.1.5"`, fails on any surviving `FILL_AT_GENERATION_TIME` placeholder (same guard as `parity_test.rs:72`), asserts `_meta.design_columns` equals the engine's expanded predictor column list (fails loudly on any column-set or ordering divergence before comparing values), then compares means/gaps/two-fold/three-fold/detailed/bootstrap-SE within the tolerance table.
6. **Write `oaxaca_blinder/tests/decomposition_properties_test.rs`** — `proptest` suite (D-4), one named case per identity.
7. **Write `oaxaca_blinder/tests/qr_location_scale_test.rs`** — the D-5 analytic + `quantreg` QR validation.
8. **Write `oaxaca_blinder/tests/quantile_detail_golden_test.rs`** — calls the EXISTING `OaxacaBuilder::decompose_quantile(τ)` directly (crate API, no WASM dependency), reads the `ddecompose` per-predictor golden, asserts engine per-predictor detail matches within tolerance, AND asserts the D-6 per-predictor→aggregate adding-up identity against the same run's aggregate output. Not gated on the WASM wiring — validates shipped code.
9. **Add defensibility citations** (W9) to the `gen_trust_goldens.R` header and the trust-suite module doc-comments.

These are all native `cargo test` targets covered by the existing `quality` job (`cargo test --workspace --all-features`, `ci.yml:37`). No new CI job.

---

## Acceptance Criteria (objectively checkable)

- **AC-1 (regen)**: `Rscript verification/gen_trust_goldens.R` exits 0 and writes `oaxaca_blinder/tests/fixtures/{employers_trust_fixture.csv, resample_indices.csv, trust_goldens_r.json}`. `trust_goldens_r.json._meta.oaxaca_version == "0.1.5"`.
- **AC-2 (PII)**: `head -1 oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv` contains neither `Employee_ID` nor `Name`; `grep -c -i 'Name\|Employee_ID' <header>` returns 0 for those two column tokens.
- **AC-3 (R golden)**: `cargo test -p oaxaca_blinder --test trust_golden_r_test` exits 0. Includes the `_meta.design_columns == engine columns` equality assertion and the placeholder guard.
- **AC-4 (properties)**: `cargo test -p oaxaca_blinder --test decomposition_properties_test` exits 0, with named cases `two_fold_adding_up`, `three_fold_adding_up`, `detailed_equals_aggregate`, `label_swap_antisymmetry`, `scale_equivariance`, `stress_returns_err` — each ≥256 proptest cases; on failure the shrunk counterexample (seed, design params) prints.
- **AC-5 (QR tau-varying)**: `cargo test -p oaxaca_blinder --test qr_location_scale_test` exits 0. Includes the discrimination assertion (`β1(0.9) − β1(0.1) ≥ c·(z_0.9 − z_0.1)·0.8`) and the `quantreg::rq()` rel-1e-4 match over tau ∈ {0.1, 0.25, 0.5, 0.75, 0.9}.
- **AC-6 (per-predictor quantile golden + identity)**: `cargo test -p oaxaca_blinder --test quantile_detail_golden_test` exits 0. Asserts per-predictor detail vs `ddecompose` within rel 1e-4 (abs floor 1e-8) AND, for each tau, `sum_k(endowment_k) == aggregate_characteristics` and `sum_k(structure_k) == aggregate_coefficients` at abs 1e-9.
- **AC-7 (INV-01 frozen)**: `git status --porcelain` shows `verification/gen_parity_golden.py`, `oaxaca_blinder/tests/parity_test.rs`, `oaxaca_blinder/tests/ground_truth_verification_test.rs`, `oaxaca_blinder/tests/fixtures/parity_fixture.csv`, `oaxaca_blinder/tests/fixtures/parity_golden.json` as unmodified; `cargo test -p oaxaca_blinder --test parity_test` still passes.
- **AC-8 (full suite green)**: `cargo test --workspace --all-features` exits 0 (the existing `quality` gate, `ci.yml:37`).

---

## Open Items (route to buildability gate)

- **OI-1 (RESOLVED by audit reconciliation — no longer a build dependency)**: Step 8 / AC-6 calls the EXISTING `OaxacaBuilder::decompose_quantile()` (`builder.rs:720`), so the golden test is authorable now against shipped crate code — it does NOT wait on the WASM wiring (engine-parallel-surface). The only remaining coupling is that the WASM-path AC-6 in the engine-parallel spec confirms the wired branch returns the same detail; this suite's crate-level golden stands alone. No methodology unknown remains — golden routine (`ddecompose::ob_decompose`), signature, and encoding all resolved (W10, L6).
- **OI-2 (environment, non-blocking)**: `gen_trust_goldens.R` requires R + `oaxaca` 0.1.5 + `quantreg` + `ddecompose` (+ optional Stata `oaxaca_rif`) at **regeneration** time only — never at `cargo test` time. If R is not installed on the build machine, the committed JSON goldens still let all Rust tests pass; regeneration is deferred to a machine with R. Pin via committed `renv.lock`.

All three phase-2 open research questions are **resolved**: R external-index hook → W5 (no hook; manual loop is the design); MM/quantile golden routine → W10 (`ddecompose`); Education_Level categories → L6 (Master/Bachelor/PhD, 2 dummies).

---

## Sources

- Phase 1: `phase1-statistical-trust-layer-golden-proptest.md`, `phase1-verification-benchmark-browser-ci.md`
- Phase 3 web: `phase3-web-w1-w2-w3-w4-w5-w6-w7-w8-w9-w10-findings.md` — W5 (mean-path manual-loop golden), W6→W10 supersession, W9 (R/Stata seed citations), W10 (per-predictor RIF-OLS math + `ddecompose::ob_decompose` golden signature)
- Phase 3 local: `phase3-local-l1-l2-l3-l4-l5-l6-findings.md` — L1 (`take` gather semantics, polars-core-0.44.2 `src/frame/mod.rs:1841`), L6 (Education_Level 3-level: Master 4930 / Bachelor 3381 / PhD 1689)
- Charter: `spec-charter.md` — In-Scope 9/12/13, INV-01, INV-08, Scope-Delta 2026-07-17 (In-Scope 12 upgrade)
- Repo anchors (verified this phase): `verification/gen_parity_golden.py:90,112`; `oaxaca_blinder/tests/parity_test.rs:24,25,61-66,72`; `oaxaca_blinder/tests/ground_truth_verification_test.rs:16-74,76-132`; `oaxaca_blinder/Cargo.toml:38-40,46`; `oaxaca_blinder/src/quantile_decomposition.rs:267-271`; `engine/src/analysis.rs:201-204`; `oaxaca_blinder/src/builder.rs:334`
- Golden-routine URLs (W10): https://cran.r-project.org/web/packages/ddecompose ; http://fmwww.bc.edu/repec/bocode/o/oaxaca_rif.sthlp ; https://eml.berkeley.edu/~cle/secnf/fortinlemieux.pdf
- W5 URL: https://cran.r-project.org/web/packages/oaxaca/oaxaca.pdf ; W9: https://r-universe.dev/manuals/boot.html
