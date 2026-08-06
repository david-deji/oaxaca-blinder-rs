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

# Build engine with WASM target (raw .wasm only — no JS glue, nothing shipped)
cargo build -p pay-equity-engine --features wasm --target wasm32-unknown-unknown

# Ship an engine change to the Meridian app — THIS is the command you want
bash scripts/build-wasm.sh                # build both artifacts + publish into the app
bash scripts/build-wasm.sh --no-publish   # build only (CI baseline recording)
```

## Shipping an engine change to Meridian

`cargo build --target wasm32-unknown-unknown` produces a raw `.wasm` and stops. It runs no
wasm-bindgen, writes no JS glue, and touches nothing the browser loads. **A green `cargo test`
plus a green cargo wasm build still means the app is running the previous engine.**

`scripts/build-wasm.sh` is the whole path: it builds both artifacts (sequential-stable and
threaded-nightly), records their raw sha256 baselines, runs wasm-bindgen, and then **publishes**
the results into the consuming app — by default the sibling checkout at
`../pay-equity-app/frontend/src/{wasm,wasm-threaded}/`. Override the destination with
`MERIDIAN_FRONTEND=/path/to/frontend/src`; the step skips with a notice (not an error) when no
app checkout is found, so this repo stays usable standalone.

Every published file is sha256-verified against its source and the script exits non-zero on a
mismatch. The copy is file-by-file, never a directory sync — `frontend/src/wasm/` also holds
frontend-owned `analysis.worker.js`, `thread-cap.js`, and `.gitignore`, which a sync would delete.

WHY this is automated rather than written down as a manual step: the app's test suites **mock the
engine**, so nothing on either side fails when the shipped blob is stale. Before 0017-P4 the copy
was manual and undocumented, and the app's blobs sat hours behind engine source with every suite
green. Do not reintroduce a manual step between an engine change and the binary that ships.

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
