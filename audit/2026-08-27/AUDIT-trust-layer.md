# Trust-Layer Audit — oaxaca-blinder-rs

Scope: the 6 named test files plus the 2 named generator scripts. Read in the order given.
Tool budget note: this is a partial audit if it says so at the end — see final section.

## Files read so far

1. `oaxaca_blinder/tests/parity_test.rs` — full read
2. `oaxaca_blinder/tests/trust_golden_r_test.rs` — full read
3. `oaxaca_blinder/tests/ground_truth_verification_test.rs` — full read
4. `oaxaca_blinder/tests/decomposition_properties_test.rs` — full read
5. `oaxaca_blinder/tests/qr_location_scale_test.rs` — full read
6. `verification/gen_parity_golden.py` — full read
(quantile_detail_golden_test.rs also read alongside gen_parity_golden.py)

Not yet read: `verification/gen_trust_goldens.R`.

## Finding 1 — Two-fold decomposition: dual external oracle

`oaxaca_blinder/tests/parity_test.rs:70` `test_parity_two_fold_decomposition` — verifies engine
`OaxacaBuilder` (GroupB scheme) two-fold `explained`/`unexplained`, `total_gap`, and every
per-variable `detailed_explained`/`detailed_unexplained` component against a committed golden
(`tests/fixtures/parity_golden.json`) produced by Python statsmodels `OaxacaBlinder`, at
`TOLERANCE: f64 = 1e-6` (line 24). A second scheme (Pooled, statsmodels
`two_fold_type='pooled'`) is cross-checked at lines 159-167, same tolerance.

`oaxaca_blinder/tests/trust_golden_r_test.rs:78` `ac3_trust_golden_r_two_fold` — SECOND,
independent oracle: R `lm()` group fits via `verification/gen_trust_goldens.R`, golden at
`tests/fixtures/trust_goldens_r.json`. Tolerance `REL_TOL: f64 = 1e-6` with `ABS_FLOOR: f64 =
1e-8` (lines 25-26) — `assert_close` at line 44 uses `max(floor, rel*|golden|)`. Also asserts
design-matrix column SET equality (lines 101-112) before any value comparison — guards against
silent base-category/dummy-naming divergence with a categorical fixture (10k rows, 3-level
categoricals per header comment line 6).

So two-fold decomposition point estimates are checked against two independent external oracles
(Python statsmodels AND R lm()), both at 1e-6 relative tolerance, both on committed goldens.

## Finding 2 — Three-fold decomposition: one external oracle + one hand-computed case

`trust_golden_r_test.rs:162-184` — three-fold aggregate (`endowments`, `coefficients`,
`interaction`) verified against the R-oracle golden (`gb["three_fold"]["aggregate"]`), same
REL_TOL/ABS_FLOOR as two-fold.

`ground_truth_verification_test.rs:19` `test_three_fold_ground_truth` — additionally verified
against a hand-computed closed-form case: two groups with deterministic linear wage functions
(`wage = 4 + 1.0*edu` for F, `wage = 5 + 1.5*edu` for M), where the algebra is worked out in the
comment (lines 20-25: `endowments = 2.0`, `coefficients = 7.5`, `interaction = 1.0`) and asserted
at `1e-6` (lines 69-71) / `1e-9` for the gap (line 45). No per-variable detail is oracle-checked
for three-fold in `parity_test.rs` (parity_test's `meanpath_point_estimates` DOES capture
three-fold aggregate into the byte-frozen baseline at lines 189-193, but that is a
regression-freeze against a prior engine run, not an external oracle — see Finding 6).

Three-fold detailed/per-variable components (`detailed_explained`/`detailed_unexplained` analog
for three-fold, if the engine exposes one) are not visibly checked against an external oracle in
any of the 6 files — only the aggregate three-way split is.

## Finding 3 — RIF quantile decomposition: external oracle (ddecompose) + one location-shift ground-truth case + weak spot at tail

`quantile_detail_golden_test.rs:91` `ac6_quantile_detail_golden_ddecompose` — validates the
shipped one-stage RIF-OLS quantile decomposition (`OaxacaBuilder::decompose_quantile`) against R
`ddecompose::ob_decompose(..., reweighting=FALSE)` at three quantiles (tau = 0.10, 0.50, 0.90).
Three layers: (1) KEY-SET guard (lines 149-159, defeats vacuous-pass), (2) aggregate vs
ddecompose (lines 161-176), (3) per-predictor vs ddecompose (lines 178-215) with a
"vacuous-pass backstop" asserting every golden key was actually compared (lines 208-215).

Tolerance is TAU-DEPENDENT and explicitly documented as measured-then-pinned (lines 24-33,
47-67): at tau <0.85, aggregate tol `1e-4`, per-predictor tol `5e-4`; at tau>=0.85 (i.e. the
upper-tail quantile 0.9 in this suite), aggregate tol widens to `5e-3` and per-predictor tol to
`2e-2` — a 40-140x loosening relative to the well-conditioned quantiles. The comment states this
reflects a real, disclosed divergence between two independent density estimators (engine's inline
`bw.nrd0` + exact single-point Gaussian kernel vs ddecompose's `bw.nrd0` + 512-pt FFT grid), not
engine error — but it does mean the external-oracle check at the tail is materially weaker (2e-2
absolute band) than at the median (5e-4).

`ground_truth_verification_test.rs:78` `test_quantile_decomposition_location_shift_ground_truth`
— separate, weaker check: a pure location-shift design (M = F + 5, identical X and noise) where
the true answer is analytically known at every quantile (gap≈5, characteristics≈0,
coefficients≈5). Asserted at wide bands: `(gap - 5.0).abs() < 1.0` (line 135), `chars.abs() <
0.75` (line 136), `(coeffs - 5.0).abs() < 1.0` (line 137) — these are Monte-Carlo-noise bands, not
precision checks; the comment (lines 111-115) explicitly says the 0.75 band was chosen because
2500 simulations still leave residual MC error. This test also runs the `#[allow(deprecated)]`
`QuantileDecompositionBuilder` (line 6-8 comment: "MM sim" — a different implementation path than
`decompose_quantile` used in AC-6), not the shipped `OaxacaBuilder::decompose_quantile` engine
path checked by AC-6. So this ground-truth test exercises a DIFFERENT (deprecated) code path than
the one checked against ddecompose.

## Finding 4 — Quantile regression (solve_qr / qr_coefficients): external oracle, WITH an explicit self-diagnosed prior test-data flaw

`qr_location_scale_test.rs:1-11` (module doc) states directly: "The existing QR unit tests use
perfectly-linear data where every quantile has the SAME slope — tau-insensitive, so a broken tau
would pass." This is the exact failure mode the audit was asked to check for, and the test file's
own authors already found and named it.

The fix in this file: `ac5_qr_location_scale_tau_varying` (line 33) uses a heteroskedastic
location-scale design `Y = b0 + b1*X + (1+c*X)*U` whose true slope is analytically
tau-VARYING (`beta1(tau) = b1 + c*z_tau`). Checked against two oracles:
- R `quantreg::rq()` slopes, `RQ_REL_TOL = 1e-4` (line 18, asserted lines 63-70)
- the closed-form analytic slope, `ANALYTIC_ABS_TOL = 0.06` (line 19, asserted lines 72-76)

Plus an explicit discrimination assertion (lines 79-89): `slope(0.9) - slope(0.1) >=
c*(z_0.9-z_0.1)*0.8` — a lower bound engineered so that a tau-insensitive (bugged) QR
implementation CANNOT pass, i.e. this is a deliberate mutation-style test, not just a value match.

Open question this raises: does "the existing QR unit tests" (perfectly-linear, tau-insensitive)
still exist elsewhere in the tree and still run in CI alongside this fixed test? This file's
existence does not by itself remove the old ones. Flagged for direct verification — see Q2 answer
below; requires reading QR unit tests outside the 6 named files (out of the given file list, but
checked via targeted grep since it directly bears on Q2).

## Finding 5 — Bootstrap SE / CI: NOT checked against an external oracle in any of the 6 files

Every file that asserts against a golden explicitly carves bootstrap fields OUT of the frozen/
oracle-checked comparison:
- `parity_test.rs:16` "only point estimates (`.estimate`) are frozen; bootstrap SE/t/p/CI are
  RNG-dependent." `bootstrap_reps(1)` is used everywhere a golden comparison happens (e.g. line
  65, line 63 in trust_golden_r_test.rs) specifically to make bootstrap fields not participate —
  comment at parity_test.rs:61-62: "reps>=1: point estimates are deterministic... No bootstrap
  field is asserted."
- `gen_parity_golden.py:179` golden JSON notes field: "Bootstrap-derived fields
  (std_err,t_stat,p_value,ci_*) are NOT frozen (unseeded RNG)."

No test among the 6 files asserts a bootstrap standard error or confidence interval value against
an external oracle (R or statsmodels bootstrap SE) or even against a hand-computed expected SE.
The only bootstrap-adjacent activity in these 6 files is: (a) setting `bootstrap_reps(1)` or
`bootstrap_reps(2)`/`bootstrap_reps(5)` to keep runs fast, without checking the resulting SE/CI
values at all, and (b) `decomposition_properties_test.rs`'s adding-up/antisymmetry/scale
properties, which operate on point-estimate fields only (`.estimate`), not on SE/CI.

This means, within the 6 files audited: bootstrap SE/CI verification standard is strictly WEAKER
than point-estimate verification — see Q3 answer below for the full statement.

## Finding 6 — parity_test.rs also carries a byte-exact regression-freeze test, distinct from the oracle test

`parity_test.rs:211` `meanpath_point_estimates_byte_unchanged` — compares CURRENT engine output
(two_fold, three_fold aggregates, detailed_explained/unexplained, for both GroupB and Pooled
schemes) against `tests/fixtures/parity_meanpath_baseline.json`, described (line 170-174) as
"captured pre-refactor by `examples/capture_meanpath_baseline.rs`." This is a
SELF-CONSISTENCY/regression check (current engine == past engine snapshot), NOT an external
oracle check — it would happily freeze a wrong number forever if the pre-refactor baseline was
itself wrong, since the file states its only job is to prove "the RNG refactor must not touch
point math" (line 223), not that the point math is correct. Its correctness burden is carried
entirely by the OTHER tests in this file (the statsmodels-oracle comparison above it in the same
file).

## Finding 7 — decomposition_properties_test.rs is explicitly self-consistency, and says so

`decomposition_properties_test.rs:10-13` (module doc): "Note (stats-reviewer caveat): adding-up
is NON-diagnostic of statistical correctness — a wrong RIF/density would still sum perfectly.
Correctness against an independent oracle lives in `trust_golden_r_test.rs` / `parity_test.rs`.
These properties guard the algebra (no double-counting, no dropped channel) across the whole
input space."

Identities checked, all self-consistency (proptest, 256 cases, `TOL = 1e-9`):
- `two_fold_adding_up` (line 60): explained + unexplained == total_gap
- `three_fold_adding_up` (line 71): endowments + coefficients + interaction == total_gap
- `detailed_equals_aggregate` (line 83): Σ detailed == aggregate, both explained and unexplained
- `label_swap_antisymmetry` (line 96): total_gap(ref=B) == -total_gap(ref=A)
- `scale_equivariance` (line 108): scaling Y by k scales gap/explained by exactly k
- `stress_returns_err` (line 130): degenerate/undersized designs return Err, never panic — this
  one is a crash guard, not a correctness guard at all (see Q-crash-vs-wrong-number below)

These run on RANDOMIZED designs (proptest strategy, lines 49-54: 24-80 rows, uniform [-10,10]),
so they cover a wide input space for algebraic identities — but by the file's own admission,
identities alone cannot catch a wrong RIF/density/regression coefficient that still sums
correctly.

## Finding 8 — gen_trust_goldens.R generates a bootstrap-SE golden with NO consuming Rust test

`verification/gen_trust_goldens.R:176-239` (Section 3) generates a bootstrap standard-error
golden (`bootstrap` block: `se_explained`, `se_unexplained`, `se_total_gap`, plus committed
per-replicate resample indices `resample_indices.csv`, 182KB, git-tracked) via a manual R
resampling loop (60 reps on an 800-row subset, exact indices committed so Rust can reproduce the
same resamples). The script's own comment at lines 231-233 states explicitly:

> "the consuming Rust bootstrap-SE test is a documented TODO — the golden + indices are ready for
> it. Without this, ... the bootstrap-SE consuming test (`tests/bootstrap_se_golden_test.rs`)
> stays BLOCKED until this golden is regenerated."

Verified directly: `oaxaca_blinder/tests/bootstrap_se_golden_test.rs` **does not exist** in the
repo (confirmed via `ls oaxaca_blinder/tests/` — full listing has no file of that name, only
`memory_ceiling_test.rs`, `quantile_threading_test.rs`, `rng_determinism.rs`, etc., none of which
consume the `bootstrap` block of `trust_goldens_r.json`). The golden data is generated, committed
(`_meta.tolerances.bootstrap_se: 1e-3` at `gen_trust_goldens.R:335`), and named for exactly this
purpose — but nothing in the 6 audited test files, or the wider `oaxaca_blinder/tests/`
directory, reads it. This means bootstrap SE has zero external-oracle verification anywhere in
the shipped test suite, not merely a weaker one — see Q3.

`gen_trust_goldens.R:16` also makes an explicit, load-bearing methodological admission for
three-fold/two-fold: "Section 2's OB arithmetic is transliterated in-script from
builder.rs/decomposition.rs ... it is NOT an independent implementation of the OB arithmetic."
i.e., the R-`lm()` oracle (`ac3_trust_golden_r_two_fold`) independently verifies the **OLS fit**
only; the OB decomposition arithmetic on top of that fit is copied from the Rust source into the
R script, so it cannot catch a decomposition-formula bug shared between the two — only
`ddecompose::ob_decompose()` (Section 5, quantile only) is a genuinely independent
re-implementation of decomposition arithmetic (line 118-119: "The independent OB-arithmetic
oracle is `ddecompose::ob_decompose()`"). The mean-path two-fold/three-fold decomposition
arithmetic itself is independently oracled only by `parity_test.rs` (statsmodels), which fits
its own independent formula in Python (`gen_parity_golden.py:105-118` `per_variable()` — uses
statsmodels' own OLS params, not a copy of Rust source).

## Finding 9 — the flawed tau-insensitive QR unit tests still exist and still run in CI

Verified directly (outside the 6 named files, but required to answer the stated Q2):
`oaxaca_blinder/src/math/quantile_regression.rs:136-152` `test_solve_qr_median` and
`:154-170` `test_solve_qr_quartile` both use the identical dataset `y = [1,2,3,4,5]`, `x =
[[1,1],[1,2],[1,3],[1,4],[1,5]]` (perfectly linear, noise-free), with the comment verbatim: "For
perfectly linear data, the QR line should be the same for all quantiles" (lines 143, 161). These
assert `expected_betas = [0.0, 1.0]` at both tau=0.5 and tau=0.25 — same expected answer for two
different tau values, on data where every quantile's true slope is identical by construction. A
QR implementation that silently ignored `tau` entirely (always solving OLS or always solving the
median) would pass both tests. These are `#[cfg(test)]` unit tests inside `src/math/
quantile_regression.rs`, run by `cargo test --workspace --all-features` (`.github/workflows/
ci.yml:41` `Run Tests`) — the same CI job that runs `qr_location_scale_test.rs`. Both the flawed
test and its fix (`qr_location_scale_test.rs`, Finding 4) currently coexist and both pass; the
flawed one was not removed when the fix was added.

## Finding 10 — no CI check that committed goldens still match what the generator would produce today

`.github/workflows/ci.yml` (grepped for `golden`, `gen_parity_golden`, `gen_trust_goldens`, `.R`,
`Rscript`, `statsmodels`) returns zero matches. The CI job "Run Tests" (`ci.yml:40-41`, `cargo
test --workspace --all-features`) only runs the Rust test binaries against the ALREADY-COMMITTED
JSON/CSV fixtures (`tests/fixtures/*.json`, `*.csv` — confirmed git-tracked via `git ls-files`:
`parity_golden.json`, `parity_fixture.csv`, `parity_meanpath_baseline.json`,
`employers_trust_fixture.csv`, `trust_goldens_r.json`, `resample_indices.csv`). Neither
`verification/gen_parity_golden.py` nor `verification/gen_trust_goldens.R` is invoked anywhere in
`.github/workflows/`. Both scripts' own headers state they are "REGENERATION TOOLING ONLY... NOT
run at test time" (`gen_parity_golden.py:5`, `gen_trust_goldens.R:5`) — by design, not oversight —
but the consequence is that there is no automated forcing function verifying the committed golden
JSON still matches what a fresh run of statsmodels/R would produce today. A drift between the
committed golden and current-library-version reference values (e.g., a statsmodels 0.14→0.15
behavior change, or hand-editing the JSON) would go undetected indefinitely; the test suite would
keep passing against a stale/wrong committed number. Regeneration is manual and trust-based:
comment instructs "run `python verification/gen_parity_golden.py` ... and re-commit both
fixtures" (`parity_test.rs:10-11`).

Additionally `gen_trust_goldens.R:55` hardcodes an absolute input path outside the repo
(`RAW_CSV <- "/home/deji/Downloads/Employers_data.csv"`) — the raw (pre-PII-strip) source data is
not present in the repo and the script cannot be re-run by anyone without that exact local file,
which further concentrates regeneration to one specific machine/person and makes independent
re-verification harder.

---

## The Matrix

Estimators are as they appear across the 6 files. Tolerance values quoted from source.

| Estimator | (1) External oracle — name, tolerance | (2) Self-consistency only | (3) Fixed hand-computed only | (4) Not verified |
|---|---|---|---|---|
| **Two-fold decomposition (point estimates, aggregate + per-variable)** | statsmodels `OaxacaBlinder`, `1e-6` — `parity_test.rs:70` `test_parity_two_fold_decomposition` (line 24 `TOLERANCE`). AND R `lm()`-fit oracle, `rel 1e-6 / abs floor 1e-8` — `trust_golden_r_test.rs:78` `ac3_trust_golden_r_two_fold` (lines 25-26). Two independent oracles. | Adding-up (`explained+unexplained==gap`) at `1e-9` — `decomposition_properties_test.rs:60` `two_fold_adding_up`; also `detailed_equals_aggregate` line 83; antisymmetry `label_swap_antisymmetry` line 96; scale-equivariance `scale_equivariance` line 108 | — | — |
| **Three-fold decomposition (aggregate only: endowments/coefficients/interaction)** | R `lm()`-fit oracle, same tol — `trust_golden_r_test.rs:162-184` (part of `ac3_trust_golden_r_two_fold`) | Adding-up at `1e-9` — `decomposition_properties_test.rs:71` `three_fold_adding_up` | Hand-computed linear-wage case (`endowments=2.0, coefficients=7.5, interaction=1.0`), `1e-6` — `ground_truth_verification_test.rs:19` `test_three_fold_ground_truth` (lines 20-25, 69-71) | Three-fold **per-variable detail** (`detailed_endowments/coefficients/interaction`, present in the golden JSON at `gen_trust_goldens.R:169-171` but never read by any of the 6 Rust test files against those keys) |
| **RIF quantile decomposition (one-stage RIF-OLS, `decompose_quantile`, the shipped path)** | R `ddecompose::ob_decompose()`, tau-dependent: aggregate `1e-4` (tau<0.85) / `5e-3` (tau≥0.85); per-predictor `5e-4` (tau<0.85) / `2e-2` (tau≥0.85) — `quantile_detail_golden_test.rs:91` `ac6_quantile_detail_golden_ddecompose` (lines 47-67) | Σdetail==aggregate at `1e-9` (`SELF_TOL`, line 42), plus key-set + vacuous-pass guards (lines 149-159, 208-215) | — | — |
| **Quantile decomposition (deprecated `QuantileDecompositionBuilder`, MM-simulation path)** | — (not oracle-checked; different code path than `decompose_quantile`) | Adding-up (`chars+coeffs==gap`) at `1e-9` — `ground_truth_verification_test.rs:130` | Location-shift hand-known case (gap≈5, chars≈0, coeffs≈5) at WIDE MC-noise bands `<1.0` / `<0.75` / `<1.0` — `ground_truth_verification_test.rs:79` `test_quantile_decomposition_location_shift_ground_truth` (lines 135-137) | — |
| **Quantile regression (`solve_qr` / `qr_coefficients`)** | R `quantreg::rq()`, `rel 1e-4` — `qr_location_scale_test.rs:33` `ac5_qr_location_scale_tau_varying` (line 18 `RQ_REL_TOL`, asserted 63-70). Also analytic closed-form slope, abs `0.06` (line 19, 72-76) | Discrimination inequality `slope(0.9)-slope(0.1) >= c*(z-spread)*0.8` — engineered so a tau-insensitive bug cannot pass (lines 79-89) | **Superseded-but-still-present** flawed case: perfectly-linear data, tau=0.5 and tau=0.25 both expect `[0.0,1.0]`, `1e-4` — `src/math/quantile_regression.rs:136-170` `test_solve_qr_median`/`test_solve_qr_quartile`. Still runs in CI; see Finding 9/Q2. | — |
| **OLS (mean-path regression underlying two-fold/three-fold)** | Implicitly via R `lm()` fit comparison (the coefficients feeding `trust_golden_r_test.rs`'s golden ARE `lm()` coefficients, i.e. OLS is being checked as a side effect of checking the decomposition built on it) — no test in the 6 files isolates raw OLS coefficients as their own assertion | — | — | No standalone OLS-coefficient-vs-oracle test among the 6 files (it rides inside the decomposition tests only) |
| **Bootstrap standard errors** | Golden DATA exists (R manual bootstrap loop + committed exact resample indices, `rel 1e-3` planned) — `verification/gen_trust_goldens.R:176-239` `_meta.tolerances.bootstrap_se: 1e-3` (line 335) — **but the consuming test `tests/bootstrap_se_golden_test.rs` does not exist in the repo.** See Finding 8. | — | — | **YES — confirmed not verified.** Every golden-comparison test explicitly excludes bootstrap fields: `parity_test.rs:16` "only point estimates (`.estimate`) are frozen; bootstrap SE/t/p/CI are RNG-dependent"; `gen_parity_golden.py:179` "Bootstrap-derived fields ... are NOT frozen." `bootstrap_reps(1)` is used in every golden-comparison test specifically to sidestep bootstrap output. |
| **Bootstrap confidence intervals** | — (same golden-not-consumed situation; no CI-specific golden even generated — the R bootstrap block only produces SEs, not CI golden values) | — | — | **Not verified anywhere in the 6 files or their fixtures.** No CI (confidence interval) golden values exist even in `trust_goldens_r.json`'s `bootstrap` block (only `se_explained`, `se_unexplained`, `se_total_gap` — no CI bounds). |
| **Design-matrix / dummy-encoding correctness (categorical base-level selection)** | Structural SET-equality guard vs R's `contr.treatment` column names — `trust_golden_r_test.rs:101-112` (AC-3 structural guard, checked before any value) and `quantile_detail_golden_test.rs:149-159` (key-set guard, per tau) | — | — | — |
| **Crash-safety on degenerate/undersized designs** | — | `stress_returns_err` — degenerate designs must return `Err`, not panic — `decomposition_properties_test.rs:130` (this is explicitly a crash guard, not a correctness guard — see Q-answer below) | — | — |
| **Point-estimate byte-stability across refactors (mean-path)** | — | `meanpath_point_estimates_byte_unchanged` — current engine output frozen against a PRE-REFACTOR SNAPSHOT (not an oracle) — `parity_test.rs:211-225` | — | — |

---

## Answers to the three questions

### Q1 — Are golden fixtures committed to the repo, or regenerated at test time? Is anything checking they still match what the generator would produce today?

**Committed.** Confirmed via `git ls-files oaxaca_blinder/tests/fixtures/`: `parity_fixture.csv`,
`parity_golden.json`, `parity_meanpath_baseline.json`, `employers_trust_fixture.csv`,
`trust_goldens_r.json`, `resample_indices.csv` are all git-tracked. Both generator scripts state
explicitly they are regeneration-only and NOT run at test time: `parity_test.rs:7-8` "This test
is OFFLINE — no network, no Python, no statsmodels at test time"; `gen_trust_goldens.R:5-6`
"NOT run at `cargo test` time — the Rust trust tests read the committed JSON offline."

**Nothing checks the goldens still match what the generator would produce today.**
`.github/workflows/ci.yml` never invokes `gen_parity_golden.py` or `gen_trust_goldens.R` (grep
returned zero matches for either script name, "golden", or "Rscript" in the workflow file).
Regeneration is a fully manual, undated, unenforced step — the only instruction is a code
comment: "Regeneration: run `python verification/gen_parity_golden.py` ... and re-commit both
fixtures" (`parity_test.rs:10-11`). No `last-verified` date, no scheduled CI job, no drift check
exists for either golden file within the audited scope. `gen_trust_goldens.R:55` additionally
hardcodes a personal absolute path to un-committed raw source data
(`/home/deji/Downloads/Employers_data.csv`), meaning even a manual regeneration can only be
performed by whoever has that exact file at that exact path — a further practical barrier to
ever re-running the check. See Finding 10.

### Q2 — Do any tests use data whose structure makes them incapable of detecting the regression they appear to guard? Specifically: does the QR-tau-insensitive-linear-data problem hold here now?

**Yes, and the codebase's own authors found and partially fixed exactly this — but did not
remove the original flawed tests, which still run in CI.**

The flaw: `oaxaca_blinder/src/math/quantile_regression.rs:136-170`
(`test_solve_qr_median`, `test_solve_qr_quartile`) both fit `y=[1,2,3,4,5]` against
`x=[[1,1]...[1,5]]` — perfectly linear, zero-noise data — and both assert the SAME expected betas
`[0.0, 1.0]` at two different tau values (0.5 and 0.25). Verbatim comment: "For perfectly linear
data, the QR line should be the same for all quantiles" (line 143/161). On this data every
quantile's true conditional slope is identical by construction, so a QR solver that silently
ignored `tau` — e.g. one that always solved OLS, or that hardcoded the median regardless of the
requested quantile — would pass both tests. This is precisely the failure mode named in the
audit prompt.

The fix exists and is explicit about diagnosing the same problem: `qr_location_scale_test.rs:1-11`
module doc states, verbatim: "The existing QR unit tests use perfectly-linear data where every
quantile has the SAME slope — tau-insensitive, so a broken tau would pass." Its replacement test
(`ac5_qr_location_scale_tau_varying`, line 33) uses a heteroskedastic location-scale design whose
true slope genuinely varies with tau, checked against R `quantreg::rq()` AND a closed-form
analytic slope, AND an explicit discrimination inequality (lines 79-89) engineered so a
tau-insensitive implementation mathematically cannot pass.

**But** the original flawed tests were not deleted when the fix landed. Both
`test_solve_qr_median`/`test_solve_qr_quartile` (in `src/math/quantile_regression.rs`) and
`ac5_qr_location_scale_tau_varying` (in `tests/qr_location_scale_test.rs`) currently coexist and
both run under `cargo test --workspace --all-features` (`.github/workflows/ci.yml:40-41`). The
flawed tests are not currently load-bearing for tau-correctness (the new test structurally cannot
be satisfied by a tau-bug) but they remain in the suite, would give false confidence to anyone
reading test names/counts without reading content, and add nothing the new test doesn't already
subsume more rigorously.

I did not exhaustively audit every other test file in the repo for the same
same-value-across-conditions flaw (e.g., whether other math modules — `probit.rs`, `logit.rs`,
`akm.rs`, `heckman.rs` — have analogous "answer doesn't depend on the parameter under test" unit
tests); that is out of the stated scope of the 6 files + 2 scripts and would need a separate pass.

### Q3 — Are point estimates and uncertainty (SEs, CIs) verified to the same standard, or is one weaker?

**Uncertainty is verified to a categorically weaker standard than point estimates — and for
confidence intervals, not verified at all within the shipped test suite.**

Point estimates for two-fold, three-fold, and RIF-quantile decomposition all have committed
external-oracle goldens (statsmodels and/or R `lm()`/`ddecompose`) at tight tolerances (`1e-6`
relative for mean-path; tau-dependent `5e-4`–`2e-2` for quantile detail), each with a
self-consistency backstop (`1e-9`) layered on top, and — for RIF-quantile and design-matrix
alignment — additional structural (key-set) guards specifically engineered to defeat vacuous
passes.

Bootstrap standard errors have a golden DATASET generated and committed (exact resample indices
+ R-computed SEs, planned tolerance `1e-3`, `gen_trust_goldens.R:176-239` / `:335`) but **zero
consuming Rust test** — `tests/bootstrap_se_golden_test.rs` does not exist (verified by directory
listing). The script itself documents this as an open TODO, not a design choice: "the consuming
Rust bootstrap-SE test is a documented TODO" (`gen_trust_goldens.R:231-232`). Every test that DOES
touch a golden explicitly sets `bootstrap_reps(1)` (or a small number, `2`/`5`, in the
ground-truth file) specifically so bootstrap output is not exercised, and every golden JSON
explicitly marks SE/CI fields as unfrozen/RNG-dependent (`parity_test.rs:16`,
`gen_parity_golden.py:179`).

Confidence intervals are weaker still: no CI golden values exist anywhere in the audited scope —
`trust_goldens_r.json`'s `bootstrap` block (per `gen_trust_goldens.R:225-239`) produces only
`se_explained`/`se_unexplained`/`se_total_gap`, no CI bounds. No test in the 6 files asserts a CI
value against any oracle or hand-computed case.

So the standard, from strongest to weakest within this codebase, is: point estimates (dual
external oracle + self-consistency + structural guards) > bootstrap SEs (oracle data exists,
unused — effectively self-consistency-only in practice today, since nothing compares it to R) >
bootstrap CIs (no oracle data exists at all; not verified by any means found in scope). A wrong
number in the reported gap or explained/unexplained split would very likely be caught by this
suite. A wrong bootstrap SE or a wrong/mis-scaled confidence interval — the numbers a firm would
actually put in front of a court alongside the point estimate — would not currently be caught by
anything in these 6 files, and would not be caught by anything in the wider `oaxaca_blinder/
tests/` directory either (no file named `bootstrap_se_golden_test.rs` exists to consume the
golden that was built for exactly this purpose).

---

## Coverage note

All 8 requested files were read in full. Two additional files were read beyond the requested set
because the audit prompt's Q2 could not be answered without them: `oaxaca_blinder/src/math/
quantile_regression.rs` (to check whether the tau-insensitive unit tests still exist) and a
`git ls-files` / `ls` check of `oaxaca_blinder/tests/` and `.github/workflows/ci.yml` (to check
whether a `bootstrap_se_golden_test.rs` exists and whether CI regenerates goldens). This audit did
NOT read: `oaxaca_blinder/src/decomposition.rs`, `builder.rs`, `rif.rs`, or any other engine
source beyond the two quoted above — so claims about *why* a number is computed a certain way are
taken from test/generator comments, not independently verified against engine source, except
where explicitly marked. This audit also did NOT check the `engine` crate (pay-equity-engine) or
`meridian-mcp` crate's own test suites, nor any test file outside the 6 named + the 2 named
scripts + the 2 files read to resolve Q2/Q1. Tool budget was not exhausted; this is a complete
answer to the stated scope, with the two explicitly-noted extensions.
