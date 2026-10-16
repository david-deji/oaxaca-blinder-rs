# CLAUDE.md

Notes for coding agents (Claude Code and similar) working in this repository. People should start with
`README.md` and `CONTRIBUTING.md`.

## Build and test

```bash
cargo build                                  # whole workspace
cargo test                                   # all tests
cargo test -p oaxaca_blinder                 # one crate (also: -p pay-equity-engine, -p meridian-mcp)
cargo test -p oaxaca_blinder test_name       # one test by name
cargo build --bin oaxaca-cli                 # the CLI

# Raw engine WASM only: no JS glue, nothing shipped
cargo build -p pay-equity-engine --features wasm --target wasm32-unknown-unknown

# Ship an engine change to the Meridian app: THIS is the command you want
bash scripts/build-wasm.sh                # build both artifacts, publish into the app (+ manifest); writes NO baseline
bash scripts/build-wasm.sh --no-publish   # build only
bash scripts/build-wasm.sh --record       # the only mode that writes engine/*.sha256 (then commit them)
bash scripts/build-wasm.sh --verify       # build raw blobs, compare with HEAD's baselines; writes/ships nothing, exit 1 on mismatch
```

Before pushing, run what CI runs: `cargo fmt --all -- --check`,
`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`,
`cargo test --locked --workspace --all-features`, `python3 scripts/check-ci-invariants.py`,
`python3 scripts/test-ci-invariants.py`.

## Shipping an engine change to Meridian

`cargo build --target wasm32-unknown-unknown` produces a raw `.wasm` and stops. It runs no wasm-bindgen, writes no
JS glue, and touches nothing the browser loads. **A green `cargo test` plus a green cargo wasm build still means the
app is running the previous engine.**

`scripts/build-wasm.sh` is the whole path: it builds both artifacts (sequential on stable, threaded on the pinned
nightly), runs wasm-bindgen, and then **publishes** the results into the consuming app, with an
`engine-manifest.json` beside each blob (raw sha256, shipped `_bg.wasm` sha256, engine commit, dirty flag).
Baselines are written only by `--record`. The destination is the sibling checkout
`../pay-equity-app/frontend/src/{wasm,wasm-threaded}/`; override it with `MERIDIAN_FRONTEND=/path/to/frontend/src`.
The step skips with a notice (not an error) when no app checkout is found, so this repository stays usable
standalone.

Every published file is sha256-verified against its source and the script exits non-zero on a mismatch.
wasm-bindgen output is not deterministic (two runs over one raw blob differ in about 120 bytes), so the shipped blob
is checked against its manifest, never against a committed hash. The copy is file-by-file, never a directory sync:
`frontend/src/wasm/` also holds frontend-owned `analysis.worker.js`, `thread-cap.js` and `.gitignore`, which a sync
would delete.

The app's test suites mock the engine, so nothing on either side fails when the shipped blob is stale. Do not
reintroduce a manual step between an engine change and the binary that ships.

### Order of operations: engine merge, republish, app commit, verify-live

The app's freshness gate reads `engine_commit` from each `engine-manifest.json` and looks that commit up in the
engine checkout. A branch commit that a squash-merge leaves out of `main` makes the gate red in every fresh clone
until the blobs are republished. So, in this order:

1. Merge the engine PR into `main`.
2. From a clean `main` checkout of the merged commit: `bash scripts/build-wasm.sh`. It writes the blobs and both
   manifests into the app.
3. In the app: `git add -f frontend/src/wasm/engine-manifest.json` together with the blobs (the directory's own
   `.gitignore` is `*`, so a plain `git add` skips the sequential manifest). Check with
   `git ls-files frontend/src/wasm | grep manifest`. Commit the app.
4. `bash scripts/verify-live.sh <epic>` from a clean engine tree. Its `app_tree_committed` check fails while the app
   tree is dirty or a published manifest is untracked, so a `pass` receipt describes a commit.

Receipts and probes under `records/ground/` are committed to this **public** repository: counts, flags and hashes
only, no absolute paths, no app file names, no git status lines. The scripts scrub them; do not hand-add either.
`records/` holds history, not documentation (see `records/README.md`).

## CI gate and audit triage

`gate` is the one required check. It fails unless every gating job is `success`, and it reads the finished run
through the jobs API (`scripts/gate-step-audit.py`): a successful job with a skipped step is red.
`scripts/check-ci-invariants.py` refuses what would switch a check off (job- or step-level `if` and
`continue-on-error`, `|| true`, `set +e`, `exit 0`, `shell:` overrides, widened permissions, unpinned actions, an
edited `gate`); `scripts/test-ci-invariants.py` runs it against mutated copies. A step-level `if` needs an entry in
`ALLOWED_STEP_IFS`, which is a reviewed change.

Security Audit: `cargo audit --deny warnings` on `main`, the Monday cron and manual runs; a pull request fails only
on vulnerabilities, so a new unmaintained notice cannot block an unrelated PR. A red audit is triaged by the steps at
the top of `.cargo/audit.toml`: take a fixed version with `cargo update -p <crate> --precise <fixed>` first; an
ignore is for a crate that is not compiled in or has no fix, and never a way to get past a red check.
`scripts/validate-audit-ignores.py` also holds minimum locked versions for crates whose advisories exist only as
GHSA (xxhash-rust >= 0.8.16), which cargo-audit cannot see.

The library must compile on Rust 1.88 (`rust-version` in `Cargo.toml`, job `msrv`). The repository itself builds on
the toolchain pinned in `rust-toolchain.toml` (1.90.0) so the WASM blobs are reproducible.

## `target/` growth

`target/` grows without warning: native, `wasm32-unknown-unknown`, threaded-nightly (`-Zbuild-std`) and
`--all-features` builds each add to it, and it has reached hundreds of GB. `du -sh target` before a long session is
cheap, and `cargo clean` is safe.

## Workspace

A Rust workspace with three crates.

### `oaxaca_blinder`: core decomposition library
The statistical engine for econometric decomposition, on Polars DataFrames with Nalgebra linear algebra.

**Decomposition methods** (each has its own module):
- `decomposition.rs`: standard Oaxaca-Blinder (two-fold and three-fold)
- `quantile_decomposition.rs`: RIF-regression quantile decomposition (Firpo-Fortin-Lemieux)
- `jmp.rs`: Juhn-Murphy-Pierce decomposition
- `dfl.rs`: DiNardo-Fortin-Lemieux reweighting
- `akm.rs`: Abowd-Kramarz-Margolis high-dimensional fixed effects
- `heckman.rs`: Heckman two-step selection correction
- `matching/`: propensity score matching (logistic model, distance metrics, matching engine)

**Math utilities** (`math/`): OLS regression, quantile regression, KDE, RIF, probit, logit, diagnostics, normalization.

**Entry points**: `OaxacaBuilder` and `QuantileDecompositionBuilder` (builder pattern). The CLI is the `oaxaca-cli`
binary.

### `engine` (`pay-equity-engine`): optimisation and verification
Wraps `oaxaca_blinder` with the remedy optimiser (budget-constrained wage adjustments), verification, the efficient
frontier and the defensibility check. It has a WASM target (`wasm` feature) for browser use. Key modules:
`analysis.rs`, `defensibility.rs`, `types.rs`. Not published to crates.io (`publish = false`): its contract is the
JSON request and response.

### `meridian-mcp`: MCP server
JSON-RPC server (stdio, or SSE/HTTP via Axum) exposing engine functions as MCP tools: `decompose`, `optimize`,
`verify_adjustments`, `calculate_efficient_frontier`, `check_defensibility`. Configurable by CLI arguments or
environment variables (`PORT`, `MCP_TRANSPORT`, `MCP_API_KEY`). Not published to crates.io; released as binaries.

## Key patterns

- **Builder pattern** for all analysis entry points (`OaxacaBuilder::new(...).predictors(...).run()`).
- **Polars DataFrames** as the interchange format; never raw Vec/arrays.
- **Nalgebra** `DMatrix`/`DVector` for all linear algebra; `clarabel` for the library's quantile regression only (the
  remedy, `optimize`, uses no solver).
- **Rayon** for parallel bootstrap iterations.
- **`f64` inside the estimators** (OLS, RIF, quantile, bootstrap). Monetary amounts round to cents only at display
  and ledger boundaries, never inside the estimator.
- Feature flags: `display` (default, comfy-table output), `wasm` and `wasm-threads` (engine WASM targets),
  `mem-profile` (native memory-profiling harness, off by default).
- The workspace patches `crossterm` with a local crate at `engine/crates/crossterm/`.
- Oracle scripts under `verification/` are never edited: each golden records the hash of its generator
  (`verification/README.md`).
