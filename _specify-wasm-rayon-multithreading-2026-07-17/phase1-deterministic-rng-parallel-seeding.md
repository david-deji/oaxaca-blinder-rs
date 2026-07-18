# Phase 1 — RNG: Schedule-Independent Parallel Seeding (+ R/Stata practice)

> Units: u5 + u6 (merged) | Domain: deterministic-rng | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

The canonical bit-identical pattern is exactly what the Rust Rand Book documents: one master seed,
one RNG instance per work unit derived from master seed + replicate index — either
`ChaCha8Rng::set_stream(rep)` (endorsed stream model) or a mixed per-rep seed. For legal
defensibility, `StdRng` is explicitly NOT reproducibility-guaranteed across versions; `rand_chacha`
(pinned version) is the recommended algorithm with published test vectors. Rayon's indexed
`collect()` preserves order deterministically; the footgun is floating-point `reduce`/summation
order — downstream statistics over bootstrap results must use sequential (or explicitly ordered)
summation to stay bit-identical across thread counts. Polars' seeded sampling is
deterministic-per-version but has no cross-version stability contract — for audit-grade
reproducibility, own the resampling (index vectors from our pinned ChaCha) and use Polars `take`
instead of `sample_n_literal`.

## Findings

1. [CONSENSUS] Pattern: `(0..B).into_par_iter().map(|rep| { let mut rng = ChaCha8Rng::from_seed(master); rng.set_stream(rep); ... })` — or per-rep seed via a proper mixing derivation. Never clone RNGs across tasks; never seed from thread identity; `seed = master + i` is a footgun (structured adjacent seeds — use set_stream or hash-mixing like seed_from_u64's internal hash). (rust-random book guide-parallel; docs.rs SmallRng)
2. [CONSENSUS] Version-stability for defensibility: StdRng explicitly non-reproducible across versions (docs recommend rand_chacha directly); SmallRng algorithm may change; **rand_chacha ChaCha8/12/20 with pinned crate version = the auditable choice** (test vectors published). Record seed, algorithm, crate version, replicate count in run metadata. (rust-random book crate-reprod; StdRng docs)
3. [CONSENSUS] Rayon: indexed parallel iterator `collect()` is order-preserving regardless of scheduling — replicate i lands at position i. Nondeterminism enters via parallel `reduce`/`fold` on floats (summation order). Bootstrap stats (std_err, CI percentiles) must be computed sequentially over the ordered collected Vec — current code already collects then computes via `bootstrap_stats`, verify that path stays sequential. (rust-random book; rayon docs via users.rust-lang; oneuptime float-order writeup)
4. [REPORTED] Polars seeded sampling: deterministic for a fixed version+seed, no documented cross-version value-stability. Recommendation: replace `sample_n_literal(n, true, false, None)` with own index-vector resampling (ChaCha-generated `Vec<u32>` → `df.take(&idx)`), making Polars a pure data container. This also fixes the silent-discard nondeterminism (failed reps become deterministic per-rep outcomes). (rust-random crate-reprod inference; session recon builder.rs:832)
5. [REPORTED — thin sourcing, Phase 3 gap] R practice: `set.seed()` before boot() is the standard auditable workflow; Stata: `set seed` before bootstrap. No primary R/Stata doc fetched in this pass — Phase 3 must fetch primary documentation (R boot vignette / Stata bootstrap manual) if the spec cites them as the benchmark practice; cross-language convention (record seed + algorithm + version) is consistent across secondary sources.
6. [CONSENSUS] MM simulation path (quantile_decomposition.rs:215,217 random_quantiles; :244-248 resample) has TWO rng sites per pass; both must derive from the same master-seed schedule with distinct stream ids (e.g. stream = rep*2, rep*2+1 or purpose-tagged derivation). (session recon + pattern application)

## Spec Implications

- API surface: builders gain `.seed(u64)` (default: fixed documented constant? or required?) — spec must decide default-seed semantics: explicit-seed-required for audit runs vs stable default. Point estimates are unaffected (deterministic already).
- Native/wasm parity: same seeded path runs everywhere → threaded-vs-sequential-vs-native parity test becomes byte-comparison of serialized results JSON (SC-03).
- Failed-rep determinism: with owned index resampling, a rep's success/failure is a pure function of (master seed, rep, data) — the silent-discard warning becomes deterministic and testable; spec should also surface discard count in results (defensibility metadata).

## Sources

- https://rust-random.github.io/book/guide-parallel.html
- https://rust-random.github.io/book/crate-reprod.html
- https://docs.rs/rand/latest/rand/rngs/struct.SmallRng.html
- https://snowbridge-rust-docs.snowfork.com/rand/rngs/struct.stdrng
- https://users.rust-lang.org/t/creating-predictable-prngs-per-rayon-task-from-a-master-prng/27341
- https://oneuptime.com/blog/post/2026-01-25-process-millions-records-parallel-jobs-rust/view
- https://docs.rs/deterministic_rand/latest/deterministic_rand/

## Research Inventory

- Perplexity ask 2026-07-17 (citations above)
- Session recon 2026-07-17: builder.rs:825-848, quantile_decomposition.rs:215-251, inference.rs (bootstrap_stats)
