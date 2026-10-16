# Phase 4 Final Spec — Memory Budget (50k rows)

> Issue: 0014-MERIDIAN · Bureau: hr-apps (Meridian) · Date: 2026-07-17
> Domain slug: `memory-budget` · Author: Phase 4 refined-spec writer
> Charter In-Scope: 5 (memory budget for 50k rows), 11 (pre-threading memory profile),
> 13 (Employers_data.csv ×5 fixture). Binds INV-03, INV-05, INV-08.
> Build handoff: `/build` reads this as the source of truth for the memory domain.

---

## Summary

Memory — not compute — is the first-class constraint (ASM-04: ~50k rows; Founder Intake
round 2: "50k, memory is of utmost importance"). Threading that ignores the memory ceiling
"optimizes speed into an OOM" (Charter In-Scope 11). This domain is therefore
**ordering-locked**: measure the current single-threaded memory curve first, then derive the
shared-memory maximum, per-thread stack, and thread cap as **formulas over the measured
quantities** — never as hardcoded guesses.

Two facts frame the whole design:

1. **The dataset is not the problem.** A 50k×10 mixed frame is ~4 MB by Arrow arithmetic
   (`phase1-memory-budget-polars-allocator.md:19`). Sizing 256–512 MiB of shared memory is not
   about holding the data; it is about holding `N` concurrent in-flight bootstrap reps.
2. **The memory-dominant term is redundant per-rep cloning.** `builder.rs:828-829` clones
   `df_a` and `df_b` on every bootstrap rep — verified: `sample_n_literal` is invoked on the
   clone at `builder.rs:831,834` and takes `&self` (it does not mutate the source), so the
   clone-pair is **pure waste** eliminable independent of the index refactor. Index-vector
   resampling (share the base frame read-only, `take(&IdxCa)` a seeded row-index vector) is
   simultaneously the memory lever and — per the determinism domain — the bit-identical-seeding
   lever.

**Shared-memory model** (locked): the threaded WASM build declares a **fixed** shared-linear-
memory `maximum`; the backing SharedArrayBuffer is allocated at that maximum up front, never
moves, and there is no detach-on-grow (`phase1-memory-budget-shared-memory-limits.md:18`).
Envelope: 256–512 MiB maximum, small initial. `dlmalloc` stays (default wasm32 allocator;
`wee_alloc` archived/leaky). Per-thread stack via `-C link-arg=-zstack-size` (~1 MiB band).

**Correction folded in (L4 — CRITICAL for this domain):** polars 0.44 **never reads
`POLARS_MAX_THREADS` on wasm** — the wasm `POOL` is a stub that delegates `join`/`scope`/`spawn`
to the global rayon registry (`phase3-local:44-49`). Every `POLARS_MAX_THREADS=1` instruction
from the Phase-2 draft is **deleted**. It is a no-op on wasm and must not be presented as a
memory safeguard. The one-parallel-layer property is reframed below (Design D6): the polars
wasm stub routes onto **our** global rayon pool — cooperative work-stealing, one pool, no
oversubscription; the only residual audit is polars-internal parallel float reductions on the
hot path, which is narrow because our stats run in nalgebra after `take`.

Deliverables: (1) an empirical memory-profile harness (native + wasm, 10k/25k/50k);
(2) formula-driven sizing for `maximum`/`initial`/`-zstack-size`/`--max-memory`;
(3) the INV-05 thread-cap formula and its `N_max_const` output; (4) the index-resampling
memory analysis; (5) the OOM behavior contract (budget-prevention + structured self-report,
per correction W4); (6) the ×5 fixture perturbation strategy.

---

## In-Scope (this domain)

- **In-Scope 5** — memory budget for 50k rows: measured profile, shared-linear-memory maximum,
  per-thread stack sizing, thread-count cap derivation.
- **In-Scope 11** — pre-threading memory profile: the CURRENT single-threaded curve at
  10k/25k/50k, produced and reviewed **before** the thread-cap constant is fixed.
- **In-Scope 13** — Employers_data.csv ×5 fixture (with the perturbation strategy defined here).

This domain **owns**: the empirical memory profile of the current single-threaded engine at
50k rows; the shared-memory model; the thread-cap formula and its `N_max_const` output; the
index-vector-resampling memory analysis; the OOM behavior contract's memory requirements; the
×5 fixture generator.

This domain **does not own** (cross-domain, referenced not re-specced): the `initThreadPool(N)`
call and the `{type,payload}` worker error protocol (meridian-integration); the placement of
`-zstack-size`/`--max-memory` in `scripts/build-wasm.sh` (toolchain-build); the seeding proof
of index resampling (deterministic-rng); the CI 50k memory-ceiling test execution
(verification-benchmark). This domain supplies the **values and the memory requirements** those
domains consume.

---

## Design & Decisions

### D1 — Profile harness (In-Scope 11; feeds SC-05)

No memory-profiling artifact exists in the repo today (`specify-plan.md` Pre-Gate Findings).
Build one. It measures, at each n ∈ {10k, 25k, 50k}, on **both** native and wasm, four
quantities, at checkpoints instrumented inside the `decompose` path:

| Symbol | Quantity | Measured at |
|---|---|---|
| `H_res(n)` | steady-state resident heap: base frame + dummy `hstack` + point-estimate matrices + allocator/framework floor | checkpoint **B** — immediately before the bootstrap `into_par_iter()` loop (`builder.rs:825`) |
| `H_peak(n)` | peak heap across a full 100-rep single-threaded `decompose` | checkpoint **C** — running maximum across all reps |
| `Sc(n)` | marginal peak added by **one** in-flight bootstrap rep (post-R4 refactor) | checkpoint **D** — allocator delta bracketing one isolated `run_single_pass` on a resampled frame |
| `St_obs(n)` | deepest observed call-stack depth (informs `-zstack-size`) | native: stack-probe; wasm: `__stack_pointer` low-water |

Checkpoint **A** — after frame load + categorical dummy `hstack` (`builder.rs:813`) — is also
recorded as a sub-component of `H_res`.

**Native instrumentation:** a tracking `GlobalAlloc` behind a `mem-profile` feature (dev/test
only, off by default per INV-01) that wraps the platform allocator and records `current` +
`peak` byte counters (pattern: `stats_alloc`-style counting allocator, or the `dhat` heap
feature). Logical frame sizes cross-checked with Polars `df.estimated_size()`
(`phase1-memory-budget-polars-allocator.md:19`).

**WASM instrumentation:** OS RSS is unavailable; use `core::arch::wasm32::memory_size(0)`
(current linear-memory pages × 64 KiB) sampled at each checkpoint. Because wasm linear memory
only ever grows, `memory_size` **is** the high-water mark of the SharedArrayBuffer footprint —
sample it after A/B and continuously across the rep loop for C. Report pages → bytes. Run
inside a `wasm-bindgen-test` harness. Checkpoint D on wasm (where allocator-delta bracketing is
not possible under `memory_size`-only instrumentation) is measured by differencing peak at
`reps=1` vs `reps=2`.

**Caveat (Risk-tracked):** `memory_size`-only wasm instrumentation conflates allocator
retention with live bytes → `Sc` may be overstated → thread cap conservative. Cross-check with
`estimated_size()`; whether dlmalloc under threaded wasm exposes per-arena in-use byte stats
(mallinfo-analogue) is an Open Item, not a blocker for the harness design.

### D2 — Shared-memory sizing as formulas over profile outputs (In-Scope 5; SC-05)

All quantities in bytes, measured at 50k via D1. Page size = 65 536. **Measured** (`H_res`,
`Sc`, `St_obs`) come from D1. **Hardcoded safety constants** (`St`, `Marg`, `Marg_init`,
`headroom_frac`, `N_target`) are band-sourced placeholders confirmed at the buildability gate
(Open Item O1).

```
round_up_page(x) = ceil(x / 65536) * 65536

# Maximum shared-memory (declared once, allocated up front, never moves, no detach-on-grow)
M_max = round_up_page( H_res + N_target * (Sc + St) + Marg )
        # constrained to the portable envelope: M_max <= 2 GiB;
        # band default 256-512 MiB until the profile substitutes H_res/Sc
        # (phase1-memory-budget-shared-memory-limits.md:13,19).
        # If M_max exceeds the chosen band ceiling, reduce N_target and recompute.

# Initial memory (small; grow handles the rest, up to M_max)
M_init = round_up_page( H_res + Sc + Marg_init )     # band default 16-64 MiB

# Thread cap (INV-05)
N_max = floor( (M_max - H_res - Marg) / (Sc + St) )
N     = min( N_max, hardwareConcurrency, 8 )         # (shared-memory-limits:27)

# Consistency assertion (must hold for the chosen N)
assert  M_max >= H_res + N * (Sc + St) + Marg

# Runtime ceiling proof (verification-benchmark owns the test execution)
assert  measured_peak(50k, N) <= M_max * (1 - headroom_frac)
```

Placeholder constants and their cited bands:

| Constant | Placeholder | Band / source |
|---|---|---|
| `St` (per-thread stack) | `1_048_576` (1 MiB) | 0.5–1 MiB (`shared-memory-limits:20`); must be ≥ `St_obs` with margin |
| `Marg` (maximum-sizing margin) | `67_108_864` (64 MiB) | `shared-memory-limits:22` |
| `Marg_init` (initial-sizing margin) | `16_777_216` (16 MiB) | derived, initial-sizing |
| `headroom_frac` (INV-05 ceiling) | `0.15`–`0.20` | measured-headroom requirement, INV-05 |
| `N_target` (desired worker count) | `4`–`8` | `shared-memory-limits:13` |

**Link-arg mapping** (values pinned post-profile; placement in `scripts/build-wasm.sh` is
toolchain-build's — cross-reference, do not re-spec):
- `RUSTFLAGS += -C link-arg=-zstack-size=<St>` (`shared-memory-limits:20`) — per-thread stack.
- `--max-memory=<M_max>` (wasm-ld / wasm-bindgen link arg) — declared maximum.
- `--initial-memory=<M_init>` if an explicit initial is set; otherwise LLVM derives initial from
  the data section (cross-reference toolchain-build for which mechanism the chosen path uses).

Whether `--max-memory` is a wasm-ld link constant or a value the rayon shim's JS glue must
inject on the `WebAssembly.Memory({maximum})` descriptor is an Open Item (O2) resolved with
toolchain-build — it determines where the profile-derived `M_max` constant is pinned, not
whether the formula is correct.

### D3 — Thread-cap `N_max_const` (INV-05; SC-05)

**`N_max_const` bakes in the `8` ceiling at emission (council MJ-6).** Define
`N_max_const := min( floor((M_max − H_res − Marg) / (Sc + St)), 8 )` — the memory-derived cap
AND the hard 8-thread ceiling folded into the single emitted integer. Then the worker's 2-term
`N = min(hardwareConcurrency, N_max_const)` (`analysis.worker.js`) is correct with no separate
`,8` term to drift. (Previously D3 wrote the UNCLAMPED quotient and relied on the worker to
re-apply `8` — which its `min(hardwareConcurrency, MEMORY_THREAD_CAP)` did not do; folding the
clamp into the value removes the seam.) `N_max_const` is a **build-time constant** from D1's
`H_res(50k)`/`Sc(50k)` and D2's `M_max`/`St`, recorded in the spec + engine repo as a documented
constant. **Ownership (resolves audit CRITICAL-2): this domain owns the VALUE `N_max_const`
(clamp included); meridian-integration's M7 step WRITES `frontend/src/wasm/thread-cap.js`
(`export const MEMORY_THREAD_CAP = <N_max_const>;`, with a comment noting the 8-clamp is already
applied) and owns the `initThreadPool(N)` call.** One value-owner, one file-writer.
`N_max_const` is available as soon as the memory profile (build step 5) lands.

### D4 — Index-vector resampling as the memory lever (In-Scope 5; determinism co-lever)

Verified anchor — `builder.rs:825-848`:

```rust
let bootstrap_results: Vec<SinglePassResult> = (0..self.bootstrap_reps)
    .into_par_iter()
    .filter_map(|_| {
        let df_a = df_a_global.clone();          // :828  REDUNDANT
        let df_b = df_b_global.clone();          // :829  REDUNDANT
        let sample_a = df_a.sample_n_literal(df_a.height(), true, false, None).ok()?;  // :831  &self
        let sample_b = df_b.sample_n_literal(df_b.height(), true, false, None).ok()?;  // :834  &self
        let sample_df = sample_a.vstack(&sample_b).ok()?;                              // :838
        self.run_single_pass(&sample_df, ...).ok()                                    // :840
    })
    .collect();
```

`sample_n_literal` is called on the cloned frame at `:831,:834` and takes `&self` (it returns a
new frame, does not mutate the source) → the `df_a.clone()`/`df_b.clone()` pair at `:828-829` is
**redundant**: `sample_n_literal` could be called directly on `df_a_global`/`df_b_global`. The
clone-drop is a certain win, independent of the index refactor.

**Memory model** (let `F(50k)` = resident bytes of one full resampled frame, `df_a`+`df_b`
combined ≈ base dataset ~4 MB per `polars-allocator.md:19`):

```
# Current per-rep peak, one in-flight rep (builder.rs:828-838):
Sc_before ~= clone(df_a) + clone(df_b)   # ~F   REDUNDANT (sample_n_literal takes &self, :831/:834)
          +  sample_a + sample_b          # ~F
          +  vstack(sample_df)            # ~F
          +  point matrices (X, y, betas) # small vs F at 50k
          ~= ~3F + matrices

# Post-refactor (shared read-only base + seeded index vector + take):
idx_vec  =  n_rows * sizeof(IdxSize)      # 50k * 4B ~= 200 KB  << F   (IdxSize = u32, no bigidx — L1)
Sc_after ~= take(&idx) materialized frame # ~F  (unavoidable — regression input)
          +  point matrices
          ~= ~F + idx_vec + matrices       # vstack foldable into one take over the concatenated base
```

Expected marginal reduction ≈ from `~3F` to `~F` (drop the redundant clone-pair + fold the
vstack) — roughly **2F per in-flight rep**. The exact multiplier is a D1 measurement
(`Sc_before` vs `Sc_after`), **not** a hardcoded figure. `take(&IdxCa)` is confirmed buildable:
bounds-checked gather, accepts duplicate/unsorted indices, returns exactly `idx.len()` rows,
materializes fresh contiguous output while the base frame stays shared/untouched
(`phase3-local:9-22`, polars-core-0.44.2 `src/frame/mod.rs:1841`).

The determinism domain requires owned per-rep index resampling anyway (replacing
`sample_n_literal(..., None)` at `builder.rs:832` and `thread_rng()` at
`quantile_decomposition.rs:215,244`) — memory and bit-identical seeding share the **same**
refactor. This domain claims only the memory consequence; the seeding proof is
deterministic-rng's.

### D5 — OOM behavior contract (INV-03; correction W4)

Reframed away from message-parsing per correction W4: an in-execution OOM **cannot** be reliably
distinguished from other traps by inspecting `WebAssembly.RuntimeError` (message is
engine-defined and non-standard; the engine may hard-crash with no JS exception —
`phase3-web:29-34`). The contract is therefore **budget-prevention first, structured
self-report second**:

1. **Proactive (primary):** the profile-derived `M_max` + `N_max_const` thread cap (D2/D3) keep
   peak under the ceiling with `headroom_frac` — OOM is prevented, not caught. This is the load-
   bearing defense.
2. **Structured self-report (secondary):** the Rust engine detects its own allocation pressure
   at rep-batch boundaries (checked allocation) and returns a structured
   `{ type:'error', payload:{ code:'OOM', message } }` **value** rather than trapping, so the
   worker gets a clean tag without parsing a `RuntimeError` message (protocol owned by
   meridian-integration; this domain requires the self-report exist).
3. **Trap fallback (last resort):** if an allocation still traps (`memory.grow` fails at the SAB
   maximum → dlmalloc returns null → Rust alloc error → abort → `RuntimeError` at the JS
   boundary), the worker catches it, posts the same structured error (INV-03: no blank screen),
   and **reinstantiates the wasm module (fresh `Memory`)** before the next compute — post-abort
   linear-memory state is inconsistent, fatal for the run, recoverable for the worker.

Hard browser-tab OOM (below the declared maximum, the browser kills the tab) is unrecoverable;
the only defense is D2/D3 keeping peak under ceiling — there is **no runtime free-memory query**
to pre-empt it (`shared-memory-limits:21`).

### D6 — One-parallel-layer, reframed (correction L4)

After `initThreadPool`, wasm-bindgen-rayon installs a global rayon registry with `N` workers.
Polars' wasm `POOL` is a stub (`polars-utils-0.44.2 src/wasm.rs`) that routes `join`/`scope`/
`spawn` onto **that same global registry** and runs `install(op)` inline-caller
(`phase3-local:44-49`). Consequence: there is exactly one pool. Our rep-level `into_par_iter`
and any polars-internal `try_apply_columns_par` (e.g. inside `take`, `phase3-local:23`) are
**cooperative work-stealing on one pool — not oversubscription.** `Sc` is therefore **not**
multiplied by nested independent Polars pools, and the thread-cap formula's `N·(Sc+St)` term
holds without a `POLARS_MAX_THREADS` pin.

`POLARS_MAX_THREADS=1` is **deleted** from this domain: it is never read on wasm (the env-read
sits in the native-only arm; `std::env::var` is empty on wasm anyway — `phase3-local:48`) and
must not be presented as a memory safeguard. The **only** residual memory/determinism audit is
any polars-internal **parallel float reduction** (sum/mean kernels) on the hot path — narrow,
because the engine computes stats via nalgebra **after** `take` (`phase3-local:49`). The spec
names this audit; it does not gate the memory design.

Sequential-fallback mode (threads not initialized) degrades safely: rayon's wasm fallback runs
everything on the current thread; the stub's `join`/`scope` run inline (INV-03).

### D7 — ×5 fixture perturbation (In-Scope 13)

Input: `/home/deji/Downloads/Employers_data.csv`, 10 000 data rows (header row 1), columns
`Employee_ID, Name, Age, Gender, Department, Job_Title, Experience_Years, Education_Level,
Location, Salary` (header verified on disk; `Education_Level` = 3-level categorical
Master/Bachelor/PhD → 2 dummies under one-hot, per `phase3-local:60`).

Produce 50 000 rows as 5 replicates `k ∈ {0,1,2,3,4}`. Replicate `k=0` is the **untouched**
original (so the 10k profile row-count is exact real data). Per replicate `k>0`, per row `r`:

| Column | Rule |
|---|---|
| `Employee_ID` | `id + k*10000` (global uniqueness; k=0 unchanged) |
| `Name` | k=0 unchanged; k>0 append `"_r{k}"` (grows the utf8 dictionary realistically — memory-relevant; not an engine predictor) |
| `Age` | k=0 unchanged; k>0 add `j ~ Uniform{-2..+2}`, clamp `[18, 70]` |
| `Experience_Years` | k=0 unchanged; k>0 add `j ~ Uniform{-2..+2}`, clamp `[0, Age-16]` |
| `Salary` | k=0 unchanged; k>0 multiply by `(1 + eps)`, `eps ~ Uniform(-0.03, +0.03)`, round to cents (Decimal(18,2) at the data boundary per INV-08) |
| `Gender` (group var) | **unchanged** — perturbing the group corrupts the decomposition target |
| `Department`, `Job_Title`, `Education_Level`, `Location` | **unchanged** — preserve category structure + dictionary size |

Seeding: fixed master seed (document the literal, e.g. `0x0014_MER1_50K_SEED`); derive a
per-`(k,r)` stream via the same `rand_chacha`/`ChaCha8Rng` family deterministic-rng selects
(`rand_chacha = "0.3"` direct dep, already in lock — `phase3-local:27-29`) so the fixture is
byte-reproducible and schedule-independent. Do **not** use `thread_rng`/`StdRng` (non-portable).

Row-count subsets: 10k = replicate `k=0` alone (exact real data); 25k = `k∈{0,1}` + first 5000
rows of `k=2`; 50k = all five replicates. Why perturb rather than raw-duplicate: raw ×5 would
(a) make bootstrap resamples draw from only 10k distinct rows (inflating ties, understating
variance) and (b) understate realistic Arrow buffer/validity-bitmap/utf8-dictionary sizes at
50k. The generated fixture is **gitignored + regenerable** (derived artifact;
`.claude/rules/repo-management.md` output-pruning posture).

### D8 — Files touched (memory-budget slice)

| Path | Change | Owner note |
|---|---|---|
| `oaxaca_blinder/src/builder.rs:825-848` | drop redundant `df_a`/`df_b` clone (:828-829); shared-base + seeded index resampling | shared with deterministic-rng (seeding) |
| `oaxaca_blinder/Cargo.toml` | add `mem-profile` feature (tracking allocator, dev/test only); add `rand_chacha = "0.3"` direct dep | `mem-profile` native-only, off default (INV-01) |
| `oaxaca_blinder/src/mem_profile.rs` (new) | tracking `GlobalAlloc` + checkpoint API | behind `mem-profile` |
| `engine/src/analysis.rs` | expose a `reps=1/2` profiling entry (feeds checkpoint D) + allocation-pressure self-report (D5) | contract shared with meridian-integration |
| `scripts/build-wasm.sh` | `-zstack-size` / `--max-memory` / `--initial-memory` **values** from D2 | **cross-ref toolchain-build** for placement |
| fixture generator (new, gitignored output) | ×5 perturbation (D7) | In-Scope 13 |

The `--max-memory`/`-zstack-size` **values** are this domain's output; their **placement** in
the build script is toolchain-build's. Do not re-spec the build path here.

---

## Build Steps (ordered — profile-before-parallelize is inviolable)

Global build order (Founder-directed): **determinism fix → memory profile → threading →
validation** (Charter In-Scope 11; Scope-Delta Log 2026-07-17). The memory profile is **step
2**, before any threading. A threading design that lands before the profile artifact exists is a
spec violation, not a sequencing preference.

1. **Add deps + feature.** `rand_chacha = "0.3"` direct dep in `oaxaca_blinder/Cargo.toml`
   (`phase3-local:29`); add a `mem-profile` feature (native, dev/test, off by default).
2. **Build the ×5 fixture generator** (D7). Deterministic, ChaCha8Rng-seeded, `k=0` untouched,
   group var + categoricals unperturbed; output gitignored + regenerable.
3. **Refactor `builder.rs:825-848`** (D4): drop the redundant `df_a`/`df_b` clone-pair
   (:828-829); move to shared read-only base + seeded row-index vector + `take(&idx)`; fold the
   vstack into one `take` over the concatenated base. (Seeding correctness owned by
   deterministic-rng; this step's memory claim is proven in step 5.)
4. **Implement the profile harness** (D1): native tracking `GlobalAlloc` + wasm
   `memory_size(0)` sampling at checkpoints A/B/C/D; report `H_res`, `H_peak`, `Sc`, `St_obs`
   for {native,wasm} × {10k,25k,50k}. Cross-check with `df.estimated_size()`.
5. **Run the profile and commit the report** (before step 7). Include the post-refactor `Sc`
   and, for the reduction proof, `Sc_before` (pre-step-3) vs `Sc_after` at 50k.
6. **Compute the sizing constants** (D2/D3) from the committed profile: `M_max`, `M_init`, `St`,
   `N_max_const`. Record each as a formula over `{H_res(50k), Sc(50k), St_obs(50k)}` with every
   hardcoded margin carrying its cited band. Emit the link-arg **values** for toolchain-build.
7. **Hand off to threading** (meridian-integration/toolchain-build): `N_max_const` and the
   link-arg values become inputs to `initThreadPool(N)` wiring and `build-wasm.sh`. The OOM
   self-report contract (D5) is handed to meridian-integration.
8. **Validation** (verification-benchmark, downstream): the 50k memory-ceiling test asserts
   measured peak at N∈{4,8} ≤ `M_max·(1−headroom_frac)`.

Strategy-A fallback note: under Strategy A (dual artifact) the same profile, formulas, and
`N_max_const` apply unchanged — only the build-path plumbing (which toolchain-build owns)
differs. This domain is Strategy-agnostic; it supplies values, not build wiring.

---

## Acceptance Criteria

Every criterion is objectively checkable (a committed file, a byte-compare, a computed integer,
an assertion).

**Profile (D1):**
- **AC-M1** — A committed memory-profile report table exists with all four symbols
  (`H_res`,`H_peak`,`Sc`,`St_obs`) × {native,wasm} × {10k,25k,50k}: each cell a measured byte
  value, or explicitly marked `estimated_size-only` where allocator instrumentation is
  unavailable. Check: file exists; grep shows 4×2×3 = 24 populated cells (or explicit marks).
- **AC-M2** — The harness is reproducible: same fixture + same seed → same reported bytes within
  a stated tolerance (allocator fragmentation band ≤ ±5%). Check: two runs; per-cell delta ≤ 5%.
- **AC-M3** — The profile report is committed and reviewed **before** `N_max_const` (AC-M6) is
  fixed. Check: the profile artifact's commit precedes the constant's; the profile-before-
  parallelize ordering is a hard precondition (Charter In-Scope 11).

**Sizing (D2):**
- **AC-M4** — `M_max`, `M_init`, `St`, `--max-memory` are each expressed as a formula over
  `{H_res(50k), Sc(50k), St_obs(50k)}` (formulas in D2), with every hardcoded margin carrying a
  cited source band. Check: the spec/report shows the formula, not a bare literal, for each.
- **AC-M5** — The chosen `M_max` sits inside the portable envelope (≤ 2 GiB), band default
  256–512 MiB used only until the profile substitutes measured values. Check:
  `M_init ≤ M_max ≤ 2 GiB` and, absent a profile, `256 MiB ≤ M_max ≤ 512 MiB`.

**Thread cap (D3):**
- **AC-M6** — `N_max_const` is a build-time integer with the 8-clamp folded in (council MJ-6).
  Check: the constant exists and `N_max_const == min(floor((M_max − H_res − Marg)/(Sc + St)), 8)`
  — the `min(_, 8)` MUST be present in the derivation, not just the quotient; and
  `thread-cap.js` emits that clamped value with a comment stating the clamp is already applied.
- **AC-M7** — Consistency assertion holds for the chosen `N`:
  `M_max ≥ H_res + N·(Sc + St) + Marg`. Check: arithmetic assertion passes.
- **AC-M8** — Enforcement point named: the worker computes `N = min(N_max_const,
  hardwareConcurrency, 8)` and passes `N` to `initThreadPool` in `analysis.worker.js`; this
  domain owns `N_max_const` and the `8` ceiling, meridian-integration owns the call. Check: spec
  names the value origin here and the call site there (no duplicate ownership).

**Index resampling (D4):**
- **AC-M9** — The current per-rep clone-pair (`builder.rs:828-829`) is characterized as
  redundant: `sample_n_literal` is called on the clone at `:831,:834` and takes `&self`. Check:
  code read confirms the `&self` signature and the clone-then-sample pattern.
- **AC-M10** — Post-refactor, `Sc_after < Sc_before` at 50k, proven by re-running the D1 harness
  (the profile is the proof, not the design note). Check: profile shows `Sc_after < Sc_before`;
  the reduction multiplier is reported as measured, not asserted as a fixed %.

**OOM contract (D5):**
- **AC-M11** — The OOM tag does **not** rely on parsing `RuntimeError` messages (correction W4):
  the engine emits a structured `{type:'error', payload:{code:'OOM', message}}` value at rep-
  batch boundaries; the trap fallback reinstantiates the module (fresh `Memory`) before the next
  compute; no path renders a blank screen (INV-03). Check: spec/protocol contains no message-
  string classification; the self-report and reinstantiation paths are named.

**L4 correction (D6):**
- **AC-M12** — No `POLARS_MAX_THREADS` instruction appears in this domain's spec or build steps,
  and the one-parallel-layer property is justified by the wasm-stub-onto-global-pool reframe.
  Check: grep this file for `POLARS_MAX_THREADS` returns zero hits outside the correction
  narrative; the D6 reframe is present.

**Fixture (D7):**
- **AC-M13** — The generator is deterministic (fixed master seed) → the 50k fixture is
  byte-reproducible. Check: two regenerations produce byte-identical files (sha256 match).
- **AC-M14** — The group variable (`Gender`) and categorical predictors
  (`Department`,`Job_Title`,`Education_Level`,`Location`) are **not** perturbed. Check: for every
  `k`, those columns equal the `k=0` values row-for-row.
- **AC-M15** — The generated fixture is gitignored + regenerable (not committed). Check:
  `git check-ignore` matches the fixture path; a `.gitignore` entry exists.

---

## Open Items (route to buildability gate)

- **O1 (founder decision — H2 margins ratification):** confirm the safety-margin values
  `St = 1 MiB`, `Marg = 64 MiB`, `Marg_init = 16 MiB`, `headroom_frac = 0.15–0.20`, and
  `N_target = 4–8` once the profile lands. These are the **only** hardcoded constants; the
  founder's "memory of utmost importance" ruling may want a larger headroom than the research
  band default. `Marg`/`Marg_init`/`headroom_frac`/`St` are ratified at the buildability gate.
- **O2 (resolve with toolchain-build):** under wasm-bindgen `--target web` + wasm-bindgen-rayon
  1.3.0, is the shared `maximum` set via a wasm-ld `--max-memory` link arg (build-time constant)
  or via the `WebAssembly.Memory({maximum})` descriptor in the rayon shim's JS glue (loader-
  injected)? Determines **where** the profile-derived `M_max` constant is pinned — not whether
  the D2 formula is correct.
- **O3 (single-agent research, non-blocking):** does dlmalloc under threaded wasm32 expose
  per-arena in-use byte statistics (mallinfo-analogue) so `Sc` can be measured as live bytes
  rather than inferred from monotonic `memory_size` deltas? If not, `Sc` is reported from
  `memory_size` deltas cross-checked with `estimated_size()` (D1 caveat), which is sufficient
  for a conservative thread cap.
- **O4 (single-agent research, non-blocking):** does `DataFrame::take(&IdxCa)` at 50k produce a
  single contiguous ~1F materialization or retain chunked references to the shared base (sub-F),
  and does Polars 0.44 `take` behave identically native vs wasm32? Sets whether `Sc_after` is
  `~F` or materially below F; the AC-M10 measurement settles it empirically regardless.

---

## Sources

- Phase 1: `phase1-memory-budget-shared-memory-limits.md:13,18,19,20,21,22,27,30`
  (fixed-maximum shared-memory model, envelope band, `-zstack-size`, no runtime free-memory
  query, thread-cap formula source); `phase1-memory-budget-polars-allocator.md:19,20,21,27,29`
  (50k×10 ≈4 MB arithmetic; dlmalloc default, wee_alloc archived).
- Phase 3 LOCAL: `phase3-local-l1-l2-l3-l4-l5-l6-findings.md` — L1 (`take` gather semantics,
  bounds-checked, IdxSize=u32, `frame/mod.rs:1841`), L2 (`rand_chacha 0.3.1` in lock,
  `ChaCha8Rng::set_stream`), **L4 (`POLARS_MAX_THREADS` never read on wasm; wasm POOL stub
  delegates to global rayon registry — the CRITICAL correction)**, L6 (Education_Level 3-level).
- Phase 3 WEB: `phase3-web-w1-w2-w3-w4-w5-w6-w7-w8-w9-w10-findings.md` — W4 (OOM taxonomy;
  budget-prevention + structured self-report, do not parse RuntimeError); § Phase 4 corrections
  #5 (L4 reframe), #1 (W4 OOM).
- Charter: `spec-charter.md` — In-Scope 5/11/13, INV-03, INV-05, INV-07/INV-08 (f64-exempt
  ruling; monetary rounds through Decimal(18,2) at the data boundary), ASM-04 (50k ceiling).
- Writer brief: `phase4-writer-brief.md` — six corrections, locked decisions, output rules.
- Code anchors (verified this phase via Read): `oaxaca_blinder/src/builder.rs:813` (dummy
  hstack), `:822-823` (`df_a_global`/`df_b_global`), `:825` (bootstrap `into_par_iter`),
  `:828-829` (redundant clone-pair), `:831,:834` (`sample_n_literal(&self)`), `:838` (vstack),
  `:847` (`filter_map(...).ok()` silent rep discard), `:851-856` (discard warning);
  `quantile_decomposition.rs:215,244` (`thread_rng()` sites, per determinism domain).
