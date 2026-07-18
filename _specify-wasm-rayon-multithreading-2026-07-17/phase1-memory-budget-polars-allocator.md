# Phase 1 — Memory: Polars WASM Footprint + Allocator

> Unit: u4 | Domain: memory-budget | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

Polars compiles and runs on wasm32-unknown-unknown (confirmed since 0.25; community MWE runs
Polars+rayon threaded in-browser with COOP/COEP). A 50k×10 mixed frame is small — ~4 MB by Arrow
layout arithmetic — so the DATASET is not the memory problem; per-rep DataFrame clones and
operation temporaries are. Default allocator is dlmalloc (keep it; wee_alloc is archived/leaky;
alternatives undocumented under threaded wasm). Critical operational finding: **Polars has its own
internal rayon usage (POLARS_MAX_THREADS)** — it will share the global rayon pool that
wasm-bindgen-rayon installs, so pool sizing must account for Polars' internal parallelism to avoid
oversubscription, or Polars threads pinned to 1 with parallelism kept at our bootstrap layer.

## Findings

1. [CONSENSUS] Polars works on wasm32-unknown-unknown for in-memory compute; native-I/O features are unusable; community MWE (polars-wasm-mwe) demonstrates threaded Polars via wasm-pack + COOP/COEP. (stackoverflow maintainer answer; github polars-wasm-mwe)
2. [CONSENSUS] Arrow sizing arithmetic: f64/i64 col at 50k rows ≈ 0.39 MB (+6 KB validity bitmap); categorical ≈ 0.25 MB (u32 keys + dictionary); utf8 ≈ ~1.15 MB at avg 20 bytes. 50k×10 mixed ≈ **~4 MB** resident. Verify empirically with `estimated_size()`. (pola.rs docs; conterval; stackoverflow 77766301)
3. [SINGLE_SOURCE] Keep dlmalloc (default on wasm32): pure Rust, adequate for large contiguous Arrow buffers; wee_alloc archived with leaks; talc/lol_alloc/mimalloc-wasm lack documented threaded-wasm behavior — treat as out of scope. (dev.to kent-tokyo; rustc platform docs)
4. [REPORTED] Polars parallelizes internally via rayon (POLARS_MAX_THREADS); in wasm it uses the same global pool wasm-bindgen-rayon initializes. Design decision needed: (a) let Polars share the pool (risk: nested-parallelism oversubscription with our par_iter bootstrap), or (b) pin POLARS_MAX_THREADS=1 and keep all parallelism at the bootstrap-rep layer. Option (b) is the deterministic-friendly default. (shuttle.dev polars post; polars-wasm-mwe; synthesis)
5. [UNVERIFIED] Exact temporary-allocation multipliers for our operation mix (dummy encoding hstack, vstack, to_ndarray) at 50k rows — no external source; must come from the In-Scope 11 empirical profile.
6. [CONSENSUS] Memory-dominant term is per-rep cloning, not the base frame: current code clones df_a+df_b per bootstrap rep (builder.rs:828-829); at ~4 MB/frame-pair with N in-flight reps that's N×~8 MB churn plus per-pass matrices. Index-based resampling (share base frame read-only; materialize row-index vectors) is the design lever. (session recon + Arrow arithmetic)

## Spec Implications

- POLARS_MAX_THREADS=1 inside wasm (env-var or ThreadPoolBuilder equivalent) unless Phase 3 finds a compelling reason otherwise — one parallel layer, ours.
- Memory profile harness should report: base frame estimated_size, peak during single decompose (100 reps), peak per concurrent rep — enough to fit the INV-05 thread-cap formula.
- Allocator: no change; dlmalloc thread-safety under wasm threads to be confirmed in Phase 3 (it's the rustc-shipped default for the threaded raytrace example, so risk is low).

## Sources

- https://stackoverflow.com/questions/74168279/how-to-use-polars-with-wasm
- https://github.com/rohit-ptl/polars-wasm-mwe
- https://pola.rs/posts/understanding-polars-data-types/
- https://docs.pola.rs/user-guide/expressions/missing-data/
- https://stackoverflow.com/questions/77766301/polars-dataframe-enum-and-categorical-type-memory-usage
- https://dev.to/kent-tokyo/why-pure-rust-wasm-is-harder-than-it-looks-4p48
- https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html
- https://www.shuttle.dev/blog/2025/09/24/pandas-vs-polars

## Research Inventory

- Perplexity ask 2026-07-17 (citations above)
- Session recon 2026-07-17: builder.rs:828-829 (per-rep clones), oaxaca_blinder Cargo.toml (polars 0.44 feature set)
