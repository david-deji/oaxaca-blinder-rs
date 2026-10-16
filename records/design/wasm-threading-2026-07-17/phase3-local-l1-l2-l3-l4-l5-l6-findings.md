# Phase 3 — LOCAL findings (L1–L6)

> Orchestrator-direct, 2026-07-17. Method: vendored-source reads in `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`,
> workspace manifest/lock reads, `cargo tree` on the wasm32 graph, CSV scan. All findings deterministic (ground-truth reads,
> not model recall). Confidence: [CONSENSUS]-equivalent — direct source evidence, file:line cited.

## L1 — polars take semantics ✅ (answers rng RK3/M3)

`DataFrame::take(&IdxCa)` exists at polars-core-0.44.2 `src/frame/mod.rs:1841`:

```rust
pub fn take(&self, indices: &IdxCa) -> PolarsResult<Self> {
    let new_col = POOL.install(|| self.try_apply_columns_par(&|s| s.take(indices)))?;
    Ok(unsafe { DataFrame::new_no_checks(indices.len(), new_col) })
}
```

- **Duplicates + unsorted indices: accepted.** It is gather semantics — each output row i = input row idx[i]; nothing sorts or dedups. With-replacement bootstrap resampling via an owned index vector is directly supported.
- **Row count: exactly `indices.len()`** (`new_no_checks(indices.len(), ...)`).
- **Bounds: checked** (per-column `s.take(indices)` returns `PolarsResult`; the safe path validates in-bounds). `take_unchecked` (`:1849`) skips validation — spec should use the safe `take` (bounds bugs → error, not UB).
- **Nulls in the index chunked array** would produce null rows — irrelevant for us: we construct the IdxCa from generated in-bounds integers, no nulls. `IdxSize` = u32 default (no `bigidx`) — 50k rows far under limit.
- **Materialization**: gather allocates new column buffers of idx.len() rows (fresh contiguous output chunks); the base frame is untouched/shared. This is the memory-lever behavior the memory-budget spec assumes.
- **Parallelism wrinkle**: the take runs under `POOL.install(|| ...try_apply_columns_par...)` — per-COLUMN parallel apply. See L4 for what POOL means on wasm. Per-column tasks are independent and collected in column order → no determinism hazard, but it composes with our rep-level parallelism on the same rayon pool (nested work-stealing, benign; same-pool = no oversubscription).

## L2 — rand_chacha pairing ✅

- Cargo.lock already contains **rand_chacha 0.3.1** (correct pairing for rand 0.8.5; the lock's 0.9.0 copy belongs to the separate rand 0.9.2 subtree pulled by another dependency — do not use it with rand 0.8 traits).
- `ChaCha8Rng::set_stream(u64)` **present** at rand_chacha-0.3.1 `src/chacha.rs:237` (macro-generated for ChaCha8/12/20). `seed_from_u64` available via `rand_core::SeedableRng` blanket impl.
- oaxaca_blinder currently declares only `rand = "0.8.5"` (`oaxaca_blinder/Cargo.toml:25`) — spec must add `rand_chacha = "0.3"` as a direct dependency (zero new transitive code; already in lock/graph).

## L3 — getrandom on wasm ✅

Verified on the actual wasm32-unknown-unknown feature graph (`cargo tree --target wasm32-unknown-unknown -p pay-equity-engine --features wasm`):

- **getrandom 0.3.4 resolves with `[wasm_js]`** — enabled explicitly by the engine (`engine/Cargo.toml:18,46` — `getrandom = { version = "0.3", optional = true }` + `"getrandom/wasm_js"` inside the `wasm` feature).
- **getrandom 0.2.16 resolves with `[js,js-sys,std,wasm-bindgen]`** — the `js` feature is already enabled transitively on the wasm graph (via the polars/ahash chain), which is why the current wasm build compiles at all (getrandom 0.2 hard-errors on wasm32-unknown without `js`).
- Consequence: **entropy-based seeding works in the browser today for both getrandom generations.** The determinism design does not depend on this (seeds are supplied/derived), but `seed_from_entropy`-style fallbacks won't break wasm, and no manifest change is needed for RNG entropy support.

## L4 — POLARS_MAX_THREADS on wasm ✅ — MAJOR CORRECTION to Phase 2 assumption

polars-core-0.44.2 `src/lib.rs:49-68`:

- **Native (`cfg(not(target_family = "wasm"))`)**: `POOL: Lazy<rayon::ThreadPool>` — reads `POLARS_MAX_THREADS` **once, at first POOL use** (Lazy static; process-wide, never re-read). Answer to the literal L4 question: read at global pool init, not dynamic.
- **Wasm (`cfg(target_family = "wasm")`)**: `POOL` is `polars_utils::wasm::Pool` — a **stub** (polars-utils-0.44.2 `src/wasm.rs`) with mixed semantics:
  - `install(op)` → runs `op()` **inline** (no pool),
  - `join(a, b)` / `scope(op)` / `spawn(f)` → **delegate to the rayon GLOBAL registry** (`rayon::join` etc.),
  - `current_num_threads()` → `rayon::current_num_threads()`.
- **`POLARS_MAX_THREADS` is never read on wasm** (the env-read sits in the native-only arm; `std::env::var` is empty on wasm anyway). The Phase-2 mitigation "set POLARS_MAX_THREADS=1 to keep one parallel layer" is a **no-op on wasm and must be dropped/reframed** in the Phase-4 specs.
- **Reframed one-parallel-layer analysis**: after `initThreadPool`, wasm-bindgen-rayon installs a global rayon registry with N workers. Polars' wasm stub then routes its `join`/`scope`/`spawn` calls onto **our same global pool** — cooperative work-stealing, not oversubscription (there is only one pool). `install`-wrapped column-parallel paths run inline-caller but their inner `par_iter` (`try_apply_columns_par`) still schedules on the global pool. Determinism: per-column apply and fork-join `join` are structured parallelism with deterministic result placement; the residual risk to audit in Phase 4 is any polars-internal **parallel float reduction** (e.g. sum/mean kernels) on the hot path — our engine computes stats via nalgebra after `take`, so the exposure is narrow, but the spec must name the audit.
- Also relevant: with threads NOT initialized (sequential fallback mode), rayon's wasm fallback runs everything on the current thread — stub semantics degrade safely (matches INV-03).

## L5 — clarabel internals ✅ (closes the frontier-gating question)

- Resolved features on our wasm graph: **`clarabel v0.11.1 [default,serde]`** (dep declared at `oaxaca_blinder/Cargo.toml:26`).
- The only rayon references in clarabel-0.11.1 live in `faer_ldl.rs` / `pardiso.rs` — behind **`faer-sparse` / `pardiso` features, NOT enabled** in our graph. Default build = built-in direct quasi-definite LDL, single-threaded, no BLAS, no rayon.
- Global state scan: `utils/infbounds.rs:15` — one module-level `AtomicF64` INFINITY bound (only mutated by explicit `set_infinity()`, which we never call); the `python/pyblas` lazy_statics are Python-binding-only, not compiled for us.
- **Conclusion: concurrent clarabel solves from parallel rayon tasks are safe** (each solve owns its workspace; shared state is one read-only-in-practice atomic). The frontier remains a sequential cumulative sweep by design (Phase-2 verdict SKIP stands), but nothing in clarabel blocks parallel independent solves if a future design wants them. `optimize` single-solve: no change.

## L6 — Education_Level categories ✅

`/home/deji/Downloads/Employers_data.csv` (10,001 lines incl. header): exactly **3 distinct values** — `Master` (4,930), `Bachelor` (3,381), `PhD` (1,689). No missing values in the column. The file encodes no ordinal ranking; natural ordinality (Bachelor < Master < PhD) exists semantically — fixture/golden scripts should treat it as **categorical** (matching engine behavior today) and note the 3-level structure gives 2 dummy columns under one-hot with base category; Gardeazabal-Ugidos invariance tests get a real 3-level categorical to exercise.

## Provenance (crates.io / docs.rs for the exact vendored versions read)

All findings above are direct reads of source in `~/.cargo/registry/src/index.crates.io-.../`. The registry sources correspond one-to-one to these published releases:

- polars-core 0.44.2 — https://docs.rs/polars-core/0.44.2/ (POOL init `src/lib.rs`; `DataFrame::take` `src/frame/mod.rs:1841`)
- polars-utils 0.44.2 — https://docs.rs/polars-utils/0.44.2/ (`src/wasm.rs` Pool stub)
- clarabel 0.11.1 — https://crates.io/crates/clarabel/0.11.1 (default features `[default,serde]`, no rayon)
- rand_chacha 0.3.1 — https://docs.rs/rand_chacha/0.3.1/ (`ChaCha8Rng::set_stream` present)
- getrandom 0.3.4 `[wasm_js]` + 0.2.16 `[js]` — https://docs.rs/getrandom/0.3.4/ , https://docs.rs/getrandom/0.2.16/

## Impact on Phase 4 inputs

1. Drop/reframe every `POLARS_MAX_THREADS=1` mention (memory-budget, engine-parallel-surface, verification drafts) per L4.
2. `take` is bounds-checked and gather-true → index-vector resampling design confirmed buildable exactly as drafted (deterministic-rng, memory-budget).
3. Add `rand_chacha = "0.3"` direct dep (already in lock — no version drift).
4. clarabel thread-safety VERIFIED (was UNVERIFIED in Phase 1/2) — frontier SKIP stands on design grounds only.
5. Fixture generator: Education_Level 3-level categorical confirmed; one-hot → 2 dummies.
