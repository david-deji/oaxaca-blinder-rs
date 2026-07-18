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

## Status

- [x] Phase 0 — init, git isolation, baseline build, E1 preflight, anchors, control files
- [~] Stage 1 — determinism (9/10 ACs PASS; AC-10 full-suite verifying, then commit)
- [ ] Stage 2 — memory profile
- [ ] Stage 3 — threading (E2/E3 preflights inside)
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
