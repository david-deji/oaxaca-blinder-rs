> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Phase 4 refined-spec writer (deterministic-rng) — FINAL, buildable
> Charter: In-Scope 3 + 10 · Binds INV-02 (bit-identical) master criterion, INV-08 (engine stays f64)
> Build order: **this is step 1 of 4** — determinism → memory profile → threading → validation. Nothing downstream builds until this ships and its parity tests pass.

# Spec: Bit-Identical Seeded RNG for Bootstrap and MM-Simulation

## Summary

The engine has three nondeterministic RNG sites, all fed by unseeded entropy — verified against source 2026-07-17:

1. **Mean-path bootstrap resample** — `builder.rs:832,835` `sample_n_literal(h, true, false, None)` (trailing `None` = seed slot; unseeded).
2. **Quantile-path bootstrap resample** — `quantile_decomposition.rs:344,347` (same unseeded `sample_n_literal`).
3. **Quantile MM simulation** — `quantile_decomposition.rs:215` `rand::thread_rng()` (draws the τ grid) and `:244` `rand::thread_rng()` (row resampling in the counterfactual loop).

Two consequences today: (a) run-to-run output is not reproducible; (b) failed bootstrap reps are **silently discarded** by `filter_map(...).ok()` (`builder.rs:827/846-847`, `quantile_decomposition.rs:339/352`), so the surviving-rep count — and therefore the standard errors — is itself nondeterministic (`builder.rs:850` reads `bootstrap_results.len()`).

This spec replaces all three sites with a single master-seed → per-unit `rand_chacha::ChaCha8Rng` stream scheme (`ChaCha8Rng::seed_from_u64(master)` + `set_stream(purpose‖unit)`), replaces Polars' internal sampler with **owned index-vector resampling** (`ChaCha8Rng` → `Vec<IdxSize>` → `DataFrame::take(&IdxCa)`, confirmed gather-semantics and bounds-checked per L1), converts the silent discard into a **deterministic, order-preserving** partition with a recorded discard count, and pins the summation path in `bootstrap_stats` as sequential. It adds a `RunMetadata` record (seed, algorithm, crate version, requested/succeeded/discarded rep counts) to both result types and serializes it into the JSON output the parity harness byte-compares.

**Master acceptance criterion (INV-02):** for a fixed seed, `decompose` output serializes byte-identically across native, wasm-sequential, and wasm-threaded (1/2/4 threads).

## In-Scope (this domain)

- Charter **In-Scope 3** — bit-identical seeded RNG: refactor `builder.rs` bootstrap and `quantile_decomposition.rs` MM simulation to schedule-independent per-rep seeding.
- Charter **In-Scope 10** — determinism as prerequisite + standalone feature: land and verify the seeded-RNG fix (incl. fixing the silent bootstrap-rep discard and the unseeded `sample_n_literal(...,None)` / `thread_rng()` sites) **before** any threading work builds on it.

Out of this domain (owned elsewhere, referenced here): the `wasm-threads` feature + `init_thread_pool` re-export (engine-parallel-surface); the memory profile and per-rep clone removal (memory-budget); the parity/benchmark harness and golden files (verification-benchmark, statistical-trust-layer). This spec **produces** the byte-identical JSON those specs gate on.

## Design & Decisions

### D1 — Seeded builder API (both builders)

`OaxacaBuilder` and `QuantileDecompositionBuilder` each gain a `seed: Option<u64>` field plus two methods:

```rust
pub const DEFAULT_SEED: u64 = 0x5EED_0A11_CA8A_0002;

impl OaxacaBuilder {                          // and QuantileDecompositionBuilder
    /// Fixed master seed for all bootstrap resampling. Reproducible-by-default.
    pub fn seed(&mut self, seed: u64) -> &mut Self { self.seed = Some(seed); self }
    /// Draw one master seed from OS entropy and RECORD it in RunMetadata so the run
    /// stays reproducible after the fact. Native/CLI + wasm both OK (getrandom verified, L3).
    pub fn seed_from_entropy(&mut self) -> &mut Self { /* rng.rs::draw_entropy_seed() */ self }
}
```

**Decision (decide-and-proceed):** default is the fixed documented constant `DEFAULT_SEED`, **not** required-explicit. Audit defensibility wants reproducible-by-default; `seed_from_entropy()` is the opt-in for run-to-run variety and still records the drawn seed. The internal `seed` field defaults to `None` and resolves to `DEFAULT_SEED` at `run()`.

`seed_from_entropy()` entropy draw uses `getrandom::getrandom(&mut buf)` into a `[u8;8]` → `u64::from_le_bytes` inside `rng.rs`. **Council correction (MINOR):** getrandom resolves on the wasm *graph* via `engine` (L3), but `oaxaca_blinder/Cargo.toml` declares **no direct `getrandom` dep** (only `rand = "0.8.5"`, which pulls getrandom 0.2 transitively) — a direct `getrandom::getrandom` call from `oaxaca_blinder/src/rng.rs` will not link on wasm without a direct dep + feature. **Decision (decide-and-proceed): scope `seed_from_entropy()` to native only** (`#[cfg(not(target_family = "wasm"))]`), since the default reproducible path uses `DEFAULT_SEED` and the wasm consumer (Meridian) always passes an explicit seed or takes the default — entropy seeding is a CLI/native affordance. If wasm entropy is later needed, add `getrandom = { version = "0.3", features = ["wasm_js"], optional = true }` to `oaxaca_blinder` under a wasm feature. AC-5 (entropy round-trip) runs native-only.

### D2 — Master-seed → per-unit stream derivation (new module `oaxaca_blinder/src/rng.rs`)

One pure helper produces every per-unit RNG. It is a closed-form function of `(master, purpose, unit)` — no thread id, no clock, no atomic counter — which is the SC-03 proof-sketch.

```rust
// oaxaca_blinder/src/rng.rs  (NEW)
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[derive(Clone, Copy)]
pub enum RngPurpose { Bootstrap = 0, QuantileTau = 1, QuantileResample = 2 }

/// Per-unit ChaCha8 stream. `set_stream` gives statistically independent streams for
/// adjacent ids by ChaCha construction; purpose is spaced into high bits to guarantee no
/// cross-site stream collision. Bit-identical across thread counts because (master,purpose,
/// unit) fully determines the stream with zero dependence on execution order.
pub fn unit_rng(master: u64, purpose: RngPurpose, unit: u64) -> ChaCha8Rng {
    let mut rng = ChaCha8Rng::seed_from_u64(master);
    rng.set_stream(((purpose as u64) << 40) ^ unit);   // set_stream present @ rand_chacha-0.3.1 chacha.rs:237 (L2)
    rng
}
```

Stream assignment (each rep gets an independent per-rep stream keyed on `rep`, satisfying "per-rep `set_stream(rep)`"):

- **Mean bootstrap rep `r`**: group A = `unit_rng(master, Bootstrap, r*2)`, group B = `unit_rng(master, Bootstrap, r*2+1)`.
- **Quantile bootstrap rep `r`**: derive `rep_master = master ^ (r.wrapping_mul(0x9E37_79B9_7F4A_7C15))` (splitmix step decorrelates the two nesting levels), passed into `run_single_pass`; the point-estimate call uses raw `master` with reserved sentinel `unit = u64::MAX`.
- **Inside `run_single_pass` (quantile)**: τ grid = `unit_rng(rep_master, QuantileTau, 0)`; MM row resampling = `unit_rng(rep_master, QuantileResample, 0)`.

### D3 — Owned index-vector resampling replaces `sample_n_literal` (L1-confirmed)

L1 verified `DataFrame::take(&IdxCa)` at polars-core-0.44.2 `frame/mod.rs:1841`: gather semantics, accepts duplicate + unsorted indices, returns exactly `indices.len()` rows, **bounds-checked** (safe `take`, not `take_unchecked`), no null indices in our construction. This is exactly with-replacement bootstrap resampling.

```rust
use polars::prelude::{IdxCa, IdxSize};
use rand::Rng;

fn resample_indices(rng: &mut ChaCha8Rng, n: usize) -> IdxCa {
    let idx: Vec<IdxSize> = (0..n).map(|_| rng.gen_range(0..n as IdxSize)).collect();
    IdxCa::from_vec("idx".into(), idx)          // with-replacement: indices may repeat
}
// per rep (mean path, replacing builder.rs:828-838):
let sample_a = df_a_global.take(&resample_indices(&mut rng_a, df_a_global.height()))?;
let sample_b = df_b_global.take(&resample_indices(&mut rng_b, df_b_global.height()))?;
let sample_df = sample_a.vstack(&sample_b)?;
```

`take` performs no float reduction and is order-preserving → Polars becomes a pure data container, removing dependence on its unseeded, cross-version-unstable sampler. The `df_a_global.clone()`/`df_b_global.clone()` at `builder.rs:828-829` and `quantile_decomposition.rs:340-341` become an immutable borrow of the shared base frame (`take` reads, does not mutate) — the memory-budget spec owns turning that into a measured lever; this spec only removes the *need* to clone.

L1 note (benign): `take` runs under `POOL.install(|| try_apply_columns_par)` — per-column parallel apply, collected in column order → deterministic, and on wasm the `POOL` stub routes onto our one global rayon pool (no oversubscription, per L4). No determinism hazard.

### D4 — Quantile MM simulation seeded (`quantile_decomposition.rs:215,244`)

```rust
// τ grid (was thread_rng @ :215)
let mut tau_rng = unit_rng(rep_master, RngPurpose::QuantileTau, 0);
let uniform_dist = Uniform::from(0.01..0.99);
let random_quantiles: Vec<f64> =
    (0..self.simulations).map(|_| uniform_dist.sample(&mut tau_rng)).collect();

// counterfactual row resampling (was thread_rng @ :244)
let mut resample_rng = unit_rng(rep_master, RngPurpose::QuantileResample, 0);
for i in 0..num_successful_sims {
    let rand_idx_a = resample_rng.gen_range(0..x_a.nrows());
    let rand_idx_b = resample_rng.gen_range(0..x_b.nrows());
    /* ... unchanged dot-products at :250-258 ... */
}
```

The per-τ `par_iter` at `:222`/`:227` **stays parallel**: `random_quantiles` is a fixed ordered `Vec` and `par_iter().filter_map(...).map(...).collect()` is indexed/order-preserving, so `betas_a`/`betas_b` are bit-identical regardless of thread count. The MM inner loop at `:246-259` stays sequential (it carries `y_aa_vec`/`y_bb_vec`/`y_ab_vec` push-order and is cheap).

### D5 — Deterministic failed-rep handling replaces the silent `filter_map` discard

Replace `filter_map(|_| {...}.ok())` (`builder.rs:827`, `quantile_decomposition.rs:339`) with an order-preserving `map` to an explicit outcome, then a **sequential** partition:

```rust
enum RepOutcome<T> { Ok(T), Failed }

let outcomes: Vec<RepOutcome<SinglePassResult>> = (0..self.bootstrap_reps)
    .into_par_iter()                       // indexed → order-preserving collect (INV-02)
    .map(|rep| match run_one_rep(rep) {    // run_one_rep pure fn of (master, rep, base frames)
        Ok(r)  => RepOutcome::Ok(r),
        Err(_) => RepOutcome::Failed,
    })
    .collect();

let mut bootstrap_results = Vec::with_capacity(outcomes.len());
let mut discarded = 0usize;                // sequential fold — deterministic order
for o in outcomes {
    match o { RepOutcome::Ok(r) => bootstrap_results.push(r), RepOutcome::Failed => discarded += 1 }
}
```

Each rep's success/failure is a pure function of `(master, rep, base frames)`, so the discard set is deterministic and thread-count-independent. The current `eprintln!` at `builder.rs:852-855` stays as a non-authoritative log; the **authoritative** count is `RunMetadata.bootstrap_reps_discarded`.

The quantile bootstrap loop (`:337-354`) gets the identical rewrite, threading `rep_master` (D2) into `run_single_pass`.

### D6 — Sequential summation guarantee in `bootstrap_stats`

`bootstrap_stats` (`oaxaca_blinder/src/inference.rs:4`) already sums via slice iterators (`iter().sum`, variance loop, `sort_unstable_by`). Requirement: it stays `fn(&[f64], f64) -> (f64, f64, (f64,f64))` with **no** `rayon` reduce/fold introduced, and its input is always the indexed-collected, order-fixed `estimates` vector built by the caller (`builder.rs:858` `process_component` closure). Add a one-line `// INV-02: sequential summation, do not parallelize` guard comment. Float addition is non-associative; a parallel reduce would reorder partial sums and break bit-identity.

### D7 — Run metadata recording

```rust
// oaxaca_blinder/src/rng.rs (or types)
#[derive(serde::Serialize, Clone)]
pub struct RunMetadata {
    pub seed: u64,
    pub rng_algorithm: &'static str,        // "ChaCha8"
    pub rand_chacha_version: &'static str,  // captured pin string, e.g. "0.3.1"
    pub bootstrap_reps_requested: usize,
    pub bootstrap_reps_succeeded: usize,
    pub bootstrap_reps_discarded: usize,
}
```

Attached to `OaxacaResults` and `QuantileDecompositionResults` with a public getter `run_metadata(&self) -> &RunMetadata`, and surfaced through the engine `DecompositionResult` (`engine/src/types.rs`) so it lands in the serialized JSON the parity harness byte-compares.

### D8 — Crate dependency (L2)

Add `rand_chacha = "0.3"` to `oaxaca_blinder/Cargo.toml` `[dependencies]` (currently only `rand = "0.8.5"` at `Cargo.toml:25`). Locked decision (writer-brief): **`"0.3"`, not an exact `=` pin** — the exact `0.3.1` is already pinned in `Cargo.lock` (L2), `ChaCha8Rng::set_stream` present there, and INV-04's committed-`Cargo.lock` + double-build sha256 (owned by toolchain/verification specs) is what enforces byte-reproducibility. Zero new transitive code (already in the graph).

### Signature deltas (verified against actual code)

- `OaxacaBuilder` / `QuantileDecompositionBuilder`: add `seed: Option<u64>` field; `.seed()`, `.seed_from_entropy()`.
- **Mean-path `run_single_pass` is UNCHANGED** — verified `builder.rs:819-820` it takes `(&df, &all_dummy_names, &category_counts, &base_categories)` and calls **no RNG**; resampling happens in the bootstrap closure *before* `run_single_pass`. Mean point estimates are byte-unchanged (guarded by `tests/parity_test.rs`).
- **Quantile `run_single_pass` gains one param**: `fn run_single_pass(&self, df, all_dummy_names, rep_master: u64) -> Result<SinglePassResult>` — verified current sig `(&self, df: &DataFrame, all_dummy_names: &[String])` **defined at `quantile_decomposition.rs:173`**; call sites `:321` (point est → `u64::MAX` sentinel path) and `:352` (bootstrap → `rep_master`).
- `OaxacaResults` / `QuantileDecompositionResults`: add `run_metadata: RunMetadata` + getter.
- **`decompose_quantile` seed forwarding (council CV-1, CRITICAL — co-owned with engine-parallel-surface).** `OaxacaBuilder::decompose_quantile` (`builder.rs:720-766`) constructs a fresh inner `OaxacaBuilder` at `:752-759` and does NOT forward `self.seed`. When In-Scope 12 wires the WASM quantile branch to `decompose_quantile`, chosen-seed reproducibility silently breaks on that path unless the seed is forwarded. **Required:** at `builder.rs:759`, add `builder.seed_opt(self.seed);` (a passthrough setter that copies the `Option<u64>` verbatim so `None`→`None` still resolves to `DEFAULT_SEED` in the inner `run()`). Add a quantile-seed AC (below).
- **New AC (quantile seed round-trip).** `.seed(1)` vs `.seed(2)` on `decompose_quantile` → non-equal serialized bytes; `.seed(X)` reproduces byte-identically; `RunMetadata.seed == X`. (The existing AC-6 covers the mean path only; this covers the RIF quantile path the WASM tool now uses.)

### RK — Risks and mitigations

| ID | Risk | Severity | Mitigation |
|---|---|---|---|
| RK1 | Quantile point estimate changes value (nondeterministic→deterministic); no stable pre-change baseline to byte-compare | Medium | Intended: the new deterministic value **is** the baseline. Parity compares across MODES at a fixed seed, not against pre-change output. Mean path separately guaranteed byte-unchanged. |
| RK2 | `set_stream`/`gen_range` semantics differ across a future `rand_chacha` bump | Medium | `Cargo.lock` pins 0.3.1 (L2); INV-04 double-build sha256 catches drift; `RunMetadata.rand_chacha_version` records it. |
| RK3 | `DataFrame::take` behaves unexpectedly on dup/unsorted indices | **Closed by L1** | Verified gather-semantics, bounds-checked, exactly `idx.len()` rows @ polars-core-0.44.2 `frame/mod.rs:1841`. |
| RK4 | `gen_range` resampler shifts SEs enough to fail a golden | Low | Golden re-baselines against the new seeded pipeline (statistical-trust-layer owns); this is a resampling-implementation change by design, not a correctness regression. |
| RK5 | Two-level seeding correlates rep streams | Low | splitmix64 step (D2) decorrelates; AC-3 disjoint-purpose + referential-transparency unit test guards it. |

## Build Steps (ordered, buildable)

1. **Add dependency.** In `oaxaca_blinder/Cargo.toml` `[dependencies]`, add `rand_chacha = "0.3"`. Run `cargo tree -p oaxaca_blinder | grep rand_chacha` — expect `rand_chacha v0.3.1`.
2. **Create `oaxaca_blinder/src/rng.rs`** with `DEFAULT_SEED`, `RngPurpose`, `unit_rng`, `resample_indices`, `draw_entropy_seed`, `RunMetadata`; add `mod rng;` to `lib.rs`.
3. **Mean path — `builder.rs:825-848`.** Replace the `into_par_iter().filter_map(...).collect()` block with the D5 `map`→`RepOutcome`→sequential-partition; inside `run_one_rep`, build `rng_a`/`rng_b` via `unit_rng(master, Bootstrap, r*2 | r*2+1)`, resample with D3 `take(&IdxCa)`, drop the `sample_n_literal` calls and the per-rep `df_a_global.clone()`/`df_b_global.clone()` (borrow instead). Resolve `master = self.seed.unwrap_or(DEFAULT_SEED)` at the top of `run()`.
4. **Mean path — metadata.** Compute `discarded` from the partition; build `RunMetadata`; thread it into `OaxacaResults`. Keep the `eprintln!` at `:852-855` as a non-authoritative log.
5. **Quantile path — `run_single_pass` signature.** Add `rep_master: u64` param (definition at `:173`); update call sites `:321` (sentinel) and `:352` (rep_master).
6. **Quantile path — MM seeding.** Replace `thread_rng()` at `:215` and `:244` with the D4 `unit_rng` streams.
7. **Quantile path — bootstrap loop `:337-354`.** Apply the D5 rewrite (owned `take` resampling via `unit_rng(...,Bootstrap,...)`, `RepOutcome` partition, `rep_master` per D2); drop `sample_n_literal` and per-rep clones.
8. **Quantile path — metadata.** Attach `RunMetadata` to `QuantileDecompositionResults` + getter.
9. **`inference.rs:4`.** Add the D6 INV-02 guard comment; confirm no rayon in the file.
10. **Engine surface — `engine/src/types.rs`.** Add `run_metadata` to `DecompositionResult` (serde) so it serializes into `decompose` JSON.
11. **Tests** (paths per AC below): `oaxaca_blinder/tests/rng_determinism.rs`, extend `tests/parity_test.rs`.
12. **Build gate.** `cargo build` (native) and `cargo build -p pay-equity-engine --features wasm --target wasm32-unknown-unknown` both succeed.

## Acceptance Criteria (objectively checkable)

- **AC-1 (grep, zero matches).** `grep -rn "sample_n_literal" oaxaca_blinder/src/` returns 0 lines. `grep -rn "thread_rng" oaxaca_blinder/src/quantile_decomposition.rs` returns 0 lines.
- **AC-2 (grep, zero matches).** `grep -rnE "par_iter|into_par_iter|par_bridge|par_extend" oaxaca_blinder/src/inference.rs` returns 0 lines.
- **AC-3 (unit test, exit 0).** `cargo test -p oaxaca_blinder rng::` passes: (a) `unit_rng(m,p,u)` referential transparency — same args → identical first 16 `u64` draws; (b) the three `RngPurpose` values yield pairwise-disjoint draw sequences for the same `(master,unit)`; (c) `resample_indices(rng,n)` for a fixed rng is bit-identical across two calls with cloned rng and every index `< n`.
- **AC-4 (default determinism, exit 0).** Two `OaxacaBuilder::run()` calls on identical input with no `.seed()` produce byte-identical serialized `OaxacaResults` (serde_json bytes equal); `.seed(1)` vs `.seed(2)` produce non-equal bytes.
- **AC-5 (entropy round-trip, exit 0).** A `.seed_from_entropy()` run records a non-`DEFAULT_SEED` value in `RunMetadata.seed`; re-running with `.seed(that_value)` reproduces byte-identical output.
- **AC-6 (INV-02 cross-mode compare, exit 0) — SPLIT per council MJ-1, pending founder INV-02 reframe.** Cross-ISA floating-point (native glibc libm vs wasm32 libm: `.exp()`/`.powf`/statrs `cdf`) makes native↔wasm **byte**-identity almost certainly unachievable, and `parity_test.rs:24` already uses a 1e-6 tolerance. The threading-safety property the founder's "bit-identical" ruling targets is *within-platform-across-thread-counts*. Therefore:
  - **WITHIN-platform byte-identical (achievable, the real threading property):** `sha256(json_native_1t) == sha256(json_native_2t) == sha256(json_native_4t)` AND (in the browser harness) `sha256(json_wasm_seq) == sha256(json_wasm_t2) == sha256(json_wasm_t4)`, for mean and quantile requests, fixed seed.
  - **ACROSS platform (native↔wasm) tolerance-parity:** `|native − wasm| ≤ 1e-6` (matching the trust-suite), NOT sha256 equality.
  **RATIFIED 2026-07-18 (ruling 2): the split.** Founder chose "Accept the split" — within-platform byte-identical + native↔wasm ≤1e-6 tolerance is INV-02 (Charter reframed). Not open.
- **AC-7 (discard determinism, exit 0).** For a seed engineered to force ≥1 rep failure, `RunMetadata.bootstrap_reps_discarded` is identical across `RAYON_NUM_THREADS=1/2/4`, and `bootstrap_reps_succeeded + bootstrap_reps_discarded == bootstrap_reps_requested`; downstream SEs are byte-identical across the three thread counts.
- **AC-8 (metadata presence, exit 0).** Serialized `decompose` output contains a `run_metadata` object with all six fields; `run_metadata.seed` echoes the effective master seed; `run_metadata.rng_algorithm == "ChaCha8"`.
- **AC-9 (mean-path byte-unchanged, exit 0).** `cargo test -p oaxaca_blinder --test parity_test` passes: mean-path point estimates (the non-bootstrap `run_single_pass` output) are byte-identical to a recorded pre-refactor baseline (RNG structure change does not touch mean point math).
- **AC-10 (native build parity, exit 0).** `cargo build` and `cargo test` (native, no wasm features) pass — determinism refactor introduces no native regression (INV-01 upheld from this spec's side).

## Open Items

None blocking; all gate decisions RATIFIED (2026-07-18). **Strategy A** ratified — this spec is artifact-agnostic (thread-count- and artifact-independent RNG design); the verification mode matrix gains the second artifact. **INV-02 split** ratified (see AC-6). **Seed propagation** into `decompose_quantile` is now a build requirement (Signature deltas §, co-owned with engine-parallel). The `rand_chacha = "0.3"` choice stands (Cargo.lock enforces the exact version for INV-04).

## Sources

- Phase 1: `phase1-deterministic-rng-parallel-seeding.md`, `phase1-engine-parallel-surface-clarabel-nalgebra.md`
- Phase 3 local: `phase3-local-l1-l2-l3-l4-l5-l6-findings.md` — L1 (`take` gather/bounds @ polars-core-0.44.2 `frame/mod.rs:1841`), L2 (`rand_chacha` 0.3.1 in lock, `set_stream` @ `chacha.rs:237`), L3 (getrandom wasm entropy), L4 (POOL stub / wasm parallelism)
- Phase 3 web: `phase3-web-w1-w2-w3-w4-w5-w6-w7-w8-w9-w10-findings.md` — W9 (R/Stata seed reproducibility, defensibility framing)
- Code anchors (verified 2026-07-17): `oaxaca_blinder/src/builder.rs:819-856`, `oaxaca_blinder/src/quantile_decomposition.rs:215,244,267-271,321,337-354`, `oaxaca_blinder/src/inference.rs:4`, `engine/src/analysis.rs:147`, `engine/src/types.rs`
- Charter: `spec-charter.md` — In-Scope 3, 10; INV-02, INV-04, INV-08; SC-03
