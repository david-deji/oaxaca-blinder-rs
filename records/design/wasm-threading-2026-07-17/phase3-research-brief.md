# Phase 3 Research Brief — gap manifest from Phase 2 drafts

> Compiled: 2026-07-17 by orchestrator from the 7 phase2-spec-*.md Gaps sections. Dedup applied.
> Execution classes: LOCAL (vendored-source/data read, orchestrator-direct, deterministic),
> WEB (Perplexity unit), EXPERIMENT (build/browser preflight — assigned to /build Phase 0, not /specify),
> HUMAN (founder decision — surfaced at gates), DEPENDENT (resolved by another deliverable, not research).

## LOCAL units (orchestrator-direct, definitive)

### L1 — polars-take-semantics (merges rng RK3/M3)
Does Polars 0.44 `DataFrame::take(&IdxCa)` accept duplicate + unsorted indices, gather with-replacement in index order, return exactly idx.len() rows; exact method name/signature + null handling; does it materialize contiguous (~1F) or retain chunked references? Read vendored polars 0.44 source in cargo registry.

### L2 — rand-chacha-pairing
rand_chacha version compatible with rand 0.8.5 (Cargo.lock check); ChaCha8Rng set_stream(u64) + seed_from_u64 present on that version.

### L3 — getrandom-js-wasm
Is getrandom with js/wasm_js feature already in the engine wasm build (Cargo.lock + engine Cargo.toml `getrandom/wasm_js`)? Determines whether seed_from_entropy() works in wasm or is native-only.

### L4 — polars-max-threads-read-time
Does Polars 0.44 read POLARS_MAX_THREADS at global pool init (process start / first use) or dynamically? Read vendored polars POOL init source.

### L5 — clarabel-internals (off critical path)
clarabel 0.11.1: internal threads, BLAS links, rayon usage? Read vendored Cargo.toml + solver source.

### L6 — education-level-categories
Distinct Education_Level values (+ any ordinal order) in /home/deji/Downloads/Employers_data.csv.

## WEB units (orchestrator-direct Perplexity)

### W1 — wbr-mechanics (merges toolchain gap 3 + surface re-export gap)
wasm-bindgen-rayon 1.3.0: (a) dep-range admits wasm-bindgen 0.2.106? (b) re-export mechanism on 1.3.0 — `pub use wasm_bindgen_rayon::init_thread_pool` vs macro? (c) init_thread_pool export name current?

### W2 — worker-coi-semantics (meridian RK2 + COOP-inheritance Phase-1 leftover)
Per HTML spec/MDN: does a dedicated worker report self.crossOriginIsolated === true when its owner document is cross-origin-isolated; do worker scripts themselves need COOP/COEP response headers or do they inherit the owner's agent cluster?

### W3 — vite-wbr-config (meridian RK3 + conditional-import question)
Working Vite 5/6 production config for wasm-bindgen --target web + wasm-bindgen-rayon (worker.format, asset URLs, vite-plugin-wasm interaction); does a conditional dynamic import() of threaded glue bundle correctly vs static-import-guarded-at-runtime?

### W4 — runtime-error-oom
Can WebAssembly.RuntimeError OOM (memory.grow failure) be distinguished from other traps in Chrome/Edge/Firefox — error message/type taxonomy for the worker's OOM tagging?

### W5 — r-oaxaca-index-hook
Does R oaxaca 0.1.5 accept externally-supplied bootstrap resample indices (read CRAN source/reference for its bootstrap internals)? Fallback already specified (manual loop) — this decides which golden-script shape.

### W6 — mm-golden-routine
Concrete R routine for MM/quantile-decomposition golden (candidate: Chernozhukov-Fernández-Val-Melly `Counterfactual`); function signature; accepts shared index matrix?

### W7 — wasm-pack-test-coi
Do wasm-pack test / wasm-bindgen-test-runner support COOP/COEP for atomics builds (issue trackers/release notes)? Decides harness: built-in runner vs tiny-header-server + Playwright.

### W8 — rust-cache-buildstd
Does Swatinem/rust-cache@v2 scope caches such that the CI double-build (separate CARGO_TARGET_DIR) genuinely recompiles std (no masking)?

### W9 — r-stata-seed-practice (Phase-1 leftover, citation hygiene)
Primary-source confirmation: R boot/set.seed and Stata `set seed` bootstrap reproducibility practice (official docs), for the spec's defensibility citations.

### W10 — per-predictor-quantile-detail-methodology (NEW — founder scope expansion, In-Scope 12 full math)
How is per-covariate (detailed) decomposition computed for quantile gaps in the RIF-regression framework (Firpo-Fortin-Lemieux 2009 Econometrica / 2018 Econometrics "Decomposing Wage Distributions Using Recentered Influence Function Regressions")? Specifically: (a) RIF-OLS makes the detailed decomposition formally identical to the mean-path Oaxaca detail — confirm the exact formulas for endowments/coefficients per predictor at quantile tau, incl. the reweighting-error and specification-error terms in the two-stage FFL procedure and whether a spec may defensibly ship the first-stage (no reweighting) detail alone; (b) does the MM simulation path admit per-predictor detail at all, or is RIF the only defensible detailed route (Fortin-Lemieux-Firpo 2011 Handbook ch. — MM path-dependence problem); (c) reference implementations for goldens: Stata `oaxaca_rif` / `rif` package (Rios-Avila 2020, Stata Journal), R `dineq::rif`/`rifreg`, `ddecompose` — which produce per-predictor quantile detail and what are their function signatures; (d) categorical-predictor base-category dependence at quantiles — does Gardeazabal-Ugidos normalization carry over to RIF detail?

## EXPERIMENT units (→ spec as /build Phase 0 preflight tasks, not /specify research)

#### E1 — nightly-compile-validation (toolchain gap 1)
Does nightly-2024-08-02 build the full dep set (polars 0.44 + clarabel + nalgebra + statrs) for wasm32 with -Zbuild-std? Forward-bump procedure specified in phase2-spec-toolchain-build.md §4.1.

#### E2 — shared-memory-link-args (toolchain gap 2 + memory M2)
Does +atomics alone emit shared memory or are --shared-memory/--import-memory link args required; where does --max-memory bind under --target web + wbr glue?

#### E3 — nested-worker-pool-spawn (meridian RK1 + verify VB2 — TOP unknown)
initThreadPool from inside a dedicated worker in evergreen Chrome/Edge (+ headless Chromium for the harness). Minimal COI page experiment; decides init call-site (worker vs main-thread relay fallback).

## HUMAN decisions (gate items, not research)

#### H1 — In-Scope 12 scope ruling — RESOLVED 2026-07-17 at council gate
Founder chose FULL per-predictor math (scope expansion, Charter Scope-Delta logged, anti-goal carved out). Research routed to new unit W10; W6 golden unit now load-bearing for the detail path too.

#### H2 — memory margins ratification (memory-budget §7.4)
Marg/Marg_init/headroom_frac/St confirmation once the profile lands. → buildability gate.

## DEPENDENT (no action in Phase 3)

> Not research gaps — resolved by other deliverables, listed for completeness.
> (1) Final --max-memory / ceiling constants come from the memory-profile output (build-time).
> (2) The Strategy-A conditional-import question is moot if Strategy B is ratified; W3 covers the live variant.
