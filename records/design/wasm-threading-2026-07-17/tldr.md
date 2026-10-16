# TL;DR — 0014-MERIDIAN

Add Rayon threading to the pay-equity-engine WASM build (Meridian, 50k rows, memory-first), gated by determinism → memory-profile → threading → validation. In-Scope 12 is wiring the existing `decompose_quantile` (RIF) into both surfaces, not new math. Rulings: Strategy A dual-artifact; INV-02 = within-platform byte-identical + native↔wasm 1e-6; both-surfaces-RIF; recompute-RIF-per-replicate.
