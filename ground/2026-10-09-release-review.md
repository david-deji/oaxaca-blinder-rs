# Release review: oaxaca-blinder-rs, v0.2.0 to main (next major release)

> Written 2026-10-09, read-only review. Nothing tagged, published, pushed or edited.
> Source of truth: `origin/main` at 6f3113e (PR #105, T8, merged 18:06 after the ledger entry; CI on it was still running when this was written). 185 commits and 303 files since v0.2.0.
> Evidence is marked: **verified** (command run here), **source-read** (read in code or commits, not executed), **unverified** (could not check).
> Scratch work (rustdoc-JSON API diff, dry-runs, probe programs) lives in the gitignored `target/rr/`; nothing outside `target/` and this file was written.

## 0. Findings that need a decision before a tag

| # | Finding | Evidence |
|---|---|---|
| F1 | `.predictors(&["education"])`, the form in the README, the crate docs and every example, no longer compiles. `predictors`, `categorical_predictors`, `normalize`, `heckman_selection` now take `IntoIterator<Item: Into<String>>`; `&&str` is not `Into<String>`. Switching the bound to `AsRef<str>` accepts `&["a"]`, `["a"]`, `Vec<String>` (reasoned, not compiled). | **verified** (compile error E0277 on main; `["education"]` compiles and runs) |
| F2 | Library default `bootstrap_reps` fell 100 to 20 (7439cb7), and a percentile CI is now refused below 41 reps (368c513). A caller who never sets reps gets `[NaN, NaN]` intervals and a noisier SE. The CLI default fell 500 to 50 (21d2478). | **verified** (probe prints `default CI: [NaN, NaN]`) |
| F3 | Library default reference coefficients changed GroupA to GroupB (1de7b3c). Same code, same data, different explained and unexplained. | **verified** (probe: explained 0.281824 to 0.269266) |
| F4 | No LICENSE file in the repo or the package; `license = "MIT"` only in Cargo.toml. GitHub reports no licence. | **verified** |
| F5 | `pay-equity-engine` and `meridian-mcp` cannot be published as they stand (path dependencies without versions). | **verified** (`cargo publish --dry-run` exit 101 for both) |
| F6 | No `#[non_exhaustive]` anywhere: `OaxacaError` gained 8 variants and `ReferenceCoefficients` 1, each a break for exhaustive `match`. Adding the attribute now costs nothing extra; later it is another break. | **source-read** |
| F7 | The release workflow no longer publishes to crates.io and attaches no binaries; publishing is a manual step. | **verified** (release.yml read) |

## 1. What shipped since v0.2.0, by who is affected

Baselines: tag v0.2.0 (afa9d30, Cargo.toml says 0.1.0, see section 4); crates.io 0.2.2 (ff8367a). Items marked [0.2.2] are already on crates.io; the rest are new to outside users.

### (a) Crate users of `oaxaca_blinder` (library API)

Between v0.2.0 and 0.2.2 [0.2.2]:
- R-style formulas, sample weights, Heckman correction, JSON and Markdown output (c90f27d)
- AKM model and matching module with Euclidean and Mahalanobis distances (78eef3c)
- Trait-based estimators (d2dd67e); VIF diagnostics module (d298884)
- Fix: nearest-neighbour index mapping (4ab824c); probit Hessian inversion, CLI panic, dependency updates (ff8367a)

New since 0.2.2:
- Seeded bootstrap: `seed`, `seed_opt`, `seed_from_entropy`, `DEFAULT_SEED`, `RunMetadata` on every result (356faab, 0014 stage 1)
- Bootstrap runs in index-ordered chunks so memory is flat in the replicate count (c005837); `mem-profile` and `wasm-threads` features (c005837, 65f13b2)
- RIF-regression is the shipped quantile path; Machado-Mata `QuantileDecompositionBuilder` is `#[deprecated]` (3858b43); RIF fails loud when a group has under 2 rows (3858b43)
- Weighted RIF; percentile CI refused under 41 reps (368c513, 0097); `qr_coefficients` helper (623ff98)
- `lib.rs` split into modules (7439cb7); builders accept string iterators (2d4841b); `OaxacaResults::explained()/unexplained()` return `Option` (e6e6dd9)
- DFL accepts categorical predictors (975b11e); OLS degrees-of-freedom guard, AKM convergence errors (4dcac3d); numeric and unwrap fixes (a68e062, c958a6d)
- Row accounting: `get_data_matrices_with_rows`, `analysed_rows`, `RowAccounting`, `ExcludedRow`, `ExclusionReason`; a third group value is a named error (567d841, 0118, PR #97)
- 0120 (PR #104): `normalize_all_categoricals`, `normalization_convention(PopulationShare | EqualShare)`, `NormalizationRecord` and friends, `ReferenceCoefficients::PooledNoIndicator`, `canonical_name`, `parse_name`, `WeightsKind` with `weights_kind()` and `weighted_quantile`, `INTERCEPT_NAME`, `rif_outcome_frame`, `OaxacaResults::format_summary`
- Performance (March 2026 batch, PRs #28 to #64): Cholesky logit, probit linear solve, no DataFrame clones in VIF, quantile and matching, KD-tree allocations
- Dependencies: rand 0.8.5 to 0.8.7, rustls-webpki bump, lockfile only (bde1f9d, PR #96). Public types still polars 0.44 and nalgebra 0.32, same as 0.2.2 (**verified** from Cargo.toml).

### (b) CLI users (`oaxaca-cli`, a bin inside the `oaxaca_blinder` package)
- [0.2.2] `report` subcommand for HTML summaries (dad2a2c)
- `--analysis-type quantile` now runs RIF, not Machado-Mata; `--simulations` is inert; `--output-json` works on the quantile path (3858b43)
- `--normalization population-share|equal-share|none`, default population-share, applied to `run` and `report` (6777a17)
- `--weights` requires `--weights-kind frequency|relative` (6777a17)
- `--ref-coeffs` gains `pooled-no-indicator`; default group-b (6777a17)
- Defaults `--bootstrap-reps` 500 to 50, `--simulations` 1000 to 200 (21d2478)
- Known, not fixed: `report` also demands the top-level run flags; an integer weights column cannot be read from CSV (LOOP-LEDGER/0120 log, **source-read**)

### (c) MCP users (`meridian-mcp`, never released; created 33d0e8a, 2026-02-19)
- HTTP/SSE mode refuses to start without `MCP_API_KEY`; key checked on every request, in constant time (5dcd681, 4135e4f); CORS tightened (a79534b); rate limiting on SSE (5a08d76); header-parse panics and poisoned-lock handling removed (c28bcc4, 96a71a4)
- Argument cloning removed (79728ee)
- `reference_coefficients` required on `forensic_decomposition` and `verify_adjustments`; unknown value is `UNKNOWN_REFERENCE_COEFFICIENTS` (6777a17)
- `confidence_level` on `check_defensibility` and `generate_efficient_frontier`; `target: Reference|Pooled` on `check_defensibility`, optimiser Pooled target (6777a17, 6f3113e)
- Result fields: `support`, `warnings[]`, `adjustments[].extrapolated`, `interval`, `quantile_report`, frontier `group_coefficient`, `degrees_of_freedom` (6777a17)
- Seam still open: unknown `target` strings fall through to Reference (6f3113e `meridian-mcp/src/main.rs`), same family as 0096 MEDIUM-1.

### (d) Meridian through the WASM engine (`pay-equity-engine`, never released to crates.io)
- Engine crate created; dual WASM artifacts, sequential and threaded with rayon (65f13b2, 0014 stage 3); byte-parity harness in Chromium (623ff98)
- Stable row keys and WASM publish step (b610467, 0017); weekly-extract snapshot diff (c2a9c75, 0018)
- Engine audit, bootstrap SEs verified against R, RIF weights (5920939, 368c513, 0096, 0097); WASM rebuild (73c7c7b, 0098)
- 0117 triage: PRs #70, #73, #80, #84 merged, test-only parts of #87, #88, #90, #91, #95 (9e4bd97)
- 0118: remedy dollars land on the employee they were computed for, blank cell or third group value no longer shifts rows (567d841)
- 0119: reproducible WASM recipe, `--verify` / `--record`, `engine-manifest.json`, CI `gate` job (6ec23be, PR #98)
- 0120 Track E (PR #104) and T8 (PR #105): normalised drivers, raw three-fold, strict scheme, support diagnostics, Student t, weight kinds, optimiser Pooled target, `check_defensibility` target
- Removed from the workspace: `optimization_engine` / `openpay_optimization` (c450713; the crate `openpay_optimization` exists on crates.io under someone's control, **verified** HTTP 200)

### (e) Python bindings
`python.rs` is not compiled. `pub mod python` was commented out in 3191c4b (2026-03-14); the `python` and `pyo3` Cargo features and the pyo3 dependencies are gone from `oaxaca_blinder/Cargo.toml` (**verified** against v0.2.2). The file still exists and still carries the `normalize_all_categoricals()` call. Stale leftovers: `oaxaca_blinder/pyproject.toml` (version 0.3.0, maturin), README section "Python Bindings" and `maturin develop --features python`, `.cargo/config.toml` PYO3 env, `verification/verify_*.py` (import `oaxaca_blinder.OaxacaBlinder`). In 0.2.2 `--features python` existed (**source-read**), so its removal is a break (section 2, B8).

## 2. Breaking changes and the semver bump each forces

`cargo-semver-checks` is not installed (**verified**: not on PATH, not in `~/.cargo/bin`); installing it writes to `~/.cargo`, so it was not installed. Substitute, **verified**: rustdoc JSON of `oaxaca_blinder` at tag v0.2.2 and at main (nightly, default features), walked from the crate root through re-exports and diffed with `target/rr/apidiff.py` (178 public items before, 271 after), plus compile probes against both. The script does not see trait-impl changes beyond Clone/Debug presence, so section 2 also lists changes read from commits.

Rule used: `oaxaca_blinder` is 0.x, so any break forces a minor bump (0.2.x to 0.3.0). The crate is already at 0.3.0 in Cargo.toml. After a 1.0, every row below would force a major.

### Library (`oaxaca_blinder`), baseline 0.2.2

| # | Break | Before -> after | Bump |
|---|---|---|---|
| B1 | Builder inputs take an iterator of `Into<String>` (`OaxacaBuilder::{predictors, categorical_predictors, normalize, heckman_selection}`, `QuantileDecompositionBuilder::{predictors, categorical_predictors}`, `AkmBuilder::controls`, `MatchingEngine::new`) | `.predictors(&["education"])` -> `.predictors(["education"])` (or `vec![..]`, or `slice.iter().copied()`). Verified: the old form fails with E0277. | minor (0.x) |
| B2 | `AkmBuilder` methods take `&mut self`, `run(&self)` | `let b = AkmBuilder::new(..).controls(&c);` -> `let mut b = AkmBuilder::new(..); b.controls(c);` (single-expression chains still compile; reasoned, not compiled) | minor |
| B3 | `MatchingEngine::match_nearest_neighbor(k, &metric)` -> `(k, use_mahalanobis: bool)`; trait `DistanceMetric` removed | `engine.match_nearest_neighbor(1, &EuclideanDistance)` -> `engine.match_nearest_neighbor(1, false)` | minor |
| B4 | `OaxacaResults::explained()` / `unexplained()` return `Option<&ComponentResult>` | `r.explained().estimate` -> `r.explained().expect("..").estimate` | minor |
| B5 | `OaxacaResults` gains public field `run_metadata` | struct literals and exhaustive destructuring break; add `..` or read through the getter | minor |
| B6 | `OaxacaError` +8 variants (`InsufficientData`, `EmptyLevelInGroup`, `TooManyGroupValues`, `ReferenceGroupAbsent`, `UnknownReferenceCoefficients`, `NormalizationError`, `WeightsKindRequired`, `InvalidWeight`); `ReferenceCoefficients` +`PooledNoIndicator` | add a `_ =>` arm. Fix now: mark both `#[non_exhaustive]` so the next addition is not a break | minor |
| B7 | `OaxacaBuilder` lost `Clone` and `Debug` | rebuild the builder per use, as the CLI and engine do | minor |
| B8 | Cargo features `python` and `pyo3` removed | `--features python` fails ("package does not have feature"). No replacement. | minor |
| B9 | A weights column needs a kind: runtime error `WEIGHTS_KIND_REQUIRED` | `.weights("w")` -> `.weights("w").weights_kind(WeightsKind::Relative)` (`Frequency` for whole-number counts) | minor |
| B10 | Detailed rows name the intercept `__ob_intercept__` (was `intercept`) | filter on `oaxaca_blinder::INTERCEPT_NAME`. Verified in probe output. | minor |
| B11 | More than one non-reference group value is `TooManyGroupValues`; was silently the first other value (0118) | recode the group column to two values first | minor |
| B12 | OLS with n <= k columns is `InsufficientData`; null outcome in AKM and quantile paths is an error (was treated as 0.0) (4dcac3d, a68e062) | drop or impute nulls before the call | minor |
| B13 | Defaults changed: reference coefficients GroupA -> GroupB, `bootstrap_reps` 100 -> 20 | set both explicitly: `.reference_coefficients(ReferenceCoefficients::GroupA).bootstrap_reps(100)`. Numbers in section 3. | minor, but silent (see F2, F3) |
| B14 | `ReferenceCoefficients::Neumark` deprecated (keeps returning Pooled); `QuantileDecompositionBuilder` deprecated | warnings only: `Neumark` -> `PooledNoIndicator` if Neumark's no-indicator fit is what you meant (different number) | none (deprecation) |

Not in the diff, so not breaks: the other public modules (`akm`, `dfl`, `formula`, `heckman`, `jmp`, `matching`, `quantile_decomposition`) have the changes above and nothing else (**verified**, no other removed or changed items).

### CLI (`oaxaca-cli`), baseline 0.2.2
| Break | Migration | Bump |
|---|---|---|
| `--weights` needs `--weights-kind` | add `--weights-kind relative` | minor |
| Quantile analysis is RIF, not Machado-Mata; `--simulations` ignored | none available; results differ (section 3) | minor |
| Defaults `--bootstrap-reps` 500 -> 50 | pass `--bootstrap-reps 500` | minor |
| `run` and `report` normalise categorical drivers by default | `--normalization none` restores raw rows (`report` has no flag, **source-read**) | minor |

### MCP and engine (never released)
No baseline to break; first publication sets the contract. Changes inside this window that Meridian saw: `reference_coefficients` required and exact; `MCP_API_KEY` required in HTTP mode; `Pooled` optimiser target re-defined (T8); Student t. These are in the CHANGELOG as BREAKING.

### Net bump
`oaxaca_blinder` 0.3.0 (already set). A 1.0.0 would not be forced by anything here; see section 6.

## 3. Statistical behaviour changes a published result would show

Probe, **verified**: one synthetic 400-row file (wage, edu, exp, 3-level sector, gender, weights; fixed LCG seed), the same program built once against v0.2.2 and once against main, `bootstrap_reps(300)` unless noted. Probe sources: `target/rr/scratch/src/main.rs` (main) and `target/rr/scratch3/src/main.rs` (v0.2.2).

### Same input, same call, different numbers

| # | Change | Probe or source | Why |
|---|---|---|---|
| S1 | Default reference coefficients GroupA -> GroupB | explained 0.281824 -> 0.269266; unexplained 4.000989 -> 4.013546 (gap 4.282813 both) | 1de7b3c |
| S2 | Default `bootstrap_reps` 100 -> 20; percentile CI refused under 41 | default explained CI `[-0.63, 1.40]` -> `[NaN, NaN]` (SE 0.5566 -> 0.6316, mixed with S1 and S5) | 7439cb7, 368c513 |
| S3 | Quantile (RIF) point estimates | tau 0.1: gap 5.600 -> 5.301; tau 0.5: 3.841 -> 3.823; tau 0.9: 3.747 -> 3.733; explained at tau 0.5: -0.0040 -> 0.0002 | sample quantile is now R type 7 instead of the ceil rule (a68e062); weights now reach the RIF, the bandwidth and the kernel (368c513). Split between the two causes is unverified. |
| S4 | CLI quantile analysis is a different estimator | n/a | Machado-Mata -> RIF (3858b43); not comparable run to run |
| S5 | Bootstrap SE, p-value, CI move for every call | e.g. GroupB unexplained SE 0.120900 -> 0.120759; Pooled explained SE 0.526597 -> 0.572034 | unseeded `thread_rng` -> seeded ChaCha8 (356faab). Same value in expectation; every number differs from a 0.2.2 run. Point estimates did not move: GroupA, GroupB, Pooled, Weighted and WLS agree to 6 decimals (**verified**). A fixed seed now reproduces to the byte across thread counts (0014 AC-6, **source-read**). |
| S6 | WLS variance | point estimates identical (verified); SE formula changed `n = sum(w)` -> `n = rows` (a68e062 `math/ols.rs`) | effect isolated from S5 is unverified |
| S7 | Frequency weights bootstrap the expanded sample | on `norm_skewed_fixture.csv` SE was 1.32x (explained) and 1.45x (unexplained) the repeated-rows SE; a p-value read 0.084 against 0.000 | 0120 S9, CHANGELOG |
| S8 | Per-level driver rows (engine, CLI; library opt-in) | aggregates unchanged; rows of every categorical move; alphabetically first level now emitted | population-share normalisation, 0120 D1. Renaming a level used to flip a driver sign. |
| S9 | Three-fold under normalisation | summed to 79% of the gap; now equals R `oaxaca` threefold to 1e-10 | computed from raw vectors (0120) |
| S10 | Prediction intervals, frontier p-value: Student t, not Normal | +2% half-width at 58 residual df, +14% at 10; remedy dollars move under `LowerBound`/`UpperBound`; `Midpoint` unchanged | 0120 S7 |
| S11 | Optimiser `Pooled` target prices against the decomposition's `Pooled` line | every Pooled-target dollar changes (fair wages, payments, `total_cost`, gaps); Reference target byte-identical | T8, PR #105 (merged at 6f3113e) |
| S12 | Remedy rows with a blank cell or a third group value | each employee's name was paired with the next employee's dollars from the first blank row on | 0118. Probe in the issue: 36 of 39 paid rows more than $1 off. |
| S13 | Remedy eligibility (`adjust_both`, `min_pct`) | amounts change for configurations that used it | e17184d, **source-read** |
| S14 | Efficient frontier with no budget | fabricated 1000.0 budget removed; one zero-budget point plus a warning | 1de7b3c |
| S15 | `is_defensible` tolerance $1 -> 1 cent; `confidence_level` now honoured | flag flips near the floor | 0120 S7 |
| S16 | Logit probabilities clamped to [1e-10, 1-1e-10]; probit Hessian solved, not inverted | matching, DFL, Heckman: only extreme fitted values move; size unmeasured | a68e062, 701825e. **unverified** magnitude |

### Not a number change, but a published result will show it
- A group with no residual degrees of freedom is refused (`INSUFFICIENT_RESIDUAL_DF`); before: zero-width interval or `t = 0, p = 1`.
- `reference_coefficients` absent at engine and MCP is an error; before: silent Pooled.
- New `warnings[]` (`outside_range`, `normalised_difference`, `few_residual_df`, `tie_share`, `ecdf_offset`) can appear on rosters that were silent.
- Oracles behind the new numbers (CHANGELOG, ledger): R `lm`/`predict.lm`, `oaxaca` 0.1.5, `ddecompose` 1.0.0, `quantreg`, `Hmisc`; bootstrap SE equals R to 12 digits on shared resamples (368c513). Not re-run here.

## 4. CHANGELOG audit

`CHANGELOG.md` at main (195 lines) has `[Unreleased]` and `[0.2.2] - 2025-12-18` only. No `[0.2.0]`, no `[0.2.1]`, no `[0.3.0]` heading. `[Unreleased]` holds 0119 and 0120 (including T8) and nothing older. The GitHub release body is not taken from it (release.yml runs git-cliff over commit subjects).

### Omitted (**verified** by grep of `CHANGELOG.md` for each epic id or term: 0 hits for 0014, 0017, 0018, 0096 to 0098, 0117, seed, CORS, python, Machado, snapshot, Hessian; 1 hit each for 0118 and row_key)
| Theme | Commits | Affects |
|---|---|---|
| Whole period 2025-12-18 to 2026-10-07 | 141 of the 185 commits dated in it (**verified**, `git rev-list --count`); 20 are older (v0.2.0 to 0.2.2), 24 are 2026-10-07 or later | all |
| Seeded RNG, `RunMetadata`, `seed*` API | 356faab | library, results shape |
| Quantile path switched to RIF; Machado-Mata deprecated; RIF fail-loud | 3858b43, 0014 stage 3 | CLI, library |
| Weighted RIF; CI refused under 41 reps; bootstrap SE oracle | 368c513 | library |
| Default changes: reference GroupA -> GroupB, reps 100 -> 20, CLI 500 -> 50 and 1000 -> 200 | 1de7b3c, 7439cb7, 21d2478 | library, CLI |
| Intercept renamed `__ob_intercept__` (the 0120 entry mentions the constant, not the rename from `intercept`) | b3b0b88, 7439cb7 | library |
| `explained()`/`unexplained()` to `Option`; string-iterator builders; AkmBuilder `&mut self`; `match_nearest_neighbor` signature; `DistanceMetric` removal; `OaxacaBuilder` Clone/Debug removal | e6e6dd9, 2d4841b, others | library (B1 to B7) |
| Python feature and pyo3 dependency removal | 3191c4b | library |
| Security: SSE API key, CORS, constant-time compare, header unwraps, lock poisoning, FFI cast | 5dcd681, a79534b, 4135e4f, c28bcc4, 96a71a4, e9dbab5 | MCP |
| Engine and MCP crates creation; `optimization_engine` removal | 33d0e8a, c450713 | workspace |
| 0014 WASM threading and memory fix; 0017 row keys; 0018 snapshot diff; 0096 to 0098 audit fixes | 623ff98 and earlier | Meridian |
| 0117 PR triage | 9e4bd97 | tests and deps |
| 0118 remedy dollars on the wrong employee (only a `row_key` mention) | 567d841 | Meridian, MCP |
| DFL categorical predictors; OLS DOF guard; AKM errors; matching and logit performance work | 975b11e, 4dcac3d, PRs #28 to #64 | library |

### Misdescribed or stale
- `[0.2.2]` lists `allow(dead_code)` as an addition and omits what the release commit says it fixed: probit Hessian inversion, CLI panic, dependency updates (ff8367a).
- Nothing records v0.2.0 or the 0.2.1 gap; the Nov-Dec 2025 work (AKM, matching, formulas, weights, Heckman, JSON/Markdown output) appears nowhere except the one-line 0.2.2 `report` entry.
- `[Unreleased]` sub-headings carry dates and epic ids ("0120-MERIDIAN S6-S9") that mean nothing to an outside reader; 0119 entries are notes on the repo's own CI and build recipe, not user-facing.
- The "Crate versions" line (S1 to S4 block) names `oaxaca_blinder` 0.3.0 and `pay-equity-engine` 0.2.0 and omits `meridian-mcp` 0.2.0 (**verified**, CHANGELOG line 77).

### Tag anomaly and what crates.io holds
| Fact | Evidence |
|---|---|
| crates.io `oaxaca_blinder`: 0.1.0 (2025-09-16, 46 KB), 0.2.0 (2025-11-27, 7.5 MB), 0.2.2 (2025-12-18, 89 KB). No 0.2.1. None yanked. Repository field says `dot-comma-hyphen/oaxaca-blinder-rs`. | **verified** (crates.io API) |
| v0.2.0 tag = afa9d30 (2025-11-26, merge of PR #15). `Cargo.toml` at the tag says `0.1.0`. The bump to 0.2.0 is 5992c9d (2025-11-27), after the GitHub release. crates.io 0.2.0 was published the day after. | **verified** (`git show v0.2.0:oaxaca_blinder/Cargo.toml`) |
| v0.2.2 tag = ff8367a (2025-12-17 21:04 -0500), Cargo 0.2.2; published 2025-12-18 02:06 UTC (same moment). No GitHub release for it; the release workflow did not exist yet. | **verified** |
| v0.2.1 tag = d92753a (2026-03-14 15:15 -0400), a descendant of v0.2.2 (`merge-base --is-ancestor v0.2.2 v0.2.1` true) whose Cargo.toml says 0.2.2. The real 0.2.1 version bump was 05a56f0 (2025-11-28); it was never tagged or published. | **verified** |
| The v0.2.1 tag push fired Release run 23094558247 (2026-03-14), which failed in 21 s. At that commit the workflow ran `cargo publish` first. Cause: logs expired (HTTP 410). Likely a version that already existed or a missing token. | failure **verified**; cause **unverified** |
| GitHub releases: one, v0.2.0 "Major Feature Release", latest. | **verified** (`gh release list`) |

Consequence: `git describe` and git-cliff treat v0.2.1 as the newest ancestor tag of main, so the next tag's `--latest` notes would start at 2026-03-14 and skip Dec to Mar (**unverified**: git-cliff is not installed).
Recommendation: leave the public tags where they are (moving them breaks anyone who pinned them), say in the 0.3.0 entry that 0.2.1 was never published, and build the GitHub release body from the CHANGELOG section, not from git-cliff.

## 5. Publish readiness per crate

Method, **verified**: `git archive origin/main` (6f3113e) into `target/rr/main2/`, then `cargo publish --dry-run -p <crate> --allow-dirty` with `CARGO_TARGET_DIR` and `CARGO_PROFILE_DEV_DEBUG=0` set inside the repo. The archive has no `.git`, so VCS-state checks did not run; the real run must be on a clean tree without `--allow-dirty`.

| Crate | Dry run | Package | Verdict |
|---|---|---|---|
| `oaxaca_blinder` 0.3.0 | exit 0; compiled the packaged crate including the `oaxaca-cli` bin; "aborting upload due to dry run" | 98 files, 605,601 bytes compressed, about 2.4 MB unpacked; largest file `tests/fixtures/employers_trust_fixture.csv` 697 KB; none over 1 MB. Includes tests, fixtures, `Cargo.lock`, benches, examples, `pyproject.toml`, `templates/report.html`. No `include`/`exclude`. crates.io limit is 10 MB. | Publishable after the metadata fixes below |
| `pay-equity-engine` 0.2.0 | exit 101: `dependency oaxaca_blinder does not specify a version` | n/a | Not publishable. Name is free on crates.io (HTTP 404). Recommend `publish = false`: it ships as a WASM blob to Meridian, not as a crate. |
| `meridian-mcp` 0.2.0 | exit 101: `dependency pay-equity-engine does not specify a version` | n/a | Not publishable (depends on the engine). Name free (404). Recommend `publish = false` for 0.3.0; distribute as a GitHub release binary later if wanted (the release job attaches none today). |

### Metadata, `oaxaca_blinder/Cargo.toml` (**verified**)
- `license = "MIT"` but no LICENSE file in the repo or the package (F4). `license-file` not set.
- `repository` and the crates.io page point to `dot-comma-hyphen/oaxaca-blinder-rs`; the public repo is `david-deji/oaxaca-blinder-rs` (whether GitHub redirects the old path is unverified).
- `authors = ["dot-comma-hyphen <poche450@gmail.com>"]`; the engine says `OpenPay Team <contact@openpay.ai>`; `meridian-mcp` has none; `pyproject.toml` repeats the personal address.
- `readme = "README.md"` (crate README, 414 lines); `keywords` 3 of 5 used; `categories = ["science"]`; `description` ends with a space.
- Missing: `homepage`, `documentation`, `rust-version`, `exclude`.

### README accuracy against the current API (**verified** by reading `oaxaca_blinder/README.md` and `README.md`, with the first snippet compiled)
| Claim in README | Status |
|---|---|
| `oaxaca_blinder = "0.1.0"`, `polars = { version = "0.38" }` | Wrong: 0.3.0 and polars 0.44 |
| `.predictors(&["education"])` (also the crate-level docs in `lib.rs` and a second example near line 195) | Does not compile (F1) |
| `cargo install oaxaca_blinder --features cli` | There is no `cli` feature; plain `cargo install oaxaca_blinder` installs `oaxaca-cli` (**source-read**: `[[bin]]` has no `required-features`). Was already wrong in 0.2.2. |
| `maturin develop --features python`; Python examples | Feature gone; `python.rs` not compiled (section 1e) |
| `--weights` example | Correct, includes `--weights-kind` |
| Feature table: Machado-Mata marked supported | Deprecated and off every shipped surface |
| "20-30x faster than R, 10x faster than Python" | **unverified**; no reproducible benchmark is committed that I ran |
| Root README: CI and Release badges use `YOUR_ORG`; "Context Anchor" links `../SYSTEM_ARCHITECTURE.md` and `../MISSION.md` | Broken links to files outside the repo |
| lib.rs doc: "Currently, the library supports numerical predictors" | Stale (categoricals, weights, Heckman exist) |

### docs.rs build risk
- Default features only on docs.rs: `display` (comfy-table). `cargo +nightly rustdoc --lib` with default features built clean in 17 s and 26 s (**verified**). Not run: `--all-features`, `RUSTDOCFLAGS=-D warnings`, the real docs.rs sandbox.
- `mem-profile` installs a `#[global_allocator]`; off by default and never enabled by docs.rs. A downstream crate that enables it replaces its own allocator; documentation should say so (**source-read**).
- `crate-type = ["rlib", "cdylib"]` is unusual for a library others depend on; dependents build only the rlib (**unverified**).
- All lib.rs doc examples are ```ignore, so no doctest guards the README/lib snippets. This is how F1 survived.

### MSRV
No `rust-version` in any manifest, no statement in README or docs. `rust-toolchain.toml` pins 1.90.0 for the repo (WASM reproducibility). **Verified** with the committed `Cargo.lock`: `cargo +1.88.0 check -p oaxaca_blinder --lib --locked` passes; `+1.85.0` fails because `home 0.5.12` (via `polars-io`) needs rustc 1.88. 1.86 and 1.87 untested. Suggest `rust-version = "1.88"` and a CI job on that toolchain.

### Release job (`.github/workflows/release.yml`, **verified** by reading)
- Trigger: push of a tag matching `v[0-9]+.[0-9]+.[0-9]+*`. Any such tag, including `v0.2.1`-style stray tags and pre-releases.
- One job, `github-release`: git-cliff `--latest` over commit subjects into `CHANGES.md`; installs a pinned, sha256-checked syft; generates CycloneDX and SPDX SBOMs; uploads them as a workflow artifact and attaches them to the GitHub Release with the git-cliff body.
- Does not publish to crates.io (removed in 09a4c6a, 2026-03-14), does not upload binaries or WASM, does not wait for CI. Publishing to crates.io is manual: `cargo publish` with a token held outside CI.
- Not exercised since the 0119 rewrite. Its only run was the failed v0.2.1 one (older workflow).
- Open Dependabot PRs touch it: #99 (softprops/action-gh-release 2.4.2 to 3.0.3), #100, #101 (artifact actions), #102, #103. Merge or close before tagging so the release runs on settled actions (**verified**, `gh pr list`).

## 6. Version recommendation

**`oaxaca_blinder`: 0.3.0, not 1.0.0.** This matches David's 2026-10-09 ruling in the ledger (one breaking release carrying C and D, no 1.0 promise yet).
- Section 2 lists 13 library breaks and one deprecation. At 0.x they cost one minor bump; at 1.x each would cost a major.
- Option D (next) will change remedy amounts again and may rewrite `OaxacaResults::optimize_budget` and `BudgetAdjustment`, which are public library API (REM-3, REM-5, REM-7, REM-8 in `ground/2026-10-08-proposal.md`).
- Four library-only modules have no external oracle and are not reachable from Meridian: `akm` (EST-7), `dfl` (EST-6 support trimming), `jmp` (QRI-8 mislabelled), `matching` (EST-5), plus Heckman null handling (EST-1, EST-2) (proposal roadmap; read from the 2026-10-08 proposal, not re-checked). Freezing them at 1.0 freezes unverified behaviour.

**Engine and MCP: version separately from the library, bump both to 0.3.0 for this release, keep `publish = false`.**
- Neither is on crates.io, so no outside user holds a 0.2.0. The number is a label for Meridian. `engine_version` is stamped into every result (6777a17), so a saved project can say which engine made its numbers.
- Their contract is the JSON request/response shape, not Rust items. This release breaks it (required `reference_coefficients`, Pooled target, new fields), so a new minor is honest. After that, bump each only when its own contract changes.
- One workspace tag `vX.Y.Z` equal to the `oaxaca_blinder` version keeps release.yml's tag glob working; record the engine and MCP versions in the release notes.

**What 1.0 would commit the project to**
- Source compatibility of every public item (271 in my walk, trait impls included), including modules that are deprecated (`quantile_decomposition`), alias variants (`Neumark`, `Cotton`) and the polars 0.44 and nalgebra 0.32 types in signatures: a polars major upgrade becomes a breaking release of this crate.
- The `to_json` shape, error codes (`WEIGHTS_KIND_REQUIRED`, `INSUFFICIENT_RESIDUAL_DF`, ...) and CLI flags.
- A numeric policy: whether a number that moves for the same input (section 3) counts as breaking. Today it does not, and S1 to S16 show how often it happens.
- A stated MSRV and a support window.

**Still moving**: Option D (real solver, `adjust_both` closure, reduction above 100%, defensibility sign); the unoracled modules above; Python bindings (dropped or revived); MCP enum fall-through and unreported `bootstrap_reps` clamp (0096 carried seams); `report` flag quirk; integer weights from CSV; the 1.0 candidates `non_exhaustive`, `AsRef<str>` bounds and Clone/Debug on the builder.

Suggested 1.0 gate: D merged and released as 0.4.x; each library-only module either gets an oracle or moves behind an `experimental` feature; the deprecated items removed; one release cycle with no break.

## 7. Release checklist

Steps 1 to 4 can run in parallel; 5 onward are in order. Each step names the check that proves it. Irreversible steps are 11 and 12.

| # | Step | Check that proves it |
|---|---|---|
| 1 | Wait for option D to merge (all of its PRs) and for CI `gate` on the final main commit to be green. PR #105 is merged (6f3113e); its CI run was in progress at review time. | `gh run list --branch main --limit 1` shows CI success for the release commit; `gh pr list --state open` has no engine PR |
| 2 | Fix the code blockers: F1 (`AsRef<str>` bounds), F2 (restore `bootstrap_reps` 100 or document 20 and the NaN CI, and fix the CLI default), F3 (decide GroupA or GroupB default and say it in the docs), F6 (`#[non_exhaustive]`). | A doc-tested snippet using `.predictors(&["education"])` compiles; `cargo test --doc` runs it |
| 3 | Make the README and crate docs true: versions, polars 0.44, install commands, drop Python and YOUR_ORG, replace `ignore` examples with compiled ones. Open-source readiness (section 8) can land in the same release or the next. | Every README code block compiled by a doctest or a CI job; `grep -n "0.1.0\|--features cli\|YOUR_ORG\|features python" README.md oaxaca_blinder/README.md` is empty |
| 4 | Licence and metadata per section 8 and section 5: LICENSE-MIT and LICENSE-APACHE (root and inside `oaxaca_blinder/`), `license = "MIT OR Apache-2.0"`, repository, authors, `rust-version = "1.88"`, `exclude`, `publish = false` on engine and MCP. | `cargo package -p oaxaca_blinder --list` shows both licence files; `cargo publish --dry-run` still exit 0; `cargo metadata` shows publish `[]` for the other two |
| 5 | Merge or close the Dependabot PRs #99 to #103. | `gh pr list --state open` empty; CI green after the last merge |
| 6 | CHANGELOG: rename `[Unreleased]` to `[0.3.0] - <date>`, add the omitted sections of section 4, state that 0.2.1 was never published, write migration lines from section 2. | `grep -c "^## \[0.3.0\]" CHANGELOG.md` is 1; every row of section 4's omitted table has a line |
| 7 | Versions: `oaxaca_blinder` 0.3.0, engine and MCP 0.3.0, `pyproject.toml` removed or aligned, `Cargo.lock` refreshed. | `cargo build --workspace --locked` passes; `grep -rn '^version' */Cargo.toml` as intended |
| 8 | Full local gates on the release commit: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-features --locked`, `scripts/build-wasm.sh --verify`. | All exit 0; `target/wasm-verify.json` says MATCH |
| 9 | API diff against 0.2.2 once more. Install `cargo-semver-checks` (writes to `~/.cargo`, needs David's OK) or rerun `target/rr/apidiff.py`. | `cargo semver-checks check-release -p oaxaca_blinder --baseline-version 0.2.2` lists only the rows of section 2 |
| 10 | Merge the release PR through `gate`; clean tree on the release commit. | `git status` clean at the commit; `cargo publish --dry-run -p oaxaca_blinder` exit 0 without `--allow-dirty` |
| 11 | Publish: `cargo publish -p oaxaca_blinder` (token outside CI, irreversible; a version can only be yanked, never reused). | `curl https://crates.io/api/v1/crates/oaxaca_blinder` lists 0.3.0; docs.rs build page for 0.3.0 is green |
| 12 | Tag `v0.3.0` on the published commit and push the tag. Release workflow runs. | Release run green; `gh release view v0.3.0` shows the notes and both SBOMs |
| 13 | Clean-room smoke test: `cargo install oaxaca_blinder --version 0.3.0`, run `oaxaca-cli --help` and one decomposition; new project from the README snippet. | Both exit 0 in an empty directory |
| 14 | Meridian: publish the WASM from the tagged commit and run the app's receipt. | App `verify-live` receipt pass; `engine-manifest.json` commit equals the tag |
| 15 | Ledger and issue housekeeping: close the release row in `LOOP-LEDGER.md`, mark 0121 items done. | Ledger shows the receipt |

## 8. Open-source readiness (issue 0121-MERIDIAN, decisions L1 to L5)

Added at the coordinator's request. Read-only: nothing below has been moved, written or changed. Facts from `git ls-tree origin/main`, file heads and `git grep`; verdicts are recommendations.

### 8.1 Every root entry and `docs/` file: keep, move, remove

`records/` below is the single folder L5 asks for (name is a suggestion). Moving the loop records has a cost: `scripts/ground.sh`, `scripts/lib/ground_probes.py`, `scripts/lib/verify_live.py` and `scripts/verify-live.sh` hard-code `ground/` and `ground/receipts/`, and CI runs `scripts/test-loop-scripts.py`. The move needs those paths updated in the same commit.

| Path | Verdict | Reason |
|---|---|---|
| `.agent/` (5 skill files) | remove from tree | AI-agent skill definitions; no use to a user or contributor |
| `.cargo/config.toml` | keep, edit | `build-std` config for the threaded WASM build; delete the stale `PYO3_USE_ABI3_FORWARD_COMPATIBILITY` env |
| `.cargo/audit.toml` | keep | `cargo audit` configuration used by CI |
| `.github/` (ci.yml, release.yml, dependabot.yml, PR template) | keep, extend | add issue templates and config (8.4) |
| `.gitignore` | keep, tidy | drop the `check_output_N.txt` lines for files that no longer exist |
| `.idea/` | remove | IDE state |
| `ARCHITECTURE.md` (root, 78 lines) | remove | duplicates `docs/ARCHITECTURE.md` (54 lines); keep one, in the docs site |
| `CHANGELOG.md` | keep | rebuilt per section 4 |
| `CLAUDE.md` | move to `records/` | agent instructions; contains `/home/deji/...` paths |
| `Cargo.lock` | keep | binary-shipping workspace, reproducible WASM |
| `Cargo.toml`, `rust-toolchain.toml` | keep | `rust-toolchain.toml` pins 1.90.0 for the WASM hashes |
| `LICENSE-MIT`, `LICENSE-APACHE` | add | L1 |
| `LOOP-LEDGER.md` | move to `records/` | the loop's own record (L5) |
| `README.md` | rewrite | 8.2 |
| `_epic-0014-close/` (12 files, screenshots) | remove | Meridian app screenshots from a closed epic, not engine documentation; stays in git history |
| `_specify-wasm-rayon-multithreading-2026-07-17/` (52 files) | move to `records/` | design history for the WASM threading and trust layer; has `/home/...` paths; useful to a maintainer, not a user |
| `audit/` (5 files) | move to `records/audits/` | point-in-time audits; shows rigor, not user docs |
| `cliff.toml` | keep or remove with the release-notes decision | section 4 recommends CHANGELOG-based notes, which makes it unused |
| `docs/` | see 8.1b | |
| `dummy_dfl.csv` | remove | scratch data at the root, no reference (**verified** by `git grep`) |
| `engine/` | keep | the Meridian engine; label it "pay-equity-engine, WASM, not on crates.io" in the README |
| `engine/vendor/cc`, `engine/crates/crossterm` | keep, add `THIRD_PARTY` note | vendored third-party code needs its licence text kept |
| `engine/src/fonts/Roboto-Regular.ttf` (297 KB) | keep, add attribution | Roboto is Apache-2.0 (**unverified** in the file itself; check the font's licence file) |
| `ground/` (5 files + receipts) | move to `records/ground/` | L5; update the scripts above |
| `meridian-mcp/` | keep | the MCP server |
| `oaxaca_blinder/` | keep | the library and CLI. Inside: remove `pyproject.toml` (Python not compiled), keep fixtures and goldens |
| `probit_bench.rs` | remove | root-level file in no crate; it imports `oaxaca_blinder::math::probit`, a private module, so it cannot compile (**source-read**) |
| `scripts/` (12 files) | keep, add `scripts/README.md` | WASM build and CI checks are needed; loop tooling stays because CI tests it |
| `verification/` (23 files) | keep, add README | the R and Python oracles and the browser parity harness are the evidence; `verify_heckman.py`, `verify_interpret.py`, `verify_plot.py` import the dead Python bindings: move to `records/` or remove |

#### 8.1b `docs/`
| File | Verdict | Feeds which site page |
|---|---|---|
| `README.md` | remove after the site exists | its index becomes the site's navigation; it still lists "Python Bindings (PyO3)" |
| `API.md` (70 lines) | keep, split | CLI and MCP reference; library reference points to docs.rs |
| `ARCHITECTURE.md` | keep | Contributing > Architecture |
| `AUDIT_REPORT.md` (2026-03-14) | move to `records/audits/` | not user-facing; findings are superseded by the 2026-08-27 audit |
| `CONTRIBUTING.md` | move to root `CONTRIBUTING.md`, rewrite | says MIT only; 30 lines; GitHub surfaces only a root or `.github/` copy |
| `DEVELOPMENT.md` | keep | Contributing > Development setup |
| `DIAGNOSTICS.md` | keep | Method reference > diagnostics, warnings and weights; User guide > reading results |
| `JULES_TASKS.md` | remove | internal agent task plan; links a `file:///home/deji/...` path in another repo |
| `NORMALIZATION.md` | keep | Method reference > categorical normalisation; FAQ |
| `WHITE_PAPER.md` (Dec 2025, "Analytical Systems Team") | rewrite or move | predates 0.3.0; its performance and reliability claims are **unverified**; do not publish as is |
| `oaxaca-blinder-decomposition-research.md` (500 lines), `Quantile-regression-decomposition-research.md` (410), `RIF-regression-decomposition-research.md` (405), `Variance-Inflation-Factor .md` (254, space in the name) | move to `records/research/`; mine for method pages | long literature notes with no checked reference list; every citation must be checked against the primary text before it is published |
| `superpowers/` (2 files) | remove | plan and spec for the March lib.rs refactor, agent working files |

### 8.2 What the README must say that it does not today
1. One paragraph: what it computes (mean and RIF-quantile Oaxaca-Blinder decomposition of a gap between two groups, with bootstrap inference), for whom (economists, HR and pay-equity analysts), and the three ways in: library, CLI, MCP server (plus the WASM engine as a fourth, for Meridian).
2. Status: 0.x, API not stable, numbers can change between minor versions (section 3), link to the CHANGELOG and a 0.2 to 0.3 migration page.
3. Install that works: `cargo add oaxaca_blinder`, `cargo install oaxaca_blinder` (installs `oaxaca-cli`), how to run `meridian-mcp` (stdio, and HTTP with the required `MCP_API_KEY`).
4. A first decomposition that compiles in CI (array form), with the output shown; the CLI line including `--weights-kind`.
5. How the numbers are checked: R `lm`/`predict.lm`, `oaxaca` 0.1.5, `ddecompose` 1.0.0, `quantreg`, `Hmisc`, with the tolerances in the CHANGELOG, and where the generators are (`verification/`). Drop "20-30x faster" unless a reproducible benchmark is committed.
6. What it is not: output is statistical evidence, not a legal compliance finding (API.md already says "does not certify legal compliance"); `warnings[]` are caveats, not blocks.
7. Defaults a reader will hit: `bootstrap_reps`, reference coefficients, seed, the 41-replicate minimum for a percentile CI.
8. Rust version (MSRV), licence (`MIT OR Apache-2.0`), owner and maintainer (L2), how to cite (`CITATION.cff`), how to report a vulnerability, link to the docs site and docs.rs.
9. Python: say plainly it is not available.
10. Remove: `YOUR_ORG` badges, the "Context Anchor" block pointing outside the repo, emoji section headings, the feature table row for Machado-Mata unless marked deprecated.

### 8.3 Docs site table of contents (language: English, L4)

| Page | Existing material that feeds it | Gap |
|---|---|---|
| 1. Start here (what, install, first run for library, CLI, MCP) | README, `oaxaca_blinder/README.md`, `docs/README.md` | rewrite; every snippet compiled in CI |
| 2. User guide | `docs/API.md`, `docs/NORMALIZATION.md`, `docs/DIAGNOSTICS.md`, CHANGELOG 0118/0120 text | data preparation and row accounting; choosing a reference scheme; categorical drivers; weights (frequency vs relative); bootstrap and seeds; quantile analysis; reading `warnings[]`; the remedy and defensibility flow; library-only estimators marked experimental |
| 3. Method reference with citations | the three research notes, VIF note, NORMALIZATION.md, DIAGNOSTICS.md, `verification/gen_*.R` comments | one page per method with formula, citation, how this crate departs from the package it is checked against; citations verified from primary texts. Core references: Oaxaca 1973, Blinder 1973, Reimers 1983, Cotton 1988, Neumark 1988, Jann 2008, Elder et al. 2010, Yun 2005, Gardeazabal and Ugidos 2004, Firpo, Fortin and Lemieux 2009, Imbens and Rubin 2015 (**unverified**: list from memory and from the repo's own notes, not checked) |
| 4. CLI and MCP reference | `docs/API.md`, `oaxaca-cli --help`, MCP tool schemas, `engine/src/types.rs` | generate the CLI page from clap at release; generate MCP tool pages from `tools/list`; error-code table |
| 5. Reproducing results in R and Stata | `verification/*.R`, goldens, CHANGELOG oracle notes | R side exists. **No Stata script is committed** (`git grep -i stata` finds option names in README and CHANGELOG only). Needs a Stata do-file or a worked manual walk-through, and a table: this crate's option, R `oaxaca` weight, `ddecompose` argument, Stata option |
| 6. FAQ | NORMALIZATION.md, DIAGNOSTICS.md, section 3 of this review | why drivers differ from Stata, why a CI is `NaN`, why p is a multiple of 1/reps, seeds, small groups, extrapolation, Python, citing |
| 7. Contributing | `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md`, root CONTRIBUTING, CI gate description | WASM build recipe and reproducibility explained for outsiders |
| 8. Changelog and migration | CHANGELOG, section 2 of this review | one migration page per breaking release |

### 8.4 Community files missing (**verified** against `git ls-tree origin/main`)
`LICENSE-MIT`, `LICENSE-APACHE` (also copies inside `oaxaca_blinder/` so `cargo package` ships them), root `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1; needs a reporting contact address, a decision), `SECURITY.md` (GitHub private vulnerability reporting plus a contact; the MCP server's authentication makes this real), `.github/ISSUE_TEMPLATE/` (bug, feature, `config.yml` pointing questions to the docs site), `CITATION.cff` (title, `authors` with the organisation name Telos Machina, version, date, licence, repository), `THIRD_PARTY` or `NOTICE` for the vendored `cc`, `crossterm` patch and the Roboto font. Exists: PR template. Settings, not files: turn the wiki off (L3), set description, topics and homepage, enable private vulnerability reporting.

Licence caution (**unverified**, not a gate): git history has commits from `David`, `david-deji`, `deji`, `poche450`, `dot-comma-hyphen`, `google-labs-jules[bot]` (51) and dependabot. The first five look like one person's accounts (shared email `poche450@gmail.com` in Cargo authors); not confirmed. Adding Apache-2.0 to code that was MIT is normally covered by the MIT grant, but the bot-authored commits and "Telos Machina as owner" (L2) each deserve one line of confirmation from the person who knows the terms.

### 8.5 Crate metadata fixes

| Field | `oaxaca_blinder` | `pay-equity-engine` | `meridian-mcp` |
|---|---|---|---|
| `repository` | `https://github.com/david-deji/oaxaca-blinder-rs` (now `dot-comma-hyphen/...`) | add same | add same |
| `authors` | `["Telos Machina"]` (now `dot-comma-hyphen <poche450@gmail.com>`); same in `pyproject.toml` if kept | replace `OpenPay Team <contact@openpay.ai>` | add |
| `license` | `"MIT OR Apache-2.0"` | add (none today) | add (none today) |
| `homepage` | docs site URL on GitHub Pages | add | add |
| `documentation` | `https://docs.rs/oaxaca_blinder` | omit (unpublished) | omit |
| `description` | strip trailing space; one sentence | add | add |
| `keywords` | `["oaxaca-blinder", "econometrics", "statistics", "decomposition", "pay-equity"]` (now 3 of 5) | n/a | n/a |
| `categories` | `["science", "mathematics"]` (now `["science"]`) | n/a | n/a |
| `rust-version` | `"1.88"` (section 5) | same | same |
| `publish` | default | `false` | `false` |
| other | add `exclude` for `pyproject.toml`; decide whether 700 KB test fixtures ship | | |

### 8.6 Docs-site tool: mdBook
mdBook is the Rust-native choice: one binary in the existing Actions pipeline, plain markdown from the repo, no Node or Python toolchain, math through the `mdbook-katex` preprocessor (the method pages need formulas), and versioned output is a per-release directory pushed to `gh-pages` (`/v0.3/`, `/latest/`). MkDocs Material has nicer built-in versioning (`mike`) but adds a Python dependency for a project whose CI is Rust and Node. Not tested here; install and a sample build are unverified.

