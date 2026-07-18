# Buildability Gate — Founder Rulings (2026-07-18)

> Authoritative. `/build` applies these over any "pending/recommended" language left in the phase4 specs. Source: AskUserQuestion buildability gate, David, post adversarial council.

## Ruling 1 — Reproducibility = **Strategy A (dual artifact)**
Ship TWO wasm artifacts: (i) the **untouched stable sequential** blob (current `scripts/build-wasm.sh` stable/`--target bundler` path + its existing committed `engine/pay_equity_engine.wasm.sha256`, unchanged) as the defensibility baseline; (ii) a **threaded** blob (`+nightly`, `--features wasm-threads`, atomics + build-std + link args, `--target web`) with its own committed baseline `engine/pay_equity_engine.threaded.wasm.sha256` and `engine/pkg-threaded/`.
- Meridian worker conditionally dynamic-`import()`s the threaded glue inside the `crossOriginIsolated` branch, else the sequential glue (meridian D5 Strategy-A path, W3(d) confirms this works under Vite `worker.format:'es'`).
- The THREADED baseline is generated inside a **pinned CI/container** (not the dev box) — cross-machine `-Zbuild-std` sha256 is fragile (council MJ-2). The stable seq baseline keeps its existing generation.
- Supersedes every spec's "written for Strategy B" primary. The Strategy-A descriptions already in toolchain D7 and meridian D5 are now the primary path.

## Ruling 2 — INV-02 = **the split** (see Charter INV-02 reframe)
- **Within-platform byte-identical** across thread counts (native 1/2/4 all sha256-equal; wasm seq/2/4 all sha256-equal) — the blocking threading-safety guarantee.
- **native↔wasm = tolerance-parity ≤ 1e-6** (NOT sha256 equality). Cross-ISA libm divergence.
- All specs' "pending INV-02 reframe" annotations are now RATIFIED to this split (deterministic-rng AC-6, verification-benchmark D-2/AC-2).

## Ruling 3 — In-Scope 12 aggregate = **a-1 (both surfaces RIF)**
- WASM/MCP quantile branch (`analysis.rs:166-206`) AND the CLI (`main.rs:247`) both switch to `OaxacaBuilder::decompose_quantile` (RIF). One coherent method everywhere; per-predictor detail on both; detail sums to the RIF aggregate.
- The displayed quantile aggregate changes MM→RIF on every surface (accepted; coherent + additive + matches the `ddecompose(reweighting=FALSE)` golden).
- `QuantileDecompositionBuilder` (MM) is not deleted (kept for any direct API consumer) but is no longer the default quantile path on either surface. AC-13 = CLI and WASM return byte-identical aggregates on the same fixture+seed.

## Ruling 4 — Quantile SE rigor = **recompute RIF per replicate**
- `decompose_quantile`'s bootstrap must re-estimate the RIF (`calculate_rif`: `q_τ` + density) **inside each bootstrap replicate**, not once on the full sample — so the CIs capture density-estimation uncertainty. Engine records `fixed_rif: false`.
- ~2× per-rep cost on the quantile path (each rep now runs `calculate_rif` on its resample before the RIF-OLS). Acceptable for a client-facing defensibility tool.
- Implementation: the RIF transform moves from `decompose_quantile`'s pre-loop (`builder.rs:730-746`) into the per-replicate closure of the inner `run()`'s bootstrap (`builder.rs:825`), OR `decompose_quantile` runs its own seeded bootstrap that recomputes RIF per rep. The trust-spec golden validates the corrected SEs against `ddecompose` (which also bootstraps).
- Seed propagation (council CV-1) still required: the per-rep RIF recompute uses the same master-seed→per-rep-stream scheme (deterministic-rng), and `decompose_quantile` forwards `self.seed` to the inner builder.

## H2 memory margins — orchestrator decide-and-proceed
Band defaults accepted (`headroom_frac` 0.15–0.20, `St` 1 MiB, `Marg` 64 MiB, `Marg_init` 16 MiB, `N_target` 4–8). Memory is non-binding at 50k (council MJ finding), so the cap resolves to `min(hardwareConcurrency, 8)`; the profile is retained for extrapolation past ASM-04's 50k ceiling. Revisit only if the profile shows binding.
