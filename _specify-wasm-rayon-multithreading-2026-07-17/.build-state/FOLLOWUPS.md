# 0014-MERIDIAN — Post-Build Follow-Ups

Disposition of the 8 non-blocking follow-ups surfaced by the stage-4 adversarial-verify
pass. Worked 2026-07-19 (David: "continue on follow up issues"). This repo has no
`issues/` tracker, so this file is the durable record. Investigation was done by an
8-agent read-only workflow; every applied fix was compiled + tested by the orchestrator
(subagent "it passes" is never trusted for statistical code).

| Item | Title | Class | Status |
|---|---|---|---|
| A | gen_trust_goldens.R "independent oracle" comment honesty | SAFE_FIX | **DONE** (comments only) |
| B | Machado-Mata module: deprecate + redirect (not delete) | DECISION → applied option B | **DONE** |
| C | pay-equity-app build.sh broken (`cd engine` aborts) | SAFE_FIX | **DONE** (rewritten) |
| D | calculate_rif n<2 silent-wrong → fail-loud | SAFE_FIX | **DONE** (real correctness fix) |
| E | bootstrap-SE golden consuming test | BLOCKED | R-unblock applied; test pending R regen |
| F | AC-13 CLI↔library/WASM numeric parity test | NEW_TEST | **DONE** (main.rs + new test) |
| G | quantile SE / density-floor tail (τ=0.1/0.9) | BLOCKED | golden gap; recorded, not applied |
| H | AC-5 (W4) structured OOM self-report | DEFER | recorded (enhancement, not a gap) |

---

## Applied (verified by orchestrator)

**A — comment honesty (`verification/gen_trust_goldens.R`).** The header, the
`library(oaxaca)` load site, and the Section-2 header claimed an "independent oracle
stack (R oaxaca/…)". Reality: the `oaxaca` R package is loaded for version provenance
only and never called; Section 2 transliterates the OB arithmetic in-script and uses R's
`lm()` only as a **fit-oracle**. The genuine independent OB-**arithmetic** oracle is
`ddecompose::ob_decompose()` (Section 5, AC-6). Three comment-only edits; zero computed
values changed.

**B — Machado-Mata: deprecate + redirect (chose option B over delete).** The MM module
(`quantile_decomposition.rs`) is orphaned from every shipped surface (CLI/WASM/MCP all use
the RIF `decompose_quantile` path) BUT is not dead code — it is public API, was the
crate's front-page doc example, and has two self-consistency tests. It is also a
statistically **distinct** estimator (Machado-Mata 2005 simulation vs RIF/Firpo-
Fortin-Lemieux linearization), not redundant. Applied:
- `#[deprecated(note=…)]` on `QuantileDecompositionBuilder` pointing to `decompose_quantile`.
- Module-doc paragraph: distinct-from-RIF, off the shipped surface, **no external oracle**
  (self-consistency only) — "treat point estimates as unverified against external ground truth."
- Rewrote the `lib.rs` crate-doc quantile example to use the RIF `OaxacaBuilder::decompose_quantile`
  path (the oracle-verified one) instead of MM.
- `#[allow(deprecated)]` / `#![allow(deprecated)]` at the 4 reference sites (module impl,
  `lib.rs` re-export, both test files) so the CI clippy `-D warnings` gate stays green.
- The two MM tests are untouched functionally — retained as self-consistency guards.

> **Open to David:** option A (delete MM entirely — breaking semver, removes an unverified
> method) remains available if you want a smaller surface. Applied the reversible,
> non-breaking option B by default. Delete is a one-word go from here.

**C — `pay-equity-app/build.sh` (broken → working).** The old script did `set -e; cd engine;
wasm-pack build …` but there is no app-local `engine/` dir, so it aborted immediately. The
shipped wasm comes from the sibling `oaxaca-blinder-rs` repo (`scripts/build-wasm.sh`, dual
seq+threaded artifacts). Rewrote build.sh to locate the sibling (default
`../oaxaca-blinder-rs`, overridable via `ENGINE_REPO`), run its build, and copy **both**
artifact sets into `frontend/src/wasm/` + `frontend/src/wasm-threaded/`, failing loud with a
clone instruction when the sibling is absent. Verified: `bash -n` clean; default
`ENGINE_REPO` resolves to the existing sibling build-wasm.sh. (Full positive run installs a
pinned nightly toolchain — not triggered here.)

**D — `calculate_rif` n<2 (silent-wrong → fail-loud). REAL CORRECTNESS FIX.** The build
plan called this branch "latent-unreachable"; the investigation proved it **REACHABLE**:
`split_groups` (builder.rs) requires ≥2 distinct group *values* but enforces **no per-group
row minimum**, so a group with one row after null-cleaning reaches `calculate_rif` with n=1
and was silently handed back its own raw series mislabeled as its RIF (agentic failure mode
#6). Replaced the silent `Ok(series.clone())` with a descriptive `PolarsError::ComputeError`.
No fixture has a group with <2 rows (all ≥80/group verified), so no golden changes. On the
bootstrap path a stray singleton resample degrades to one discarded replicate; on the
point-estimate path a genuinely-degenerate group now errors loudly instead of producing a
bogus number — the correct fail-safe direction for a pay-equity engine.

**F — AC-13 CLI↔library/WASM numeric parity test.** AC-13 (CLI shares the WASM/MCP
`decompose_quantile` path) was asserted only by a code comment. Two changes:
- `main.rs run_quantile_analysis`: wired the already-existing `OaxacaResults::to_json()` into
  the quantile loop (`--output-json` was previously silently ignored on the quantile path;
  multi-quantile runs now get a `.qX.XX` suffix per tau).
- New `tests/cli_wasm_parity_test.rs`: runs the CLI subprocess on `tests/data/wage.csv`
  (τ=0.5, GroupB, 2 reps) and compares its JSON to a direct in-process
  `OaxacaBuilder::decompose_quantile` call at 1e-9. Both resolve to `DEFAULT_SEED` (no
  `--seed` flag exists) so the bootstrap std_errs are bit-identical. CLI==library proves
  CLI==WASM transitively without a browser (the browser leg was proven in stage 4D).

---

## Blocked (need R-machine golden regeneration — cannot be honestly written now)

**E — bootstrap-SE consuming test. BLOCKED.** `trust_goldens_r.json.bootstrap` has the SE
goldens and `resample_indices.csv` has the 60×800 within-group indices, BUT the generator
(`gen_trust_goldens.R` §3) drew a *random* 800-row subset via `sub_idx <-
sort(sample.int(nrow(fxr), 800))` and **never committed `sub_idx`**. The Rust side cannot
know which 800 of the 10,000 rows R used, and cannot reproduce R's Mersenne-Twister
`sample.int` (no equivalent sampler exists in the engine). Writing the test now would
compare SEs computed on *different* row sets — a fabricated green.
- **Applied unblock:** added `sub_idx_0based = as.integer(sub_idx - 1L)` to the `bootstrap`
  list in `gen_trust_goldens.R` (unverified — no R here). 
- **To complete:** run `Rscript verification/gen_trust_goldens.R` on an R+oaxaca/ddecompose
  machine to regenerate `trust_goldens_r.json` with `sub_idx_0based` populated (800 distinct
  ints in [0,9999]); THEN write `tests/bootstrap_se_golden_test.rs` (reconstruct
  `sub`=fixture[sub_idx], split A/B, apply resample_indices per rep, GroupB
  `bootstrap_reps(1)` decompose per rep, sample-SD, assert vs `se_explained/se_unexplained/
  se_total_gap` at rel 1e-3 with a key-set/vacuous-pass guard per AC-6).
- Not blocking: point-agreement (1e-6) + INV-02 determinism already cover the SE intent.

**G — quantile SE / density-floor tail (τ=0.1, τ=0.9). BLOCKED (golden gap).** The AC-6
ddecompose golden is **point-estimate only** — no per-tau SE fields exist in
`trust_goldens_r.json.quantile_detail`, even though `gen_trust_goldens.R` §5 runs
`ddecompose(..., bootstrap=TRUE, 200 iters)` (the SEs are computed then discarded before
write). To build this: §5 must extract ddecompose's per-tau/per-predictor SE column and add
`*_se` fields to the golden, then a new test asserts engine-vs-ddecompose tail-tau SE at a
measured-then-pinned tau-dependent tolerance (looser at τ=0.9, where `1/f(q_tau)` amplifies
density-estimator divergence into the SE). **NOT applied:** the investigator's draft §5 edit
*guesses* the ddecompose SE column names (`Std_error_Composition` etc.) unverified against a
live package — applying it could break golden regeneration. Depends on E's R-machine access.
Lowest priority.

---

## Deferred (enhancement, not a gap)

**H — AC-5 (W4) structured OOM self-report. DEFERRED.** Unimplemented. The PRIMARY OOM
defense is in place and validated: bounded-parallel bootstrap (chunked to pool size) +
`N_max_const=8` + `--max-memory` link budget; AC-4 measured peak 198 MiB at 50k rows / N=8
vs a 326 MiB budget (~128 MiB headroom). W4 would turn a future over-budget allocation
(an opaque `WebAssembly.RuntimeError` trap) into a caught, structured `{error:"OOM",…}`
self-report the caller can surface. Because the system stays under budget by construction,
this is graceful-degradation for workloads beyond the validated envelope — not a correctness
or reliability gap. Re-surface if a real workload approaches the budget.

---

## Verification summary

- `cargo build -p oaxaca_blinder` (default features): clean, no deprecation warnings.
- `cargo test -p oaxaca_blinder`: (see build-plan / session log for the run result).
- `bash -n build.sh` (pay-equity-app): clean; default ENGINE_REPO resolves to the sibling.
- A/E edits to `gen_trust_goldens.R` are regeneration-only tooling — not exercised at
  `cargo test` time; unverified pending an R-equipped machine.

**Not committed.** All changes left uncommitted pending David's review (global rule: no
auto-commit without explicit request). Spans oaxaca-blinder-rs (A,B,D,E,F) + pay-equity-app (C).
