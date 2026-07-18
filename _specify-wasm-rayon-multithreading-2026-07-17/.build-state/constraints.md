# Build Constraints — 0014-MERIDIAN (WASM Rayon + statistical trust)

> Read this file in full on every worker invocation. These are non-negotiable.
> Source of truth: `spec-charter.md` + `buildability-gate-rulings.md` + the 7 `phase4-final-*.md` specs.

## Inviolable build order (founder-directed)

**determinism → memory profile → threading → validation.** Never land threading before the
memory profile is committed and reviewed. Each stage gates the next.

## Charter Invariants (must hold in produced code)

- [INV-01] Native builds (CLI, meridian-mcp, engine rlib) remain byte-equivalent in behavior; all wasm-threads changes are feature-gated OFF the native path. (Source: Founder Intake; apps-file-caution.md)
- [INV-02] Same input + same seed → byte-identical output WITHIN a platform across thread counts and threaded/sequential modes (native 1/2/4 sha256-equal; wasm seq/2/4 sha256-equal). The native↔wasm leg is tolerance-parity ≤ 1e-6, NOT byte-identity (cross-ISA libm). (Source: buildability gate 2026-07-18, the split)
- [INV-03] A non-cross-origin-isolated context degrades to working sequential execution — never a crash, never a blank screen. (Source: build-safety.md)
- [INV-04] Every shipped wasm blob has a committed sha256 baseline + a pinned toolchain that rebuilds it byte-identically (per artifact under Strategy A). (Source: Track-0 SC-04)
- [INV-05] Peak memory at 50k rows fits the declared shared-memory maximum with measured headroom; thread count is capped by the memory budget, not just hardwareConcurrency. (Source: Intake round 2)
- [INV-06] audit-forge header changes are additive and scoped to /pay-equity/ + /api/ only; the 9 pre-existing blueprints' surfaces stay untouched. (Source: webui/__init__.py:127)
- [INV-07] Monetary values remain Decimal(18,2); no Float64 introduction during refactors — scoped by INV-08. (Source: comp-audit-suite rule)
- [INV-08] The statistical engine is EXEMPT from Decimal(18,2); f64 is correct for regression/decomposition math; monetary values round through Decimal(18,2) at display/ledger boundaries the spec names. Record this ruling in the engine repo CLAUDE.md. (Source: Founder ruling 2026-07-17)

## Anti-Goals (prohibition → preferred alternative)

- Changing the Meridian worker postMessage `{type, payload}` API or any Vue/Pinia layer → keep the contract byte-compatible; all changes stay inside worker init + engine.
- Parallelizing native (non-wasm) code paths further → native already uses rayon; only the wasm enablement layer changes.
- Public/hosted deployment or new serving infra → local-only (audit-forge Flask loopback + Vite dev); a future host inherits the headers checklist.
- Algorithm changes to decomposition/optimization math EXCEPT the founder-approved In-Scope 12 quantile detail → existing paths: only RNG seeding structure changes, point-estimate math untouched, verified by parity tests. In-Scope 12: additive (new outputs), validated by its own golden + identity tests.
- Dropping/weakening the reproducible-build model → the chosen strategy preserves a hashable, re-buildable raw-wasm baseline (Strategy A: two baselines).
- Adopting wasm32-wasip1-threads or non-browser thread targets → browser target only (wasm32-unknown-unknown + wasm-bindgen-rayon).
- Committing pkg/ or nightly-toolchain binaries to git → pkg/ stays gitignored; toolchains pinned by config files.

## Founder rulings (buildability gate 2026-07-18)

1. Reproducibility = Strategy A (dual artifact; threaded baseline generated in a pinned container/toolchain).
2. INV-02 = the split (see above).
3. In-Scope 12 aggregate = a-1: switch BOTH the WASM branch (engine/src/analysis.rs:166-206) AND the CLI (oaxaca_blinder/src/main.rs:247) to `OaxacaBuilder::decompose_quantile` (RIF). MM aggregate changes MM→RIF; one coherent method everywhere.
4. Quantile SE = recompute RIF per bootstrap replicate (`fixed_rif: false`).

## Build-time reconciliations (from Phase 0 probe, 2026-07-18)

- Nightly pin = `nightly-2025-06-27` (E1-proven to build-std the full wasm graph with +atomics,+bulk-memory,+mutable-globals; the spec's `nightly-2024-08-02` was provisional). The MJ-4 subgraph-trim (default-features=false / cfg-gate askama) is NOT required for compile feasibility on this nightly. Any AC text citing `nightly-2024-08-02` reads as `nightly-2025-06-27`.
- `rand_chacha 0.3.1` already present in the dependency graph (E1 log) — determinism RNG dep available.
