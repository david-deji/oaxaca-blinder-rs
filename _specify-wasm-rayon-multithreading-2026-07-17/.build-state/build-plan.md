# Build Plan — 0014-MERIDIAN: WASM Rayon Multithreading + Statistical Trust

> Created: 2026-07-18 · Tier: balanced · Charter mode: TRUE
> Spec: `_specify-wasm-rayon-multithreading-2026-07-17/` (7 phase4-final-*.md + spec.yaml + spec-charter.md + buildability-gate-rulings.md)
> Deliverable type: app (in-place modification of 3 existing repos — NOT an apps/ scaffold)
> Blast radius: client-facing (wrong stats numbers = defensibility harm on David's day-job)

## Repos & branches (isolation)

| Repo | Path | Build branch | Base |
|---|---|---|---|
| oaxaca-blinder-rs (engine, primary) | apps/hr-apps/oaxaca-blinder-rs | `build/wasm-rayon-mt/main` | main @ cfcca71 |
| pay-equity-app (Meridian frontend) | apps/hr-apps/pay-equity-app | (create at threading stage) | main |
| audit-forge (Flask server) | /home/deji/telos/audit-forge | (create at threading stage) | master (has 1 foreign uncommitted: _shell.html) |

## Phase 0 findings (2026-07-18)

- **E1 GREEN**: current engine graph compiles to wasm32 under `nightly-2025-06-27` + `-Zbuild-std=panic_abort,std` + `+atomics,+bulk-memory,+mutable-globals` (1m22s, exit 0). → nightly pin = **2025-06-27** (spec's 2024-08-02 was provisional; MJ-4 subgraph-trim NOT needed for compile).
- **rand_chacha 0.3.1** already in the dep graph (E1 log). `set_stream` available.
- Native workspace builds clean (3m43s). Test baseline: <PENDING — buhvi62pe>.
- Code anchors re-verified 2026-07-18: builder.rs:720 decompose_quantile; engine/analysis.rs:166 MM-branch (rewire target); main.rs:247 CLI (rewire target).
- E2 (shared-memory link args) + E3 (nested-worker spawn) deferred to threading stage (need the wasm-bindgen-rayon integration first).

## Inviolable stage order (founder-directed)

**determinism → memory profile → threading → validation.** Each stage lands + verifies before the next.

| Stage | Domains (phase4-final-*.md) | Repos | Gate to next stage |
|---|---|---|---|
| **1. determinism** | deterministic-rng | oaxaca-blinder-rs | AC-1..AC-10 pass; within-platform byte-identity holds; native build+test green |
| **2. memory profile** | memory-budget | oaxaca-blinder-rs | AC-M1..M15; profile report committed + reviewed; N_max_const computed |
| **3. threading** | toolchain-build, engine-parallel-surface, meridian-integration | all 3 repos | AC-1..16 (toolchain), AC-1..13 (engine), AC-M*.* (meridian); threaded blob links; E2/E3 resolved |
| **4. validation** | statistical-trust-layer, verification-benchmark | oaxaca-blinder-rs | trust goldens pass; mode-parity + memory-ceiling + reproducibility CI green |

## Stage 1 results (determinism) — 2026-07-18

Implementation: dispatched Opus worker did rng.rs + mean-path builder.rs + CV-1 seed forwarding, then
stopped mid-quantile-path (budget). Orchestrator completed inline: full quantile path (run_single_pass
rep_master param, D4 MM seeding, D5 bootstrap rewrite, seed API + RunMetadata on quantile builder/results),
inference.rs D6 guard, engine run_metadata surfacing (DecompositionResult + both analysis.rs branches),
and 6 missing tests (rng_determinism.rs AC-4/5/7/8/11 + parity_test AC-9 byte-identity). Design refinement:
rep-master = master ^ ((rep+1)*phi) so bootstrap rep 0 never collides with the raw-master point pass.

AC verification (9/10 confirmed; AC-10 full-suite running):
- AC-1 PASS (grep: 0 sample_n_literal, 0 thread_rng in quantile_decomposition)
- AC-2 PASS (grep: 0 par-iter in inference.rs)
- AC-3 PASS (rng::tests 3/3 + 35 other lib tests = 38/0)
- AC-4 PASS · AC-5 PASS · AC-7 PASS · AC-8 PASS · AC-11 PASS (rng_determinism 5/5)
- AC-6 PASS — CORE: sha256 d74efb3c... byte-identical across RAYON_NUM_THREADS 1/2/4 (INV-02 within-platform)
- AC-9 PASS (meanpath_point_estimates_byte_unchanged vs pre-refactor baseline; statsmodels parity still green)
- AC-10 (native build both green; wasm32 engine build green; full-suite test RUNNING b2qqjku6a with CARGO_BUILD_JOBS=2)

Build lesson: `cargo test --workspace` OOMs on 31GiB when uncapped (parallel linking of ~15 polars test
binaries). Always run with CARGO_BUILD_JOBS=2, or one --test target at a time.

## Stage 2 results (memory profile) — 2026-07-18

Implementation: dispatched Sonnet worker created `mem_profile.rs` (tracking `GlobalAlloc` +
checkpoint A/B API), wired checkpoints A/B into `builder.rs`, added the `mem-profile` feature
(off by default, INV-01) — then stopped at budget (same pattern as stage 1), before the
empirical half. Orchestrator completed inline: `examples/gen_mem_fixture.rs` (×5 D7
perturbation generator), `examples/mem_profile_harness.rs` (D1 measurement driver, gated by
`required-features = ["mem-profile"]`), `.gitignore` entry for the fixture, ran the profile,
computed the constants, wrote the committed report.

Feature compile: `cargo build -p oaxaca_blinder --features mem-profile` exit 0 (1m28s).
Fixture: seed 0x0014_50CE_5EED_A115, 50000 rows, sha256 8de0a364… (deterministic across runs).

AC verification:
- AC-M13/14/15 PASS (fixture determinism / unperturbed group+categoricals / gitignored).
- AC-M1 PASS — profile report committed (`.build-state/mem-profile-report.md`) with H_res/H_peak/Sc/St_obs.
- AC-M9 revised / AC-M10 RETIRED — profile FALSIFIED the D4 "memory lever": Polars clone is
  Arc-shallow, so Sc_before (8.352 MiB) ≈ Sc_after (8.386 MiB), no reduction. Stage-1 refactor
  kept for determinism (INV-02), not memory.
- AC-M6 PASS — N_max_const=8, a TRUE safe (bounded peak 248 MiB @ N=8 < ceiling 277 MiB).

### Founder decision + corrected diagnosis (2026-07-18) — Finding 2

David chose **Option A (bounded/streaming refactor)** at the AskUserQuestion gate. The DIAGNOSIS
was then corrected twice by measurement (both wrong hypotheses recorded in the report for honesty):
- ❌ "retained bootstrap results dominate" — refuted: `RepEstimates` extraction (dropping heavy
  per-rep `SinglePassResult`) left peak UNCHANGED. current-after-collect ≈ H_res → no retention.
- ❌ "D2 formula omits an R×retained term / 40 GiB leak" — refuted: no leak; the peak is transient.

**True mechanism:** unbounded `into_par_iter().map().collect()` lets rayon keep many reps'
`Sc`-sized working sets in flight → peak scales with reps AND threads (461 MiB N=1 … 899 MiB N=16
@ R=100). Sequential `into_iter` → flat 64 MiB. Benign natively (reclaimed); **fatal in WASM**
(linear memory only grows, never returns pages → permanent SharedArrayBuffer bloat).

**Fix (both in `builder.rs`, honoring Option A):**
1. `RepEstimates` — map extracts only the scalars the SE/CI reduction reads, drops heavy result
   per-rep. Bounds RETENTION (original ~400 KB/rep → 4 GB at R=10000; now ~2.3 KB/rep). Necessary.
2. **Bounded-parallel (chunked) bootstrap** — reps run in index-ordered chunks of the pool size →
   at most N working sets in flight → peak = H_res + N·Sc, FLAT in rep count. Index-ordered
   consumption keeps the FP reduction order fixed across thread counts.
Quantile path unchanged (its `SinglePassResult` = 3 scalars/quantile, already memory-safe).

**Verification (all PASS):** bounded peak @ N=8/50k = 248 MiB (was 860), flat in reps → in-band
(512) and no rep-count OOM at R=10000; AC-6 sha256 `d74efb3c…` IDENTICAL across
RAYON_NUM_THREADS 1/2/4/8 (INV-02, same hash as stage 1); AC-9 byte-identity; rng_determinism 5/5;
parity 2/2. **Threading invariant:** the bootstrap must stay bounded-parallel — never revert to
unbounded `into_par_iter().collect()`.

## Status

- [x] Phase 0 — init, git isolation, baseline build, E1 preflight, anchors, control files
- [x] Stage 1 — determinism (ALL 10 ACs PASS; AC-6 sha256 byte-identical across threads; AC-10 workspace 85/0). Committed 356faab.
- [x] Stage 2 — memory profile (GATE PASS: profile committed; N_max_const=8 true safe; WASM-OOM
      hazard found + fixed via bounded-parallel bootstrap; peak 248 MiB @ N=8/50k in-band; INV-02
      byte-identity + AC-9 + determinism all hold). AC-M10 retired (clone-lever falsified).
- [ ] Stage 3 — threading (E2/E3 preflights inside) — INVARIANT: keep bootstrap bounded-parallel; N_max_const=8; link-args in report
- [ ] Stage 4 — validation
- [ ] Phase 3 — review panel (Charter-compliance reviewer incl.)
- [ ] Phase 4 — integration, cross-surface parity, BUILD-REPORT.md, merge to main

## Resumability

On resume: read this file + features.json. Current stage = the first unchecked stage above.
Re-verify the stage's ACs before assuming it's done. Build branch: `build/wasm-rayon-mt/main` in oaxaca-blinder-rs.

## Workflow State

```yaml
phase: executing
current: stage-1-determinism
completed: [phase-0-init]
next: stage-2-memory-profile
safe_to_retry: true
updated: 2026-07-18
```
