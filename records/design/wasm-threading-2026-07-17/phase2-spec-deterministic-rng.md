> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (deterministic-rng) — Draft

# Spec: Bit-Identical Seeded RNG for Bootstrap and MM-Simulation

Covers Charter In-Scope 3 and 10; binds INV-02 (bit-identical) as master acceptance criterion, INV-07/08 (Decimal boundary — engine stays f64). Point-estimate math untouched for the mean path; RNG *structure* only.

---

## 1 Executive Summary

The engine has three nondeterministic RNG sites, all fed by unseeded entropy:

1. Mean-path bootstrap resample — `builder.rs:832,835` `sample_n_literal(h, true, false, None)` (the trailing `None` is the seed).
2. Quantile-path bootstrap resample — `quantile_decomposition.rs:344,347` (same `None`-seed `sample_n_literal`).
3. Quantile MM simulation — `quantile_decomposition.rs:215` `thread_rng()` (draws the τ grid) and `quantile_decomposition.rs:244` `thread_rng()` (row resampling in the counterfactual loop).

Two consequences today: (a) run-to-run output is not reproducible, and (b) failed bootstrap reps are *silently discarded* by `filter_map(...).ok()` (`builder.rs:827`, `quantile_decomposition.rs:339`), so the surviving rep count — and therefore the standard errors — is itself nondeterministic.

This spec replaces all three sites with a single master-seed → per-unit `rand_chacha::ChaCha8Rng` stream scheme (`ChaCha8Rng::seed_from_u64(master)` + `set_stream(purpose‖unit)`), replaces Polars' internal sampler with **owned index-vector resampling** (`ChaCha8Rng` → `Vec<IdxSize>` → `DataFrame::take`), converts the silent `filter_map` discard into a **deterministic, order-preserving** partition with a recorded discard count, and pins the summation path in `bootstrap_stats` (`inference.rs:4`) as sequential. It adds a `RunMetadata` record (seed, algorithm, crate version, requested/succeeded/discarded rep counts) to both result types and serializes it into the JSON output that the parity harness byte-compares.

The master acceptance criterion is INV-02: for a fixed seed, `decompose` output serializes byte-identically across native, wasm-sequential, and wasm-threaded (1/2/4 threads).

## 2 Requirements

### R1 — Seeded builder API

Both `OaxacaBuilder` and `QuantileDecompositionBuilder` gain seed control.

```rust
// oaxaca_blinder/src/builder.rs (and quantile_decomposition.rs builder)
pub const DEFAULT_SEED: u64 = 0x5EED_0A11_CA8A_0002;

impl OaxacaBuilder {
    /// Set the master seed for all bootstrap resampling. Deterministic by default
    /// (DEFAULT_SEED); call to override for a distinct reproducible run.
    pub fn seed(&mut self, seed: u64) -> &mut Self { self.seed = Some(seed); self }
    /// Draw one master seed from OS entropy and RECORD it in RunMetadata, so the
    /// run stays reproducible after the fact. For genuine cross-run resample variety.
    pub fn seed_from_entropy(&mut self) -> &mut Self { /* OsRng.next_u64() */ self }
}
```

Semantics decision (founder-facing, resolved by decide-and-proceed): **default is a fixed documented constant `DEFAULT_SEED`, not required-explicit.** Rationale: audit defensibility wants reproducible-by-default; `seed_from_entropy()` is the opt-in for run-to-run variety and still records the drawn seed. The `seed` field defaults to `None` internally and resolves to `DEFAULT_SEED` at `run()`.

**Acceptance:** two `run()` calls on identical input with no `.seed()` produce byte-identical serialized results. `.seed(1)` and `.seed(2)` produce different results. A `.seed_from_entropy()` run records a non-default seed in `RunMetadata.seed` and re-running with that recorded seed reproduces it byte-identically.

### R2 — Master-seed → per-unit stream derivation (both paths, all sites)

A single shared helper produces the per-unit RNG. It is a pure function of `(master, purpose, unit)` and therefore independent of thread scheduling.

```rust
// oaxaca_blinder/src/rng.rs  (NEW)
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[derive(Clone, Copy)]
pub enum RngPurpose { Bootstrap = 0, QuantileTau = 1, QuantileResample = 2 }

/// Per-unit ChaCha8 stream. `set_stream` gives statistically independent streams for
/// adjacent ids by ChaCha construction (rust-random book guide-parallel), so no seed
/// mixing is required; purpose is spaced into the high bits to guarantee no cross-site
/// stream collision. Bit-identical across thread counts because (master,purpose,unit)
/// fully determines the stream with zero dependence on execution order.
pub fn unit_rng(master: u64, purpose: RngPurpose, unit: u64) -> ChaCha8Rng {
    let mut rng = ChaCha8Rng::seed_from_u64(master);
    rng.set_stream(((purpose as u64) << 40) ^ unit);
    rng
}
```

Stream assignment:
- Mean bootstrap rep `r`: group A = `unit_rng(master, Bootstrap, r*2)`, group B = `unit_rng(master, Bootstrap, r*2+1)`.
- Quantile bootstrap rep `r`: the derived per-rep master passed into `run_single_pass` is `master ^ (r.wrapping_mul(0x9E37_79B9_7F4A_7C15))` (splitmix step to decorrelate the two levels); the point-estimate call uses the raw `master` with a reserved sentinel `unit = u64::MAX`.
- Inside `run_single_pass` (quantile): τ grid = `unit_rng(rep_master, QuantileTau, 0)`; MM row resampling = `unit_rng(rep_master, QuantileResample, 0)`.

**Acceptance:** SC-03 proof-sketch holds — each stream is a closed-form function of `(master, purpose, unit)` containing no thread id, no `thread_rng`, no clock, no atomic counter; a unit test asserts `unit_rng` is referentially transparent (same args → identical first 16 `u64` draws) and that the three purposes yield disjoint draw sequences for the same `(master, unit)`.

### R3 — Owned index-vector resampling replaces `sample_n_literal`

Replace both `sample_n_literal(h, true, false, None)` sites (mean `builder.rs:831-836`, quantile `quantile_decomposition.rs:343-348`) with self-owned resampling:

```rust
use polars::prelude::{IdxCa, IdxSize};
use rand::Rng;

fn resample_indices(rng: &mut ChaCha8Rng, n: usize) -> IdxCa {
    let idx: Vec<IdxSize> = (0..n).map(|_| rng.gen_range(0..n as IdxSize)).collect();
    IdxCa::from_vec("idx".into(), idx)         // with-replacement: indices may repeat
}
// per rep:
let sample_a = df_a_global.take(&resample_indices(&mut rng_a, df_a_global.height()))?;
let sample_b = df_b_global.take(&resample_indices(&mut rng_b, df_b_global.height()))?;
let sample_df = sample_a.vstack(&sample_b)?;
```

This makes Polars a pure data container (`take` is deterministic and order-preserving; it performs no float reduction), removes dependence on Polars' unseeded, cross-version-unstable sampler, and — per the cross-domain memory finding — lets the base frames be shared read-only instead of cloned per rep (the `df_a_global.clone()` at `builder.rs:828`/`quantile_decomposition.rs:340` becomes an immutable borrow; the memory-budget spec owns that lever).

**Acceptance:** no `sample_n_literal` call remains in `builder.rs` or `quantile_decomposition.rs` (grep returns zero). Draw sequence of `resample_indices` is bit-identical across thread counts for a fixed rng.

### R4 — Quantile MM simulation seeded (`quantile_decomposition.rs:215,244`)

```rust
// τ grid (was thread_rng at :215)
let mut tau_rng = unit_rng(rep_master, RngPurpose::QuantileTau, 0);
let uniform = Uniform::from(0.01..0.99);
let random_quantiles: Vec<f64> = (0..self.simulations).map(|_| uniform.sample(&mut tau_rng)).collect();
// counterfactual row resampling (was thread_rng at :244)
let mut resample_rng = unit_rng(rep_master, RngPurpose::QuantileResample, 0);
for _ in 0..num_successful_sims {
    let rand_idx_a = resample_rng.gen_range(0..x_a.nrows());
    let rand_idx_b = resample_rng.gen_range(0..x_b.nrows());
    /* ... unchanged dot-products ... */
}
```

The per-τ `par_iter` at `:222`/`:227` stays parallel: `random_quantiles` is a fixed ordered `Vec` and `par_iter().filter_map(...).collect()` is indexed/order-preserving, so `betas_a`/`betas_b` are bit-identical regardless of thread count.

**Acceptance:** no `thread_rng()` call remains in `quantile_decomposition.rs`. Quantile `run_single_pass` is a pure function of `(rep_master, data)` — same args → byte-identical `SinglePassResult`.

### R5 — Deterministic failed-rep handling replaces the silent `filter_map` discard

Replace `filter_map(|_| {...}.ok())` (`builder.rs:827`, `quantile_decomposition.rs:339`) with an order-preserving map to an explicit outcome, then a sequential partition:

```rust
enum RepOutcome<T> { Ok(T), Failed }

let outcomes: Vec<RepOutcome<SinglePassResult>> = (0..self.bootstrap_reps)
    .into_par_iter()                       // indexed → order-preserving collect
    .map(|rep| match run_one_rep(rep) {     // run_one_rep is pure fn of (master, rep, data)
        Ok(r)  => RepOutcome::Ok(r),
        Err(_) => RepOutcome::Failed,
    })
    .collect();

let mut bootstrap_results = Vec::with_capacity(outcomes.len());
let mut discarded = 0usize;                 // sequential fold — deterministic order
for o in outcomes { match o { RepOutcome::Ok(r) => bootstrap_results.push(r), RepOutcome::Failed => discarded += 1 } }
```

Because each rep's success/failure is a pure function of `(master, rep, data)`, the discard set is deterministic and thread-count-independent — the current `eprintln!` warning becomes a testable metadata field, not a race.

**Acceptance:** for a fixed seed, `RunMetadata.discarded` is identical across 1/2/4 threads and across native/wasm; the surviving-rep ordering in `bootstrap_results` is identical (proven by byte-identical downstream SEs).

### R6 — Sequential summation guarantee in `bootstrap_stats`

`bootstrap_stats` (`inference.rs:4`) already sums via slice iterators (`inference.rs:10` `iter().sum`, `:11-16` variance, `:26-27` `sort_unstable_by`). Requirement: it stays `fn(&[f64], f64)` with no `rayon` reduce/fold introduced, and the input slice is always the indexed-collected, order-fixed `estimates` vector. Add a one-line invariant comment anchoring this to INV-02.

**Acceptance:** grep for `par_iter`/`into_par_iter`/`par_bridge` in `inference.rs` returns zero; `bootstrap_stats(&e, p)` is a pure function of slice contents and order.

### R7 — Run metadata recording

```rust
// oaxaca_blinder/src/rng.rs or types
#[derive(serde::Serialize, Clone)]
pub struct RunMetadata {
    pub seed: u64,
    pub rng_algorithm: &'static str,        // "ChaCha8"
    pub rand_chacha_version: &'static str,  // env!("CARGO_PKG_VERSION")-captured pin, e.g. "0.3.1"
    pub bootstrap_reps_requested: usize,
    pub bootstrap_reps_succeeded: usize,
    pub bootstrap_reps_discarded: usize,
}
```

Attached to `OaxacaResults` and `QuantileDecompositionResults` with a public getter (`run_metadata(&self) -> &RunMetadata`), and surfaced through the engine `DecompositionResult` (engine/src/types.rs) so it lands in the serialized JSON.

**Acceptance:** serialized `decompose` output contains a `run_metadata` object with all six fields; `seed` echoes the effective master seed; `..._discarded == requested - succeeded`.

## 3 Technical Architecture

```
OaxacaBuilder.seed(u64)?  ──resolve──► master: u64 (DEFAULT_SEED if None)
QuantileDecompositionBuilder.seed(u64)?
        │
        ├─ mean bootstrap  (builder.rs:825 par_iter, order-preserving)
        │     rep r ─► unit_rng(master,Bootstrap,r*2 | r*2+1) ─► resample_indices ─► df.take
        │
        └─ quantile bootstrap (qd.rs:337 par_iter)  +  point est (qd.rs:321)
              rep r ─► rep_master ─► run_single_pass(rep_master)
                          ├─ τ grid   : unit_rng(rep_master,QuantileTau,0)   (qd.rs:215)
                          ├─ per-τ QR : par_iter :222/:227 (indexed collect)
                          └─ MM resamp: unit_rng(rep_master,QuantileResample,0) (qd.rs:244)
        │
   Vec<RepOutcome> ──sequential partition──► Vec<SinglePassResult> + discarded
        │
   bootstrap_stats(&[f64]) sequential (inference.rs) ──► ComponentResult
        │
   RunMetadata{seed,alg,ver,req,ok,disc} ──serde──► JSON (parity byte-compare, SC-03)
```

Crate: add `rand_chacha = "=0.3.1"` to `oaxaca_blinder/Cargo.toml` `[dependencies]` (pinned exact, compatible with the existing `rand = "0.8.5"` at `Cargo.toml:25`). The `=` pin is load-bearing for INV-04 reproducibility.

## 4 Implementation Details

| Site | Anchor | Before | After |
|---|---|---|---|
| Mean bootstrap loop | `builder.rs:825-848` | `filter_map` + `sample_n_literal(...,None)` + per-rep `df.clone()` | `map`→`RepOutcome`, `unit_rng`+`resample_indices`+`take`, shared-borrow frames |
| Mean discard warning | `builder.rs:850-856` | `eprintln!` on shortfall | fold into `RunMetadata`, keep `eprintln!` as non-authoritative log |
| Quantile bootstrap loop | `quantile_decomposition.rs:337-353` | same `filter_map`/`sample_n_literal` | same rewrite; pass `rep_master` into `run_single_pass` |
| Quantile τ grid | `quantile_decomposition.rs:215-219` | `thread_rng()` | `unit_rng(rep_master,QuantileTau,0)` |
| Quantile MM resample | `quantile_decomposition.rs:244-259` | `thread_rng()` `gen_range` | `unit_rng(rep_master,QuantileResample,0)` |
| `run_single_pass` (quantile) | `quantile_decomposition.rs:~185` sig + call sites `:321,:352` | `(&self,df,names)` | add `rep_master: u64` param |
| `bootstrap_stats` | `inference.rs:4` | sequential (already) | add INV-02 guard comment; no code change |
| New module | `oaxaca_blinder/src/rng.rs` | — | `unit_rng`, `RngPurpose`, `RunMetadata`, `resample_indices`, `DEFAULT_SEED` |
| Cargo | `oaxaca_blinder/Cargo.toml:18-33` | — | `rand_chacha = "=0.3.1"` |

Signature deltas:
- `OaxacaBuilder`/`QuantileDecompositionBuilder`: add `seed: Option<u64>` field; `.seed()`, `.seed_from_entropy()`.
- `fn run_single_pass(&self, df, all_dummy_names, rep_master: u64) -> Result<SinglePassResult>` (quantile; +1 param).
- `OaxacaResults`/`QuantileDecompositionResults`: add `run_metadata: RunMetadata` + getter.

Point-estimate note: the mean path calls no RNG in `run_single_pass` (`builder.rs:819-820`) — mean point estimates are byte-unchanged and guarded by `tests/parity_test.rs`. The quantile point estimate DID use `thread_rng` and was therefore nondeterministic; seeding it changes it from nondeterministic to deterministic. This is the intended fix (same MM procedure, now seeded), not an algorithm change — see Risk RK1.

## 5 Dependencies and Integrations

- **Consumes:** nothing from other Phase-2 domains at code level. Shares the index-vector resampling primitive with the memory-budget spec (that spec owns the shared-borrow / per-rep-clone-removal lever at `builder.rs:828`).
- **Produces for:** engine-parallel-surface spec (RunMetadata flows through `engine/src/types.rs` → JSON); verification-benchmark spec (the byte-identical JSON is the parity gate SC-03); memory-budget spec (owned `take` enables read-only frame sharing).
- **Crate pin:** `rand_chacha =0.3.1` must be present before any threading work builds on this (Charter In-Scope 10 ordering: determinism lands and is verified first).
- **INV-08 boundary:** all engine RNG math stays f64; no monetary value passes through this layer, so Decimal(18,2) rounding is out of this spec's surface.

## 6 Risk Assessment

| ID | Risk | Severity | Mitigation |
|---|---|---|---|
| RK1 | Quantile point estimate changes value (nondeterministic → deterministic); no stable pre-change baseline exists to byte-compare against | Medium | Document as intended; the new deterministic value becomes the baseline. Parity tests compare across MODES at a fixed seed, not against pre-change output. Mean path is separately guaranteed byte-unchanged. |
| RK2 | `ChaCha8Rng::set_stream` semantics or `gen_range` distribution differ across a future `rand_chacha` bump | Medium | Exact-pin `rand_chacha =0.3.1`; INV-04 double-build sha256 catches drift; RunMetadata records the version. |
| RK3 | `DataFrame::take` on Polars 0.44 with out-of-order/duplicate indices behaves differently than assumed (with-replacement) | Medium | Verify `take` accepts repeated indices and preserves them (it does — gather semantics); parity test covers it. > NEEDS RESEARCH below. |
| RK4 | `gen_range(0..n)` modulo-style bias vs Polars' prior sampler changes SEs enough to fail a golden test | Low | Golden tests (statistical-trust-layer spec) re-baseline against the new seeded pipeline; this is a resampling-implementation change by design, not a correctness regression. |
| RK5 | Two-level seeding (rep_master derivation) accidentally correlates rep streams | Low | splitmix64 step decorrelates; R2 referential-transparency + disjoint-purpose unit test guards it. |

## Gaps Requiring Deeper Research

> NEEDS RESEARCH: Does Polars 0.44 `DataFrame::take(&IdxCa)` accept an `IdxCa` containing duplicate and unsorted `IdxSize` values and gather them with-replacement in index order (rows repeated as specified), returning a frame of exactly `idx.len()` rows? Confirm the exact method name/signature (`take` vs `take_unchecked` vs `_take`) and null handling on the 0.44 API. Single-agent: read the vendored polars 0.44 `DataFrame` gather API in Cargo registry source.

> NEEDS RESEARCH: Confirm `rand_chacha` 0.3.1 pairs with `rand` 0.8.5 and that `ChaCha8Rng` implements `set_stream(u64)` and `SeedableRng::seed_from_u64` on that version (API moved across 0.3/0.9). Single-agent: read docs.rs/rand_chacha/0.3.1 + local Cargo.lock.

> NEEDS RESEARCH: `seed_from_entropy()` needs an OS entropy source that works on `wasm32-unknown-unknown` (getrandom js feature). Confirm whether `getrandom` with the `js` feature is already enabled transitively in the wasm build, or must be added; if unavailable, `seed_from_entropy()` is native/CLI-only and wasm callers must pass an explicit seed. Single-agent: grep Cargo.lock for getrandom features + engine wasm build.

## 8 Spark Notes

- Three unseeded RNG sites → one master-seed → `ChaCha8Rng` `set_stream` scheme; `rand_chacha =0.3.1` pinned.
- `sample_n_literal(...,None)` → owned `Vec<IdxSize>` + `DataFrame::take` (also the memory-sharing lever).
- Silent `filter_map(...).ok()` discard → deterministic `map`→`RepOutcome`→sequential partition + recorded `discarded` count.
- `thread_rng()` at quantile τ grid + MM resample → seeded purpose-tagged streams; per-τ `par_iter` already order-preserving.
- `bootstrap_stats` stays sequential (no rayon reduce) — INV-02 guard.
- RunMetadata{seed, algorithm, version, requested/succeeded/discarded} serialized into JSON.
- Master acceptance: fixed seed → byte-identical JSON across native / wasm-seq / wasm-threaded 1·2·4 (SC-03).
- Mean point estimates byte-unchanged (parity_test.rs); quantile point estimate transitions nondeterministic→deterministic (intended, RK1).


## Phase 1 Sources

- phase1-deterministic-rng-parallel-seeding.md
- phase1-engine-parallel-surface-clarabel-nalgebra.md
