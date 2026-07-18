# Distilled — WASM Rayon Multithreading + Statistical Trust (0014-MERIDIAN)

Add real Rayon thread-pool parallelism to the `pay-equity-engine` WASM build (consumed by Meridian in one Web Worker, served by audit-forge Flask at `/pay-equity/` loopback:5137), plus the determinism, memory, and trust work around it. Scale 50k rows; memory first-class.

**Build order (inviolable):** determinism → memory profile → threading → validation.

**7 domains (phase4-final-*.md):** deterministic-rng (seeded ChaCha8 per-rep streams + owned index resampling; INV-02 split); memory-budget (profile-before-threading; thread-cap `min(floor(mem-formula), 8)`; kill redundant per-rep clones); engine-parallel-surface (5-entry audit; `init_thread_pool` re-export; delete no-op `POLARS_MAX_THREADS`; In-Scope 12); toolchain-build (nightly+build-std+atomics+`--target web`; Strategy A dual artifact); meridian-integration (COI feature-detect → `initThreadPool`; page-level COOP/COEP; Vite `worker.format:'es'`; sequential fallback never crashes); statistical-trust-layer (R `oaxaca`/`ddecompose` goldens; proptest identities); verification-benchmark (mode-parity + memory-ceiling + reproducibility + speedup CI).

**In-Scope 12 = wiring, not new math:** `OaxacaBuilder::decompose_quantile` (`builder.rs:720`) already does one-stage RIF-OLS with per-predictor detail; route both WASM and CLI through it, recompute RIF per bootstrap replicate, forward the seed.

**Founder rulings:** Strategy A; INV-02 within-platform-byte-identical + native↔wasm ≤1e-6; In-Scope 12 both-surfaces-RIF; recompute-RIF-per-replicate.

**/build Phase 0 preflights:** E1 nightly-compile (subgraph-trim), E2 shared-memory link args, E3 nested-worker spawn (main-thread-relay fallback).
