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
bash scripts/build-wasm.sh                # build both artifacts + publish into the app (+ manifest); writes NO baseline
bash scripts/build-wasm.sh --no-publish   # build only
bash scripts/build-wasm.sh --record       # the only mode that writes engine/*.sha256 (then commit them)
bash scripts/build-wasm.sh --verify       # build raw blobs, compare with HEAD's baselines; writes/ships nothing, exit 1 on mismatch
```

## Shipping an engine change to Meridian

`cargo build --target wasm32-unknown-unknown` produces a raw `.wasm` and stops. It runs no
wasm-bindgen, writes no JS glue, and touches nothing the browser loads. **A green `cargo test`
plus a green cargo wasm build still means the app is running the previous engine.**

`scripts/build-wasm.sh` is the whole path: it builds both artifacts (sequential-stable and
threaded-nightly), runs wasm-bindgen, and then **publishes**
the results into the consuming app, with an `engine-manifest.json` beside each blob (raw sha256, shipped
`_bg.wasm` sha256, engine commit, dirty flag). Baselines are written only by `--record` — by default the sibling checkout at
`../pay-equity-app/frontend/src/{wasm,wasm-threaded}/`. Override the destination with
`MERIDIAN_FRONTEND=/path/to/frontend/src`; the step skips with a notice (not an error) when no
app checkout is found, so this repo stays usable standalone.

Every published file is sha256-verified against its source and the script exits non-zero on a
mismatch. wasm-bindgen output is not deterministic (two runs over one raw blob differ in ~120 bytes), so
the shipped blob is checked against its manifest, never against a committed hash; the app's
`scripts/lib/wasm-freshness.mjs` does that and compares the manifest with the engine source (no mtimes). The copy is file-by-file, never a directory sync — `frontend/src/wasm/` also holds
frontend-owned `analysis.worker.js`, `thread-cap.js`, and `.gitignore`, which a sync would delete.

WHY this is automated rather than written down as a manual step: the app's test suites **mock the
engine**, so nothing on either side fails when the shipped blob is stale. Before 0017-P4 the copy
was manual and undocumented, and the app's blobs sat hours behind engine source with every suite
green. Do not reintroduce a manual step between an engine change and the binary that ships.

### Order of operations: engine merge, republish, app commit, verify-live

The app's freshness gate reads `engine_commit` from each `engine-manifest.json` and looks that commit up in
the engine checkout. A branch commit that a squash-merge leaves out of `main` makes the gate red in every
fresh clone until the blobs are republished. So, in this order:

1. Merge the engine PR into `main` with a **merge commit or a rebase merge, never a squash** (or squash, and
   then do step 2 from the merged `main` commit; the manifests must name a commit that exists on `main`).
2. From a clean `main` checkout: `bash scripts/build-wasm.sh`. It writes the blobs and both manifests into the app.
3. In the app: `git add -f frontend/src/wasm/engine-manifest.json` together with the blobs (the directory's own
   `.gitignore` is `*`, so a plain `git add` skips the sequential manifest). Check with
   `git ls-files frontend/src/wasm | grep manifest`. Commit the app.
4. `bash scripts/verify-live.sh <epic>` from a clean engine tree. Its `app_tree_committed` check fails while
   the app tree is dirty or a published manifest is untracked, so a `pass` receipt describes a commit.

Receipts and probes under `ground/` are committed to this **public** repository: counts, flags and hashes only,
no absolute paths, no app file names, no git status lines. The scripts scrub them; do not hand-add either.

## CI gate and audit triage

`gate` is the one required check (S5). It fails unless every gating job is `success`, and it reads the finished
run through the jobs API (`scripts/gate-step-audit.py`): a successful job with a skipped step is red.
`scripts/check-ci-invariants.py` refuses what would switch a check off (job- or step-level `if` and
`continue-on-error`, `|| true`, `set +e`, `exit 0`, `shell:` overrides, widened permissions, unpinned actions,
an edited `gate`); `scripts/test-ci-invariants.py` runs it against mutated copies. A step-level `if` needs an
entry in `ALLOWED_STEP_IFS`, which is a reviewed change.

Security Audit: `cargo audit --deny warnings` on `main`, the Monday cron and manual runs; a pull request fails
only on vulnerabilities, so a new unmaintained notice cannot block an unrelated PR. A red audit is triaged by
the steps at the top of `.cargo/audit.toml`. `scripts/validate-audit-ignores.py` also holds minimum locked
versions for crates whose advisories exist only as GHSA (xxhash-rust >= 0.8.16), which cargo-audit cannot see.

## Repo location and `target/` growth

Real bytes live at `~/hr-apps/oaxaca-blinder-rs` (NVMe). `apps/hr-apps/oaxaca-blinder-rs` in the
telos-machina monorepo is a **symlink** to it — kept there because `scripts/repo-sync.sh:169`
requires every `repos.yaml` `local_path` to be relative and inside the monorepo. Do not "fix" the
symlink. `pay-equity-app` is relocated the same way and **must stay a sibling**: `build-wasm.sh`
publishes to `$PWD/../pay-equity-app/frontend/src`, and the app's `wasmFreshnessScope.spec.js`
resolves `../../oaxaca-blinder-rs`.

**Watch `target/`.** On 2026-08-28 it had reached **214 GB** — 99.97% of the checkout — from
accumulated native + `wasm32-unknown-unknown` + threaded-nightly (`-Zbuild-std`) + `--all-features`
variants. It was cleaned during the relocation and regenerates on first build. Nothing warns when it
grows; `du -sh target` before a long session is cheap, and `cargo clean` is safe (cargo writes its
own `CACHEDIR.TAG` there marking the directory disposable).

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
