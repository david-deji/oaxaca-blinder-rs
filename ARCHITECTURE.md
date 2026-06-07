# Project Architecture

## Key Components

The project is a Rust workspace with **three** member crates:

### 1. `oaxaca_blinder` — Core Decomposition Library

The statistical engine implementing econometric decomposition methods for pay-equity analysis.
Operates on **Polars DataFrames** with linear algebra via **Nalgebra**.

- **Purpose**: Decompose mean wage gaps (or other outcome differentials) between two groups into
  an *explained* part (differences in observable characteristics) and an *unexplained* part
  (differences in the returns to those characteristics).
- **Decomposition modules**:
    - `decomposition.rs`: Standard Oaxaca-Blinder (two-fold and three-fold).
    - `quantile_decomposition.rs`: RIF-Regression quantile decomposition (Firpo-Fortin-Lemieux).
    - `jmp.rs`: Juhn-Murphy-Pierce decomposition.
    - `dfl.rs`: DiNardo-Fortin-Lemieux reweighting.
    - `akm.rs`: Abowd-Kramarz-Margolis high-dimensional fixed effects.
    - `heckman.rs`: Heckman two-step selection correction.
    - `matching/`: Propensity-score matching (logistic model, distance metrics, matching engine).
- **Math utilities** (`math/`): OLS, quantile regression, KDE, RIF, probit, logit, diagnostics,
  coefficient normalization.
- **Entry points**: `OaxacaBuilder` and `QuantileDecompositionBuilder` (builder pattern).
- **Interfaces**:
    - **CLI**: `src/main.rs` exposes the `oaxaca-cli` binary.
    - **Python**: `python.rs` holds PyO3 bindings behind a `python` feature flag. The flag is
      currently **disabled** (commented out in `Cargo.toml`); the module does not compile in the
      default build.

### 2. `pay-equity-engine` (directory: `engine/`) — Optimization, Verification & WASM

Wraps `oaxaca_blinder` with the analysis layer the Meridian app consumes.

- **Purpose**: Budget-constrained wage-adjustment optimization, adjustment verification, efficient
  frontier calculation, and defensibility scoring.
- **Key modules**:
    - `analysis.rs`: decomposition driver, budget optimizer, and efficient-frontier calculation.
    - `defensibility.rs`: per-adjustment defensibility scoring against the reference-group standard.
    - `types.rs`: request/response structs shared across the WASM and MCP surfaces.
    - `access.rs`: partner offline-access codes, gated behind the off-by-default `partner-access`
      feature (not part of the default or standard WASM build).
- **WASM target**: the `wasm` feature exposes `decompose`, `optimize`, `verify_adjustments`,
  `calculate_efficient_frontier`, and `check_defensibility` to the browser via `wasm-bindgen`.

### 3. `meridian-mcp` — MCP Server

A JSON-RPC server (stdio, or SSE/HTTP via Axum) exposing the engine functions as MCP tools:
`decompose`, `optimize`, `verify_adjustments`, `calculate_efficient_frontier`,
`check_defensibility`. Configurable via CLI args or env vars (`PORT`, `MCP_TRANSPORT`, `MCP_API_KEY`).

## Data Flow

1.  **Input**: Data is ingested as **Polars DataFrames** (from CSV bytes at the WASM/MCP boundary).
2.  **Decomposition** (`oaxaca_blinder`): linear algebra via **Nalgebra**; bootstrap replications
    parallelized with **Rayon**.
3.  **Optimization** (`pay-equity-engine`): budget-constrained wage adjustments. Fair-wage standards
    are solved with direct linear algebra (OLS/SVD via Nalgebra); convex optimization is available
    via **Clarabel**.
4.  **Output**: results are returned as Rust structs (CLI / MCP), as `JsValue` across the WASM
    boundary, or printed to stdout (CLI).

## Tech Stack

-   **Language**: Rust (Edition 2021)
-   **Data Processing**: `polars` (lazy evaluation, high performance)
-   **Math / Stats**:
    -   `nalgebra`: linear algebra (`DMatrix`/`DVector`).
    -   `statrs`: statistical distributions.
    -   `clarabel`: convex optimization solver.
-   **Parallelism**: `rayon` (bootstrap iterations).
-   **CLI**: `clap`.
-   **WASM**: `wasm-bindgen` (+ `serde-wasm-bindgen`), behind the `wasm` feature.
-   **MCP transport**: `axum` (SSE/HTTP) or stdio.

> Monetary values are represented with fixed-point precision, never `Float64`, per the
> comp-audit-suite rule (see `CLAUDE.md`).
