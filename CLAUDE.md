# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# Build the entire workspace
cargo build

# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p oaxaca_blinder
cargo test -p pay-equity-engine
cargo test -p meridian-mcp

# Run a single test by name
cargo test -p oaxaca_blinder test_name

# Build the CLI binary
cargo build --bin oaxaca-cli

# Build Python bindings (requires maturin)
cd oaxaca_blinder && maturin develop --features python

# Build engine with WASM target
cargo build -p pay-equity-engine --features wasm --target wasm32-unknown-unknown
```

## Workspace Architecture

This is a Rust workspace with three crates:

### `oaxaca_blinder` — Core Decomposition Library
The statistical engine implementing econometric decomposition methods for pay equity analysis. Operates on Polars DataFrames with linear algebra via Nalgebra.

**Decomposition methods** (each has its own module):
- `decomposition.rs` — Standard Oaxaca-Blinder (two-fold and three-fold)
- `quantile_decomposition.rs` — RIF-Regression quantile decomposition (Firpo-Fortin-Lemieux)
- `jmp.rs` — Juhn-Murphy-Pierce decomposition
- `dfl.rs` — DiNardo-Fortin-Lemieux reweighting
- `akm.rs` — Abowd-Kramarz-Margolis high-dimensional fixed effects
- `heckman.rs` — Heckman two-step selection correction
- `matching/` — Propensity score matching (logistic model, distance metrics, matching engine)

**Math utilities** (`math/`): OLS regression, quantile regression, KDE, RIF, probit, logit, diagnostics, normalization.

**Entry points**: `OaxacaBuilder` and `QuantileDecompositionBuilder` (builder pattern). CLI via `oaxaca-cli` binary. Python bindings via PyO3 behind `python` feature flag.

### `engine` (pay-equity-engine) — Optimization & Verification
Wraps `oaxaca_blinder` with optimization (budget-constrained wage adjustments), verification, efficient frontier calculation, and defensibility scoring. Has a WASM target (`wasm` feature) for browser use. Key modules: `analysis.rs`, `defensibility.rs`, `types.rs`.

### `meridian-mcp` — MCP Server
JSON-RPC server (stdio or SSE/HTTP via Axum) exposing engine functions as MCP tools: `decompose`, `optimize`, `verify_adjustments`, `calculate_efficient_frontier`, `check_defensibility`. Configurable via CLI args or env vars (`PORT`, `MCP_TRANSPORT`, `MCP_API_KEY`).

## Key Patterns

- **Builder pattern** for all analysis entry points (`OaxacaBuilder::new(...).predictors(...).run()`)
- **Polars DataFrames** as the universal data interchange format; never raw Vec/arrays
- **Nalgebra** `DMatrix`/`DVector` for all linear algebra; `clarabel` for convex optimization
- **Rayon** for parallel bootstrap iterations
- All monetary values must use `Decimal(18,2)`, never `Float64` (comp-audit-suite rule)
  - **Statistical-math exemption (INV-08 — 0014-MERIDIAN, founder ruling 2026-07-17):** the decomposition/regression engine (`oaxaca_blinder`, `engine`) is EXEMPT — `f64` is correct for OLS/RIF/quantile/bootstrap math and must not be forced to `Decimal`. Monetary values round through `Decimal(18,2)` only at the display/ledger boundaries the spec names, never inside the estimator.
- Feature flags: `display` (default, comfy-table output), `python` (PyO3 bindings), `wasm` (engine WASM target)
- The `engine` crate patches `crossterm` via a local vendored crate at `engine/crates/crossterm/`
