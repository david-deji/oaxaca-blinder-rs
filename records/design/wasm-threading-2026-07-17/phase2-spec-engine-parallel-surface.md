> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (engine-parallel-surface) — Draft

# Spec: WASM Parallel Surface, Thread-Pool Export, Feature Gating, Quantile API Exposure

Covers Charter In-Scope 4 (full-surface parallelization audit), part of 1 (`init_thread_pool` export + feature gating), 12 (quantile detailed-components API). Binds INV-01 (native byte-equivalent) and INV-03 (sequential fallback). Depends on the deterministic-rng spec for all per-rep seeding (referenced, not respecced).

---

## 1 Executive Summary

The engine exposes exactly five WASM entry points (`engine/src/lib.rs:26,34,42,50,59`): `decompose`, `optimize`, `verify_adjustments`, `calculate_efficient_frontier`, `check_defensibility`. This spec audits all five, delivers a parallelize/skip verdict with rationale each (SC-04), and specifies the thread-pool enablement layer: re-export `wasm_bindgen_rayon::init_thread_pool` behind a new `wasm-threads` feature, pin Polars to one thread so our rep-level `par_iter` is the sole parallel layer, and feature-gate the wasm-threads machinery entirely off the native path (INV-01).

Two audit corrections against the Phase-1 sketch: (1) `calculate_efficient_frontier` is **not** a grid of independent optimizations — the budget loop at `analysis.rs:1135-1166` is a sequential cumulative sweep carrying `current_y`/`pay_idx`/`budget_cursor` across steps, with a single upfront `clarabel` solve (`analysis.rs:950`) as the only heavy compute; verdict is SKIP, not "strong candidate." (2) `optimize`'s single solve gates `frontier`, but neither adds a parallel site, so clarabel thread-safety verification is only needed if a *future* per-solve grid is introduced — it does not block this spec.

`init_thread_pool` is a re-export, not new logic; the parallel sites (`builder.rs:825`, `quantile_decomposition.rs:222,227,337`) already exist as `rayon` iterators and run sequentially in today's WASM via rayon's built-in fallback. This spec makes them use a real pool without touching the iterator code.

Charter In-Scope 12 (quantile detailed-components API, `analysis.rs:201`) is partially blocked: the quantile MM path computes only aggregate `gap/characteristics/coefficients` (`quantile_decomposition.rs:267-271`), never per-predictor detail, so *exposing* detail requires *computing* it — which touches decomposition math (an anti-goal). This spec exposes the API surface and flags the computation gap.

## 2 Requirements

### R1 — Five-entry-point parallelize/skip audit (SC-04)

| # | Entry point | Export | Inner fn | Parallel site(s) | Verdict | Rationale |
|---|---|---|---|---|---|---|
| 1 | `decompose` (mean) | `lib.rs:26` | `decompose_inner` → `OaxacaBuilder::run` | `builder.rs:825` rep `into_par_iter` | **PARALLELIZE** (exists) | 100-rep bootstrap; hot path; seeded per rng-spec R2/R3. Order-preserving indexed collect. |
| 1b | `decompose` (quantile) | `lib.rs:26` | `QuantileDecompositionBuilder::run` | `quantile_decomposition.rs:337` rep par_iter + `:222,:227` per-τ QR par_iter | **PARALLELIZE** (exists) | rep bootstrap + per-τ `solve_qr` both independent; seeded; MM inner resample loop (`:246`) stays sequential (cheap, carries `y_*_vec`). |
| 2 | `optimize` | `lib.rs:34` | `optimize_inner` `analysis.rs:309` | none (single clarabel LP/QP solve) | **SKIP** | One solve; nothing to fan out. Document skip. |
| 3 | `verify_adjustments` | `lib.rs:42` | `verify_inner` `analysis.rs:40` | inherits `decompose` bootstrap | **PARALLELIZE via inheritance** | Recomputes decomposition on adjusted data; no new code — inherits site #1. |
| 4 | `calculate_efficient_frontier` | `lib.rs:50` | `..._frontier_inner` `analysis.rs:875` | one `optimize_inner` (`:950`) + **sequential** budget loop (`:1135-1166`) | **SKIP** | Budget loop carries `current_y`/`pay_idx`/`budget_cursor` state across steps (`:1131-1157`); `compute_t_stat` is one projector multiply (`:1089-1109`) — trivial. Not independent grid. Corrects Phase-1 sketch. |
| 5 | `check_defensibility` | `lib.rs:59` | `check_defensibility_inner` `defensibility.rs:9` | none | **SKIP** | Scalar scoring over decompose output; trivial/serial. Document skip. |

**Acceptance:** the spec contains this table covering all five exports; each has a verdict + rationale; the two SKIP-with-a-solve entries (optimize, frontier) name the solve location. Verifiable against `engine/src/lib.rs` export list.

### R2 — `init_thread_pool` re-export behind `wasm-threads`

```rust
// engine/src/lib.rs — after existing wasm wrappers
#[cfg(all(feature = "wasm", feature = "wasm-threads"))]
pub use wasm_bindgen_rayon::init_thread_pool;
```

The `pub use` is what wasm-bindgen-rayon's proc-macro expands into the JS-visible `initThreadPool` export (per crate README). No hand-written binding.

**Acceptance:** a wasm build with `--features wasm,wasm-threads` emits an `initThreadPool` export in the generated JS glue; a wasm build with `--features wasm` (no threads) does not, and still compiles/links.

### R3 — Feature plumbing (engine + oaxaca_blinder)

```toml
# engine/Cargo.toml
[dependencies]
wasm-bindgen-rayon = { version = "1.3.0", optional = true }
[features]
wasm-threads = ["dep:wasm-bindgen-rayon", "wasm", "oaxaca_blinder/wasm-threads"]

# oaxaca_blinder/Cargo.toml
[features]
wasm-threads = []   # gates POLARS_MAX_THREADS pinning + any wasm-thread cfg; rayon stays unconditional (Cargo.toml:23)
```

`rayon` remains an unconditional dependency (`oaxaca_blinder/Cargo.toml:23`) so native parallelism is unchanged. The `wasm-threads` feature only adds wasm-bindgen-rayon and the Polars pin — nothing on the native path.

**Acceptance (INV-01):** `cargo build` (native, no features) and `cargo build -p pay-equity-engine` produce byte-equivalent behavior to pre-change (native parity tests pass); `wasm-bindgen-rayon` does not appear in the native dependency graph (`cargo tree` without `wasm-threads` shows it absent).

### R4 — Polars pinned to one thread (one parallel layer)

Our rep-level `par_iter` is the outer parallel layer; Polars' internal rayon usage must not oversubscribe the fixed WASM pool. Pin Polars to 1 thread via a `std::sync::Once` guard called at the top of each `*_inner`:

```rust
// engine/src/analysis.rs (or a small engine util)
#[cfg(feature = "wasm-threads")]
fn ensure_single_layer() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| std::env::set_var("POLARS_MAX_THREADS", "1"));
}
```

Note (determinism boundary): the Polars ops actually used — `filter`, `take`, `vstack`, `hstack` — are deterministic regardless of thread count (no nondeterministic float reduction), so this pin is a **performance/memory** measure (avoid pool oversubscription per cross-domain memory finding), NOT a bit-identity measure. Bit-identity is owned entirely by the rng spec.

**Acceptance:** with `wasm-threads`, `POLARS_MAX_THREADS=1` is set before the first Polars operation; the memory-ceiling benchmark shows no oversubscription regression. On native (no `wasm-threads`), `ensure_single_layer` is not compiled and Polars threading is unchanged (INV-01).

> NEEDS RESEARCH — see §7: does Polars 0.44 read `POLARS_MAX_THREADS` at runtime-first-use or only at process start? If process-start-only, the `Once` approach fails and the pin must move to build-time / global-pool init.

### R5 — Quantile detailed-components API exposure (In-Scope 12, `analysis.rs:201`)

`analysis.rs:201-204` returns empty `d_exp`/`d_unexp` vectors with the comment "Currently not exposed in public API for QuantileDecompositionDetail." Two-layer fix:

1. Surface layer — add public accessors on `QuantileDecompositionDetail` mirroring the mean path's detailed `Vec<ComponentResult>`:
   ```rust
   impl QuantileDecompositionDetail {
       pub fn detailed_characteristics(&self) -> &[ComponentResult];
       pub fn detailed_coefficients(&self) -> &[ComponentResult];
   }
   ```
   Then `analysis.rs:203-204` populates `d_exp`/`d_unexp` from these instead of `Vec::new()`.

2. Computation layer — **blocked**: the MM path computes only aggregate effects (`quantile_decomposition.rs:267-271` `DecomposedEffects{gap,characteristics,coefficients}`), no per-predictor breakdown exists to expose. Computing one requires new RIF/counterfactual per-predictor attribution, which touches decomposition math (Charter anti-goal: "Algorithm changes to decomposition mathematics"). See §7.

**Acceptance:** the accessor methods exist and compile; `analysis.rs:201` no longer returns unconditional empties *if* the computation gap is resolved. If the computation is deferred (out-of-scope math), the accessors return an explicit empty-with-reason and the deferral is recorded — the API gap (missing methods) is closed even when the data is deferred.

## 3 Technical Architecture

```
Meridian analysis.worker.js
   └─ await initThreadPool(cap)   ──►  engine/src/lib.rs  pub use wasm_bindgen_rayon::init_thread_pool
                                          (cfg: wasm + wasm-threads)
   └─ decompose(req) ─► decompose_inner ─► OaxacaBuilder::run ─► builder.rs:825 into_par_iter ─┐
                                                                                               ├─ WASM rayon pool
   quantile path ─► QuantileDecompositionBuilder::run ─► qd.rs:337 rep par_iter + :222/:227 τ ─┘
   optimize / frontier / check_defensibility ─► single/serial (SKIP)

POLARS_MAX_THREADS=1 (ensure_single_layer, cfg wasm-threads)  ► Polars stays serial ► one parallel layer
Native build (no wasm-threads): wasm-bindgen-rayon absent, ensure_single_layer absent ► INV-01 unchanged
```

Feature matrix:

| Build | features | wasm-bindgen-rayon | init_thread_pool export | Polars pin | Parallel exec |
|---|---|---|---|---|---|
| native CLI/mcp/rlib | (none) | absent | absent | absent | rayon default pool (unchanged) |
| wasm sequential | `wasm` | absent | absent | absent | rayon serial fallback |
| wasm threaded | `wasm,wasm-threads` | present | present | `=1` | rayon WASM pool |

## 4 Implementation Details

| Change | Anchor | Detail |
|---|---|---|
| `init_thread_pool` re-export | `engine/src/lib.rs` (after `:64`) | `#[cfg(all(feature="wasm",feature="wasm-threads"))] pub use wasm_bindgen_rayon::init_thread_pool;` |
| engine feature + dep | `engine/Cargo.toml` | `wasm-bindgen-rayon = {version="1.3.0", optional=true}`; `wasm-threads = ["dep:wasm-bindgen-rayon","wasm","oaxaca_blinder/wasm-threads"]` |
| oaxaca feature | `oaxaca_blinder/Cargo.toml:34-36` | add `wasm-threads = []` |
| Polars pin | `engine/src/analysis.rs` inner-fn tops (`:309`, `:875`, `decompose_inner`, `verify_inner`) + `defensibility.rs:9` | `#[cfg(feature="wasm-threads")] ensure_single_layer();` guarded by `Once` |
| Quantile accessors | `quantile_decomposition.rs` (`QuantileDecompositionDetail` impl) | `detailed_characteristics()`, `detailed_coefficients()` → `&[ComponentResult]` |
| Wire accessors | `engine/src/analysis.rs:201-204` | replace `Vec::new()` with accessor calls (or explicit deferred-empty) |
| Frontier/optimize/defensibility | `analysis.rs:309,875`; `defensibility.rs:9` | no parallel code added; add a one-line SKIP-rationale doc comment each |

No change to the `#[wasm_bindgen]` wrapper signatures (`lib.rs:26-64`) — the postMessage/JS contract is preserved (Charter anti-goal). The parallel iterator bodies (`builder.rs:825`, `quantile_decomposition.rs:222,227,337`) are untouched by this spec except for the seeding the rng spec owns.

## 5 Dependencies and Integrations

- **Depends on:** deterministic-rng spec for all per-rep seeding (this spec adds no RNG). Toolchain-build spec owns the nightly/build-std/`+atomics,+bulk-memory` flags and `--target web` migration that make `wasm-bindgen-rayon` link — this spec assumes that layer (per cross-domain summary "Established facts").
- **Consumed by:** Meridian-integration spec — `analysis.worker.js` calls the `initThreadPool` export this spec re-exports; the fallback path (INV-03) is that spec's surface.
- **wasm-bindgen-rayon 1.3.0 / wasm-bindgen 0.2.106** compatibility per ASM-01 (re-verify at build).
- **clarabel/nalgebra:** current audit adds no parallel solve site, so clarabel thread-safety is not on the critical path for this spec; nalgebra confirmed per-task-safe on wasm (Phase-1 finding 2). See §7 for the conditional gate.
- **INV-06/07/08:** untouched — no header, no monetary, no Decimal surface here.

## 6 Risk Assessment

| ID | Risk | Severity | Mitigation |
|---|---|---|---|
| RK1 | `POLARS_MAX_THREADS` runtime-set ignored by Polars 0.44 → oversubscription of the fixed WASM pool | Medium | §7 research; fallback = build-time env or explicit rayon global-pool cap. Does not threaten bit-identity (only performance/memory). |
| RK2 | Quantile detailed-components computation is genuine decomposition math → scope creep into an anti-goal | High (scope) | Expose API surface only; defer computation with a recorded reason; escalate the compute decision (§7, human gate). |
| RK3 | `pub use init_thread_pool` collides with an existing symbol or fails under `--target bundler` | Medium | Toolchain spec mandates `--target web` migration regardless; verify export presence in a smoke build. |
| RK4 | A future frontier/optimize per-solve grid gets added and needs clarabel N-independent-solve safety, currently UNVERIFIED | Low (out of scope now) | Not required by this audit (all solve sites are single); §7 flags the local-inspection question for whenever such a grid is specced. |
| RK5 | Feature `wasm-threads` accidentally leaks onto native via a default or transitive enable → INV-01 break | High | `wasm-threads` is opt-in, never in `default`; `cargo tree` assertion in CI (verification spec). |

## Gaps Requiring Deeper Research

> NEEDS RESEARCH: Does Polars 0.44 honor `POLARS_MAX_THREADS` when set via `std::env::set_var` at first-`_inner`-call, or does it read the var only once at global rayon pool initialization (process start)? If the latter, the `Once`-guard approach is ineffective and the pin must move to build config or an explicit `rayon::ThreadPoolBuilder`. Single-agent: read vendored polars 0.44 `POOL`/`get_global_pool` init source in the Cargo registry.

> NEEDS RESEARCH: clarabel 0.11.1 internal thread-safety — does it spawn internal threads, link BLAS, or use rayon internally such that N independent solves from rayon tasks are unsafe? Not on this spec's critical path (all current solve sites are single), but required before any future per-solve parallel grid. Single-agent: local inspection of the vendored clarabel 0.11.1 Cargo.toml + solver source (cheap, definitive — it's in Cargo.lock). [Known Phase-1 gap, u7 finding 1.]

> NEEDS RESEARCH: Is closing the quantile detailed-components gap (In-Scope 12) a pure API-plumbing task, or does it require computing per-predictor RIF-quantile characteristic/coefficient contributions that do not exist in `quantile_decomposition.rs` today (`:267-271` computes aggregate effects only)? If computation is required, it is a decomposition-math change (Charter anti-goal) needing an explicit founder decision on scope. HUMAN DECISION — surfaced for the plan gate: expose-empty-API-now vs. add per-predictor quantile decomposition (new math, expanded scope).

> NEEDS RESEARCH: Confirm `wasm-bindgen-rayon` 1.3.0's `init_thread_pool` re-export mechanism (`pub use` vs a macro invocation) against the 1.3.0 README/source, since the crate's expansion pattern changed across 1.x. Single-agent: read docs.rs/wasm-bindgen-rayon/1.3.0 + local source.

## 8 Spark Notes

- Five WASM exports audited: decompose (mean+quantile) PARALLELIZE, verify_adjustments PARALLELIZE-by-inheritance, optimize/frontier/check_defensibility SKIP.
- Frontier is NOT an independent grid — budget loop (`analysis.rs:1135`) is a sequential stateful sweep; only one upfront clarabel solve. Corrects Phase-1 sketch.
- `pub use wasm_bindgen_rayon::init_thread_pool` behind `#[cfg(all(wasm,wasm-threads))]` — re-export, no hand binding.
- `wasm-threads` feature adds wasm-bindgen-rayon (optional dep) + Polars pin only; rayon stays unconditional; nothing touches native (INV-01, `cargo tree` CI assertion).
- `POLARS_MAX_THREADS=1` via `Once` = one parallel layer (perf/memory, not bit-identity — Polars ops used are order-deterministic).
- Quantile detailed-components: API accessors added; per-predictor *computation* is missing and likely math-scope — flagged for human decision.
- All per-rep seeding owned by the deterministic-rng spec; this spec touches no RNG.


## Phase 1 Sources

- phase1-engine-parallel-surface-clarabel-nalgebra.md
- phase1-deterministic-rng-parallel-seeding.md
