> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Phase 4 refined-spec writer (engine-parallel-surface) — FINAL, buildable
> Charter: In-Scope 4 (full-surface parallelization audit) + part of 1 (`init_thread_pool` export + feature gating) + **12 (full per-predictor quantile decomposition math + API)** · Binds INV-01 (native byte-equivalent), INV-03 (sequential fallback)
> Build order: **this is step 3 of 4** — determinism → memory profile → threading → validation. The `wasm-threads` feature layer here builds on the seeded RNG (step 1, `phase4-final-deterministic-rng.md`); the In-Scope-12 quantile math is additive and lands with its own golden (step 4, statistical-trust-layer).
> Depends on the deterministic-rng spec for all per-rep seeding (referenced, not respecced).

# Spec: WASM Parallel Surface, Thread-Pool Export, Feature Gating, and Full Per-Predictor Quantile Decomposition

## Summary

The engine exposes exactly five WASM entry points (`engine/src/lib.rs:26,34,42,50,59`): `decompose`, `optimize`, `verify_adjustments`, `calculate_efficient_frontier`, `check_defensibility`. This spec:

1. **Audits all five** with a parallelize/skip verdict + rationale (SC-04).
2. **Enables the thread pool**: re-export `wasm_bindgen_rayon::init_thread_pool` behind a new `wasm-threads` feature, feature-gated entirely off the native path (INV-01).
3. **Deletes the `POLARS_MAX_THREADS=1` mechanism** (L4 correction — it is a no-op on wasm) and reframes "one parallel layer" as the polars wasm `POOL` stub cooperatively routing onto our global rayon registry.
4. **Implements Charter In-Scope 12 by WIRING existing tested code** (founder scope expansion, Charter Scope-Delta 2026-07-17). **Reconciliation correction (build-readiness audit CRITICAL-1, 2026-07-18):** the per-predictor RIF-OLS quantile math ALREADY EXISTS and is tested — `OaxacaBuilder::decompose_quantile(τ)` (`builder.rs:720-766`) computes `calculate_rif` per group (`math/rif.rs:14`), swaps the outcome for its RIF, and runs the standard `OaxacaBuilder::run()`, which produces full per-predictor `detailed_explained`/`detailed_unexplained` (`builder.rs:698-699,921-946`); `rif_test.rs:38` exercises `.decompose_quantile(0.9)`. The WASM path is empty (`analysis.rs:201-204`) only because it calls the OTHER builder — `QuantileDecompositionBuilder` (MM-simulation, aggregate-only) at `analysis.rs:168`. In-Scope 12 therefore = route the WASM quantile branch through the existing `decompose_quantile()` and populate detail with the SAME extraction code the mean branch already uses (`analysis.rs:261-269`). This is ~a dozen lines, zero new statistical primitives, and eliminates the divergent-code risk of reimplementing RIF with a different KDE. **One founder decision falls out** (buildability gate): switching the quantile branch to `decompose_quantile()` changes the quantile AGGREGATE from MM-simulation to RIF-regression — a client-facing number change that makes aggregate + detail coherent (both from one additive method). See D5.

Two audit corrections retained from Phase 2 (verified against code 2026-07-17): (a) `calculate_efficient_frontier` is **not** an independent-optimization grid — its budget loop is a sequential cumulative sweep with a single upfront `clarabel` solve; verdict **SKIP**. (b) `optimize`'s single solve gates `frontier` but adds no parallel site. clarabel concurrent-solve safety is now **VERIFIED** (L5) — it does not block this spec and un-blocks any future per-solve grid.

`init_thread_pool` is a re-export, not new logic (W1 confirms plain `pub use`, no macro); the parallel sites (`builder.rs:825`, `quantile_decomposition.rs:222,227,337`) already exist as `rayon` iterators and run sequentially in today's WASM via rayon's built-in fallback. This spec makes them use a real pool without touching the iterator bodies.

## In-Scope (this domain)

- **In-Scope 4** — full-surface parallelization audit (all 5 WASM entry points, verdict + rationale each).
- **In-Scope 1 (part)** — `init_thread_pool` re-export + `wasm-threads` feature gating (toolchain flags owned by toolchain-build spec).
- **In-Scope 12 (per-predictor quantile detail — WIRING, corrected 2026-07-18)** — the RIF-OLS quantile decomposition with per-predictor detail already exists (`OaxacaBuilder::decompose_quantile`, `builder.rs:720-766`, tested by `rif_test.rs`). The WASM path returns empty detail (`analysis.rs:201-204`) because it calls the MM builder (`QuantileDecompositionBuilder`) instead. Route the WASM quantile branch through `decompose_quantile()`, extract detail via the existing `two_fold.detailed_explained()`/`detailed_unexplained()` accessors (the mean branch's exact code, `analysis.rs:261-269`), validate against a `ddecompose reweighting=FALSE` golden. Aggregate method shifts MM→RIF (founder decision, D5 + buildability gate).

Out of this domain: per-rep RNG seeding (deterministic-rng spec); the memory profile / thread-cap formula (memory-budget); toolchain nightly pin + `+atomics,+bulk-memory` + `--max-memory` + `--target web` (toolchain-build); Meridian `analysis.worker.js` wiring + fallback (meridian-integration); the golden R script itself + property tests (statistical-trust-layer). Two-stage DFL-reweighting quantile detail = **documented v2, explicitly out of scope** (W10).

## Design & Decisions

### D1 — Five-entry-point parallelize/skip audit (SC-04)

| # | Entry point | Export | Inner fn | Parallel site(s) | Verdict | Rationale |
|---|---|---|---|---|---|---|
| 1 | `decompose` (mean) | `lib.rs:26` | `decompose_inner` → `OaxacaBuilder::run` | `builder.rs:825` rep `into_par_iter` | **PARALLELIZE** (exists) | Bootstrap over `reps` (default 100, `analysis.rs:147`); hot path; seeded per rng-spec D2/D5. Order-preserving indexed collect. |
| 1b | `decompose` (quantile) | `lib.rs:26` | `QuantileDecompositionBuilder::run` | `quantile_decomposition.rs:337` rep par_iter + `:222,:227` per-τ QR par_iter | **PARALLELIZE** (exists) | Rep bootstrap + per-τ `solve_qr` both independent; seeded; MM inner resample loop (`:246-259`) stays sequential (cheap, carries `y_*_vec` push-order). |
| 1c | `decompose` (quantile **detail**, In-Scope 12 — WIRING) | `lib.rs:26` | `OaxacaBuilder::decompose_quantile` (`builder.rs:720`, EXISTING) → `run()` | inherits mean-path bootstrap `into_par_iter` (site 1) | **PARALLELIZE via inheritance** | `decompose_quantile` runs the standard `run()` on RIF-transformed data → reuses site-1's seeded rep loop + detail machinery. No new parallel site. Detail already computed by `run()`. |
| 2 | `optimize` | `lib.rs:34` | `optimize_inner` `analysis.rs:309` | none (single clarabel LP/QP solve) | **SKIP** | One solve; nothing to fan out. clarabel single-threaded default (L5). Document skip. |
| 3 | `verify_adjustments` | `lib.rs:42` | `verify_inner` `analysis.rs:40` | inherits `decompose` bootstrap | **PARALLELIZE via inheritance** | Recomputes decomposition on adjusted data; no new code — inherits site 1. |
| 4 | `calculate_efficient_frontier` | `lib.rs:50` | `..._frontier_inner` `analysis.rs:875` | one `optimize_inner` (`:950`) + **sequential** budget loop (`:1135-1166`) | **SKIP** | Budget loop carries `current_y`/`pay_idx`/`budget_cursor` across steps; `compute_t_stat` is one projector multiply (`:1089-1109`) — trivial. Not an independent grid. Corrects Phase-1 sketch. |
| 5 | `check_defensibility` | `lib.rs:59` | `check_defensibility_inner` `defensibility.rs:9` | none | **SKIP** | Scalar scoring over decompose output; trivial/serial. Document skip. |

**clarabel concurrent-solve safety (L5, VERIFIED):** resolved features on the wasm graph = `clarabel v0.11.1 [default,serde]`; the only rayon references live behind unused `faer-sparse`/`pardiso` features; default build is built-in direct quasi-definite LDL, single-threaded, no BLAS, no rayon. Global state = one module-level `AtomicF64` INFINITY bound (`utils/infbounds.rs:15`) mutated only by `set_infinity()` (never called). **Conclusion: concurrent clarabel solves from parallel rayon tasks are safe.** Frontier stays sequential *by design* (D1 verdict SKIP), not because clarabel forbids parallel — a future per-solve grid is unblocked.

### D2 — `init_thread_pool` re-export behind `wasm-threads` (W1)

```rust
// engine/src/lib.rs — after existing wasm wrappers (after :64)
#[cfg(all(feature = "wasm", feature = "wasm-threads"))]
pub use wasm_bindgen_rayon::init_thread_pool;
```

W1 confirmed: plain function re-export (no macro); re-exporting makes wasm-bindgen emit an async `initThreadPool(numThreads) → Promise` in the generated JS glue; `wasm-bindgen = "0.2"` caret admits the repo's 0.2.106. Meridian worker calls `await init(); await initThreadPool(cap);` (that wiring is the meridian-integration spec's surface).

### D3 — Feature plumbing (engine + oaxaca_blinder), native untouched (INV-01)

```toml
# engine/Cargo.toml
[dependencies]
wasm-bindgen-rayon = { version = "1.3.0", optional = true }
[features]
wasm-threads = ["dep:wasm-bindgen-rayon", "wasm", "oaxaca_blinder/wasm-threads"]

# oaxaca_blinder/Cargo.toml
[features]
wasm-threads = []   # cfg gate only; rayon stays UNCONDITIONAL (Cargo.toml:23) so native parallelism is unchanged
```

`rayon` remains an unconditional dependency of `oaxaca_blinder` — native builds are byte-equivalent in behavior (INV-01). The `wasm-threads` feature adds only the optional `wasm-bindgen-rayon` dep and cfg gates; **nothing on the native path**. `wasm-threads` is never in any `default` feature set.

### D4 — DELETE `POLARS_MAX_THREADS=1`; reframe "one parallel layer" (L4 — MAJOR correction)

**Phase-2's `ensure_single_layer()` / `POLARS_MAX_THREADS=1` mechanism is DELETED.** L4 verified against polars-core-0.44.2 `src/lib.rs:49-68` + polars-utils-0.44.2 `src/wasm.rs`:

- On wasm, `POOL` is `polars_utils::wasm::Pool` — a **stub**. `POLARS_MAX_THREADS` is **never read** on wasm (the env-read sits in the native-only arm). Setting it via `std::env::set_var` is a **no-op**.
- The stub's semantics: `install(op)` runs `op()` **inline**; `join`/`scope`/`spawn` **delegate to the rayon GLOBAL registry** (`rayon::join` etc.); `current_num_threads()` → `rayon::current_num_threads()`.

**Reframed one-parallel-layer analysis:** after `initThreadPool`, wasm-bindgen-rayon installs a global rayon registry with N workers. Polars' wasm stub routes its `join`/`scope`/`spawn` onto **our same global pool** — cooperative work-stealing on one pool, **not** oversubscription (there is only one pool). `install`-wrapped column-parallel paths (e.g. `take`'s `try_apply_columns_par`, L1) run inline-caller but schedule their inner `par_iter` on the global pool. Determinism is unaffected: per-column apply and fork-join `join` are structured parallelism with deterministic result placement.

**Residual audit (narrow, must be named):** the only bit-identity exposure would be a polars-internal **parallel float reduction** (sum/mean kernel) on the hot path. Our engine computes all statistics in **nalgebra after `take`** (the Polars ops used are `filter`, `take`, `vstack`, `hstack` — all order-deterministic, no float reduction), so the exposure is confined to any accidental Polars aggregation. AC-4 greps the hot path to confirm no Polars `.sum()/.mean()/.agg()` runs between resample and nalgebra. With threads NOT initialized (sequential fallback), the stub runs everything on the current thread — degrades safely (INV-03).

### D5 — Per-predictor quantile detail: WIRE the existing `decompose_quantile()` (In-Scope 12; corrected 2026-07-18 per build-readiness audit CRITICAL-1)

**Ground truth (verified this phase, `builder.rs:710-766` + `analysis.rs:150-269` + `rif_test.rs`).** The crate has TWO quantile decomposition paths:

1. **`OaxacaBuilder::decompose_quantile(τ)`** (`builder.rs:720-766`) — the **RIF-regression** path. It calls `calculate_rif` per group (`math/rif.rs:14`, which computes exactly `RIF = q_τ + (τ − 1{y ≤ q_τ})/f_Y(q_τ)` with its own R-Type-7 quantile + Silverman Gaussian KDE, `rif.rs:22-85`), replaces the outcome column with the RIF, and runs the **standard `OaxacaBuilder::run()`** on the transformed data. `run()` already computes **full per-predictor detail** (`detailed_explained`/`detailed_unexplained` via `process_detailed_components`, `builder.rs:634,698-699,921-946`). This is one-stage RIF-OLS with additive per-predictor detail — **exactly what W10 recommends, already built**, and tested by `rif_test.rs:38` (`.decompose_quantile(0.9)`).
2. **`QuantileDecompositionBuilder`** (`quantile_decomposition.rs:21`) — the **MM-simulation** path, aggregate-only (`:267-271`). This is what the WASM `engine/src/analysis.rs:168` currently calls, which is the ONLY reason detail is empty at `analysis.rs:201-204` ("Currently not exposed in public API for QuantileDecompositionDetail").

**Therefore In-Scope 12 requires NO new statistical code.** The earlier draft's premise ("the math does not exist today") was wrong — it read only the MM path. The corrected task is to route the WASM quantile branch through the existing RIF path:

```rust
// engine/src/analysis.rs — replace the quantile branch (currently :166-206) with:
if let Some(q) = req.quantile {
    let mut builder = OaxacaBuilder::new(df, &req.outcome_variable,
                                         &req.group_variable, &req.reference_group);
    builder.predictors(predictors.iter().copied());
    builder.reference_coefficients(ref_coef);
    if let Some(cats) = &cats_vec { builder.categorical_predictors(cats.iter().copied()); }
    builder.bootstrap_reps(reps);
    let results = builder.decompose_quantile(q).map_err(|e| e.to_string())?;  // RIF path, returns OaxacaResults

    // Extract aggregate + detail with the SAME code the mean branch already uses (analysis.rs:250-269):
    let two_fold = results.two_fold();
    let (mut explained, mut unexplained, mut unexplained_std_err) = (0.0, 0.0, None);
    for c in two_fold.aggregate() {
        match c.name() { "explained" => explained = *c.estimate(),
                         "unexplained" => { unexplained = *c.estimate(); unexplained_std_err = Some(*c.std_err()); },
                         _ => {} }
    }
    let d_exp:   Vec<DetailedComponent> = two_fold.detailed_explained().iter().map(to_detailed).collect();
    let d_unexp: Vec<DetailedComponent> = two_fold.detailed_unexplained().iter().map(to_detailed).collect();
    (*results.total_gap(), explained, unexplained, None, d_exp, d_unexp, unexplained_std_err)
} else { /* mean/OLS branch unchanged (analysis.rs:207-269) */ }
```

`OaxacaResults` (returned by `decompose_quantile`) is the **same type** the mean branch consumes, so `two_fold().detailed_explained()` and the `DetailedComponent` mapping at `analysis.rs:261-269` apply verbatim. The `#[wasm_bindgen]` signatures (`lib.rs:26-64`) and the postMessage `{type,payload}` contract are **unchanged** — detail flows through the `DecompositionResult` fields the mean path already serializes.

**SEAM FIX — seed propagation (council CV-1, CRITICAL).** `decompose_quantile` builds a **fresh inner `OaxacaBuilder`** (`builder.rs:752-759`) and today does NOT forward `self.seed`. Since the deterministic-rng spec adds a `seed: Option<u64>` field + `.seed()`/`.seed_from_entropy()` to `OaxacaBuilder`, wiring the RIF path would silently no-op chosen-seed reproducibility on the quantile path (INV-02 "same seed" clause broken there; `RunMetadata.seed` wrong). **Required in the same change:** at `builder.rs:759`, forward the seed to the inner builder — `builder.seed_opt(self.seed);` (or `.seed(self.seed.unwrap_or(DEFAULT_SEED))`). This is co-owned with the deterministic-rng spec (which defines the seed field) — both specs state it. AC-11 (below) verifies it.

**SE rigor (council MJ-5 — RATIFIED: recompute RIF per replicate).** `decompose_quantile` today computes the RIF **once** on the full sample then bootstraps the RIF-transformed frame, so it does NOT re-estimate `q_τ`/`f_Y(q_τ)` per replicate — understating the quantile CIs. Founder ruling 4: **fix it** — move the `calculate_rif` transform INTO the per-replicate bootstrap closure so each resample re-estimates its own quantile + density before the RIF-OLS. Concretely: `decompose_quantile` no longer pre-transforms and calls the standard `run()`; instead it runs its own seeded bootstrap (reusing the deterministic-rng per-rep stream + owned index resampling) where each rep = resample → `calculate_rif` on the resample → RIF-OLS + OB detail. The point estimate still uses the full-sample RIF. Engine records `fixed_rif:false`. Cost ~2× per quantile rep (extra `calculate_rif` per rep) — accepted. Exercise τ∈{0.10,0.50,0.90} for finiteness (`rif.rs:75` density floor 1e-8). The trust-spec golden validates these corrected SEs against `ddecompose`'s bootstrap SEs.

**Categoricals + normalization already handled:** `decompose_quantile` → `run()` already routes categoricals through `create_dummies_manual` + the existing Gardeazabal-Ugidos `normalize_categorical_coefficients` (`math/normalization.rs:5`, wired via `.normalize(...)` at `builder.rs:759`). Education_Level (3-level, L6) is handled with no new code. Seeding comes from the deterministic-rng spec's rep stream (same `run()` bootstrap loop) — no separate RNG here.

**RATIFIED (buildability gate 2026-07-18): option a-1 + recompute-RIF-per-replicate.** Founder chose to switch BOTH the WASM/MCP branch (`analysis.rs:166-206`) AND the CLI (`main.rs:247`) to `decompose_quantile` (RIF) — one coherent method everywhere; the quantile aggregate changes MM→RIF on both surfaces (accepted). AND the bootstrap must **recompute the RIF inside each replicate** (ruling 4) for correct CIs (`fixed_rif:false`). The a/b/c analysis below is retained for rationale; a-1 is the decision. See `buildability-gate-rulings.md`.

**Historical decision analysis (a-1 chosen).** Routing the quantile branch through `decompose_quantile()` shifts the quantile **aggregate** from the MM-simulation estimator to the RIF-regression estimator — different numbers, but coherent with the detail. The options considered:

| Option | Aggregate | Detail | Coherence | Verdict |
|---|---|---|---|---|
| **(a) Switch to RIF `decompose_quantile`** (recommended) | RIF | RIF (existing) | ✅ detail sums to aggregate (additive, W10) | Reuses tested code; defensible; ~12 lines; **changes displayed quantile numbers** (MM→RIF) |
| (b) Keep MM aggregate + bolt RIF detail on | MM | RIF | ❌ RIF detail does NOT sum to the MM aggregate | Incoherent — reject (silent-failure #6) |
| (c) Expose BOTH (MM aggregate + RIF decomposition as separate output fields) | MM + RIF | RIF | ⚠️ two methods surfaced; consumer must choose | Backward-compatible on MM aggregate; most UI/serialization work; new fields break the "postMessage API unchanged" anti-goal |

Option (a) is the coherent, additive, defensible route and is what W10's methodology + the existing code support. It is a **client-facing methodology change** to the quantile aggregate, so the founder ratifies at the buildability gate. Option (c) preserves the current MM aggregate if backward-compat on those specific numbers matters, at the cost of a serialization change. Option (b) is rejected (detail wouldn't reconcile with the aggregate).

**CROSS-SURFACE consequence (council CV-2, MAJOR).** Option (a) switches only the WASM/MCP quantile branch (`analysis.rs`). `oaxaca-cli` (`main.rs:247`) still constructs `QuantileDecompositionBuilder` (MM) — so the **same tool returns different quantile numbers on the CLI vs the browser**, and the two use different sample-quantile definitions (MM path nearest-rank vs `rif.rs` R-Type-7). **RATIFIED a-1:** the CLI (`main.rs:247`) switches to `decompose_quantile` in the same change — one method (RIF) on every surface, no CLI-vs-browser divergence. `QuantileDecompositionBuilder` (MM) stays exported for any direct-API consumer but is no longer the default quantile path. AC-13 asserts CLI and WASM return byte-identical quantile aggregates on the same fixture+seed.

**Golden (statistical-trust-layer owns the R script):** R `ddecompose::ob_decompose(formula, data, group, rifreg_statistic="quantiles", rifreg_probs=c(0.10,0.50,0.90), reweighting=FALSE, bootstrap=TRUE)` — `reweighting=FALSE` = one-stage RIF-OLS = FFL-2009 form = exactly what `decompose_quantile()` computes; fixed `set.seed()`; per-covariate detail in the summary. This validates the EXISTING `decompose_quantile` output (which `rif_test.rs` currently only smoke-tests), closing a real coverage gap. Optional Stata `oaxaca_rif` cross-check.

**Documented limitations the spec must state (W10 §Defensibility):** (i) local-linear approximation of `E[RIF|X]` — nonlinearity → specification error; (ii) no double-robustness without reweighting; (iii) sensitivity to the `f_Y(q_τ)` density estimate; (iv) base-category dependence for categoricals (mitigated by G-U normalization). Two-stage DFL-reweighting (four-term doubly-robust) = documented v2, out of scope.

### D6 — Feature matrix (post-L4: no Polars pin row)

| Build | features | wasm-bindgen-rayon | initThreadPool export | Parallel exec |
|---|---|---|---|---|
| native CLI/mcp/rlib | (none) | absent | absent | rayon default pool (unchanged, INV-01) |
| wasm sequential | `wasm` | absent | absent | rayon serial fallback (INV-03) |
| wasm threaded | `wasm,wasm-threads` | present | present | rayon WASM global pool; polars stub co-schedules on it |

### RK — Risks and mitigations

| ID | Risk | Severity | Mitigation |
|---|---|---|---|
| RK1 | Polars-internal parallel float reduction on the hot path breaks bit-identity | Low | D4 residual audit + AC-4 grep confirms stats run in nalgebra after `take`; no Polars aggregation between resample and nalgebra. |
| RK2 | Reimplementing RIF would create a SECOND divergent RIF/KDE path in the crate (audit CRITICAL-1) | **Closed by correction** | In-Scope 12 now WIRES the existing `decompose_quantile()` (`builder.rs:720`) — no new RIF/KDE code, no divergence. The `ddecompose` golden validates the existing path (which was only smoke-tested). |
| RK3 | `pub use init_thread_pool` collides with a symbol or fails under `--target bundler` | Medium | Toolchain spec mandates `--target web` regardless; AC-2 smoke-build asserts the export appears. |
| RK4 | RIF density estimate `f_Y(q_τ)` numerically unstable at extreme τ | Low | The existing `calculate_rif` (`rif.rs:51-75`) already floors density at 1e-8 and clamps spread; golden covers τ∈{0.10,0.50,0.90}; the `ddecompose` golden + `E[RIF]=q_τ` identity catch gross errors (AC-8). Pre-existing behavior, not new. |
| RK5 | `wasm-threads` leaks onto native via a default/transitive enable → INV-01 break | High | Opt-in only, never in `default`; `cargo tree` assertion (AC-3) proves `wasm-bindgen-rayon` absent from the native graph. |

## Build Steps (ordered, buildable)

1. **Feature plumbing.** `engine/Cargo.toml`: add optional `wasm-bindgen-rayon = "1.3.0"` + `wasm-threads` feature (D3). `oaxaca_blinder/Cargo.toml:34-36`: add `wasm-threads = []`.
2. **Re-export.** `engine/src/lib.rs` (after `:64`): add the D2 `#[cfg(all(feature="wasm",feature="wasm-threads"))] pub use wasm_bindgen_rayon::init_thread_pool;`.
3. **Delete Polars pin.** Confirm no `POLARS_MAX_THREADS` / `ensure_single_layer` exists anywhere (`grep -rn "POLARS_MAX_THREADS\|ensure_single_layer" engine/ oaxaca_blinder/` → 0); if a Phase-2 draft artifact exists, remove it. Add a one-line doc comment at each `*_inner` top noting the L4 single-pool reasoning instead.
4. **SKIP doc comments.** Add a one-line SKIP-rationale doc comment at `optimize_inner` (`analysis.rs:309`), `..._frontier_inner` (`analysis.rs:875`), `check_defensibility_inner` (`defensibility.rs:9`), each naming its solve/scoring location.
5. **Seed forwarding + per-rep RIF (rulings CV-1 + 4).** Modify `decompose_quantile` (`builder.rs:720-766`): (i) forward `self.seed` to the inner path; (ii) run a seeded bootstrap that recomputes `calculate_rif` inside each replicate (resample → `calculate_rif` on resample → RIF-OLS + OB detail), point estimate on the full-sample RIF; record `fixed_rif:false`. Reuses deterministic-rng's per-rep stream + owned index resampling.
6. **Wire BOTH surfaces to the RIF path (ruling a-1).** (a) In `engine/src/analysis.rs`, replace the quantile branch (`:166-206`) with `OaxacaBuilder::new(...).decompose_quantile(q)` → extract aggregate + detail via `two_fold().detailed_explained()`/`detailed_unexplained()` + the existing `DetailedComponent` mapping (`analysis.rs:261-269`). (b) In `oaxaca-cli` (`main.rs:247`), replace the `QuantileDecompositionBuilder` construction with `OaxacaBuilder::...decompose_quantile(q)` so the CLI uses RIF too. No new statistical primitives.
7. **Golden coverage.** statistical-trust-layer adds the `ddecompose reweighting=FALSE` golden validating `decompose_quantile` output incl. the per-replicate-RIF bootstrap SEs — closes the pre-existing coverage gap (`rif_test.rs:38` was smoke-only).
8. **Keep MM builder compiling.** `QuantileDecompositionBuilder` (`quantile_decomposition.rs`) is no longer the default quantile path on either surface but is NOT deleted (still exported `lib.rs:84` for direct-API consumers); confirm it compiles and its own tests pass.
9. **Cross-surface parity (AC-13).** Confirm CLI and WASM quantile aggregates are byte-identical on the same fixture+seed (both RIF now).
10. **Build gate.** `cargo build` (native) + `cargo build -p pay-equity-engine --features wasm --target wasm32-unknown-unknown` + `cargo build -p pay-equity-engine --features wasm,wasm-threads --target wasm32-unknown-unknown` all succeed.

## Acceptance Criteria (objectively checkable)

- **AC-1 (audit table present).** This spec's D1 table covers all five `engine/src/lib.rs` exports (`decompose`, `optimize`, `verify_adjustments`, `calculate_efficient_frontier`, `check_defensibility`), each with a verdict + rationale; the two SKIP-with-a-solve rows name the solve location (`analysis.rs:950`, `analysis.rs:1135-1166`). Verifiable against the `#[wasm_bindgen]` export list at `lib.rs:26,34,42,50,59`.
- **AC-2 (export smoke, exit 0).** A wasm build with `--features wasm,wasm-threads` emits an `initThreadPool` symbol in the generated JS glue (`grep -q "initThreadPool" pkg/*.js`); a build with `--features wasm` (no threads) does **not** emit it and still compiles/links.
- **AC-3 (native isolation, exit 0).** `cargo tree -p pay-equity-engine 2>/dev/null | grep -c wasm-bindgen-rayon` returns `0` (absent from the native graph); `cargo build` + `cargo test` (native, no features) pass — INV-01 upheld.
- **AC-4 (single-parallel-layer, grep + exit 0).** `grep -rn "POLARS_MAX_THREADS\|ensure_single_layer" engine/ oaxaca_blinder/` returns 0 lines. A hot-path grep confirms no Polars float reduction between resample and nalgebra: `grep -nE "\.sum\(\)|\.mean\(\)|\.agg\(|group_by" oaxaca_blinder/src/builder.rs oaxaca_blinder/src/quantile_decomposition.rs` shows no Polars-aggregation call on the resample→stats path (nalgebra/slice sums only).
- **AC-5 (SKIP rationale, grep).** Each of `optimize_inner`, `..._frontier_inner`, `check_defensibility_inner` carries a doc comment containing the word `SKIP` and its solve/scoring anchor (`grep -A2 "fn optimize_inner\|fn calculate_efficient_frontier_inner\|fn check_defensibility_inner"` shows it).
- **AC-6 (In-Scope 12 API closed, exit 0).** After wiring the WASM quantile branch to `decompose_quantile()`, a test on `Employers_data.csv` (quantile request, ≥2 predictors incl. Education_Level) asserts `engine::analysis::decompose(...)` returns `detailed_exp`/`detailed_unexp` vectors of length = predictor count (post-dummy), **not** empty — i.e. `analysis.rs:201` no longer returns unconditional empties. (No new accessor is added; the existing `two_fold().detailed_explained()` is used.)
- **AC-7 (golden parity, exit 0).** For the fixed-seed `Employers_data.csv` fixture at τ∈{0.10,0.50,0.90}, each per-predictor `C_{k,τ}` and `S_{k,τ}` matches the R `ddecompose::ob_decompose(..., reweighting=FALSE)` golden within a stated tolerance (tolerance + R script owned by statistical-trust-layer; this spec requires the test to exist and pass).
- **AC-8 (adding-up + RIF identities, exit 0).** Property test on the `decompose_quantile` output: (a) `Σ_k explained_k == aggregate explained` and `Σ_k unexplained_k == aggregate unexplained` within f64 tolerance (additive by construction — the RIF path IS the mean-path OB algebra on RIF-transformed data, so this reuses the mean-path adding-up test); (b) the `calculate_rif` output satisfies `mean(rif) ≈ q_τ` (`rif.rs` identity); (c) G-U invariance — permuting the Education_Level base category leaves each per-predictor detail invariant (reuses the mean-path G-U test since `decompose_quantile` runs the same `run()`).
- **AC-9 (MM path intact under option (a), exit 0).** `QuantileDecompositionBuilder` (`quantile_decomposition.rs`) is unmodified and still exported (`lib.rs:84`); `cargo test -p oaxaca_blinder` (incl. any MM/quantile tests) passes — the WASM path stops calling it but it is not deleted. Under option (c), additionally assert the MM aggregate value is byte-identical to pre-change.
- **AC-11 (seed propagation on the RIF quantile path, exit 0 — council CV-1).** `decompose_quantile` forwards the seed: a test asserts `.seed(1)` vs `.seed(2)` on `decompose_quantile` yields non-equal serialized bytes, `.seed(X)` reproduces byte-identically across two runs, and `RunMetadata.seed == X`. (Guards the seam between the In-Scope 12 wiring and the deterministic-rng seed API.)
- **AC-12 (quantile SE validation, exit 0 — ruling 4).** RIF recomputed per bootstrap replicate → engine records `fixed_rif: false`; the RIF-path per-predictor bootstrap SEs match `ddecompose`'s bootstrap SEs (statistical-trust-layer golden) within the measured-then-pinned tolerance. τ∈{0.10,0.50,0.90} all return finite detail (density floor 1e-8).
- **AC-13 (cross-surface parity, exit 0 — ruling a-1).** A CLI quantile run (`oaxaca-cli`, now RIF) and a WASM quantile run on the same fixture+seed return **byte-identical aggregates** — one method (RIF) on both surfaces, no divergence.
- **AC-10 (threaded build, exit 0).** `cargo build -p pay-equity-engine --features wasm,wasm-threads --target wasm32-unknown-unknown` succeeds; the `pub use` compiles under the cfg.

## Open Items

All buildability-gate decisions for this spec are **RATIFIED** (2026-07-18, `buildability-gate-rulings.md`):
1. **Strategy A** (dual artifact) — this spec is artifact-agnostic; the `wasm-threads` build produces the threaded artifact, the untouched stable seq blob remains. Feature gating, L4 reframe, In-Scope-12 wiring unchanged.
2. **In-Scope 12 = a-1 + recompute-RIF-per-replicate** — both surfaces (WASM + CLI) use RIF `decompose_quantile`; per-rep RIF for correct CIs (`fixed_rif:false`). No open decision remains.

The In-Scope-12 golden tolerance + R `ddecompose` script live in the statistical-trust-layer spec. clarabel per-solve-grid parallelization is **not** in scope (all current solve sites are single); L5 verified it is safe if a future spec wants it.

## Sources

- Phase 1: `phase1-engine-parallel-surface-clarabel-nalgebra.md`, `phase1-deterministic-rng-parallel-seeding.md`
- Phase 3 local: `phase3-local-l1-l2-l3-l4-l5-l6-findings.md` — L1 (`take` per-column POOL.install), **L4** (POLARS_MAX_THREADS no-op on wasm; POOL stub routes onto global rayon), **L5** (clarabel 0.11.1 single-threaded default, concurrent solves safe), L6 (Education_Level 3-level categorical → 2 dummies)
- Phase 3 web: `phase3-web-w1-w2-w3-w4-w5-w6-w7-w8-w9-w10-findings.md` — **W1** (`init_thread_pool` plain `pub use`, no macro; async `initThreadPool` Promise), **W10** (one-stage RIF-OLS detail methodology, additive, G-U normalization, MM non-additive, `ddecompose` golden signature). W10 URLs: FFL 2009/2018 (eml.berkeley.edu/~cle/secnf/fortinlemieux.pdf), `ddecompose` (cran.r-project.org/web/packages/ddecompose), `oaxaca_rif` (fmwww.bc.edu/repec/bocode/o/oaxaca_rif.sthlp), Toepfer 2017 (econstor.eu/bitstream/10419/168422)
- Deterministic-rng dependency: `phase4-final-deterministic-rng.md` (all per-rep seeding; RIF-OLS detail reuses the rep stream)
- Code anchors (verified 2026-07-17; In-Scope 12 anchors re-verified 2026-07-18): `engine/src/lib.rs:26,34,42,50,59,84`, `engine/src/analysis.rs:40,147,166-206,207-269,261-269,309,875,950,1089-1109,1135-1166`, `engine/src/defensibility.rs:9`, `oaxaca_blinder/src/builder.rs:720-766` (`decompose_quantile`, EXISTING RIF path), `:634,698-699,921-946` (`run()` detail machinery), `:800-814,825`, `oaxaca_blinder/src/quantile_decomposition.rs:21,173` (`run_single_pass` — MM path), `:215,222,227,244,267-271` (MM parallel sites), `oaxaca_blinder/src/math/rif.rs:14` (`calculate_rif`, EXISTING), `math/{ols,normalization}.rs`, `oaxaca_blinder/tests/rif_test.rs:38`, `engine/Cargo.toml:18,46`, `oaxaca_blinder/Cargo.toml:23,25`
- Charter: `spec-charter.md` — In-Scope 1, 4, 12; INV-01, INV-03, INV-06/07/08; SC-04; Scope-Delta Log 2026-07-17 (In-Scope 12 upgrade + anti-goal carve-out)
