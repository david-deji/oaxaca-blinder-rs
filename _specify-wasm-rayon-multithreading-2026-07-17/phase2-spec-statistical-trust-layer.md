> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (statistical-trust-layer) — Draft

# Domain Spec — Statistical Trust Layer (Golden Files, Property Tests, QR Validation)

Charter items covered: In-Scope 9 (statistical trust layer), In-Scope 13 (Employers_data.csv fixture). Bound invariant: INV-08 (engine math is f64; Decimal only at boundaries — this suite operates entirely inside the f64 engine surface). This suite is **threading-independent — it runs native (`cargo test`) and validates the METHODS.** The MODE parity suite (native vs wasm-seq vs wasm-threaded, INV-02/SC-03) is specced separately in `phase2-spec-verification-benchmark.md`; the two are kept distinct throughout.

---

## 1. Executive Summary

The engine currently has one method-golden test — `oaxaca_blinder/tests/parity_test.rs`, reading committed fixtures produced by `verification/gen_parity_golden.py` (statsmodels oracle, synthetic seed-42 data, two-fold point estimates only). Three trust gaps remain: (a) no R `oaxaca`/`quantreg` cross-check and no bootstrap-SE golden; (b) no property-based coverage of the adding-up identities on randomized data — only fixed hand-computed cases (`ground_truth_verification_test.rs`); (c) the QR unit tests (`quantile_regression.rs::test_solve_qr_median`, `::test_solve_qr_quartile`) use perfectly-linear data where every quantile has the same slope, so they cannot detect a tau-varying-slope regression.

This spec adds a second, independent R-based golden pipeline (R `oaxaca` 0.1.5 + `quantreg`, pinned, fixed seed, shared resample-index matrix), a `proptest` property suite for the decomposition identities, and heteroskedastic location-scale QR designs with analytically-known tau-varying slopes plus `quantreg::rq()` goldens. The existing statsmodels asset is **extended (kept as a second oracle), not replaced** — two independent references (statsmodels + R oaxaca) agreeing to 1e-6 is strictly stronger than one.

PII handling (mandatory): `Employers_data.csv` carries `Employee_ID` and `Name` (direct identifiers). The committed trust fixture strips both columns before any golden is generated; goldens contain only aggregate decomposition statistics. See §4.5.

---

## 2. Requirements

### R-TL-1 — R golden pipeline (regeneration tooling, offline at test time)

A pinned R script `verification/gen_trust_goldens.R` generates JSON goldens from (a) the PII-stripped Employers fixture and (b) synthetic designs, using R `oaxaca` 0.1.5 + `quantreg`. Rust tests read the committed JSON offline (no R, no network at test time) — mirroring the existing `parity_test.rs` architecture.

**Acceptance criteria**
- `Rscript verification/gen_trust_goldens.R` writes `oaxaca_blinder/tests/fixtures/trust_goldens_r.json` and prints a `_meta` block with R version, `oaxaca` version (`== 0.1.5`), `quantreg` version, seed, and n. Command exits 0.
- A new offline Rust test `oaxaca_blinder/tests/trust_golden_r_test.rs` reads that JSON and passes: `cargo test -p oaxaca_blinder --test trust_golden_r_test`.
- The test asserts `_meta.oaxaca_version == "0.1.5"` and fails loudly if a `FILL_AT_GENERATION_TIME` placeholder survives (same guard as `parity_test.rs:72`).
- Golden surface present in JSON: group means, `total_gap`, two-fold (explained/unexplained for GroupB and Pooled/Neumark schemes), three-fold (endowments/coefficients/interaction), per-variable detailed rows each side, bootstrap SEs (see R-TL-4).

### R-TL-2 — Canonical model formula, fit identically by R and Rust

The spec fixes one model formula, factor coding, and reference levels so R `oaxaca` and the Rust `OaxacaBuilder` fit the same design matrix.

**Acceptance criteria**
- Formula recorded verbatim in `trust_goldens_r.json._meta.formula` and in the Rust test header. Canonical choice: `log(Salary) ~ Age + Experience_Years + Education_Level + Department + Location`, group = `Gender`, reference group = the alphabetically-first level present (documented explicitly, matching `builder.rs::split_groups` reference convention).
- `Job_Title` is **excluded** from predictors (high-cardinality, near-collinear with Department) — the exclusion is stated in `_meta.excluded_predictors` with the reason.
- Categorical coding fixed: `Department`, `Location`, `Gender` use treatment contrasts (first level = reference, dropped); `Education_Level` handled per R-TL-2a below. R uses `contr.treatment` with reference level named in `_meta`; the Rust dummy expansion must produce the same columns in the same order (verify by comparing `_meta.design_columns` to the engine's expanded predictor names).
- `test_parity_two_fold_decomposition` continues to pass unchanged (INV-01 — existing method golden untouched).

**R-TL-2a** — `Education_Level` is ordinal (categorical in the CSV). The spec commits to **one** encoding: ordered-integer (Bachelor=3, Master=4, …) mapped in a committed lookup, OR treatment-coded dummies. The chosen encoding is recorded in `_meta.education_encoding` and both sides apply it identically. Acceptance: `_meta.education_encoding` present and the Rust fixture-loader applies the same mapping (verified by a design-column-name equality assertion).

### R-TL-3 — Extend-vs-replace decision for existing assets (recorded)

**Acceptance criteria** — the spec records this decision (also in §3):
- `verification/gen_parity_golden.py` + `parity_test.rs`: **KEEP** as the statsmodels method-golden. Rename-recommendation flagged (its name collides with the threading MODE parity suite; see Risk R-TL-C). Do not modify its fixtures (INV-01).
- New R pipeline is **additive**: new files only (`gen_trust_goldens.R`, `trust_goldens_r.json`, `trust_golden_r_test.rs`, the stripped Employers fixture, the resample-index fixture). No existing fixture bytes change.
- Acceptance: `git status` after the build shows the four existing trust files unmodified; only new files added under `verification/` and `oaxaca_blinder/tests/`.

### R-TL-4 — Shared resample-index matrix for exact bootstrap-SE comparison

Both R and Rust consume one committed integer index matrix (reps × n, fixed seed) so bootstrap SEs compare exactly rather than distributionally.

**Acceptance criteria**
- The R script writes `oaxaca_blinder/tests/fixtures/resample_indices.csv` (or `.parquet`): `reps` rows × `n` columns of 0-based row indices, plus `_meta` recording seed + generator algorithm + version.
- R computes each bootstrap replicate by re-fitting the decomposition on `data[indices[r,]+1, ]` (1-based in R); Rust computes replicates on `df.take(indices row r)`. This aligns with the RNG design in the cross-domain summary (owned-index resampling replaces `sample_n_literal(...,None)` / `thread_rng()`).
- Golden SEs = std-dev across replicates, frozen in `trust_goldens_r.json.bootstrap.se_*`.
- Rust test asserts engine bootstrap SE within the R-TL-6 SE tolerance of the golden, using the same committed index matrix.
- **Gap gate**: whether R `oaxaca` 0.1.5 accepts an externally-supplied index matrix is unverified — see §7 NEEDS RESEARCH. Fallback (specified so /build is not blocked): reimplement the bootstrap loop in the R script directly (`lm()`/`oaxaca` per replicate over the index rows) rather than calling `oaxaca(..., n.iterations=)`.

### R-TL-5 — proptest property suite (adding-up identities on randomized data)

A `proptest`-driven suite in `oaxaca_blinder/tests/decomposition_properties_test.rs` asserts the algebraic identities across randomized valid designs.

**Acceptance criteria** — `cargo test -p oaxaca_blinder --test decomposition_properties_test` passes with each property as a named case:
- **two-fold adding-up**: `explained + unexplained == total_gap` (abs tol 1e-9).
- **three-fold adding-up**: `endowments + coefficients + interaction == total_gap` (abs tol 1e-9).
- **detailed-vs-aggregate**: `sum(detailed_explained) == explained` and `sum(detailed_unexplained) == unexplained`, each side (abs tol 1e-9).
- **label-swap antisymmetry**: swapping the two group labels negates `total_gap` (rel tol 1e-9 on the magnitude; sign flips).
- **scale equivariance**: scaling the outcome by constant `c > 0` scales `total_gap` and every component by `c` (rel tol 1e-8).
- proptest config: ≥256 cases per property; on failure the minimal shrunk counterexample (seed, design params) is printed.

### R-TL-6 — Generator guards (property-suite validity domain)

The proptest generators produce only well-conditioned designs; degenerate designs are `prop_assume`-filtered, not asserted against.

**Acceptance criteria**
- Every generated group has size ≥ 10 (`prop_assume`).
- Every predictor column has sample variance > 1e-6 within each group (rejects constant columns).
- `X'X` condition number < 1e8 per group (rejects near-collinear designs); computed via nalgebra SVD ratio σ_max/σ_min.
- Numeric ranges bounded (predictors in a documented finite range, e.g. [-1e3, 1e3]) so no overflow.
- Two generator strategies present: `well_behaved()` (feeds the identity assertions above) and `stress()` (undersized/constant-column designs that must return a structured `Err`, never panic — asserts the error path).

### R-TL-7 — Heteroskedastic location-scale QR designs (analytic tau-varying slopes)

QR validation replaces the tau-insensitive linear tests with designs whose true slope varies with tau by construction, plus `quantreg::rq()` goldens on identical data.

**Acceptance criteria** — `cargo test -p oaxaca_blinder --test qr_location_scale_test` passes:
- Design: `Y = b0 + b1·X + (1 + c·X)·U`, `U ~ N(0,1)`, `X > 0` on its support so `(1 + c·X) > 0`. True slope at quantile tau is `β1(tau) = b1 + c·z_tau` where `z_tau = Φ⁻¹(tau)` (analytic, tau-VARYING).
- Grid `tau ∈ {0.1, 0.25, 0.5, 0.75, 0.9}`; engine `solve_qr` estimated slope matches `β1(tau)` within a sampling tolerance (documented, e.g. abs 0.05 at n≥5000; tolerance derived from QR asymptotic SE, not guessed).
- A **discrimination assertion**: `β1(0.9) − β1(0.1)` is bounded away from zero (≥ `c·(z_0.9 − z_0.1)·0.8`), so a hypothetical tau-insensitive implementation would fail — the exact hole the current tests leave open.
- `quantreg::rq()` goldens: the R script fits `rq(Y ~ X, tau=…)` on the same committed synthetic sample and freezes coefficients in `trust_goldens_r.json.qr[tau]`; the Rust test asserts `solve_qr` matches to rel 1e-4 (LP-solver-to-LP-solver tolerance, consistent with the existing `1e-4` in `quantile_regression.rs:149`).

---

## 3. Technical Architecture

Two independent oracles, one offline-golden pattern:

```
                        ┌─ verification/gen_parity_golden.py  (statsmodels)  [KEEP, unchanged]
Oracles (regen only) ───┤
                        └─ verification/gen_trust_goldens.R   (R oaxaca 0.1.5 + quantreg)  [NEW]
                                   │ writes (committed)
                                   ├─ oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv  (PII-stripped)
                                   ├─ oaxaca_blinder/tests/fixtures/resample_indices.csv          (shared bootstrap)
                                   └─ oaxaca_blinder/tests/fixtures/trust_goldens_r.json          (goldens)
                                   ▼ read offline by
Rust tests (cargo test, native) ──┬─ parity_test.rs                    (statsmodels golden — existing)
                                  ├─ trust_golden_r_test.rs            (R oaxaca golden — NEW)
                                  ├─ decomposition_properties_test.rs  (proptest identities — NEW)
                                  └─ qr_location_scale_test.rs         (analytic + quantreg QR — NEW)
```

**Extend-vs-replace decision (recorded).** EXTEND. The existing statsmodels pipeline is a valid independent oracle and its fixtures are frozen (INV-01). The R pipeline adds the surface the Python one lacks: R `oaxaca`'s native two/three-fold + detailed + Gardeazabal-Ugidos category-invariance + bootstrap SEs via shared indices, and `quantreg` QR goldens. Keeping both gives cross-oracle agreement (statsmodels ↔ R oaxaca to 1e-6) as a bonus trust signal.

**Why threading-independent.** Every test here runs `cargo test` native and asserts mathematical identities or reference agreement — no wasm, no thread pool. The point-estimate paths are deterministic single passes (as `parity_test.rs:61` already relies on); the bootstrap-SE path is made deterministic by the committed index matrix (R-TL-4), not by thread count. This is the METHODS suite; the MODES suite lives in the verification-benchmark spec.

---

## 4. Implementation Details

### 4.1 Pinned R environment
- Pin via `renv` lockfile committed at `verification/renv.lock`, OR an explicit version guard in the script (`stopifnot(packageVersion("oaxaca") == "0.1.5")`). Record R version, `oaxaca`, `quantreg`, and RNG kind (`RNGkind("Mersenne-Twister")` + `set.seed(N)`) in `_meta`.
- The script is regeneration-only tooling (like `gen_parity_golden.py`); it never runs in CI or at `cargo test` time.

### 4.2 Canonical formula and design-matrix alignment
- Outcome: `log(Salary)` (natural log). Predictors as R-TL-2. Reference group and categorical reference levels named explicitly in `_meta` and asserted equal to engine expansion by comparing `_meta.design_columns` (ordered) to the engine's expanded predictor list.
- Intercept naming: engine uses `__ob_intercept__` (`builder.rs:325`); the R JSON maps R's `(Intercept)` to `__ob_intercept__` for detailed rows — same convention `gen_parity_golden.py:112` established.

### 4.3 Tolerance table
| Quantity | Comparison | Tolerance | Rationale |
|---|---|---|---|
| Group means, total_gap | engine vs golden | rel 1e-6 | matches `parity_test.rs:24` |
| Two/three-fold aggregate + detailed point estimates | engine vs golden | rel 1e-6, **abs floor 1e-8** for near-zero components | closed-form OLS; floor prevents rel-tol blowup near zero (e.g. tenure-like zero channel) |
| Bootstrap SEs | engine vs golden (shared indices) | rel 1e-3 | LP/linear-algebra path divergence across R↔Rust accumulates; still exact-index, so not distributional |
| QR coefficients (analytic) | engine vs true β1(tau) | abs, n-derived (≈0.05 @ n≥5000) | finite-sample sampling error, not implementation error |
| QR coefficients (quantreg) | engine vs `rq()` | rel 1e-4 | LP-solver-to-LP-solver, matches existing QR tol |
| Internal identities | engine self-consistency | abs 1e-9 | matches `parity_test.rs:25` |

### 4.4 proptest generators
- Crate: `proptest` as a dev-dependency in `oaxaca_blinder/Cargo.toml` (verify not already present; add under `[dev-dependencies]`).
- `well_behaved()` composes: group sizes `10..=200`, predictor count `1..=4`, each column drawn from a bounded normal, then `prop_assume` the variance + condition-number guards (R-TL-6). Outcome built from a random-but-finite linear model plus bounded noise.
- Label-swap and scale-equivariance cases derive a second design from the first (swap the group vector / multiply `y` by `c`) so the identity is exact, not statistical.

### 4.5 PII handling (mandatory, explicit)
- `Employers_data.csv` columns: `Employee_ID, Name, Age, Gender, Department, Job_Title, Experience_Years, Education_Level, Location, Salary`. `Employee_ID` and `Name` are **direct identifiers** and are dropped before anything is written.
- The R script reads `/home/deji/Downloads/Employers_data.csv` (uncommitted, local only), selects `Age, Gender, Department, Job_Title, Experience_Years, Education_Level, Location, Salary`, and writes `oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv` with `Employee_ID`/`Name` absent. `Job_Title` is retained in the fixture for provenance but excluded from the model (R-TL-2).
- Goldens (`trust_goldens_r.json`) contain only aggregate statistics (means, gaps, components, SEs, QR coefficients) — no row-level data, no identifiers.
- The committed fixture is quasi-identifier-bearing (age/department/location); it is a synthetic-shaped HR dataset already in `~/Downloads` and is committed only in PII-stripped form. State in the fixture header comment: "Direct identifiers (Employee_ID, Name) stripped at generation; aggregate goldens only." No re-identification join key is committed.

---

## 5. Dependencies and Integrations
- **New dev-dependency**: `proptest` (`oaxaca_blinder/Cargo.toml` `[dev-dependencies]`).
- **R environment** (regen only, not CI): R + `oaxaca` 0.1.5 + `quantreg`, pinned via `renv.lock`. Not a build/test dependency.
- **Existing assets reused unchanged**: `gen_parity_golden.py`, `parity_test.rs`, `ground_truth_verification_test.rs` (INV-01).
- **Cross-domain**: the shared resample-index matrix (R-TL-4) is the same owned-index-resampling design the RNG/determinism work adopts (cross-domain summary §RNG). This suite consumes the index matrix as a fixture; it does not respec the engine RNG refactor.
- **CI**: these are native `cargo test` targets already covered by the existing `quality` job (`cargo test --workspace --all-features`, `ci.yml:37`). No new CI job — they run wherever `cargo test` runs. (The MODE parity/benchmark CI jobs are in the verification-benchmark spec.)

---

## 6. Risk Assessment
- **R-TL-A (medium)** — R `oaxaca` bootstrap may not accept an external index matrix, so exact SE comparison (R-TL-4) could require hand-rolling the R bootstrap loop. Mitigation: the fallback (manual per-replicate loop in the R script) is pre-specified so /build proceeds either way.
- **R-TL-B (medium)** — Categorical/ordinal coding mismatch between R `contr.treatment` and the engine dummy expansion silently shifts detailed contributions. Mitigation: the `_meta.design_columns` equality assertion (R-TL-2) fails loudly on any column-set or ordering divergence before values are compared.
- **R-TL-C (low, naming)** — `parity_test.rs` / `gen_parity_golden.py` are named "parity" but are METHOD goldens; the new threading suite is the MODE "parity". Two different "parity"s invite confusion. Mitigation: name the new files `trust_golden_r_test.rs` / `decomposition_properties_test.rs` (method) and the threading suite `mode_parity_*` (verification-benchmark spec); document the collision in both specs.
- **R-TL-D (low)** — QR analytic tolerance guessed too tight → flaky. Mitigation: derive the tolerance from the QR asymptotic SE at the chosen n (documented), not a round number; use n≥5000 synthetic rows so the tolerance is comfortable.
- **R-TL-E (low)** — `log(Salary)` fails on non-positive salaries. Mitigation: the R script asserts `all(Salary > 0)` on the fixture and errors otherwise; documented in `_meta`.

---

## Gaps Requiring Deeper Research

> NEEDS RESEARCH: Does R `oaxaca` 0.1.5 expose a hook to supply externally-computed bootstrap resample indices (for exact cross-implementation SE comparison), or must the R script reimplement the bootstrap loop manually over a committed index matrix? (single-agent: read the `oaxaca` CRAN reference + source for `n.iterations`/resampling internals.)

> NEEDS RESEARCH: What concrete R routine generates a Machado-Mata / quantile-decomposition golden for the RIF-QR path (candidate: Chernozhukov-Fernández-Val-Melly `Counterfactual` package)? Confirm the package, function signature, and whether it accepts a shared resample-index matrix. (single-agent: identify one working R routine + minimal call.)

> NEEDS RESEARCH: Confirm the exact `Education_Level` category set and any ordinal ordering present in `/home/deji/Downloads/Employers_data.csv`, to fix the R-TL-2a encoding lookup deterministically. (single-agent: read the distinct `Education_Level` values from the CSV.)

---

## 8. Spark Notes
- Two oracles, one pattern: KEEP statsmodels (`gen_parity_golden.py`), ADD R `oaxaca` 0.1.5 + `quantreg` (`gen_trust_goldens.R`) — additive, existing fixtures frozen (INV-01).
- Committed goldens are read offline by native `cargo test`; R/Python never run at test time.
- Shared resample-index matrix fixture makes bootstrap SEs compare **exactly**, not distributionally — same owned-index design the RNG refactor uses.
- proptest asserts adding-up (2- and 3-fold), detailed==aggregate, label-swap antisymmetry, scale equivariance; generators guard group≥10, variance floor, condition-number.
- QR fix: location-scale `Y=b0+b1·X+(1+c·X)·U` gives true slope `b1+c·z_tau` (tau-VARYING) — antidote to the tau-insensitive `test_solve_qr_median/quartile`; plus `quantreg::rq()` goldens.
- PII: strip `Employee_ID` + `Name` before any golden; commit only the stripped fixture + aggregate goldens.
- This is the METHODS suite (native, threading-independent). The MODES suite (native vs wasm-seq vs wasm-threaded, INV-02) is in `phase2-spec-verification-benchmark.md`. Don't conflate the two "parity"s (Risk R-TL-C).
- Open: R external-index hook (fallback pre-specified), MM golden routine, exact Education_Level categories.


## Phase 1 Sources

- phase1-statistical-trust-layer-golden-proptest.md
- phase1-verification-benchmark-browser-ci.md
