# Release plan: oaxaca-blinder-rs 0.3.0 (issue 0121-MERIDIAN)

> Written 2026-10-16. Plan only: nothing tagged, published, or changed in GitHub settings.
> Measured at `origin/main` d614d68 (PR #110, 2026-10-16 11:45 -0400). The inventory
> (`ground/2026-10-09-release-review.md`) was measured at 6f3113e; 5 PRs (#106 to #110), 52 files and
> +38,687 / -428 lines have landed since. 190 commits and 325 files since tag v0.2.0.
> Binding: issue 0121 decisions L1 to L7. Target 0.3.0, no 1.0 promise.
> Evidence marks: **verified** (command run for this plan), **source-read** (read in code, CHANGELOG
> or commits at d614d68, not executed).

## 0. Blockers before any PR in this plan can merge

| # | Blocker | Evidence | What clears it |
|---|---|---|---|
| X1 | CI `gate` is red on main. Security Audit fails on 7 advisories published after 2026-10-10: quinn-proto 0.11.14 (RUSTSEC-2026-0338, -0339, -0340, -0342; fixed in 0.11.17/0.11.18) and rustls 0.23.35 (RUSTSEC-2026-0346, -0348, -0349; fixed in 0.23.46). All are vulnerabilities, so `cargo audit` fails on pull requests too: every PR in this plan, this one included, has a red `gate` until it is cleared. | **verified**: main run 38926548245 (push of #110), job 117059434135 log; #110's own PR checks ran on 2026-10-10, before the advisories | P0 below. Both crates are lock-only (reqwest via polars-io / object_store optional http; `cargo tree -i` prints nothing, **verified**), so a lock bump changes no compiled artifact. |
| X2 | Five Dependabot PRs open (#99 to #103). | **verified** (`gh pr list`) | Swept before P4 (section 6). |
| X3 | The dev box holds an uncommitted re-run of `ground/2026-10-09-probes.json` (+73/-60) and untracked `ground/raw/`. P1 moves `ground/`, so after P1 merges `git pull --ff-only` on the dev box refuses until that change is committed or dropped by whoever produced it. | **verified** (`git status`, `git diff --stat`) | Its owner commits or drops it before P1 merges. P1 itself is built in a separate worktree and never touches it (section 2). |
| X4 | The dev loop's bookkeeping (`telos-machina/scripts/dev-loop/loop.py`, lines 64, 120, 143, 149, 158, 176, 196, 221, 275, 352) hard-codes `<app>/ground/` and `<app>/LOOP-LEDGER.md`. L5 moves both into one folder. | **verified** (grep) | P1-T below lands in the telos-machina repo before P1 merges. |

### P0: clear the Security Audit (own PR, before P1)

Triage step 1 of `.cargo/audit.toml` (a fixed version the lockfile can take):
`cargo update -p quinn-proto --precise 0.11.18` and `cargo update -p rustls --precise 0.23.46`
(needs the crates.io index), then delete the two older ignores for the same crate versions
(RUSTSEC-2026-0185 quinn-proto, RUSTSEC-2026-0285 rustls): once 0.11.14 and 0.23.35 leave the lockfile the
validator reports them as stale and turns CI red. Adding seven more `lock-only` ignores was tried for this
plan and refused by this session's safety check as a CI bypass; the lock bump is the actual fix.

Files: `Cargo.lock`, `.cargo/audit.toml`.
Proof: `cargo audit --deny warnings` exit 0; `python3 scripts/validate-audit-ignores.py --no-fetch` exit 0
after a fetching `cargo audit`; `git diff Cargo.lock` touches only quinn-proto, rustls and what they pull;
CI WASM Sequential / Threaded hash compares green with no `.sha256` change (the crates are not in the
WASM graph). If the bump drags a compiled crate along, the hash compare goes red: stop and report.

### P1-T: dev loop reads the records folder (telos-machina repo, before P1)

`scripts/dev-loop/loop.py`: resolve the records root once, `<app>/records/` when
`<app>/records/LOOP-LEDGER.md` exists, else `<app>/` (other apps unchanged). Update the engine row of
`internal-ops-bureau/knowledge/dev-loop.md` § Per-app parameterization and the setup check in
`.claude/skills/davids-loop/SKILL.md`. Proof: `loop.py` run against a temp copy of the engine tree with
`records/` writes `records/ground/picks.jsonl`; run against pay-equity-app writes `ground/` as today.

## 1. Re-measure: what 0122 changed in the inventory

0122-MERIDIAN ("the remedy says what it does", PRs #107 to #110) is option D; with it merged, inventory
checklist step 1 (wait for D) is done. 0120's live receipt (#106) is the other new commit.

### 1.1 Breaking changes (inventory section 2), additions

Library `oaxaca_blinder` (baseline 0.2.2). No new compile break; 0122 adds one deprecation and one
runtime behaviour change. The library's public API diff is otherwise unchanged (`types.rs` +20 lines,
`python.rs` +1, both **source-read**).

| # | Change | Migration | Bump |
|---|---|---|---|
| B15 | `OaxacaResults::optimize_budget` is `#[deprecated(since = "0.3.0")]` (allocates on the decomposition's residuals, targets the raw `total_gap`) | `#[allow(deprecated)]`, or move to the engine's `optimize` (not a published crate, see section 7) | none (deprecation) |
| B16 | `optimize_budget` with a NaN, infinite or negative budget, or a non-finite `target_gap`, returns no adjustments. A NaN budget used to fund the full need. | pass a finite budget | minor (0.x), behaviour |

CLI: no change from 0122 (`oaxaca_blinder/src/main.rs` untouched since 6f3113e, **verified** by diff).

Engine and MCP (never released; first publication of the JSON contract). 0122 adds, all **source-read**
from CHANGELOG at d614d68:
- `check_defensibility` gaps (`original_gap`, `new_gap`, `original_unexplained_gap`, `new_unexplained_gap`) flip sign to `optimize`'s convention: compared minus line, negative while underpaid.
- `required_budget` is the compared group's need in both entry points; `total_cost` stays money spent on both groups. New fields `cost_target`, `cost_reference`, `need_target`, `need_reference`, `target_line`.
- `target_gap` is read and derives the budget; new result fields `best_reachable_gap`, `target_gap_reachable`, `shortfall_to_target`, `target_budget`, `budget_binding`, `unfunded_amount`, `unfunded_count`, `threshold_excluded_count`, `closure`, `overshoot_mean`.
- Refused by name: `INVALID_BUDGET`, `INVALID_TARGET_GAP`, `INVALID_MIN_GAP_PCT`, `TARGET_GAP_WITH_REFERENCE_RAISES`, `INVALID_STEPS`; MCP `UNKNOWN_TARGET`, `UNKNOWN_STRATEGY`, `UNKNOWN_RANGE_TARGET`. This closes the inventory's open MCP seam ("unknown `target` strings fall through to Reference") and the ledger's 0096 MEDIUM-1 enum fall-through for these three arguments; the ledger still lists MEDIUM-1 as open.
- Each adjustment gains `source`, `range_position`, `range_position_before`; `check_defensibility` gains `position_counts` and `group_test` (closes 0120 T8's "not built" note).
- `EfficientFrontierRequest` gains `strategy`, `target`, `range_target`, `min_gap_pct`, `adjust_both_groups`.
- Engine enums `OptimizationTarget`, `AllocationStrategy`, `RangeTarget` derive `Serialize`, `Clone`, `Copy`, `PartialEq`.

### 1.2 Statistical changes a published result would show (inventory section 3), additions

All engine/MCP (the remedy, defensibility and frontier paths). Numbers are 0122's own probes as recorded
in the CHANGELOG (**source-read**); not re-run here.

| # | Change | Probe or source |
|---|---|---|
| S17 | `check_defensibility` gap signs flipped on all four gap fields | CHANGELOG 0122 |
| S18 | `adjust_both_groups` no longer credits reference raises to the compared group | probe roster: remaining gap +2,103 now -434 |
| S19 | `new_unexplained_gap` is the gap on the line refitted to the schedule's wages (the reference line, and under `Pooled` the group coefficient, move) | equals R `lm` refit to 1e-6 on 62 cases (`verification/gen_remedy_goldens.R`) |
| S20 | `required_budget` differs between `optimize` and `check_defensibility` when `range_target` or `min_gap_pct` is set | 4 % threshold on the probe roster: 16,884 against 19,607 |
| S21 | `target_gap` derives the budget; already met pays 0 (a budget of 0 used to read as no cap and pay the full need); out of reach pays every eligible shortfall | CHANGELOG 0122; brute-force oracle `verification/gen_remedy_bruteforce.py` |
| S22 | A negative or non-finite `budget` is refused (used to fund the full need) | `INVALID_BUDGET` |
| S23 | Under `LowerBound` / `UpperBound`, the unexplained gaps are read on the midpoint line, and reference employees raised with `adjust_both_groups` go to the same point of their own range (they went to the midpoint) | CHANGELOG 0122 |
| S24 | Frontier: `Equitable` recomputed per step, ties paid in `optimize`'s order, default budget axis ends at the remedy's full cost (was 1.1x), `confidence_level` also sets the interval a bound-target remedy pays to | CHANGELOG 0122 |
| S25 | A misspelt `strategy` / `target` / `range_target` at MCP is refused (a misspelt `"equitable"` ran Greedy) | CHANGELOG 0122 |
| S26 | Library `optimize_budget`: NaN budget pays nothing (paid the full need) | `oaxaca_blinder/tests/optimize_budget_test.rs` case 5 |
| S27 | Result serialisation: an empty sum is `0.0`, no longer `-0.0` | CHANGELOG 0122 |

Unchanged by 0122: the decomposition itself (S1 to S16 stand as written). Byte parity native vs WASM and
the committed WASM hashes were green on #110's PR run (**verified**, run 38092533223).

### 1.3 CHANGELOG audit (inventory section 4), delta

- `[Unreleased]` grew from 195 to 258 lines (**verified**): a 63-line "Changed, BREAKING (0122-MERIDIAN ...)" block at the top. It is engine/MCP-facing and written for Meridian: epic ids and internal tags (REM-3, C-09, T10, D6) in headings and body, probe-roster numbers without the roster. The 0.3.0 rewrite keeps its facts and drops the ids.
- Still no `[0.2.0]`, `[0.2.1]` or `[0.3.0]` heading. The "Crate versions" line moved to line 140 and still omits `meridian-mcp`.
- Every omission in the inventory's table still holds (no 0014, 0017, 0018, 0096 to 0098, 0117 entries; seed, CORS, Python removal, Machado-Mata, Hessian still absent).
- The library deprecation (B15) is recorded, under an engine-centred heading; it moves to the library section.

### 1.4 Inventory facts that changed or were new at d614d68

| Fact | Evidence |
|---|---|
| F1 to F7 all still open: `Into<String>` bounds (8 methods), library default `bootstrap_reps: 20` (`builder.rs` 347, 381), CLI default 50 (`main.rs` 169), no LICENSE file, unversioned path deps, no `#[non_exhaustive]` anywhere, release.yml unchanged | **verified** (grep at d614d68) |
| `engine/src/fonts/Roboto-Regular.ttf` (297 KB) is a saved GitHub HTML page, not a font, and nothing references it. The inventory's "keep, add attribution" becomes "remove". | **verified** (`<!DOCTYPE html>` header; no reference in any crate) |
| `engine/vendor/cc/` (4 files from the `cc` crate) is referenced by no manifest or config; only the `crossterm` stub under `engine/crates/` is patched in. Remove `vendor/cc`; no THIRD_PARTY file is then needed (the crossterm stub is local code). | **verified** (grep of Cargo.toml files and `.cargo/`) |
| Three editor backups are tracked: `meridian-mcp/Dockerfile.~1~`, `meridian-mcp/docker-compose.yml.~1~`, `meridian-mcp/src/main.rs.~1~` | **verified** (`git ls-tree`) |
| `meridian-mcp/src/main.rs` line 598 hard-codes `"version": "0.2.0"` in its server info | **verified** |
| `engine/tests/remedy_rules_test.rs` (0122) reads a fixed list of docs (`README.md`, `ARCHITECTURE.md`, `CLAUDE.md`, `oaxaca_blinder/README.md`, `docs/API.md`, `docs/DIAGNOSTICS.md`, `docs/README.md`) and fails on statute citations and stale remedy wording. Moving or deleting any of them breaks it; P3 must point it at the book. | **verified** (lines 916 to 948) |
| The R and Python oracle tests recompute the sha256 of their generators (`verification/gen_*.R`, `gen_*.py`). Editing a generator, even a comment, fails them. No PR in this plan touches a generator. | **source-read** (0120 normalisation oracle, 0122 remedy oracle CHANGELOG notes) |
| The engine builds its decompositions with an explicit `bootstrap_reps` (`analysis.rs` 226, 299, 374); its other `OaxacaBuilder`s only extract matrices. Raising the library default changes no engine number. | **verified** (grep; only one `.run()` in `engine/src`, at 376, after `bootstrap_reps(reps)`) |
| Engine and MCP do not use `akm`, `dfl`, `jmp` or `matching`. The CLI does (`--analysis-type akm|match`, `main.rs` 31-32, 381-432). | **verified** (grep) |
| `engine/tests/fixtures/remedy_goldens_r.json` is 1.0 MB; the engine is not packaged for crates.io, so it ships nowhere | **verified** |

## 2. Rules every PR follows

- Each PR is built in its own worktree inside the repo: `git worktree add target/wt-<pr> -b release/<pr> origin/main`, with `CARGO_TARGET_DIR=<repo>/target` (shared cache, inside the repo) and `CARGO_PROFILE_DEV_DEBUG=0`. The main checkout, its modified probe file and `ground/raw/` are never touched.
- Commits name explicit paths; moves with `git mv`, removals with `git rm`. Message `type(0121-MERIDIAN): ...` ending `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; PR body ends with the Claude Code line.
- Merge: poll `gh pr checks` every few minutes until `gate` is final; `gh pr merge --squash` on green only (never `--admin`); then remove the worktree and branch the next PR from the new `origin/main`.
- Local gates before pushing: `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`, `cargo test --locked --workspace --all-features`, `python3 scripts/check-ci-invariants.py`, `python3 scripts/test-ci-invariants.py`.
- WASM hashes: if a PR changes the raw WASM bytes (P2 may, P4 will), take the new sha256 from the CI run's `wasm-raw-seq` / `wasm-raw-threaded` artifacts (`raw-sha256.txt`), confirm it equals a local `bash scripts/build-wasm.sh --record --no-publish`, and commit both `engine/*.wasm.sha256` in the same PR. Allowed only while Browser Byte-Parity and every engine golden stay green: the bytes moved, the numbers did not.
- Never edit an oracle generator or a golden. A golden that moves is a stop-and-report, except the library-default tests P2 names.
- No tag, no `cargo publish` (only `--dry-run`), no repo settings, no Pages, no `gh-pages` push.

## 3. P1: repo hygiene and metadata (L1, L2, L5, F4, F5; community files)

Built after P0 and P1-T, in its worktree (X3).

| Action | Paths |
|---|---|
| Add | `LICENSE-MIT`, `LICENSE-APACHE`; copies `oaxaca_blinder/LICENSE-MIT`, `oaxaca_blinder/LICENSE-APACHE` (only the published crate packages them); `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1, reporting address per section 9); `SECURITY.md` (GitHub private vulnerability reporting plus the same address; supported version 0.3.x; the MCP server's HTTP key is in scope); `CITATION.cff` (title, `authors: - name: "Telos Machina"`, version 0.3.0, licence `MIT OR Apache-2.0`, repository URL, date filled at P4); `.github/ISSUE_TEMPLATE/bug_report.yml`, `.github/ISSUE_TEMPLATE/feature_request.yml`, `.github/ISSUE_TEMPLATE/config.yml` (questions to the docs site); `records/README.md` (what the loop records are, who writes them, that they are not user docs); `scripts/README.md`; `verification/README.md` (each oracle, its generator, its fixture, its tolerance) |
| Move (`git mv`) | `docs/CONTRIBUTING.md` -> `CONTRIBUTING.md` (rewritten: licence, `gate`, local gates, WASM hash rule, generators are never edited); `LOOP-LEDGER.md` -> `records/LOOP-LEDGER.md`; every tracked file under `ground/` -> `records/ground/` (same names, `receipts/` included); `_specify-wasm-rayon-multithreading-2026-07-17/` -> `records/design/wasm-threading-2026-07-17/`; `audit/README.md`, `audit/2026-08-27/*` -> `records/audits/`; `docs/AUDIT_REPORT.md` -> `records/audits/2026-03-14-audit-report.md`; `docs/oaxaca-blinder-decomposition-research.md`, `docs/Quantile-regression-decomposition-research.md`, `docs/RIF-regression-decomposition-research.md` -> `records/research/`; `docs/Variance-Inflation-Factor .md` -> `records/research/variance-inflation-factor.md`; `docs/WHITE_PAPER.md` -> `records/research/2025-12-white-paper.md` (its claims are unverified; not published) |
| Remove (`git rm`) | `.agent/` (5), `.idea/` (3), `_epic-0014-close/` (12), `dummy_dfl.csv`, `probit_bench.rs`, `docs/JULES_TASKS.md`, `docs/superpowers/` (2), `oaxaca_blinder/pyproject.toml`, `oaxaca_blinder/src/python.rs` (not compiled since 3191c4b), `meridian-mcp/Dockerfile.~1~`, `meridian-mcp/docker-compose.yml.~1~`, `meridian-mcp/src/main.rs.~1~`, `engine/src/fonts/Roboto-Regular.ttf`, `engine/vendor/cc/` (4), `verification/verify_heckman.py`, `verification/verify_interpret.py`, `verification/verify_plot.py` (import the dead Python binding) |
| Edit, metadata | `Cargo.toml`: `[workspace.package]` with `license = "MIT OR Apache-2.0"`, `authors = ["Telos Machina"]`, `repository = "https://github.com/david-deji/oaxaca-blinder-rs"`, `homepage = "https://david-deji.github.io/oaxaca-blinder-rs/"`, `rust-version = "1.88"`, `edition = "2021"`. `oaxaca_blinder/Cargo.toml`: inherit those; `documentation = "https://docs.rs/oaxaca_blinder"`; description one sentence, no trailing space; `keywords = ["oaxaca-blinder", "econometrics", "statistics", "decomposition", "pay-equity"]`; `categories = ["science", "mathematics"]`; `include = ["/src", "/templates", "/README.md", "/LICENSE-MIT", "/LICENSE-APACHE"]` (tests, fixtures, benches and examples stay in the repo). `engine/Cargo.toml`: inherit; description; `publish = false`; `oaxaca_blinder = { path = "../oaxaca_blinder", version = "0.3.0" }` (F5). `meridian-mcp/Cargo.toml`: inherit; description; `publish = false`; `pay-equity-engine = { path = "../engine", version = "0.2.0" }` (F5; P4 moves it to 0.3.0) |
| Edit, paths and stale text | `scripts/ground.sh`, `scripts/lib/ground_probes.py`, `scripts/lib/verify_live.py` (also its dirty-tree prefix, line 296), `scripts/verify-live.sh`, `scripts/test-loop-scripts.py`: `ground/` -> `records/ground/`, `LOOP-LEDGER.md` -> `records/LOOP-LEDGER.md`. `.gitignore`: drop the eight `check_output*.txt` lines, fix the `ground/` comment. `.cargo/config.toml`: drop `PYO3_USE_ABI3_FORWARD_COMPATIBILITY`. `CLAUDE.md`: stays at the root (Claude Code reads it there; `.claude` is gitignored), rewritten as notes for coding agents: machine paths and the `python` feature line removed, new paths. `.github/PULL_REQUEST_TEMPLATE.md`: the repo's real checklist (gate, WASM hashes, CHANGELOG line, no generator edits) |
| Edit, CI | `.github/workflows/ci.yml`: new gating job `msrv` (`dtolnay/rust-toolchain` at the already-pinned nightly-branch SHA with `toolchain: 1.88.0`; `cargo +1.88.0 check --locked -p oaxaca_blinder --lib`; `timeout-minutes`), added to `gate.needs` |

Not in P1: root `ARCHITECTURE.md` and the remaining `docs/*.md` (P3 moves them with the remedy-wording test); `cliff.toml` (P4).

## 4. P2: library fixes (F1, F2, F3, F6, L6)

| Item | Change | Paths |
|---|---|---|
| F1 | `S: Into<String>` -> `S: AsRef<str>` (convert with `.as_ref().to_string()`) on `OaxacaBuilder::{predictors, categorical_predictors, normalize, heckman_selection}`, `QuantileDecompositionBuilder::{predictors, categorical_predictors}`, `AkmBuilder::controls`, `MatchingEngine::new`. Accepts `&["a"]`, `["a"]`, `vec![String]`, `&Vec<String>`, and the engine's existing `.iter().copied()` / `.map(String::as_str)` calls | `oaxaca_blinder/src/builder.rs`, `src/quantile_decomposition.rs`, `src/akm.rs`, `src/matching/engine.rs` |
| F1 test | New compile-and-run test calling all eight methods with each argument form above; the `&["education"]` form from 0.2.2 is the one that must pass | `oaxaca_blinder/tests/api_compat_test.rs` (new) |
| F2 | Library default `bootstrap_reps` 20 -> 100 in both constructors; rustdoc on `new`, `from_formula` and `bootstrap_reps` states 100, the 41-replicate floor for a percentile interval, and `DEFAULT_SEED`. CLI `--bootstrap-reps` default 50 -> 100, so library and CLI share one default; `--help` states it. 0.2.2's library default was 100 and its CLI default 500; the CHANGELOG says both | `src/builder.rs`, `src/main.rs`, `src/rng.rs` (doc only) |
| F2 test | A run with no `bootstrap_reps` call records `bootstrap_reps_requested == 100` and a finite percentile interval | `tests/api_compat_test.rs` |
| F3 | Default reference coefficients stay `GroupB`; rustdoc on `new` and `reference_coefficients` says so and names 0.2.2's `GroupA`; test asserts the default | `src/builder.rs`, `src/decomposition.rs` (doc), `tests/api_compat_test.rs` |
| F6 | `#[non_exhaustive]` on `OaxacaError` and `ReferenceCoefficients`; also on the public enums new since 0.2.2 (`WeightsKind`, `NormalizationConvention`, `ExclusionReason`), where it costs nothing now and a break later. Downstream matches in `engine` (e.g. `engine/src/rows.rs` 25) get a wildcard arm if they lack one | `src/error.rs`, `src/decomposition.rs`, `src/math/weights.rs`, `src/math/normalization.rs`, `src/rows.rs`, `engine/src/rows.rs` (if needed) |
| L6 | Feature `experimental = []`, off by default. `#[cfg(feature = "experimental")]` on `pub mod akm, dfl, jmp, matching` and their re-exports; `#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]` and a module doc line "Experimental: no external oracle; results may change in any release". CLI `--analysis-type akm|match` and `run_akm_analysis` / `run_matching_analysis` behind the same cfg. Helpers left unused by the default build (`math::kde`, `math::logit`, used by `dfl`) gated or `cfg_attr(not(feature = "experimental"), allow(dead_code))`. `[package.metadata.docs.rs] features = ["experimental"]`, `rustdoc-args = ["--cfg", "docsrs"]`; `#![cfg_attr(docsrs, feature(doc_cfg))]` in `lib.rs` | `oaxaca_blinder/Cargo.toml`, `src/lib.rs`, `src/akm.rs`, `src/dfl.rs`, `src/jmp.rs`, `src/matching/mod.rs`, `src/main.rs`, `src/math/mod.rs` |
| L6 tests | `#![cfg(feature = "experimental")]` on `tests/test_akm.rs`, `tests/matching_test.rs`; `#[cfg(feature = "experimental")]` on `test_jmp_decomposition` and `test_dfl_reweighting` in `tests/features_test.rs` (its `test_reference_groups` stays core); `[[bench]] distance_bench` gets `required-features = ["experimental"]` | those files, `oaxaca_blinder/Cargo.toml` |
| L6 CI | `quality` already builds and tests with `--all-features`, so the experimental code stays compiled and tested. Add to `quality`: `cargo clippy --locked -p oaxaca_blinder --all-targets -- -D warnings` and `cargo test --locked -p oaxaca_blinder` (default features: proves the default build is clean without it) | `.github/workflows/ci.yml` |

Allowed number changes: library tests that relied on the default of 20 (`features_test.rs`, `null_handling_test.rs`, `optimize_budget_test.rs`, one run in `row_accounting_test.rs`, `test_akm.rs`) now run 100 replicates; their assertions are on point estimates or shapes and must pass unchanged. Every R golden, the parity golden and Browser Byte-Parity stay green. The raw WASM may change bytes (the default constant is compiled in): section 2's hash rule.

## 5. P3: documentation (L3, L4; inventory 8.2, 8.3, 8.6)

| Item | Paths |
|---|---|
| mdBook source under `docs/` | `docs/book.toml` (title, `src = "src"`, `[output.html]` with `git-repository-url`, `mathjax-support = true`, so no preprocessor binary is needed); `docs/src/SUMMARY.md` |
| Pages (8.3) | `docs/src/introduction.md` (what it computes, for whom, the four ways in, status 0.x); `start-here.md` (install, first decomposition from library, CLI and MCP); `user-guide/`: `data-preparation.md` (row accounting), `reference-schemes.md`, `categorical-drivers.md`, `weights.md`, `bootstrap-and-seeds.md` (defaults 100, the 41 floor, seeds), `quantile.md`, `remedy.md` (engine/MCP only), `experimental.md` (akm, dfl, jmp, matching: opt-in, no oracle); `method/`: `oaxaca-blinder.md`, `reference-coefficients.md`, `normalization.md`, `rif-quantile.md`, `heckman.md`, `references.md`; `reference/`: `cli.md` (+ generated `cli-help.txt`), `mcp.md` (+ generated `mcp-tools.json`), `diagnostics.md`, `error-codes.md`; `reproducing/`: `r.md`, `stata.md`; `faq.md`; `contributing/`: `architecture.md`, `development.md`, `ci-and-wasm.md`; `changelog.md` (`{{#include ../../CHANGELOG.md}}`); `migrating-0.2-to-0.3.md` (stub filled at P4) |
| Moves into the book (`git mv`, then edit) | `docs/API.md` -> `docs/src/reference/mcp.md` (CLI part split to `cli.md`); `docs/DIAGNOSTICS.md` -> `docs/src/reference/diagnostics.md`; `docs/NORMALIZATION.md` -> `docs/src/method/normalization.md`; `docs/ARCHITECTURE.md` -> `docs/src/contributing/architecture.md` (merged with the root `ARCHITECTURE.md`, which 0122 corrected and which is then `git rm`'d); `docs/DEVELOPMENT.md` -> `docs/src/contributing/development.md`; `docs/README.md` `git rm` |
| Snippets that compile | Every Rust snippet in the book is an example file included with `{{#include}}`: `oaxaca_blinder/examples/first_decomposition.rs`, `weights_and_categoricals.rs`, `quantile_rif.rs` (new; inline `df!` data, no file paths). CI builds examples already (`--all-targets`) |
| Generated references kept true | Test that runs `oaxaca-cli run --help` and `report --help` and compares with `docs/src/reference/cli-help.txt`; test that takes the MCP `tools/list` JSON and compares with `docs/src/reference/mcp-tools.json`. A drifted page fails CI | `oaxaca_blinder/tests/cli_help_doc_test.rs`, `meridian-mcp` test (new) |
| README rewrite (8.2) | `README.md` (repo front page) and `oaxaca_blinder/README.md` (crates.io page): the ten points of 8.2; badges on `david-deji`; no Context Anchor, no `YOUR_ORG`, no Python section, no speed claim without a committed benchmark; defaults table; licence, MSRV, citation, security, docs site and docs.rs links |
| README and crate docs doctested | `oaxaca_blinder/src/lib.rs`: `#![doc = include_str!("../README.md")]` (crate docs on docs.rs are the crate README; its snippets become doctests); `#[cfg(doctest)] #[doc = include_str!("../../README.md")] pub struct RootReadme;` for the root README. Every ```` ```ignore ```` block becomes a real one or ```` ```text ````. The stale "supports numerical predictors" line goes |
| Remedy-wording guard follows the docs | `engine/tests/remedy_rules_test.rs` `remedy_text_files()`: replace `ARCHITECTURE.md`, `docs/API.md`, `docs/DIAGNOSTICS.md`, `docs/README.md` with every `docs/src/**/*.md`; keep the READMEs, `CLAUDE.md`, `CHANGELOG.md`. The book must pass it: no statute citations |
| CI | New gating job `docs`: install a pinned `mdbook` (prebuilt via `taiki-e/install-action` at the SHA already pinned; exact version recorded in the step), `mdbook build docs`, `mdbook test docs` is not used (snippets are examples); `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p oaxaca_blinder --no-deps --all-features`; `cargo test --locked -p oaxaca_blinder --doc` (default features). Added to `gate.needs` |
| Citations | The references page lists only works whose bibliographic entry was checked against a primary source. That check needs web access, which the PR executor does not have: the orchestrator runs it (research tool) while P3 is in review and hands the verified list over; unchecked entries are left out, not marked |
| Stata | No Stata is installed or committed. `reproducing/stata.md` gives the option mapping table (this crate, R `oaxaca`, `ddecompose`, Stata `oaxaca`) and a do-file in the page, stated as not run in CI |

## 6. Dependabot sweep (before P4)

#103 (rust-cache), #102 (rust-toolchain SHA): rebase, gate, merge. #100, #101 (download-artifact 8, upload-artifact 7, majors): merge only on a green gate with the browser-parity artifact hand-off intact; otherwise close with the failing job named. #99 (action-gh-release 3.0.3): closed, superseded by P4, which pins that version's SHA read from the PR's own diff.

## 7. P4: release (CHANGELOG, versions, F7)

| Item | Paths |
|---|---|
| CHANGELOG 0.3.0 | `CHANGELOG.md`: `[Unreleased]` becomes `## [0.3.0] - <merge date>` with: summary; **Migrating from 0.2** (B1 to B16 with before/after code, CLI flag changes, removed Python feature); **Statistical changes a published result would show** (S1 to S27, regrouped by user: library, CLI, engine/MCP; relative to 0.2.2 the library default reps is unchanged at 100 and S2 reduces to the 41-replicate floor); Added, Changed, Deprecated, Removed, Fixed, Security (MCP key, CORS, constant-time compare); engine and MCP as a first contract; the omitted themes of inventory 4; experimental modules named. Epic ids and internal tags out. Below it: `## [0.2.2] - 2025-12-18` corrected (probit Hessian, CLI panic, dependency updates), `## [0.2.1]` "never published to crates.io; tag explained below", `## [0.2.0] - 2025-11-27` reconstructed from inventory 1(a). A "Tags" note: tag v0.2.0 is on afa9d30, whose Cargo.toml says 0.1.0 (the 0.2.0 bump was 5992c9d, a day later); tag v0.2.1 is on d92753a (2026-03-14), a later commit carrying 0.2.2's version, pushed by mistake; the real 0.2.1 bump (05a56f0) was never tagged or published. No tag is moved. `docs/src/migrating-0.2-to-0.3.md` holds the migration part |
| Versions | `engine/Cargo.toml` and `meridian-mcp/Cargo.toml` 0.3.0; path-dependency versions 0.3.0; `meridian-mcp/src/main.rs` server info `env!("CARGO_PKG_VERSION")`; `CITATION.cff` version and date; `Cargo.lock` (workspace versions only). `oaxaca_blinder` is already 0.3.0. The engine version is stamped into every result (`analysis.rs` 27), so the raw WASM changes: section 2's hash rule, both `engine/*.wasm.sha256` |
| release.yml (F7) | On a `v*` tag: `check-tag` (tag equals `v` + `oaxaca_blinder` version, else fail; the tag commit's `gate` check run is `success`, read with `gh api`); `verify` (fmt, clippy, `cargo test --workspace --locked --all-features`, `cargo publish --workspace --dry-run --locked`); `publish-crates` (`cargo publish --workspace --locked` with `CARGO_REGISTRY_TOKEN`: dependency order, crates with `publish = false` skipped); `binaries` (matrix x86_64-unknown-linux-gnu, aarch64-apple-darwin, x86_64-pc-windows-msvc; `oaxaca-cli` and `meridian-mcp` built `--release --locked`, archived with both licence files and a sha256); `github-release` (body = the tag's CHANGELOG section, extracted by script; binaries and both SBOMs attached; syft steps kept as they are); `book` (`mdbook build docs`, then commit the output to the `gh-pages` branch under `v0.3/` and `latest/` with a root redirect, so older versions stay; Pages is set to serve that branch after David's review). `cliff.toml` `git rm` |
| CI guard follows | `scripts/check-ci-invariants.py`: `JOB_PERMISSIONS` rows for the new release jobs (`contents: write` for `github-release` and `book`, `checks: read` for `check-tag`, nothing else, still no `id-token`); `scripts/test-ci-invariants.py`: its release.yml mutations (anchored on `syft --version`, the job's permissions) re-pointed at the new file. `scripts/release-notes.py` (new) plus its test in `quality` |

## 8. Which crates publish (for David, before anything is published)

| Crate | Recommendation | Reason |
|---|---|---|
| `oaxaca_blinder` 0.3.0 | **Publish** | It is already on crates.io (0.1.0, 0.2.0, 0.2.2) and has outside users; 0.3.0 carries the breaking changes and the fixes they need. Dry run passes at 6f3113e; P1 and P4 re-run it. |
| `pay-equity-engine` 0.3.0 | **`publish = false`** | Its contract is the JSON request and response Meridian's WASM uses, not Rust items; publishing makes every public item a semver promise and every remedy change a breaking crate release. It ships to Meridian as WASM. The name is free on crates.io (404 at 6f3113e); a later release can publish it, a published version can never be removed. |
| `meridian-mcp` 0.3.0 | **`publish = false`; ship as release binaries** | An MCP server is run, not depended on. `cargo install meridian-mcp` would also require publishing the engine. The release attaches prebuilt binaries for Linux, macOS (Apple silicon) and Windows instead. Publishing under the Meridian name is a brand step David may want later. |

P1 sets `publish = false` on the two; P4's `cargo publish --workspace` follows whatever the manifests say, so changing a recommendation is a one-line manifest edit before the tag.

## 9. Checks that prove each PR, and how each could fail

| PR | Check | Could fail when |
|---|---|---|
| P0 | `cargo audit --deny warnings` 0; validator 0; CI hash compares green, no `.sha256` change | the precise versions do not fit reqwest's requirement (then `cargo update -p reqwest` within 0.12, re-check); an old ignore left behind reads as stale; the bump pulls a compiled crate and moves the WASM |
| P1-T | `loop.py` writes `records/ground/` on the engine copy and `ground/` on pay-equity-app | another reader of `ground/` outside `loop.py` (`/davids-loop` setup, app scripts) is missed: grep both repos for `ground/` before merging |
| P1 | `cargo package -p oaxaca_blinder --list` shows both licence files and no `tests/`; `cargo publish --dry-run -p oaxaca_blinder --locked` exit 0 on the clean worktree (no `--allow-dirty`); `cargo metadata --no-deps` shows `publish: []` for engine and MCP; `python3 scripts/test-loop-scripts.py ground-nopath ground-validator verify-live-app-tree verify-live-scrub` green; `bash scripts/ground.sh` writes under `records/ground/`; msrv job green; `git ls-tree -r origin/main` after merge has none of the removed paths; GitHub shows two licences (`gh api repos/david-deji/oaxaca-blinder-rs/license`) | `include` drops a file the build needs (`templates/` for askama): dry run fails; the dry-run verify builds with the real `crossterm`, not the workspace's stub, and fails; a script still writes `ground/`; MSRV 1.88 fails after the lock moved (`home`, polars); a removed file was referenced (`git grep` for each removed path first) |
| P2 | `tests/api_compat_test.rs` passes on default features and all features; default-feature clippy and test green; `--all-features` suite green; every R golden, `parity_test`, `bootstrap_se_golden_test`, Browser Byte-Parity green; `cargo doc --all-features` clean; `oaxaca-cli run --help` shows 100 | a downstream exhaustive `match` in `engine` breaks under `non_exhaustive`; a test pinned to 20 replicates; `AsRef<str>` breaks an engine call shape; dead-code warnings in the default build; the WASM hash moves (expected: re-record under the parity proof, never without it) |
| P3 | `docs` job green (`mdbook build`, `cargo doc -D warnings`, doctests); the CLI-help and MCP-tools comparison tests green; `remedy_rules_test` green over `docs/src`; `grep -rn "YOUR_ORG\|0\.1\.0\|features cli\|features python\|maturin" README.md oaxaca_blinder/README.md docs/src` empty; every SUMMARY entry resolves (mdBook fails on a missing file) | a README snippet does not compile (the point); a page cites a statute or uses retired remedy wording; generated help drifts; mdBook version not in the install-action manifest at the pinned SHA |
| Sweep | each Dependabot PR merges on green, or is closed with its failing job named | artifact v5+ changes the browser-parity hand-off |
| P4 | `grep -c "^## \[0.3.0\]" CHANGELOG.md` is 1; each row of inventory section 4's omitted table and S1 to S27 has a line; `python3 scripts/release-notes.py v0.3.0` prints that section only; `cargo publish --workspace --dry-run --locked` exit 0 (publishes only `oaxaca_blinder`); `cargo build --workspace --locked`; check-ci-invariants and its tests green; recorded WASM hashes equal CI's; Browser Byte-Parity green; `git tag` unchanged | `cargo publish --workspace` not in the pinned 1.90.0 cargo (fallback: `cargo publish -p oaxaca_blinder`); `actionlint`-level errors only visible on a real tag run (the first `v0.3.0` push is the first execution: `check-tag` runs first, nothing is published if it fails); the `gh-pages` push needs the branch to exist |

After P4 merges, the rest waits for David: the publish list (section 8), `CARGO_REGISTRY_TOKEN` as a repo secret, the `v0.3.0` tag, Pages on `gh-pages`, wiki off, description, topics, homepage, private vulnerability reporting. Then inventory checklist steps 13 to 15 (clean-room install, Meridian WASM from the tag, ledger).

## 10. Open for David

1. Publish list: `oaxaca_blinder` to crates.io; engine and MCP kept off crates.io, MCP shipped as downloadable binaries (section 8). Approve or change.
2. Address printed in CODE_OF_CONDUCT and SECURITY for reports: proposed david@telosmachina.ai.
3. Licence line: the history has commits from accounts `dot-comma-hyphen`, `poche450`, `deji`, `david-deji` (taken here as yours) and 51 from Google's Jules bot. Confirm they are yours to relicense as MIT OR Apache-2.0 under Telos Machina.
