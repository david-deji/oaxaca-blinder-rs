# Phase 4 Writer Brief — shared context for all refined-spec writers

> Read this once, then your assigned phase2 draft + the two phase3 findings files + the Charter.
> You are producing the FINAL buildable spec for your domain(s). The phase2 draft is your starting
> point; Phase 3 resolved its open gaps and produced six corrections you MUST fold in where they touch
> your domain. This is a build handoff — /build reads your output as the source of truth.

## Inputs every writer reads
- Your assigned `phase2-spec-<domain>.md` draft (your starting point).
- `phase3-local-l1-l2-l3-l4-l5-l6-findings.md` (vendored-source facts, definitive).
- `phase3-web-w1-w2-w3-w4-w5-w6-w7-w8-w9-w10-findings.md` (Perplexity findings + § Phase 4 corrections + § Sources).
- `spec-charter.md` (13 In-Scope items, anti-goals, SC-01..07, INV-01..08, ASM-01..06). Your spec must not contradict it.
- Code anchors live in the engine repo root `apps/hr-apps/oaxaca-blinder-rs/`; verify any claim you carry forward with a Read.

## The six Phase-3 corrections (fold in where they touch your domain)
1. **W4 — worker OOM**: do NOT parse `WebAssembly.RuntimeError` messages to detect OOM (message is engine-defined, unreliable; OOM indistinguishable from other traps). Use budget-prevention (enforce thread-cap + `--max-memory`) + a structured engine self-report of allocation pressure. (verification-benchmark, meridian)
2. **W5 — mean-path golden**: R `oaxaca(R=...)` accepts NO external resample index and uses its own `sample()` loop — the golden is a bespoke manual-loop R script (fixed seed/index + `lm()` per replicate), not a call to `oaxaca()`. (statistical-trust-layer)
3. **W7 — threaded browser CI**: `wasm-pack test` cannot serve COOP/COEP headers → threaded/SAB browser tests need a custom ~30-line COOP(same-origin)/COEP(require-corp) static server + Playwright headless Chromium. Non-threaded unit tests may stay on `wasm-pack test`. (verification-benchmark)
4. **W8 — reproducibility double-build**: Swatinem/rust-cache@v2 caches only `./target`; the double-build must use two distinct non-default `CARGO_TARGET_DIR`s (or disable rust-cache on that job) so both builds recompile std under `-Zbuild-std` — no masking. (toolchain-build, verification-benchmark)
5. **L4 — POLARS_MAX_THREADS**: polars 0.44 never reads it on wasm (wasm `POOL` is a stub delegating to the global rayon registry). DELETE every `POLARS_MAX_THREADS=1` instruction. Reframe "one parallel layer" as: the polars wasm stub routes `join`/`scope`/`spawn` onto OUR global rayon pool — cooperative work-stealing, not oversubscription; the only residual audit is any polars-internal parallel float reduction on the hot path (narrow — our stats run in nalgebra after `take`). (memory-budget, engine-parallel-surface, verification-benchmark)
6. **W10 — In-Scope 12 full per-predictor quantile math** (founder scope expansion, Charter Scope-Delta 2026-07-17): implement per-predictor detailed quantile decomposition via **one-stage RIF-OLS** (RIF(y;q_τ,F) = q_τ + (τ−1{y≤q_τ})/f_Y(q_τ); OLS of RIF on X per group; then the existing mean-path Oaxaca detail algebra on the RIF-OLS coefficients: per-predictor endowments C_k=(X̄_A,k−X̄_B,k)·β_B,τ,k and structure S_k=X̄_A,k·(β_A,τ,k−β_B,τ,k)). Additive to current aggregates (quantile_decomposition.rs:267-271 unchanged). Route categoricals through the existing Gardeazabal-Ugidos normalization (base-category dependence carries over identically). Do NOT extend MM simulation for detail (MM detail is path-dependent, non-additive). Golden = R `ddecompose::ob_decompose(formula, data, group, rifreg_statistic="quantiles", rifreg_probs=..., reweighting=FALSE, bootstrap=TRUE)` + optional Stata `oaxaca_rif` cross-check; new math gets its own golden + identity tests. Two-stage DFL-reweighting = documented v2, out of scope. (engine-parallel-surface, statistical-trust-layer)

## Locked decisions from Phase 3 (do not relitigate)
- `worker.format: 'es'` REQUIRED in Vite (rayon nested ES-module workers); `vite-plugin-wasm` omitted for `--target web`; page-level COOP/COEP on the audit-forge `/pay-equity/` document is sufficient (workers inherit isolation, same-origin) — no per-worker headers. (W2, W3)
- `take(&IdxCa)` is bounds-checked gather, accepts duplicate/unsorted indices, returns exactly idx.len() rows — index-vector resampling confirmed buildable. (L1)
- Add `rand_chacha = "0.3"` as a direct dep of oaxaca_blinder (already in Cargo.lock; ChaCha8Rng::set_stream present). (L2)
- clarabel default build is single-threaded, no rayon, no BLAS — concurrent solves are safe; frontier stays sequential by design. (L5)
- Education_Level = 3-level categorical (Master/Bachelor/PhD) → 2 dummies under one-hot. (L6)
- getrandom wasm entropy works (0.3.4 [wasm_js] + 0.2.16 [js] both resolve on the wasm graph). (L3)
- Strategy B (single threaded artifact, rayon seq fallback) is the writer recommendation; founder ratifies at buildability gate — write the spec for Strategy B with a one-paragraph Strategy-A fallback note.

## Output rules
- Write ONE file per assigned domain: `phase4-final-<domain>.md` (exact domain slug from your phase2 draft filename).
- Structure: `## Summary` · `## In-Scope (this domain)` · `## Design & Decisions` (with code anchors file:line) · `## Build Steps` (ordered, buildable) · `## Acceptance Criteria` (verifiable) · `## Open Items` (route to buildability gate, none if clean) · `## Sources` (phase1/phase3 file refs + any URL).
- Every acceptance criterion must be objectively checkable (a test, an exit code, a byte-compare, a file check) — no "works correctly".
- Preserve INV-02 bit-identical and the build order (determinism → memory profile → threading → validation) — do not reorder.
- Do not invent code that doesn't exist; cite the anchor you're building against.
- Write your full spec to your output path AND return it as your final message (belt-and-suspenders).
