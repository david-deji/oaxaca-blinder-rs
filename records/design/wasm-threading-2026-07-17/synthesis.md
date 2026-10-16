# Final Synthesis — WASM Rayon Multithreading + Statistical Trust for pay-equity-engine

> **This is a build handoff document.** The Phase 4 specifications (`phase4-final-*.md`, post-reconciliation + council fixes + founder rulings) are the single source of truth for implementation. `buildability-gate-rulings.md` and the Charter `spec-charter.md` govern. Council files (`council-*.md`, `council-synthesis.md`) and the audit (`phase4-build-readiness-audit.md`) are audit trail only.

## What is being built

Real Rayon thread-pool parallelism in the `pay-equity-engine` WASM build (consumed by Meridian, a Vue app whose compute runs in one dedicated Web Worker, served by the audit-forge Flask offline bundle at `/pay-equity/` on loopback:5137), plus the determinism, memory, and statistical-trust work that must precede and validate it. Scale: 50k employee rows; memory is the first-class constraint. Issue: 0014-MERIDIAN.

## Build order (inviolable — founder-directed)

**determinism → memory profile → threading → validation.** Threading that lands before the memory profile "optimizes speed into an OOM" (Charter In-Scope 11). Each stage gates the next.

## The 7 domains (each a phase4-final-*.md spec)

1. **deterministic-rng** (step 1) — master-seed → per-rep `ChaCha8Rng::set_stream` + owned index-vector resampling (`take(&IdxCa)`) replacing unseeded `sample_n_literal`/`thread_rng`; sequential float summation; deterministic failed-rep handling (fixes the silent `filter_map(.ok())` discard); `RunMetadata` in output. **INV-02 (reframed): within-platform byte-identical across thread counts; native↔wasm ≤1e-6.**
2. **memory-budget** (step 2) — empirical single-threaded memory profile at 10k/25k/50k BEFORE threading; shared-memory model (fixed max, allocated up front); thread-cap `N_max_const := min(floor((M_max−H_res−Marg)/(Sc+St)), 8)`; index-resampling as the memory lever (kills the redundant per-rep `df_a/df_b` clones at `builder.rs:828-829`). Memory is non-binding at 50k → cap resolves to `min(hwConcurrency, 8)`; profile retained for extrapolation.
3. **engine-parallel-surface** (step 3) — audit of the 5 WASM entry points (decompose bootstrap = hot parallel target; optimize/frontier/check = SKIP; clarabel concurrent-safe, L5); `wasm-threads` feature + `pub use init_thread_pool`; **DELETE `POLARS_MAX_THREADS`** (no-op on wasm — polars wasm POOL stub routes onto our global rayon pool, L4); **In-Scope 12** (below).
4. **toolchain-build** (step 3) — pinned nightly + `-Zbuild-std` + atomics/bulk-memory + `--target web`; **Strategy A (dual artifact)**; E1/E2 build-preflights (with subgraph-trim); reproducibility double-build in two non-default `CARGO_TARGET_DIR`s.
5. **meridian-integration** (step 3) — `analysis.worker.js` `crossOriginIsolated` feature-detect → `await initThreadPool(cap)` → mode posted; sequential fallback never crashes (INV-03); page-level COOP `same-origin` + COEP `require-corp` on `/pay-equity/` (workers inherit, W2); Vite `worker.format:'es'` (W3); conditional dynamic-import of the threaded glue (Strategy A); OOM = budget-prevention + structured self-report (NOT RuntimeError parsing, W4); writes `thread-cap.js`.
6. **statistical-trust-layer** (step 4) — R `oaxaca` 0.1.5 manual-loop golden (W5); proptest adding-up identities; QR location-scale designs; the **In-Scope 12 quantile-detail golden** (R `ddecompose(reweighting=FALSE)`), tolerance measured-then-pinned + a track-2 custom-R fidelity check (MJ-3); recompute-per-rep SE validation (`fixed_rif:false`); extends (never replaces) the existing statsmodels golden.
7. **verification-benchmark** (step 4) — CI: mode-parity (within-platform byte-identical + native↔wasm 1e-6, blocking); memory-ceiling@50k (blocking); reproducibility double-build (Strategy A threaded blob, container baseline); speedup (>1.5×, non-blocking); custom COOP/COEP server + Playwright for threaded browser tests (NOT `wasm-pack test`, W7).

## In-Scope 12 — per-predictor quantile decomposition (the scope expansion)

**It is WIRING, not new math** (audit CRITICAL-1). `OaxacaBuilder::decompose_quantile()` (`builder.rs:720`) already implements one-stage RIF-OLS quantile decomposition with full per-predictor detail (via `calculate_rif` + the standard `run()`); it was only smoke-tested and only reachable via the crate API, not the WASM path (which called the MM builder). **Founder rulings:** switch BOTH the WASM/MCP branch (`analysis.rs:166-206`) and the CLI (`main.rs:247`) to `decompose_quantile` (ruling a-1 — one coherent RIF method everywhere; quantile aggregate changes MM→RIF); recompute the RIF inside each bootstrap replicate for correct CIs (ruling 4, `fixed_rif:false`); forward `self.seed` into `decompose_quantile`'s inner path (council CV-1 — else chosen-seed reproducibility silently breaks). Validated against the `ddecompose(reweighting=FALSE)` golden.

## The four founder rulings (buildability gate 2026-07-18)

1. **Reproducibility = Strategy A** (dual artifact; threaded baseline generated in a pinned container).
2. **INV-02 = the split** (within-platform byte-identical + native↔wasm ≤1e-6).
3. **In-Scope 12 aggregate = a-1** (both surfaces → RIF).
4. **Quantile SE = recompute RIF per replicate.**

## /build Phase 0 preflights (run before dispatching workers — NOT /specify research)

- **E1** — validate the nightly compiles the (trimmed) wasm subgraph under `-Zbuild-std`; forward-bump the pin if needed; the threaded baseline is PROVISIONAL until E1 fixes the pin (regenerate in the container).
- **E2** — determine the minimal shared-memory link args (`--shared-memory`/`--import-memory` vs `+atomics` alone).
- **E3 (top unknown)** — does the rayon pool spawn from inside a dedicated worker? Main-thread-relay fallback specified if not (postMessage contract byte-identical either way).

## Invariants (Charter)

INV-01 native byte-equivalent (feature-gated off native); INV-02 (reframed split); INV-03 non-COI degrades to working sequential; INV-04 committed sha256 baselines + pinned rebuild (per artifact under Strategy A); INV-05 memory-capped thread count; INV-06 additive scoped Flask headers; INV-07/08 f64 in the engine, Decimal(18,2) at boundaries.

## Handoff

`/build` reads `distilled.md` (Phase 0) + the 7 `phase4-final-*.md` specs (per-domain worker slices) + `spec.yaml` (machine-readable contract) + `buildability-gate-rulings.md`. Charter is the contract; council files are audit trail. (This file is `synthesis.md` — the canonical Bundle-C name `/build` Phase 0.1 parses; workers must NOT read it in full, per `/build`'s context-thrash guard.) All acceptance criteria are objectively checkable (test/exit-code/byte-compare/grep). Every automated quality gate passed (Phase 1: 19/0, Phase 2: 16/0, Phase 3: 4/0, Phase 4: 22/0); citation verifiers passed (Phase 1.5 fixed-in-place, Phase 3.5 3/3); build-readiness audit CRITICAL findings resolved; adversarial council (5 seats, all SHIP_WITH_FIXES) fixes applied.
