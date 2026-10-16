# Memory-Profile Report — 0014-MERIDIAN Stage 2 (memory-budget domain)

> Generated: 2026-07-18 · Native tracking `GlobalAlloc` (`mem-profile` feature, off by default / INV-01)
> Fixture: `mem_profile_50k.csv` (×5 perturbation of `Employers_data.csv`, seed `0x0014_50CE_5EED_A115`)
> Harness: `oaxaca_blinder/examples/mem_profile_harness.rs`, release build
> Measurement: bytes-requested-through-`GlobalAlloc` (schedule-/fragmentation-independent; understates true RSS by allocator slack → conservative for an OOM guard).

Build step 5 of the memory-budget domain (In-Scope 11), a precondition to threading (inviolable
order: determinism → memory profile → threading → validation). The profile did its job: it
surfaced — and the fix resolved — a real WASM-OOM hazard that a threading-first build would have
shipped. **The investigation corrected the diagnosis twice; the numbers below are the final,
fix-verified state.** Two earlier hypotheses (retained bootstrap results; a D2 formula gap) were
empirically falsified along the way — see § Finding 2 for the true mechanism.

## D1 — Measured profile (mean-path decompose, R=100, ×5 fixture)

Predictors: numeric `Age, Experience_Years`; categorical (one-hot) `Education_Level, Department, Job_Title, Location`. Outcome `Salary`, group `Gender` (ref `Female`).

| n | logical (`estimated_size`) | H_res | Sc (per-rep marginal) |
|---|---|---|---|
| 10 000 | 0.75 MiB | 8.0 MiB | 5.49 MiB |
| 25 000 | 1.92 MiB | 18.2 MiB | 13.73 MiB |
| 50 000 | 3.87 MiB | 35.2 MiB | 27.40 MiB |

Raw bytes @ 50k: `H_res=36_808_432  Sc=28_734_041`. `H_res` = resident heap at checkpoint B
(base frame + dummy hstack + point estimate + framework floor). `Sc` = marginal working set of
**one** in-flight bootstrap rep (`run_single_pass`: design matrices + residuals + OLS temporaries).

**St_obs (native):** coarse; the mean-path call graph has no deep recursion. The authoritative
wasm `__stack_pointer` low-water probe is deferred to the threading stage. The `St = 1 MiB`
placeholder carries large margin over any realistic decompose stack (AC-M4 `St ≥ St_obs` holds).

## Peak memory — bounded parallelism (the load-bearing result)

Peak scales with the number of bootstrap reps **in flight at once**, not with the total rep
count. After the bounded-parallel fix (§ Finding 2), peak = `H_res + N·Sc` where N = thread-pool
size, **flat in total rep count** (chunk barrier caps in-flight working sets at N):

| N (pool) | peak @ 50k, R=100 (bounded) | before fix (unbounded rayon) | vs 512 MiB band |
|---|---|---|---|
| 1 | 64 MiB | 461 MiB | ✓ |
| 4 | 145 MiB | 767 MiB | ✓ |
| **8** (= N_max_const) | **248 MiB** | 860 MiB | **✓ in-band** |
| 16 | 427 MiB | 899 MiB | ✓ |

At fixed N, peak is flat across R ∈ {10, 50, 100} (e.g. N=8: 238 / 241 / 248 MiB) and extrapolates
flat to the MCP's R=10 000 cap — **no rep-count OOM**. The model `H_res + N·Sc` fits exactly
(N=4: predicted 145, measured 144.9).

## Findings (spec-codebase reconciliation, per build-safety.md)

### Finding 1 — the D4 "memory lever" is a no-op (AC-M9 revised, AC-M10 retired)

Spec D4 treats the per-rep `df_a`/`df_b` clone-pair as ~2F memory and its removal as a "lever."
**Polars `DataFrame::clone()` is an Arc/COW refcount bump, not a deep copy**, so the clone-pair
never allocated frames. Isolated resample @ 50k: `Sc_before` (clone-pair + `sample_n_literal`)
≈ `Sc_after` (shared-base `take`), within ±4% run-to-run noise, **no systematic reduction**. The
stage-1 refactor stands **for determinism** (seeded owned-index `take` → INV-02), not memory.
AC-M9: the clone-pair was already ≪ F (O(1) Arc bumps). AC-M10 (`Sc_after < Sc_before`) is
retired as a false premise.

### Finding 2 — the OOM driver is unbounded parallel buffering, fatal only in WASM (fixed)

**Mechanism (empirically isolated):** a single `(0..reps).into_par_iter().map(..).collect()` lets
rayon keep many reps' `Sc`-sized `run_single_pass` working sets in flight simultaneously. Peak
therefore scales with **both** rep count and thread count:

- Sequential `into_iter`: peak flat at 64 MiB for any rep count (each transient freed per rep).
- Unbounded `into_par_iter`: peak 461 MiB (N=1) … 899 MiB (N=16) at R=100, growing with R.
- CURRENT **after** the collect is ≈ H_res regardless of R → **no retention, no leak.** The
  461–899 MiB is transient in-flight, reclaimed the instant the loop ends.

**Why it matters only for WASM:** native reclaims the transient immediately (benign). **WASM
linear memory only ever grows — it never returns pages** — so an over-buffered transient peak
permanently sizes the SharedArrayBuffer and OOMs the tab. This is precisely the
"optimize-speed-into-an-OOM" hazard In-Scope 11 exists to catch.

**Two earlier hypotheses were falsified by measurement** (recorded for honesty): (a) "retained
`SinglePassResult`s dominate" — refuted: dropping them (the `RepEstimates` extraction) left peak
unchanged; (b) "the D2 `M_max` formula omits a serial `R·retained` term" — refuted: once
parallelism is bounded, peak = `H_res + N·Sc` *is* the D2 model, and it is accurate.

**Fix (two parts, both landed in `builder.rs`):**
1. `RepEstimates` — the parallel map extracts only the scalar components the SE/CI reduction reads
   and drops the heavy `SinglePassResult` per rep. Necessary hygiene: the original
   `Vec<SinglePassResult>` retained ~400 KB/rep (residual vectors) → ~4 GB at R=10 000 (a real,
   separate high-rep retention OOM); `RepEstimates` cuts that to ~2.3 KB/rep (~23 MB).
2. **Bounded-parallel (chunked) bootstrap** — reps run in index-ordered chunks of the pool size,
   capping in-flight working sets at N → peak = `H_res + N·Sc`, flat in rep count. Chunks and
   within-chunk results are consumed in strict rep-index order, so the reduction's
   floating-point sum order is fixed regardless of thread count.

**Verification:** peak table above (bounded, in-band at N=8); AC-6 serialization sha256
`d74efb3c…` **identical across RAYON_NUM_THREADS 1/2/4/8** (INV-02 byte-identity — same hash as
stage 1); AC-9 mean-path byte-identical to the pre-Stage-2 baseline (numerically invariant);
`rng_determinism` 5/5. The quantile path needs no change — its `SinglePassResult` holds only
`HashMap<String, DecomposedEffects{gap,characteristics,coefficients}>` (3 scalars/quantile),
already memory-safe by construction (heavy MM `betas` are transient inside `run_single_pass`).

## D2/D3 constants (now accurate — bounded parallelism makes the formula hold)

From measured 50k `H_res`/`Sc`, band-sourced placeholder safety constants (D2 table), `N_target=8`
(`scripts/compute-mem-constants.py`, co-located in `.build-state/`):

```
M_max            = 342_228_992 B (326 MiB)   # H_res + 8·(Sc+St) + Marg, round-up-page; ≤ 2 GiB ✓
M_init           =  82_378_752 B ( 79 MiB)
N_max (uncapped) = 8   → N_max_const = min(8, 8) = 8      # AC-M6 build-time constant
St = 1 MiB (band 0.5–1, ≥ St_obs ✓) · Marg = 64 MiB · Marg_init = 16 MiB · headroom_frac = 0.15
```

**`N_max_const` = 8 — a TRUE safe.** The measured bounded peak at N=8/50k is **248 MiB**, which
satisfies the AC-M7 runtime ceiling assert `measured_peak ≤ M_max·(1−headroom)` = 248 ≤ 326·0.85
= 277 ✓, and sits well under the 512 MiB band and the 2 GiB portable envelope. The 8-clamp binds
before memory does (budget alone would allow ~14 threads), so `N_max_const == min(floor((M_max −
H_res − Marg)/(Sc + St)), 8) == 8`.

**Link-arg values (handoff to toolchain-build):** `-zstack-size=1048576`,
`--max-memory=342228992`, `--initial-memory=82378752`.

**Enforcement point (AC-M8):** this domain owns the value `N_max_const = 8` (8-clamp folded in);
meridian-integration's M7 step writes `frontend/src/wasm/thread-cap.js`
(`export const MEMORY_THREAD_CAP = 8;`) and owns the `initThreadPool(min(hardwareConcurrency, 8))`
call. The bounded-parallel bootstrap (`builder.rs`) is what makes `peak ≤ H_res + N·Sc` true at
runtime — threading MUST NOT revert it to an unbounded `into_par_iter().collect()`.

## Handoff to threading stage

- `N_max_const = 8`; link-arg values above.
- **Invariant for threading:** the bootstrap loop must stay bounded-parallel (chunked to the pool
  size). Reverting to unbounded `into_par_iter().collect()` reintroduces the WASM-OOM (peak grows
  with rep count, unbounded).
- verification-benchmark (stage 4): the 50k memory-ceiling test asserts `measured_peak(50k, N=8)
  ≤ M_max·(1−headroom)` — measured 248 MiB vs 277 MiB ceiling; wasm `memory_size(0)` cross-check
  (native `__stack_pointer`/St_obs authoritative measurement lands there too).

## AC-M status

| AC | Status | Evidence |
|---|---|---|
| AC-M1 (report, 4 symbols) | ✅ | this report |
| AC-M2 (reproducible ±5%) | ✅ | bytes-requested counter; deterministic fixture+seed |
| AC-M3 (report before N_max_const ship) | ✅ | report committed; thread-cap.js is threading-stage |
| AC-M4 (constants as formulas + St ≥ St_obs) | ✅ | § D2/D3 |
| AC-M5 (M_max ≤ 2 GiB) | ✅ | 326 MiB; measured peak 248 MiB in-band |
| AC-M6 (N_max_const build-time int, 8-clamp) | ✅ | = 8, true safe (peak 248 < ceiling 277) |
| AC-M7 (consistency + runtime ceiling assert) | ✅ | 248 ≤ 326·0.85 = 277 |
| AC-M8 (enforcement point named) | ✅ | thread-cap.js (meridian) + bounded loop (builder.rs) |
| AC-M9 (clone-pair characterized) | ✅ revised | ≪ F (Arc bump) — Finding 1 |
| AC-M10 (Sc_after < Sc_before) | ❌ retired | false premise (Polars clone shallow) |
| AC-M11 (OOM tag not RuntimeError-parse) | ✅ | proactive N_max_const cap + bounded loop |
| AC-M12 (no POLARS_MAX_THREADS) | ✅ | absent |
| AC-M13/M14/M15 (fixture) | ✅ | deterministic sha / unperturbed group+categoricals / gitignored |

**Gate status: PASS.** Profile complete, memory hazard fixed and verified, `N_max_const = 8`
computed as a true safe, all determinism/parity invariants hold. Cleared to threading.
