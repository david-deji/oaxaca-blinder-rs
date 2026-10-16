# Architecture

The repository is a Rust workspace with three crates.

| Crate | Directory | Depends on | Ships as |
|---|---|---|---|
| `oaxaca_blinder` | `oaxaca_blinder/` | none | crates.io: library and `oaxaca-cli` |
| `pay-equity-engine` | `engine/` | `oaxaca_blinder` | WASM package |
| `meridian-mcp` | `meridian-mcp/` | `pay-equity-engine` | release binaries |

## `oaxaca_blinder`: the library and the command line

The statistics. It works on Polars `DataFrame`s with linear algebra in Nalgebra. It is the only crate published to
crates.io.

- `builder.rs`: `OaxacaBuilder`, the entry point, and the cleaning of the table into design matrices.
- `decomposition.rs`: the two-fold and three-fold arithmetic and the reference schemes.
- `inference.rs`, `rng.rs`: the bootstrap, its seeded per-replicate streams and its percentile intervals.
- `math/`: ordinary and weighted least squares, quantile regression, the RIF, probit, categorical normalisation,
  diagnostics and weights.
- `heckman.rs`: the selection correction.
- `quantile_decomposition.rs`: Machado-Mata simulation (deprecated).
- `akm.rs`, `dfl.rs`, `jmp.rs`, `matching/`: the experimental methods, behind the `experimental` feature.
- `main.rs`: the `oaxaca-cli` binary.

## `pay-equity-engine`: the engine (directory `engine/`)

The analysis layer on top of the library. It is not published to crates.io: its contract is the JSON request and
response the WASM package uses.

- `analysis.rs`: the decomposition driver, the remedy, and the efficient frontier.
- `defensibility.rs`: scores proposed adjustments against the reference group's pay line.
- `support.rs`: the support block and the warnings.
- `types.rs`: the request and response structs shared by the WASM and MCP surfaces.
- The `wasm` feature exposes `decompose`, `optimize`, `verify_adjustments`, `calculate_efficient_frontier` and
  `check_defensibility` to the browser through `wasm-bindgen`.

The remedy uses no solver. Amounts are a sort-and-cap over each employee's shortfall to the pay line, and
`target_gap` is a rule for the budget along the chosen strategy's order.

## `meridian-mcp`: the MCP server

A JSON-RPC server over standard input and output, or HTTP, that exposes the engine as MCP tools. It is run, not
depended on, and is distributed as release binaries.

## Data flow

1. **Input**: a CSV or a `DataFrame` is cleaned: rows with blanks in model columns are set aside and counted.
2. **Setup**: the builder prepares the design matrices for the two groups.
3. **Estimation**: least squares by Cholesky, or quantile regression for the RIF.
4. **Decomposition**: the gap is split into explained and unexplained parts, in total and by predictor.
5. **Inference**: the bootstrap repeats steps 2 to 4 on resamples, in parallel with Rayon, each replicate on its own
   seeded stream.
6. **Output**: structs for the library, tables for the command line, JSON for the engine surfaces.

## Where the numbers are held

`verification/` generates the reference values with R and Python. The tests in each crate read the committed values.
See [Reproducing results in R](../reproducing/r.md).

## Tech stack

Rust 2021, Polars for tables, Nalgebra and Statrs for numerics, Clarabel for quantile regression, Rayon for the
bootstrap, Clap for the command line, `wasm-bindgen` for the browser build, Axum for MCP over HTTP.
