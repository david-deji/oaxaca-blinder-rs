# Reachability & Documentation Audit — oaxaca-blinder-rs

Scope: which estimator capabilities are reachable from a shipped surface (WASM, CLI, MCP, Python bindings) vs. only reachable via direct crate API; whether READMEs/ARCHITECTURE.md describe the code that exists.

Audit method: read source files in the order given, grep for call sites establishing reachability, quote claims against code. Findings appended as found, not batched.

Budget note: this is a read-heavy audit (no edits). Tool-call budget tracked; will stop and mark partial if half-spent before completion.

---

## Working notes (raw findings, organized into final answers at the end)

### lib.rs (oaxaca_blinder/src/lib.rs, 140 lines)

- `lib.rs:35-37` — module doc states explicitly: "The shipped quantile-decomposition path (CLI, WASM, MCP) is RIF-regression (Firpo-Fortin-Lemieux 2009) via [`OaxacaBuilder::decompose_quantile`] — call it once per target quantile:"
- `lib.rs:61-63` — module doc on the `quantile_decomposition` module (Machado-Mata): "[`crate::quantile_decomposition`] (Machado-Mata simulation) is a statistically distinct alternative kept for direct-API consumers, but it is off the shipped surface and has no external-oracle verification — see that module's doc for details."
  - This is a strong, load-bearing, self-declared admission in lib.rs itself: Machado-Mata quantile decomposition is (a) not on any shipped surface, (b) unverified against an external oracle. Need to confirm both claims against actual call sites and against `quantile_decomposition.rs`'s own module doc (Q2).
- `lib.rs:65-81` — module declarations. Private (not `pub`): `builder`, `decomposition`, `display`, `error`, `estimation`, `inference`, `math`, `rng`, `types`. Public (`pub mod`): `akm`, `dfl`, `formula`, `heckman`, `jmp`, `matching`, `quantile_decomposition`.
  - Note `builder` module is NOT `pub mod` — but `OaxacaBuilder` itself is re-exported at `lib.rs:102` (`pub use builder::OaxacaBuilder;`), so the two-fold/three-fold Oaxaca-Blinder API is reachable via the public re-export, not the module path.
- `lib.rs:101-112` — public re-exports (crate API surface):
  - `pub use akm::{AkmBuilder, AkmResult};`
  - `pub use builder::OaxacaBuilder;`
  - `pub use decomposition::{BudgetAdjustment, ReferenceCoefficients};`
  - `pub use dfl::run_dfl;`
  - `pub use error::OaxacaError;`
  - `pub use heckman::heckman_two_step;`
  - `pub use jmp::decompose_changes;`
  - `pub use matching::engine::MatchingEngine;`
  - `#[allow(deprecated)] pub use quantile_decomposition::QuantileDecompositionBuilder;` — note the `#[allow(deprecated)]` attribute itself signals `QuantileDecompositionBuilder` (Machado-Mata) is marked `#[deprecated]` somewhere in its own module. To confirm in Q2.
  - `pub use rng::{RunMetadata, DEFAULT_SEED};`
  - `pub use types::{ComponentResult, DecompositionDetail, OaxacaResults, TwoFoldResults};`
- `lib.rs:114-131` — `qr_coefficients()` free function, doc comment: "Quantile-regression coefficient solver, exposed for the statistical-trust-layer QR validation (0014-MERIDIAN AC-5). Thin, allocation-only wrapper over the internal `math::quantile_regression::solve_qr`" — this is a crate-API-only export whose stated purpose is a validation/verification hook, not a decomposition capability itself.
- `lib.rs:86-96` — `mem_profile` module and global allocator are gated behind `#[cfg(feature = "mem-profile")]`, explicitly "Never compiled into the production/wasm path" (line 84-85 comment). Not a decomposition capability; noted for completeness of module inventory.
- `lib.rs:98-99` — commented-out: `// #[cfg(feature = "python")]` / `// pub mod python;`. **Python bindings module is present in source but commented out — not compiled at all currently.** This is a major reachability finding for Q1 (Python bindings claim).

### Reachability map (repo-wide grep, confirms/extends lib.rs claims)

Workspace = 3 crates: `oaxaca_blinder` (the library + `oaxaca-cli` bin), `engine` (crate name `pay-equity-engine`, the WASM target — `engine/Cargo.toml:11` `oaxaca_blinder = { path = "../oaxaca_blinder" }`), `meridian-mcp` (the MCP server — `meridian-mcp/Cargo.toml` depends on `pay-equity-engine = { path = "../engine" }`, **not** on `oaxaca_blinder` directly). So MCP reachability is transitive: MCP → engine → oaxaca_blinder.

- **Python bindings**: `oaxaca_blinder/src/python.rs` exists (432 lines, uses `pyo3::prelude::*`, wraps `run_dfl`, `MatchingEngine`, `heckman_selection`, core `OaxacaBuilder`). It is **not** declared as a module anywhere (`lib.rs:98-99` is commented out) and **`pyo3` is not a dependency in any workspace `Cargo.toml`** (confirmed: `grep -rn pyo3 --include=Cargo.toml .` returns zero hits in `oaxaca_blinder/Cargo.toml`, `engine/Cargo.toml`, `meridian-mcp/Cargo.toml`, root `Cargo.toml`). The file would not compile if included. **Python bindings do not exist as a buildable, let alone shipped, surface.**
- **`engine/src/*.rs`** (the WASM crate) imports from `oaxaca_blinder` only: `use oaxaca_blinder::RunMetadata;` (`engine/src/types.rs:3`), `use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};` (`engine/src/defensibility.rs:3`, `engine/src/analysis.rs:3`). Confirmed by `grep -rn "oaxaca_blinder" engine/src/*.rs` — no other import line. RIF quantile reachability: `engine/src/analysis.rs:184-207`, comment "Route through OaxacaBuilder::decompose_quantile (one-stage RIF-OLS...)" and call `let results = builder.decompose_quantile(q).map_err(...)?;` (`engine/src/analysis.rs:207`).
- **`meridian-mcp/src/main.rs`** exposes exactly 5 MCP tools (`tools/list` handler, `meridian-mcp/src/main.rs:543,563,582,608,635`): `forensic_decomposition`, `simulate_remediation`, `verify_adjustments`, `check_defensibility`, `generate_efficient_frontier`. `handle_tool_call` (`meridian-mcp/src/main.rs:699-798`) routes each to `engine::{decompose_inner, optimize_inner, verify_inner, check_defensibility_inner, calculate_efficient_frontier_inner}` — all in `engine/src/analysis.rs` / `engine/src/defensibility.rs`, which (per above) only ever construct `OaxacaBuilder` (mean two-fold/three-fold + RIF quantile via the `quantile: Option<f64>` field on `DecompositionRequest`, `engine/src/types.rs:29` comment `// For RIF Regression`). **No MCP tool reaches AKM, Heckman, JMP, DFL, Machado-Mata quantile, or matching.**
- **`oaxaca_blinder/src/main.rs`** (the `oaxaca-cli` bin) has `AnalysisType::{Mean, Quantile, Akm, Match}` (`main.rs:28-32`) and dispatches at `main.rs:163-169`. Confirmed call sites:
  - Mean → `OaxacaBuilder::new(...).run()` (`main.rs:185-193, 202`), including `builder.heckman_selection(sel_outcome, ...)` at `main.rs:209` when `--selection-outcome`/`--selection-predictors` are passed.
  - Quantile → `builder.decompose_quantile(q)` (`main.rs:266`), **explicitly documented as RIF, not Machado-Mata**: `main.rs:221-224` comment: "RIF-regression quantile decomposition (0014-MERIDIAN ruling a-1): the CLI uses the SAME OaxacaBuilder::decompose_quantile path as the WASM/MCP surface — one coherent method everywhere, no CLI-vs-browser divergence (AC-13). QuantileDecompositionBuilder (MM simulation) stays exported for direct-API consumers but is no longer the CLI default; `--simulations` is therefore inert on this path (kept for backward CLI compatibility)." — **the CLI still exposes a `--simulations` flag (`main.rs:100-101`, doc: "The number of simulations for the Machado-Mata algorithm (for quantile analysis)") that the code's own comment calls "inert."** This is a live UX-level mismatch between an arg's help text and its actual effect — flagged for Q3.
  - Akm → `AkmBuilder::new(...).controls(...).run()` (`main.rs:301-311`).
  - Match → `MatchingEngine::new(...)` + `.match_psm(...)` or `.run_matching(...)` (`main.rs:333-346`), method chosen by `--matching-method` (`euclidean|mahalanobis|psm`, `main.rs:127`).
  - **No CLI subcommand/flag reaches JMP (`decompose_changes`), DFL (`run_dfl`), or Machado-Mata quantile (`QuantileDecompositionBuilder`).**
- **`run_dfl`** (DFL): defined `oaxaca_blinder/src/dfl.rs:34`; call sites are `dfl.rs:232` (its own `#[test]`), `python.rs:336` (dead — python.rs not compiled), and `oaxaca_blinder/tests/features_test.rs:76` (integration test). Re-exported `pub use dfl::run_dfl;` (`lib.rs:104`). **No CLI, WASM/engine, or MCP call site found. Reachable only via direct crate API (`oaxaca_blinder::run_dfl`) or by running the test suite.**
- **`decompose_changes`** (JMP): defined `oaxaca_blinder/src/jmp.rs:44`; only call site outside its own module is `oaxaca_blinder/tests/features_test.rs:61` (`let jmp_results = decompose_changes(&builder_t1, &builder_t2).expect("JMP failed");`). Re-exported `pub use jmp::decompose_changes;` (`lib.rs:107`). Not referenced in `python.rs`, CLI, engine, or MCP. **No shipped-surface call site found. Reachable only via direct crate API or tests.**
- **`heckman_two_step`**: defined `oaxaca_blinder/src/heckman.rs:38`; called internally by `oaxaca_blinder/src/estimation.rs:127-128` (`res_a = heckman_two_step(...)`, `res_b = heckman_two_step(...)`), which is invoked by `OaxacaBuilder`'s Heckman-selection code path (`builder.rs:406` defines `pub fn heckman_selection`). Reachable from the **CLI** via `--selection-outcome`/`--selection-predictors` (`main.rs:203-212`, `builder.heckman_selection(sel_outcome, sel_preds_refs.iter().copied())` at `main.rs:209`). **Not reachable from engine/WASM or MCP** — confirmed via `grep -rn -i "heckman\|selection" engine/src/*.rs meridian-mcp/src/*.rs`, zero hits referencing Heckman/selection logic (only unrelated "selected as a continuous variable" error strings matched). Also called from `python.rs:307` (dead, not compiled) and `oaxaca_blinder/tests/heckman_test.rs:61`.
- **`QuantileDecompositionBuilder`** (Machado-Mata): defined `oaxaca_blinder/src/quantile_decomposition.rs:50`, re-exported with `#[allow(deprecated)] pub use quantile_decomposition::QuantileDecompositionBuilder;` (`lib.rs:110`) — the `#[allow(deprecated)]` on the re-export confirms the item itself carries a `#[deprecated]` attribute in its own module (to verify by reading `quantile_decomposition.rs` directly, next). Call sites: only `oaxaca_blinder/tests/ground_truth_verification_test.rs:107` and `oaxaca_blinder/tests/integration_test.rs:243`, both files opening with `#![allow(deprecated)] // QuantileDecompositionBuilder (MM sim) is deprecated but kept as a ...`. **No CLI, engine/WASM, or MCP call site. Confirms lib.rs's own claim (`lib.rs:61-63`) that it is "off the shipped surface."**
- **`MatchingEngine`**: defined `oaxaca_blinder/src/matching/engine.rs:10`, re-exported `pub use matching::engine::MatchingEngine;` (`lib.rs:108`). Reachable from the **CLI** (`main.rs:334-346`, `AnalysisType::Match`). Also referenced in `python.rs:395-409` (dead) and `oaxaca_blinder/tests/matching_test.rs`. **Not referenced anywhere in `engine/src/*.rs` or `meridian-mcp/src/*.rs`** (confirmed by grep) — so matching is CLI-reachable but not WASM/MCP-reachable.

**Interim Q1 reachability table (pending final read of quantile_decomposition.rs / akm.rs / heckman.rs / jmp.rs / dfl.rs for the module-level self-assessment notes):**

| Estimator | WASM (engine) | CLI | MCP | Python | Direct crate API only |
|---|---|---|---|---|---|
| Oaxaca-Blinder two-fold/three-fold (mean) | Yes — `engine/src/analysis.rs:3` `OaxacaBuilder` | Yes — `main.rs:185-202` | Yes — `forensic_decomposition` etc. via `engine::decompose_inner` | No (python.rs not compiled) | — |
| RIF quantile (`decompose_quantile`) | Yes — `engine/src/analysis.rs:207` | Yes — `main.rs:266` | Yes — `DecompositionRequest.quantile` field, `engine/src/types.rs:29` | No | — |
| Machado-Mata quantile (`QuantileDecompositionBuilder`) | No | No | No | No (dead code even in python.rs? — not found in earlier grep of python.rs for "quantile"; to confirm) | **Yes — tests + direct API only** |
| AKM (`AkmBuilder`) | No (no `Akm` reference in `engine/src/*.rs`) | Yes — `main.rs:301-311` | No | No | Reachable via CLI + direct API; not WASM/MCP |
| Heckman (`heckman_two_step` via `builder.heckman_selection`) | No | Yes — `main.rs:209` (opt-in flags) | No | No | Reachable via CLI + direct API; not WASM/MCP |
| JMP (`decompose_changes`) | No | No | No | No | **Yes — tests + direct API only** |
| DFL (`run_dfl`) | No | No | No | No | **Yes — tests + direct API only** |
| Matching (`MatchingEngine`) | No | Yes — `main.rs:334-346` | No | No | Reachable via CLI + direct API; not WASM/MCP |

(AKM absence from WASM/MCP still needs confirmation by grep specifically for "Akm"/"akm" in engine/src, done above — zero hits. Will re-verify once akm.rs itself is read for any self-declared status note.)

### quantile_decomposition.rs (615 lines) — Q2 primary source

Module-level doc, `oaxaca_blinder/src/quantile_decomposition.rs:1-19`, verbatim (full block quoted — this IS the self-assessment note the audit asks for):

> ```
> //! Machado-Mata Quantile Regression Decomposition
> //!
> //! This module provides the implementation for performing a quantile regression
> //! decomposition using the Machado-Mata (2005) simulation-based method.
> //!
> //! MM is a genuinely distinct method from the RIF-regression quantile decomposition in
> //! [`crate::OaxacaBuilder::decompose_quantile`] — MM simulates the full conditional
> //! quantile process from repeated quantile regressions at random taus, while RIF linearizes
> //! the quantile via a single recentered-influence-function OLS pass. It is NOT redundant
> //! with the RIF path; it is a different estimator with different bias/variance tradeoffs
> //! (heavier compute, no linearization assumption).
> //!
> //! **Status (0014-MERIDIAN follow-up):** no shipped surface (CLI, WASM, MCP) calls this
> //! module — `main.rs::run_quantile_analysis` and the WASM/MCP paths use `decompose_quantile`
> //! (RIF) exclusively. This module is reachable only via direct use of the `oaxaca_blinder`
> //! crate API. Unlike the RIF path, MM has no external-oracle verification (e.g. an R
> //! `quantreg`/`rifreg` cross-check) — only the self-consistency checks in
> //! `tests/ground_truth_verification_test.rs` and `tests/integration_test.rs` (adding-up:
> //! characteristics + coefficients == total gap). Treat point estimates from this module as
> //! unverified against an external ground truth.
> ```

Plus the struct-level doc immediately above the `#[deprecated(...)]` attribute (`quantile_decomposition.rs:36-40`):

> "Off the shipped surface (CLI/WASM/MCP use the RIF path); see the module-level doc for why this method is statistically distinct rather than redundant, and its verification status (self-consistency only, no external oracle)."

And the `#[deprecated(note = "...")]` attribute itself (`quantile_decomposition.rs:41-46`):

> "not on the shipped CLI/WASM/MCP surface — those use OaxacaBuilder::decompose_quantile (RIF, Firpo-Fortin-Lemieux). MM simulation is a distinct, statistically valid method kept for direct-API consumers, but has no external-oracle verification (self-consistency tests only). See the module doc for details."

This is a three-layer, internally consistent self-declaration: module doc, struct doc, and compiler-enforced `#[deprecated]` note all say the same thing (off shipped surface; self-consistency-only verification). This is the strongest, most explicit self-assessment in the codebase — no other module read so far comes close. Checking akm.rs / heckman.rs / jmp.rs / dfl.rs next for a comparable note (Q2's second half: does any module that SHOULD carry one lack it).
