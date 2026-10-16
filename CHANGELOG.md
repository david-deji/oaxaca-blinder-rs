# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Before 1.0 a minor version can break the programming
interface and can move a published number; every such change is listed under "Migrating" and "Statistical changes".

## [Unreleased]

## [0.3.0] - 2026-10-16

This is the first release since 0.2.2. It breaks the library and the command line in the ways listed under
"Migrating from 0.2", moves some reported numbers (see "Statistical changes"), and turns the workspace into a project
others can build on: a documentation site, a licence, a stated minimum Rust version and a release process.

| Crate | Version | Where it ships |
|---|---|---|
| `oaxaca_blinder` (library and `oaxaca-cli`) | 0.3.0 | crates.io |
| `pay-equity-engine` (remedy, defensibility and frontier layer; WASM) | 0.3.0 | not on crates.io; built from this repository |
| `meridian-mcp` (MCP server) | 0.3.0 | not on crates.io; prebuilt binaries on the GitHub release |

There is no 1.0 promise. The full migration guide, with before and after code, is in the documentation site
(`migrating-0.2-to-0.3`).

### Migrating from 0.2

Library, relative to 0.2.2:

| Change | Before | After |
|---|---|---|
| Default reference coefficients | `GroupA` (the 0.2.2 setter's documentation said `GroupB`, the code used `GroupA`; the 0.2.2 command line already defaulted to `group-b`) | `GroupB`. Call `.reference_coefficients(ReferenceCoefficients::GroupA)` to keep the old numbers. |
| `OaxacaResults::explained()` and `unexplained()` | returned `&ComponentResult` | return `Option<&ComponentResult>`; call `.expect(..)` or match |
| `OaxacaResults` | no `run_metadata` field | has a public `run_metadata` field; struct literals and exhaustive destructuring need `..` |
| `OaxacaError` | 0.2.2 variants | 9 more: `InsufficientData`, `EmptyLevelInGroup`, `TooManyGroupValues`, `ReferenceGroupAbsent`, `UnknownReferenceCoefficients`, `NormalizationError`, `WeightsKindRequired`, `InvalidWeight`, `UnsupportedCombination` |
| `OaxacaError`, `ReferenceCoefficients`, `WeightsKind`, `NormalizationConvention`, `ExclusionReason` | exhaustive | `#[non_exhaustive]`; a `match` from another crate needs a `_` arm |
| `ReferenceCoefficients` | no `PooledNoIndicator` | has `PooledNoIndicator`, which is what `Neumark` was meant to compute |
| A weights column | `.weights("w")` | `.weights("w").weights_kind(WeightsKind::Relative)`, or `Frequency` for whole-number counts; without a kind the run fails with `WEIGHTS_KIND_REQUIRED` |
| Name of the intercept row | `intercept` | `oaxaca_blinder::INTERCEPT_NAME` (`__ob_intercept__`) in the results and the JSON. `summary()` prints it as `oaxaca_blinder::INTERCEPT_LABEL` (`(intercept)`). |
| `.normalize(["sector"])` and `normalize_all_categoricals()` | per-level rows deviate from the simple average of the levels (equal share) | deviate from the population-share average. Call `.normalization_convention(NormalizationConvention::EqualShare)` first to keep the 0.2.2 rows. |
| `.categorical_predictors(&[])` | compiled | does not compile (the type of the empty slice cannot be inferred); write `Vec::<&str>::new()` or leave the call out |
| A group column with more than two values | the first other value was used silently | error `TOO_MANY_GROUP_VALUES`; recode to two values first |
| OLS with as many columns as rows, and a null outcome in the AKM and quantile paths | no check; a null was read as 0.0 | `InsufficientData` for the first, an error for the second; drop or fill nulls first |
| `AkmBuilder` | methods took `self` | methods take `&mut self` and `run(&self)`; single-expression chains still compile |
| `MatchingEngine::match_nearest_neighbor(k, &metric)` and the `DistanceMetric` trait | metric object | `match_nearest_neighbor(k, use_mahalanobis: bool)`; the trait is gone |
| `OaxacaBuilder` | `Clone` and `Debug` | neither; build a new builder for each run |
| AKM, DFL, JMP and matching | always available | behind the cargo feature `experimental`, off by default |
| Cargo features `python` and `pyo3` | present in the manifest | removed; the Python binding has not been compiled since it was switched off |
| `ReferenceCoefficients::Neumark`, `QuantileDecompositionBuilder`, `OaxacaResults::optimize_budget` | supported | `#[deprecated]` |

Column-name arguments (`predictors`, `categorical_predictors`, `normalize`, `heckman_selection`, `controls`) accept
anything that is `AsRef<str>`, so `&["education"]` from 0.2.2 compiles unchanged.

Command line, relative to 0.2.2:

| Change | Before | After |
|---|---|---|
| `--weights` | alone | needs `--weights-kind frequency` or `--weights-kind relative` |
| `--bootstrap-reps` default | 500 | 100 (the library default is 100 too) |
| `--analysis-type quantile` | Machado-Mata | RIF regression; `--simulations` (default 1000 before, 200 now) is ignored |
| Intercept row in the tables, the `report` HTML and `--output-json` | `intercept` | `(intercept)` in the tables and the `report`; `__ob_intercept__`, the same token as the library, in `--output-json` |
| Group labels in the console summary and the `report` | "Group A (Advantaged)" and "Group B (Reference)" | "Compared group (A)" and "Reference group (B)"; the compared group is every row outside the reference group, whoever earns more |
| Categorical drivers | raw treatment coding | population-share normalisation; `--normalization none` restores the raw coding for `run` |
| `--analysis-type akm` and `match` | in the default build | `cargo install oaxaca_blinder --features experimental`; the default build refuses them and says so |
| `--outcome` | always required | not needed with `--formula`, which names the outcome |
| `--ref-coeffs` | no `pooled-no-indicator` | `pooled-no-indicator` is a new value; the default stays `group-b` |

Engine and MCP have no earlier release to break. Anyone who ran a development build should read the engine and MCP
items under "Statistical changes".

### Statistical changes a published result would show

Numbers below were measured on one synthetic 400-row file (wage, education, experience, a three-level sector, gender,
weights), once built against 0.2.2 and once against this release, with 300 bootstrap replicates unless stated.
The decomposition itself (the gap, the explained and unexplained parts for a fixed reference scheme) did not move.
A statistical result that a published figure depends on should be rerun on 0.3.0.

Library and command line:

- **Default reference coefficients are `GroupB`, not `GroupA`.** Same data, same call: explained 0.281824 becomes
  0.269266 and unexplained 4.000989 becomes 4.013546; the gap, 4.282813, is the same.
- **A percentile confidence interval has `NaN` ends below 41 bootstrap replicates.** The run still succeeds and the
  standard error and p-value are finite (below 2 replicates they are `NaN` too). The default is 100, as in 0.2.2, so a
  caller who never sets the replicate count sees no change. The command line warns on standard error.
- **Bootstrap standard errors, p-values and intervals differ from a 0.2.2 run in every digit.** The random generator is
  now seeded (ChaCha8, default seed `DEFAULT_SEED`), so a run repeats exactly, on any number of threads. Point
  estimates did not move: `GroupA`, `GroupB`, `Pooled`, `Weighted` and weighted least squares agree with 0.2.2 to six
  decimals. Example: the `GroupB` unexplained standard error went from 0.120900 to 0.120759.
- **Quantile (RIF) point estimates moved.** The sample quantile follows R's type 7 instead of a ceiling rule, and
  weights now reach the RIF, its bandwidth and its kernel. Gap at tau 0.1: 5.600 to 5.301; at 0.5: 3.841 to 3.823;
  at 0.9: 3.747 to 3.733; explained at 0.5: -0.0040 to 0.0002.
- **The command line's quantile analysis is a different estimator** (RIF, not Machado-Mata), so it is not comparable
  with 0.2.2 output.
- **Weighted least squares standard errors** use the row count, not the sum of weights, as `n`. Point estimates are
  identical.
- **Frequency weights bootstrap the expanded sample.** A replicate used to draw one row per row and carry the weight.
  On the skewed test file the standard errors were 1.32 times (explained) and 1.45 times (unexplained) those of the
  repeated rows, and one p-value read 0.084 where the repeated rows gave 0.000.
- **Per-level rows of categorical drivers are normalised** (command line by default, library opt-in with
  `normalize_all_categoricals()`). Aggregates are unchanged; the rows of every categorical change, and the
  alphabetically first level now appears. Renaming a level used to flip the sign of a driver.
- **The library's `normalize(..)` and `normalize_all_categoricals()` now use population-share, not equal-share.** A
  0.2.2 caller of either gets different per-level rows: on the 400-row file `sector_Sales` explained went from 141.23
  to 95.07, `sector_Admin` from 596.44 to 516.27, and the intercept from 3938.91 to 4028.97. Call
  `.normalization_convention(NormalizationConvention::EqualShare)` first and the rows equal 0.2.2 to six decimals.
- **The three-fold decomposition under normalisation** used to sum to 79 percent of the gap and moved with the base
  level. It is now computed from the raw coding and equals R `oaxaca` `threefold` to 1e-10.
- **Logit probabilities are clamped to [1e-10, 1 - 1e-10] and the probit Hessian is solved, not inverted.** On a
  2000-row Heckman simulation the four schemes that run (`GroupA`, `GroupB`, `Weighted`, `Cotton`) are identical to
  8 decimals. Matching and DFL results move only for extreme fitted values; the size was not measured.
- **`optimize_budget` with a NaN, infinite or negative budget pays nothing.** A NaN budget used to fund the full need.

Engine and MCP server (results Meridian reads):

- **Prediction intervals and the frontier p-value use Student t**, not Normal. The half-width grows by about 2 percent at
  58 residual degrees of freedom and 14 percent at 10. Remedy dollars move under `LowerBound` and `UpperBound`;
  `Midpoint` payments do not. `is_defensible` allows one cent below the floor, not one dollar, and `confidence_level` is
  honoured.
- **The optimiser's `Pooled` target prices against the decomposition's `Pooled` line** (the pooled regression with a
  group indicator, read at indicator 0). Every Pooled-target dollar changes. The Reference target is byte-identical.
- **A remedy row no longer takes the next employee's dollars** when a cell is blank or the group column holds a third
  value. In the case that exposed it, 36 of 39 paid rows were more than 1 dollar off.
- **Remedy eligibility changed** for runs that used `adjust_both_groups` or `min_gap_pct`.
- **The efficient frontier with no budget** no longer invents a budget of 1000; it returns one zero-budget point and a
  warning.
- **The frontier follows the remedy.** Its default budget axis ends at the remedy's full cost, not 1.1 times it;
  `Equitable` is recomputed at each step; ties are paid in `optimize`'s order; and `confidence_level` also sets the
  interval a `LowerBound` or `UpperBound` remedy pays to.
- **`check_defensibility` gaps flip sign.** `original_gap`, `new_gap`, `original_unexplained_gap` and
  `new_unexplained_gap` are the compared group's figure minus the line: negative while the compared group is
  underpaid, rising with money paid. A consumer that negated them must stop.
- **`adjust_both_groups` no longer credits reference raises to the compared group.** On a probe roster the compared
  group's remaining gap was +2,103 and is -434. `new_unexplained_gap` is the gap on the line refitted to the schedule's
  wages; it equals R `lm` on the adjusted wages to 1e-6 on 62 cases.
- **`required_budget` is the compared group's need in both entry points** and `total_cost` is the money spent on both
  groups. With `range_target` or `min_gap_pct` set the two entry points differ (a 4 percent threshold on the probe
  roster: 16,884 against 19,607).
- **`target_gap` sets the budget.** Already met pays 0 (a budget of 0 used to read as no cap and paid the full need);
  out of reach pays every eligible shortfall.
- **A negative or non-finite `budget` is refused** (`INVALID_BUDGET`); it used to fund the full need.
- **Under `LowerBound` and `UpperBound`** the unexplained gaps are read on the midpoint line, and reference employees
  raised with `adjust_both_groups` go to the same point of their own range (they went to the midpoint).
- **A misspelt `strategy`, `target` or `range_target` at the MCP server is refused by name.** A misspelt
  `"equitable"` used to run `Greedy`.
- **An empty sum serialises as `0.0`**, not `-0.0`.

Not a number change, but a published result will show it: a group with no residual degrees of freedom is refused
(`INSUFFICIENT_RESIDUAL_DF`; it was a zero-width interval or `t = 0, p = 1`); a missing `reference_coefficients` is an
error at the engine and the MCP server (it was a silent `Pooled`); and result `warnings[]` can now appear on rosters
that were silent (`outside_range`, `normalised_difference`, `few_residual_df`, `tie_share`, `ecdf_offset`).

### Added

Library:
- Seeded bootstrap: `seed`, `seed_opt`, `seed_from_entropy`, `DEFAULT_SEED`, and a `RunMetadata` record on every
  result (requested and discarded replicates, seed, normalisation, reference scheme). The bootstrap runs in
  index-ordered chunks, so memory is flat in the number of replicates.
- `DEFAULT_BOOTSTRAP_REPS` (100), the one default shared by the library and the command line.
- Row accounting: `get_data_matrices_with_rows`, `analysed_rows`, `RowAccounting`, `ExcludedRow`, `ExclusionReason`
  say which rows were analysed and why others were left out.
- Weights: `WeightsKind` (`Frequency` or `Relative`), `weights_kind()` and `weighted_quantile`. Weighted RIF.
- Normalisation: `normalize_all_categoricals`, `normalization_convention(PopulationShare | EqualShare)`,
  `NormalizationRecord`, `canonical_name`, `parse_name`, `INTERCEPT_NAME`.
- `ReferenceCoefficients::PooledNoIndicator` (Neumark 1988; Stata `omega`; R `oaxaca` weight -1).
- `rif_outcome_frame`, `qr_coefficients` and `OaxacaResults::format_summary`.
- `oaxaca_blinder::polars`, a re-export of the Polars crate the library is built on, so `cargo add oaxaca_blinder` is
  the only dependency a first program needs.
- Public types that were not listed before: `ShareMap`, `FactorShares`, `LevelShare`, `GroupMatrices` and
  `DataMatricesWithRows`.
- DFL accepts categorical predictors. Cargo features `mem-profile` and `wasm-threads`.
- Cargo feature `experimental`, which gates AKM, DFL, JMP and matching (see "Changed").
- Doctested examples: `first_decomposition`, `weights_and_categoricals` and `quantile_rif`.

Command line:
- `--normalization population-share|equal-share|none`, `--weights-kind frequency|relative`, and
  `--ref-coeffs pooled-no-indicator`. `--output-json` works on the quantile path.
- A whole-number outcome or weights column is read as a decimal, `--outcome` is not needed with `--formula`, a
  `--bootstrap-reps` below 41 prints a warning on standard error, and a runtime error prints the cause without the
  help text.

Engine and MCP server, a first contract for Meridian:
- The MCP tools `forensic_decomposition`, `simulate_remediation`, `verify_adjustments`, `check_defensibility` and
  `generate_efficient_frontier`, over standard input and output or HTTP with server-sent events.
- Result fields: `support`, `warnings[]`, `adjustments[].extrapolated`, `interval`, `source`, `range_position`,
  `range_position_before`, `quantile_report`, `position_counts`, `group_test`, frontier `group_coefficient` and
  `degrees_of_freedom`, and for a target gap `best_reachable_gap`, `target_gap_reachable`, `shortfall_to_target`,
  `target_budget`, `budget_binding`, `unfunded_amount`, `unfunded_count`, `threshold_excluded_count`, `closure` and
  `overshoot_mean`. `required_budget` comes with `cost_target`, `cost_reference`, `need_target`, `need_reference` and
  `target_line`.
- `reference_coefficients` is required and exact on `forensic_decomposition` and `verify_adjustments`:
  `GroupA`, `GroupB`, `Pooled`, `PooledNoIndicator` or `Weighted`; anything else is
  `UNKNOWN_REFERENCE_COEFFICIENTS`. `check_defensibility` and the frontier take `confidence_level`,
  `target` (`Reference` or `Pooled`), `strategy`, `range_target`, `min_gap_pct` and `adjust_both_groups`.
- Error codes by name: `INVALID_BUDGET`, `INVALID_TARGET_GAP`, `INVALID_MIN_GAP_PCT`, `INVALID_CONFIDENCE_LEVEL`,
  `INVALID_STEPS`, `TARGET_GAP_WITH_REFERENCE_RAISES`, `UNKNOWN_TARGET`, `UNKNOWN_STRATEGY`,
  `UNKNOWN_RANGE_TARGET`, `INSUFFICIENT_RESIDUAL_DF`, `WEIGHTS_KIND_REQUIRED`, `INVALID_WEIGHT`.
- Stable row keys: a row's key comes from its content, and a row-counter column (`No.`, an employee id running 1 to N)
  is left out, so a re-export with an inserted row keeps every annotation on its employee.
- A snapshot diff for weekly HRIS extracts, as a WASM session that holds the previous week compactly: about 520 MB for
  a 130,000-row, 120-column week against about 1,370 MB for the JavaScript version it replaces. The diff itself is about
  twice as slow.
- Two WASM builds, sequential and threaded (Rayon on web workers), each with its SHA-256 committed. The build recipe
  is reproducible, and a browser test compares the threaded build with the native result.
- `engine_version`, `reference_coefficients_used` and `method` are stamped into `run_metadata`.

Project:
- A documentation site (mdBook): start page, user guide, method pages, command-line and MCP reference, error codes,
  how to reproduce results in R and Stata, FAQ and contributor pages. The command-line page and the MCP tool schemas
  are generated from the shipped programs and compared with them in CI.
- Both READMEs rewritten; their code is compiled as doctests, and the crate documentation on docs.rs is the crate
  README.
- `LICENSE-MIT`, `LICENSE-APACHE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `CITATION.cff`, issue
  templates and a pull request template.
- A release workflow (see "Release process").
- Oracles in `verification/` that check results against R `lm` and `predict.lm`, `oaxaca` 0.1.5, `ddecompose` 1.0.0,
  `quantreg` and `Hmisc`, with the SHA-256 of each generator and fixture checked when the tests load them. Bootstrap
  standard errors equal R's to 12 digits on shared resamples.

### Changed

- **Licence: `MIT OR Apache-2.0`** on every crate. 0.2.2 was `MIT` only.
- **Minimum supported Rust version 1.88** (`rust-version`), compiled by the `msrv` CI job.
- **Bootstrap replicates: one default of 100** in the library and the command line. A development build of 0.3 used 20
  in the library and 50 in the command line; 0.2.2 used 100 and 500.
- AKM, DFL, JMP and matching sit behind the cargo feature `experimental`. They have no external check, and their
  results can change in any release.
- Detailed rows name the intercept `__ob_intercept__`.
- Column-name arguments accept any iterable of string-like values. `lib.rs` is split into modules. Performance work:
  a Cholesky solve in logit, a linear solve in probit, and fewer DataFrame clones and KD-tree allocations in the VIF,
  quantile and matching code.
- Package metadata for `oaxaca_blinder`: repository, homepage, documentation, keywords, categories, and an `include`
  list so the package carries the sources, the report template, the README and both licences.
- `pay-equity-engine` and `meridian-mcp` are `publish = false`. Their versions track this release.
- Loop records moved from the repository root into `records/`. Files that were never part of a crate are gone from the
  tree: IDE and agent folders, editor backups, a saved web page named `Roboto-Regular.ttf`, an unused copy of the `cc`
  crate, and scratch files.
- Test-only parts and fixes from the pull requests triaged in October 2026 (#70, #73, #80, #84, and parts of #87, #88,
  #90, #91 and #95) are in.
- CI: every action is pinned to a commit, every cargo command takes `--locked`, one `gate` check needs every gating
  job, and `scripts/check-ci-invariants.py` refuses a workflow that could silence itself.

### Deprecated

- `QuantileDecompositionBuilder` (Machado-Mata). The shipped quantile path is RIF regression.
- `ReferenceCoefficients::Neumark`, which keeps computing `Pooled`. Use `PooledNoIndicator` for Neumark's estimator.
- `OaxacaResults::optimize_budget`, which allocates on the decomposition's residuals. The engine's `optimize` is the
  remedy Meridian runs.

### Removed

- The Python binding (the `python` and `pyo3` features) and the verification scripts that imported it. It has not been
  compiled since it was switched off.
- The `DistanceMetric` trait.
- `Clone` and `Debug` on `OaxacaBuilder`.
- The `optimization_engine` crate from the workspace.

### Fixed

- Remedy dollars land on the employee they were computed for when a cell is blank or a third group value appears.
- `meridian-mcp` answers requests that carry the standard `jsonrpc` key. The field was declared `_jsonrpc` without a
  rename, so a compliant client got "Parse error" on every request, on standard input and on HTTP. `_jsonrpc` is
  still accepted. The server reports its crate version in `serverInfo`.
- OLS with no spare degrees of freedom is an error (`InsufficientData`) and the engine refuses a fitted group with none
  (`INSUFFICIENT_RESIDUAL_DF`); AKM convergence errors surface; nulls are no longer read as 0.0 in the AKM and quantile
  paths.
- Many numeric and `unwrap` panics in the library; header-parse panics and poisoned-lock handling in the MCP server.
- RIF quantile decomposition fails with a message when a group has fewer than 2 rows. It returned the raw series as its
  own RIF.
- Documentation: the READMEs no longer show code that does not compile, name the wrong version, or describe Python
  bindings.
- In the library a whole-number outcome (or selection outcome) column is converted to a decimal instead of refused with
  `expected Float64, got i64`. A weights column of whole numbers was already accepted. The command line did this
  already.
- `oaxaca-cli report` and `run` take the main options (`--data`, `--outcome`, `--group`, `--reference` and the rest)
  after the subcommand alone. Before, `report` needed them before the word `report` and again after it. Giving them
  before still works, and so does giving them in both places; the later one wins.
- A Heckman selection model with `Pooled`, `PooledNoIndicator` or `Neumark` returns `UNSUPPORTED_COMBINATION`, naming
  the combination and the schemes that work, instead of panicking on a matrix dimension mismatch. 0.2.2 panicked for
  `Pooled` too.
- The WASM package manifests written by `scripts/build-wasm.sh` name Telos Machina, carry the engine crate's version
  and the licence `MIT OR Apache-2.0`. They named "OpenPay Team", version 0.1.0 and no licence.

### Security

- `meridian-mcp` in HTTP mode refuses to start without `MCP_API_KEY`, checks the key on every request in constant time,
  has tightened CORS, and rate-limits the event stream.
- Lockfile updates: `rand` 0.8.7, `rustls-webpki`, `quinn-proto` 0.11.18 and `rustls` 0.23.46. Neither `quinn-proto`
  nor `rustls` is compiled into a shipped binary or the WASM engine.
- An unsafe pointer cast between Polars versions is gone with the Python binding.
- `SECURITY.md` describes how to report a vulnerability.

### Known issues

- A quantile run's `total_gap` is the RIF model total, not the sample percentile gap (4,463 against 4,750 at the 90th
  percentile of the test file).
- `--simulations` is accepted and ignored; its help text still names Machado-Mata.
- The Heckman correction has no R or Stata run to compare with, only simulated data with a known answer.

### Release process

Publishing a version is a tag push (`v0.3.0`). The release workflow checks that the tag equals the `oaxaca_blinder`
version and that `gate` passed on the tagged commit, re-runs formatting, clippy, the tests and a dry-run publish,
builds `oaxaca-cli` and `meridian-mcp` for Linux, macOS (Apple silicon) and Windows, publishes `oaxaca_blinder` to
crates.io, attaches the binaries and two SBOMs to the GitHub release (its text is this section), and deploys the
documentation under `v0.3/` and `latest/` so older versions stay readable.

### About the tags

Tag `v0.2.0` is on a commit whose `Cargo.toml` still says 0.1.0; the bump to 0.2.0 came a day later and crates.io
0.2.0 was published from it. Tag `v0.2.1` is on a later commit that carries version 0.2.2, pushed by mistake; 0.2.1
itself was never tagged or published. No tag has been moved, because moving one breaks anyone who pinned it. Use the
versions on crates.io (0.1.0, 0.2.0, 0.2.2, then 0.3.0) as the record.

## [0.2.2] - 2025-12-18

### Added
- AKM model, and a matching module with Euclidean and Mahalanobis distances and logistic propensity scores.
- Trait-based estimators.
- A `diagnostics` module (variance inflation factors) with Polars integration.
- A `report` command in the command line, for static HTML summaries.
- Class-based Python bindings.

### Fixed
- The probit Hessian inversion.
- A panic in the command line.
- `match_nearest_neighbor` index mapping.
- Findings from a review of the optimisation engine; lint warnings; dependency configuration and updates.

### Changed
- `allow(dead_code)` on several result types, to reduce warning noise in API use.

## [0.2.1]

Never published to crates.io and never released. Version 0.2.1 was set in the manifest on 2025-11-28 and the work
went out as 0.2.2. Tag `v0.2.1` exists by mistake; see "About the tags" under 0.3.0.

## [0.2.0] - 2025-11-27

### Added
- Sample weights: weighted least squares, weighted means, weighted bootstrap.
- Heckman two-step selection correction (probit selection equation, inverse Mills ratio).
- R-style formula interface (`OaxacaBuilder::from_formula`), with `C()` and `factor()` for categorical variables.
- Python bindings (`pyo3`, `maturin`).
- RIF quantile decomposition, Juhn-Murphy-Pierce and DiNardo-Fortin-Lemieux decompositions, variance inflation
  factors, a budget allocation helper, JSON, LaTeX and Markdown export, and a stand-alone command line.
- Results checked against R's `oaxaca` package.
