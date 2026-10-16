# Phase 2 Spec — Memory Budget (50k rows)

> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (memory-budget) — Draft

Domain scope: Charter In-Scope 5 (memory budget for 50k rows), 11 (pre-threading memory
profile), 13 (Employers_data.csv ×5 fixture). Binds: INV-03 (graceful degradation, never
blank screen), INV-05 (peak fits declared maximum with measured headroom; thread count
capped by memory). Other domains referenced via cross-domain summary
(`phase2-cross-domain-summary.md`), never re-specced.

---

## 1 Executive Summary

Memory — not compute — is the first-class constraint for this build (Founder Intake:
"50k, memory is of utmost importance", `specify-plan.md` § Founder Intake). The threaded WASM
build must declare a **fixed** shared-linear-memory `maximum`; the engine allocates the
backing SharedArrayBuffer at that maximum up front and never moves it
(`phase1-memory-budget-shared-memory-limits.md:18`). Threading that ignores the memory ceiling
"optimizes speed into an OOM" (Founder pre-gate review item 3, Charter In-Scope 11).

Therefore the domain is **ordering-locked**: measure the current single-threaded memory curve
first, then derive the shared-memory maximum, per-thread stack, and thread cap as **formulas
over the measured quantities** — not as hardcoded guesses. The research band (256–512 MiB
maximum, 4–8 workers, ~1 MiB stack) is carried only as a **placeholder** until the profile
fixes the numbers (`phase1-memory-budget-shared-memory-limits.md:13,22`).

The dataset itself is not the problem: a 50k×10 mixed frame is ~4 MB by Arrow arithmetic
(`phase1-memory-budget-polars-allocator.md:19`). The memory-dominant term is **per-rep
DataFrame cloning** in the bootstrap loop (`builder.rs:828-829` clones `df_a`/`df_b` per
in-flight rep). Index-vector resampling (share the base frame read-only, materialize only a
row-index vector) is simultaneously the memory lever and — per the determinism domain — the
bit-identical-seeding lever.

Deliverables: (1) an empirical memory-profile harness (native + wasm, 10k/25k/50k rows);
(2) a formula-driven sizing design for `maximum`/`initial`/`-zstack-size`/`--max-memory`;
(3) the INV-05 thread-cap formula and its enforcement point; (4) the index-resampling memory
analysis; (5) the OOM behavior contract; (6) the ×5 fixture perturbation strategy.

---

## 2 Requirements (with acceptance criteria)

### R1 — Pre-threading memory-profile harness (Charter In-Scope 11; feeds SC-05)

Build an empirical harness that measures the **current single-threaded** memory behavior of
`decompose` before any parallel design is finalized. No memory-profiling artifact exists in
the repo today (`specify-plan.md` Pre-Gate Findings).

The harness measures, at each row count n ∈ {10k, 25k, 50k}, on **both** native and wasm:

| Symbol | Quantity | How measured |
|---|---|---|
| `H_res(n)` | steady-state resident heap: base frame + dummy hstack + point-estimate matrices + allocator/framework floor | allocator high-water at checkpoint B (below), cross-checked with `df.estimated_size()` |
| `H_peak(n)` | peak heap across a full 100-rep single-threaded `decompose` | allocator high-water sampled across the whole run |
| `Sc(n)` | marginal peak added by **one** in-flight bootstrap rep | allocator delta around a single isolated `run_single_pass` on a resampled frame |
| `St_obs(n)` | deepest observed call-stack depth (informs `-zstack-size`) | native: stack-probe / wasm: `__stack_pointer` low-water |

Checkpoints instrumented inside the decompose path:
- **A** — after frame load + categorical dummy `hstack` (`builder.rs:813`).
- **B** — immediately before the bootstrap `into_par_iter()` loop (`builder.rs:825`) → `H_res`.
- **C** — running maximum across all reps → `H_peak`.
- **D** — allocator delta bracketing one `run_single_pass` call in isolation → `Sc`.

Acceptance criteria:
- AC-R1.1: A committed memory-profile report table exists with all four symbols × {native,
  wasm} × {10k, 25k, 50k}, each cell a measured byte value (not an estimate) or explicitly
  marked `estimated_size-only` where allocator instrumentation is unavailable.
- AC-R1.2: The harness is reproducible: same fixture + same seed → same reported bytes within
  a stated tolerance (allocator fragmentation noise band, ≤ ±5%).
- AC-R1.3: The report is produced and reviewed **before** the thread-cap constant (R3) is
  fixed. The profile artifact's existence is a precondition of R3's acceptance — the
  profile-before-parallelize ordering is inviolable (Charter In-Scope 11; Founder directive).

### R2 — Shared-memory sizing design as formulas over profile outputs (In-Scope 5; SC-05)

The build pins `maximum`, `initial`, `-zstack-size`, and `--max-memory` as constants **derived
from R1's measured quantities**, not hardcoded. Only the safety margins are hardcoded; the band
defaults are placeholders pending the profile.

Acceptance criteria:
- AC-R2.1: `maximum`, `initial`, `stack`, `--max-memory` each expressed as a formula over
  `{H_res(50k), Sc(50k), St_obs(50k)}` (formulas in § 3.2), with every hardcoded constant
  (margins) carrying a cited source band.
- AC-R2.2: The chosen `maximum` sits inside the portable envelope (≤ 2 GiB) with the band
  default 256–512 MiB used only until the profile substitutes measured values
  (`phase1-memory-budget-shared-memory-limits.md:13,19`).
- AC-R2.3: A consistency assertion is stated: `M_max ≥ H_res + N·(Sc + St) + Marg` for the
  actually-chosen `N` (§ 3.2).

### R3 — Thread-cap formula and enforcement (INV-05; SC-05)

Thread count is capped by the memory budget, not just `hardwareConcurrency` (INV-05).

Formula: `N_max = floor((M_max − H_res − Marg) / (Sc + St))`, then
`N = min(N_max, hardwareConcurrency, 8)`
(`phase1-memory-budget-shared-memory-limits.md:27`).

Acceptance criteria:
- AC-R3.1: `N_max` is computed as a build-time constant from R1's `H_res(50k)`/`Sc(50k)` and
  R2's `M_max`/`St`, and recorded in the spec + engine repo (as a documented constant, not a
  magic number).
- AC-R3.2: Enforcement point named: the worker computes `N = min(N_max_const,
  hardwareConcurrency, 8)` and passes `N` to `initThreadPool` in
  `analysis.worker.js` (wiring owned by meridian-integration; the **cap value and its ceiling**
  originate here). This domain owns `N_max_const`; meridian-integration owns the call.
- AC-R3.3: A memory-ceiling test (owned by verification-benchmark) asserts measured peak at
  50k for N ∈ {4, 8} stays under `M_max · (1 − headroom_frac)` — this is the runtime proof of
  INV-05 (§ 3.2 headroom placeholder).

### R4 — Index-vector-resampling memory reduction (In-Scope 5; determinism co-lever)

Replace the per-rep clone-then-sample pattern (`builder.rs:828-838`) with shared-base-frame
index resampling: keep `df_a_global`/`df_b_global` read-only (shared reference), materialize
only a seeded row-index vector per rep, and `take(idx)`.

Acceptance criteria:
- AC-R4.1: The current per-rep allocation is characterized (§ 3.3): `df_a.clone()` +
  `df_b.clone()` (`builder.rs:828-829`) are **redundant** — `sample_n_literal` takes `&self`
  and does not mutate the source, so the clone-pair is pure waste eliminable independent of the
  index refactor.
- AC-R4.2: Expected marginal reduction stated as a formula over frame size `F(50k)` with the
  measured multiplier confirmed by R1 (`Sc` before vs after), not asserted as a fixed %.
- AC-R4.3: The refactor's memory claim is verified by re-running R1's harness post-refactor and
  showing `Sc_after < Sc_before` at 50k (the profile is the proof, not the design note).

### R5 — OOM behavior contract (INV-03; build-safety.md)

A non-cross-origin-isolated context degrades to working sequential execution; an actual
out-of-memory condition surfaces a graceful error to the UI — never a blank screen, never a
silent hang (`phase1-memory-budget-shared-memory-limits.md:21,30`; Charter INV-03).

Acceptance criteria:
- AC-R5.1: `memory.grow` failure path documented: dlmalloc returns null → Rust alloc error →
  abort → `WebAssembly.RuntimeError` catchable at the JS worker boundary.
- AC-R5.2: The worker catches `RuntimeError`, posts a structured
  `{type:'error', payload:{code, message}}` to the UI (protocol owned by meridian-integration;
  contract required here), and treats the run as fatal — the wasm module is reinstantiated
  (fresh `Memory`) before the next compute, because post-abort linear-memory state is
  inconsistent.
- AC-R5.3: Hard browser-tab OOM is acknowledged as unrecoverable; the mitigation is the
  profile-derived `M_max` + thread cap keeping peak under ceiling with headroom (R2/R3), NOT a
  runtime free-memory query (none exists — `phase1-memory-budget-shared-memory-limits.md:21`).

### R6 — ×5 fixture perturbation strategy (In-Scope 13)

`Employers_data.csv` (10,000 data rows; header row 1) is replicated ×5 to 50k with a defined
perturbation so the memory/benchmark fixture is realistic and reproducible (§ 4.2).

Acceptance criteria:
- AC-R6.1: The generator is deterministic (fixed master seed) → the 50k fixture is
  byte-reproducible.
- AC-R6.2: The group variable (`Gender`) and categorical predictors are **not** perturbed
  (perturbing the group corrupts the decomposition target; § 4.2).
- AC-R6.3: The generated fixture is gitignored + regenerable (not committed — it is a derived
  artifact; `.claude/rules/repo-management.md` output-pruning posture).

---

## 3 Technical Architecture

### 3.1 Profile harness architecture

Native instrumentation: a lightweight tracking global allocator behind a `mem-profile`
feature that wraps the platform allocator and records `current` + `peak` byte counters
(pattern: `stats_alloc`-style counting `GlobalAlloc`, or the `dhat` heap feature). Logical
frame sizes cross-checked with Polars `df.estimated_size()`
(`phase1-memory-budget-polars-allocator.md:19`).

WASM instrumentation: OS RSS is unavailable; use `core::arch::wasm32::memory_size(0)` (current
linear-memory pages × 64 KiB) sampled at each checkpoint. Because wasm linear memory only ever
grows, `memory_size` is itself the high-water mark of the SharedArrayBuffer footprint —
sample it after checkpoints A/B and continuously across the rep loop for C. Report
pages → bytes. Run inside a `wasm-bindgen-test` harness (browser-target COI support is a
verification-benchmark Phase-3 unknown — do not block the harness design on it).

> NEEDS RESEARCH: does dlmalloc under threaded wasm expose per-arena in-use byte stats
> (analogous to `mallinfo`) so `Sc` can be measured directly, or must `Sc` be inferred purely
> from `memory_size` deltas which conflate allocator retention with live bytes?

Checkpoint D (marginal `Sc`) is measured by bracketing a single isolated `run_single_pass` on
a pre-resampled frame with allocator `current`-counter reads (native) or by running
`decompose` at `reps=1` vs `reps=2` and differencing peak (wasm fallback where D-bracketing is
not possible under `memory_size`-only instrumentation).

### 3.2 Sizing formulas (measured quantities symbolic; margins hardcoded with cited band)

All quantities in bytes, measured at 50k rows via R1. Page size = 65 536.

Measured (from R1):
- `H_res` = `H_res(50k)` — steady-state resident heap.
- `Sc` = `Sc(50k)` — marginal peak per concurrent in-flight bootstrap rep (post-R4).
- `St_obs` = observed deepest stack at 50k.

Hardcoded safety constants (band-sourced placeholders, replaced/confirmed post-profile):
- `St` (per-thread stack) — band **0.5–1 MiB**, placeholder `St = 1_048_576`
  (`phase1-memory-budget-shared-memory-limits.md:20`); must be ≥ `St_obs` with margin.
- `Marg` (maximum-sizing margin) — placeholder **64 MiB** = `67_108_864`
  (`phase1-memory-budget-shared-memory-limits.md:22`).
- `Marg_init` (initial-sizing margin) — placeholder **16 MiB** = `16_777_216`.
- `headroom_frac` (INV-05 ceiling test) — placeholder **0.15–0.20**
  (measured-headroom requirement, INV-05).
- `N_target` — desired upper worker count, band **4–8**
  (`phase1-memory-budget-shared-memory-limits.md:13`).

Formulas:

```
round_up_page(x) = ceil(x / 65536) * 65536

# Maximum shared-memory (declared once, allocated up front, never moves)
M_max = round_up_page( H_res + N_target * (Sc + St) + Marg )
        # constrained to the portable envelope: M_max <= 2 GiB,
        # band default 256-512 MiB until the profile substitutes H_res/Sc.
        # If M_max exceeds the chosen band ceiling, reduce N_target and recompute.

# Initial memory (small; grow handles the rest)
M_init = round_up_page( H_res + Sc + Marg_init )
         # band default 16-64 MiB (shared-memory-limits:22)

# Thread cap (INV-05)
N_max = floor( (M_max - H_res - Marg) / (Sc + St) )
N     = min( N_max, hardwareConcurrency, 8 )

# Consistency assertion (must hold for the chosen N)
assert  M_max >= H_res + N * (Sc + St) + Marg

# Runtime ceiling proof (verification-benchmark owns the test)
assert  measured_peak(50k, N) <= M_max * (1 - headroom_frac)
```

Link-arg mapping (values pinned post-profile; placement in `scripts/build-wasm.sh` owned by
toolchain-build — cross-reference, do not re-spec):
- `RUSTFLAGS += -C link-arg=-zstack-size=<St>` (`phase1-memory-budget-shared-memory-limits.md:20`)
- `--max-memory=<M_max>` (wasm-ld / wasm-bindgen link arg)
- `--initial-memory=<M_init>` if an explicit initial is set; otherwise LLVM derives initial
  from the data section — cross-reference toolchain-build for which mechanism the chosen
  build path uses.

> NEEDS RESEARCH: under wasm-bindgen `--target web` + wasm-bindgen-rayon 1.3.0, is
> `--max-memory` passed through wasm-ld link-args or must it be set on the
> `WebAssembly.Memory` descriptor emitted by the rayon shim's JS glue? (Determines whether the
> maximum is a build-time link constant or a JS-side value the loader must inject — affects
> where the profile-derived constant lives.)

### 3.3 Index-resampling memory model

Let `F(50k)` = resident bytes of one full resampled frame (`df_a`+`df_b` combined ≈ base
dataset, ~4 MB per `phase1-memory-budget-polars-allocator.md:19`).

Current per-rep peak (`builder.rs:828-838`), one in-flight rep:
```
Sc_before ~= clone(df_a) + clone(df_b)   # ~F   (REDUNDANT - sample_n_literal takes &self)
          +  sample_a + sample_b          # ~F
          +  vstack(sample_df)            # ~F
          +  point matrices (X, y, betas) # small vs F at 50k
          ~= ~3F + matrices
```

Post-refactor (shared read-only base + seeded index vector + `take`):
```
idx_vec = n_rows * sizeof(IdxSize)         # 50k * 4B ~= 200 KB  << F
Sc_after ~= take(idx) materialized frame   # ~F (unavoidable - regression input)
         +  point matrices
         ~= ~F + idx_vec + matrices  (vstack foldable into a single take over concatenated base)
```

Expected marginal reduction ≈ from `~3F` to `~F` (drop the redundant clone-pair + fold the
vstack), i.e. roughly a **2F reduction per in-flight rep** — but the exact multiplier is an R1
measurement (`Sc_before` vs `Sc_after`), not a hardcoded figure (AC-R4.2/AC-R4.3).

> NEEDS RESEARCH: does Polars `DataFrame::take(&IdxCa)` at 50k allocate a single contiguous
> materialization (~1F) or does it retain chunked references to the base frame (sub-F)? The
> answer sets whether `Sc_after` is `~F` or materially below F.

The determinism domain requires owned per-rep index resampling anyway (replacing
`sample_n_literal(..., None)` at `builder.rs:832` and `thread_rng()` at
`quantile_decomposition.rs:215,244`) — memory and bit-identical seeding share the same
refactor. This domain claims only the memory consequence; the seeding proof is
deterministic-rng's (cross-reference summary § RNG/determinism).

### 3.4 OOM contract flow

```
memory.grow fails (SAB at max, no more pages)
  -> dlmalloc returns null
  -> Rust global alloc error handler -> abort
  -> WebAssembly.RuntimeError thrown across the wasm/JS boundary
  -> analysis.worker.js compute() try/catch catches RuntimeError
  -> post { type:'error', payload:{ code:'OOM', message } } to UI  (INV-03: no blank screen)
  -> worker reinstantiates the wasm module (fresh Memory) before next compute
     (post-abort linear memory is inconsistent -> fatal for the run, recoverable for the worker)
```

Hard browser-tab OOM (below the declared max, browser kills the tab) is unrecoverable; the
only defense is R2/R3 keeping peak under ceiling — there is no runtime free-memory query to
pre-empt it (`phase1-memory-budget-shared-memory-limits.md:21`).

---

## 4 Implementation Details

### 4.1 Files touched (memory-budget slice)

| Path | Change | Owner note |
|---|---|---|
| `oaxaca_blinder/src/builder.rs:825-848` | remove redundant `df_a`/`df_b` clone; shared-base + seeded index resampling | shared with deterministic-rng (seeding) |
| `oaxaca_blinder/Cargo.toml` | add `mem-profile` feature (tracking allocator, dev/test only) | native-only; INV-01 keeps it off default |
| `oaxaca_blinder/src/` (new) `mem_profile.rs` | tracking `GlobalAlloc` + checkpoint API | behind `mem-profile` |
| `scripts/build-wasm.sh` | `-zstack-size` / `--max-memory` / `--initial-memory` constants | **cross-ref toolchain-build** — values from § 3.2 |
| `engine/src/analysis.rs` | expose `reps=1/2` profiling entry or a `decompose`-profile shim | feeds checkpoint D |
| test fixture generator (new, gitignored output) | ×5 perturbation (§ 4.2) | In-Scope 13 |

The `--max-memory`/`-zstack-size` **values** are this domain's output; their **placement** in
the build script is toolchain-build's (cross-domain summary § Toolchain). Do not re-spec the
build path here.

### 4.2 ×5 fixture perturbation (implementable spec)

Input: `/home/deji/Downloads/Employers_data.csv`, 10 000 data rows, columns
`Employee_ID, Name, Age, Gender, Department, Job_Title, Experience_Years, Education_Level,
Location, Salary` (header verified on disk).

Produce 50 000 rows as 5 replicates `k ∈ {0,1,2,3,4}`. Replicate `k=0` is the **untouched**
original (guarantees the 10k subset used for the 10k profile row-count is exact real data).

Per replicate `k`, per row `r`:

| Column | Rule |
|---|---|
| `Employee_ID` | `id + k*10000` (keeps global uniqueness; k=0 unchanged) |
| `Name` | k=0 unchanged; k>0 append `"_r{k}"` (grows the utf8 dictionary realistically — memory-relevant; not an engine predictor) |
| `Age` | k=0 unchanged; k>0 add integer jitter `j ~ Uniform{-2..+2}`, clamp to `[18, 70]` |
| `Experience_Years` | k=0 unchanged; k>0 add `j ~ Uniform{-2..+2}`, clamp to `[0, Age-16]` |
| `Salary` | k=0 unchanged; k>0 multiply by `(1 + eps)`, `eps ~ Uniform(-0.03, +0.03)`, round to cents (Decimal(18,2) at the data boundary per INV-08) |
| `Gender` (group var) | **unchanged** — perturbing the group corrupts the decomposition target (AC-R6.2) |
| `Department`, `Job_Title`, `Education_Level`, `Location` | **unchanged** — preserve category structure + dictionary size |

Seeding: fixed master seed (document the literal, e.g. `0x0014_MER1_50K_SEED`); derive a
per-`(k, r)` stream via the same `rand_chacha`/`ChaCha8Rng` family the determinism domain
selects (cross-reference summary § RNG/determinism) so the fixture is byte-reproducible and
schedule-independent. Do not use `thread_rng`/`StdRng` (non-portable — same reason the engine
refactor drops them).

Why perturb rather than raw-duplicate: raw ×5 duplication would (a) make bootstrap resamples
draw from only 10k distinct rows, inflating ties and understating variance; (b) understate
realistic Arrow buffer/validity-bitmap/utf8-dictionary sizes at 50k. Continuous jitter +
unique IDs/names yield 50k semi-distinct rows — a realistic memory profile
(`phase1-memory-budget-polars-allocator.md:19` sizing arithmetic assumes distinct-ish values).

Row-count subsets for the profile: 10k = replicate k=0 alone (exact real data); 25k = k∈{0,1}
+ first 5000 rows of k=2; 50k = all five replicates.

### 4.3 Profile-before-parallelize gate (inviolable)

The build order (Founder-directed): determinism fix → **memory profile of current build** →
threading → validation (cross-domain summary § Build Order). Concretely: R1's profile artifact
must exist and be reviewed before R3's `N_max_const` is fixed and before any `initThreadPool`
wiring is finalized. AC-R1.3 encodes this as a hard precondition — a threading design that
lands before the profile is a spec violation, not a sequencing preference.

---

## 5 Dependencies and Integrations

- **deterministic-rng** — shares the `builder.rs` resampling refactor; owns the seeding proof.
  This domain owns only the memory consequence of index resampling. The fixture generator
  reuses its `ChaCha8Rng` choice (summary § RNG/determinism).
- **toolchain-build** — owns placement of `-zstack-size` / `--max-memory` / `--initial-memory`
  in `scripts/build-wasm.sh` and the `--target web` migration. This domain supplies the
  **values**; toolchain supplies the **wiring** (summary § Toolchain; Strategy decision).
- **engine-parallel-surface** — the number of concurrently in-flight reps `N` and whether
  POLARS_MAX_THREADS is pinned to 1 (one parallel layer) directly set the `N·(Sc+St)` term.
  Summary § Engine surface + `phase1-memory-budget-polars-allocator.md:21,27`: POLARS_MAX_THREADS=1
  is assumed here so `Sc` is not multiplied by nested Polars parallelism. If that domain lets
  Polars share the pool, `Sc` and the thread-cap formula must be re-derived.
- **meridian-integration** — owns `analysis.worker.js` `initThreadPool(N)` wiring and the
  `{type,payload}` error-post protocol. This domain owns `N_max_const` and the OOM contract
  requirements the worker must satisfy.
- **verification-benchmark** — owns the 50k memory-ceiling CI test that proves INV-05
  (AC-R3.3) and consumes the ×5 fixture (AC-R6.1).
- **Allocator**: dlmalloc unchanged (default on wasm32; wee_alloc archived/leaky; alternatives
  undocumented under threaded wasm — `phase1-memory-budget-polars-allocator.md:20`). Thread
  safety of dlmalloc under wasm threads to be confirmed by engine-parallel-surface Phase 3
  (summary; `polars-allocator.md:29`).

---

## 6 Risk Assessment

| Risk | Severity | Mitigation |
|---|---|---|
| Profile numbers land above the 256–512 MiB band; `N_target=8` infeasible | High | Formulas reduce `N_target` automatically (§ 3.2); band is placeholder, profile governs |
| `memory_size`-only wasm instrumentation conflates allocator retention with live bytes → `Sc` overstated → thread cap too conservative | Medium | Cross-check with `estimated_size()`; NEEDS-RESEARCH item on dlmalloc arena stats (§ 3.1) |
| `take(idx)` materializes ~1F anyway → index refactor saves less than the clone-drop implies | Medium | Claim is formula + R1-verified (AC-R4.3), not asserted; clone-drop win (2F→ ) is independent and certain |
| Nested Polars rayon oversubscription inflates `Sc` unpredictably | Medium | POLARS_MAX_THREADS=1 assumption (§ 5); re-derive if engine-parallel-surface overrides |
| Post-abort wasm memory inconsistency causes stale results if worker not reinstantiated | High | AC-R5.2 mandates module reinstantiation on OOM |
| Hard tab-OOM below declared max (browser kill) | Medium | headroom_frac in ceiling test (AC-R3.3); no runtime pre-emption possible (§ 3.4) |
| `--max-memory` must be set on the JS Memory descriptor not the link arg → constant lives in the wrong place | Medium | NEEDS-RESEARCH item (§ 3.2); resolve with toolchain-build before build |
| Perturbed fixture drifts group means enough to change the decomposition sign vs real 10k | Low | Group var + categoricals unchanged; only continuous jitter ±3% Salary, ±2 Age/Exp |

---

## Gaps Requiring Deeper Research

> NEEDS RESEARCH: does dlmalloc under threaded wasm32 expose per-arena in-use byte statistics
> (mallinfo-analogue) so `Sc` can be measured as live bytes rather than inferred from
> monotonic `memory_size` deltas?

> NEEDS RESEARCH: under wasm-bindgen `--target web` + wasm-bindgen-rayon 1.3.0, is the shared
> `maximum` set via a wasm-ld `--max-memory` link arg (build-time constant) or via the
> `WebAssembly.Memory({maximum})` descriptor in the rayon shim's JS glue (loader-injected
> value)? Determines where the profile-derived `M_max` constant is pinned.

> NEEDS RESEARCH: does `DataFrame::take(&IdxCa)` at 50k rows produce a single contiguous ~1F
> materialization or retain chunked references to the shared base frame (sub-F), and does
> Polars 0.44 (repo-pinned) `take` behave identically native vs wasm32?

> NEEDS RESEARCH (human decision): confirm the safety-margin values (`Marg=64 MiB`,
> `Marg_init=16 MiB`, `headroom_frac=0.15–0.20`, `St=1 MiB`) once the profile lands — these are
> the only hardcoded constants and the founder's "memory of utmost importance" ruling may want
> a larger headroom than the research band's default.

---

## 8 Spark Notes

- Memory is first-class; **profile before parallelize** is inviolable (Charter In-Scope 11,
  AC-R1.3). A thread design landing before the profile artifact is a spec violation.
- The dataset is ~4 MB — not the problem. The problem is **per-rep clones** (`builder.rs:828-829`);
  those clones are provably **redundant** (`sample_n_literal` takes `&self`).
- Sizing is **formulas over measured `{H_res, Sc, St_obs}`**; only margins are hardcoded, each
  band-cited. 256–512 MiB / 4–8 workers / 1 MiB stack are **placeholders**, not decisions.
- Thread cap (INV-05): `N = min(floor((M_max − H_res − Marg)/(Sc + St)), hardwareConcurrency, 8)`.
  `N_max_const` originates here; `initThreadPool(N)` call is meridian-integration's.
- OOM: catch `RuntimeError` at the worker boundary → structured error to UI → reinstantiate
  module. Never a blank screen (INV-03). No runtime free-memory query exists.
- ×5 fixture: perturb continuous cols + unique IDs/names (ChaCha8Rng-seeded, k=0 untouched);
  **never** perturb the group var or categoricals. Generated fixture is gitignored/regenerable.
- Four research gaps, all narrow single-agent questions; one is a founder decision on margins.
