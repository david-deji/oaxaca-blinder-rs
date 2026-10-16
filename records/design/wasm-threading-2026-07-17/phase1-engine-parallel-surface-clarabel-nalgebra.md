# Phase 1 — Parallel Surface: clarabel/nalgebra Thread-Safety

> Unit: u7 | Domain: engine-parallel-surface | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

Weakest-sourced unit of Phase 1 — several load-bearing claims remain unverified and are flagged
for Phase 3 (primary-source or code-inspection verification). Working picture: nalgebra DMatrix is
single-threaded per-operation and safe for independent per-task matrices (Send/Sync value types);
clarabel's public docs show no internal thread spawning or BLAS dependency, so N independent
solves from rayon tasks should be safe — but this needs verification from clarabel's source/docs
(it's the engine of the optimize/frontier path). The known wasm-specific hazard is initialization:
compute must not start until the pool is ready (initThreadPool is async; awaiting it is mandatory),
and the classic deadlock pattern is blocking the spawning thread immediately after pool init.

## Findings

1. [UNVERIFIED → Phase 3] clarabel internal threading/BLAS: public docs show a pure-Rust interior-point solver with no documented internal parallelism; no rayon/BLAS in its dependency claims found this pass. MUST verify by inspecting clarabel's Cargo.toml/source (it's vendored in our Cargo.lock — local inspection is cheap and definitive). (docs.rs/clarabel — inconclusive)
2. [CONSENSUS] nalgebra: no internal parallelism for DMatrix ops; supports wasm targets; independent matrices per task are safe. (nalgebra.org wasm_and_embedded_targets; docs.rs DMatrix)
3. [UNVERIFIED → Phase 3] ndarray threading guarantees (used in QR path: solve_qr takes Array views) — same per-task-ownership logic applies but uncited; verify via ndarray docs.
4. [CONSENSUS] One parallel layer: cap any library-internal parallelism (Polars, hypothetical BLAS) to 1 when our rep-level par_iter is the outer loop; oversubscription is the standard hazard. (multiple secondary sources; consistent with u4 finding 4)
5. [REPORTED] Standard pitfalls for parallel per-rep OLS/QR solves: allocator contention (dlmalloc lock) with many small allocations, false sharing on adjacent output slots (collect() into Vec of owned structs avoids this), per-task scratch preallocation preferred. (secondary sources)
6. [REPORTED → Phase 3 sharpen] wasm-bindgen-rayon execution semantics: rayon work runs on the pool's worker threads; `initThreadPool(n)` is async and MUST be awaited before first par_iter; long-running sync compute on the calling worker thread immediately after init can stall pool startup (known pattern). Exact calling-thread participation semantics unverified — Phase 3: read wasm-bindgen-rayon README/source on this point. (inference + secondary)

## Per-Entry-Point Parallelization Sketch (to be refined in Phase 2/3)

| Entry point | Inner structure | Parallel candidate | Note |
|---|---|---|---|
| decompose (mean path) | 100 bootstrap reps × OLS passes | reps via par_iter (exists) | hot path; seeded streams per u5 |
| decompose (quantile path) | per-tau solve_qr ×2 groups + MM sim + bootstrap | per-tau par_iter (exists :222,227) + reps | clarabel per solve — verify (finding 1) |
| optimize | single LP/QP solve | likely NOT parallel (one solve) | document skip verdict |
| calculate_efficient_frontier | grid of independent optimizations | par_iter over grid points | strong candidate; clarabel verification gates it |
| verify_adjustments | recompute decomposition on adjusted data | inherits decompose parallelism | no extra work |
| check_defensibility | scoring over decompose outputs | likely trivial/serial | document skip verdict |

## Sources

- https://docs.rs/clarabel/latest/clarabel/
- https://nalgebra.org/docs/user_guide/wasm_and_embedded_targets/
- https://docs.rs/nalgebra/latest/nalgebra/base/type.DMatrix.html
- https://webassembly.org/docs/security/
- https://www.usenix.org/system/files/atc19-jangda.pdf

## Research Inventory

- Perplexity ask 2026-07-17 (citations above — thin; three findings flagged for Phase 3 local/primary verification)
- Session recon 2026-07-17: engine/src/analysis.rs entry points; quantile_decomposition.rs:222,227,338
