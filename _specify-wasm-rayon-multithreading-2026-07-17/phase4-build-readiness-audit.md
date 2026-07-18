# Phase 4 Build-Readiness Audit — WASM Rayon Multithreading (0014-MERIDIAN)

> Auditor: Build-Readiness Auditor (adversarial, evidence-based)
> Date: 2026-07-17
> Scope: 7 final specs in `_specify-wasm-rayon-multithreading-2026-07-17/`
> Method: every anchor below was Read against the live repo tree (`oaxaca-blinder-rs/`, `pay-equity-app/`, `audit-forge/webui/__init__.py`) this session.

---

## 1. phase4-final-toolchain-build.md

**Anchor verification: excellent fidelity — every checked citation matched byte-for-byte.**

- `rust-toolchain.toml:5` = `channel = "1.90.0"` — confirmed. `:6-7` components/targets — confirmed.
- `engine/Cargo.toml:14` (`wasm-bindgen` optional dep), `:34` (`default = []`), `:38-47` (`wasm` feature block) — all confirmed exact.
- `oaxaca_blinder/Cargo.toml:23` (`rayon = "1.11.0"`) — confirmed exact.
- `scripts/build-wasm.sh` (51 lines total): `:24` remap, `:38-39` baseline write, `:43` `--target bundler` — all confirmed exact.
- `.github/workflows/ci.yml`: `:14-37` quality job, `:20-22` toolchain setup, `:39-82` wasm-verify job, `:60` remap, `:62` nondeterminism comment, `:70-82`/`:70-73` sha256 verify block, `:84-93` security job — all confirmed exact, including single-line precision (`:62` lands exactly on the "Smoke only... nondeterministic" comment).

No CRITICAL or MAJOR findings in this domain's anchors.

**MINOR**
- None found beyond general polish. The `.cargo/config.toml` precedence caveat (D3) is correct: Cargo does not merge env `RUSTFLAGS` with `[target.*].rustflags` — this is accurately stated and load-bearing for why the script re-lists static flags.

**Acceptance criteria**: all 16 ACs are grep/exit-code/byte-compare checkable (AC-4 `grep -R "nightly-2024-08-02"`, AC-6 `cargo tree ... -i wasm-bindgen-rayon`, AC-9 `grep -c "target bundler"`, AC-13 double-build sha256 compare). No subjective language ("works correctly") found.

**Fix-forward note**: D3/D8 correctly hand `--max-memory`/`-zstack-size` **values** to memory-budget as a cross-domain contract, and Build Step 6 explicitly re-runs the baseline-record step after the memory domain delivers final literals. This seam is well-specified.

---

## 2. phase4-final-deterministic-rng.md

**Anchor verification: mostly excellent, one real citation defect.**

- `builder.rs:825-848` bootstrap block, `:828-829` clone pair, `:831,834` `let sample_a/sample_b =` bindings, `:832,835` `sample_n_literal(...)` calls, `:838` vstack, `:846-847` `.ok()`/`})`, `:850` `bootstrap_results.len()`, `:819-820` mean-path `run_single_pass` call with unchanged signature — **all confirmed exact** against `oaxaca_blinder/src/builder.rs`.
- `oaxaca_blinder/Cargo.toml:25` (`rand = "0.8.5"`, no `rand_chacha` yet) — confirmed; the spec's D8 "add `rand_chacha = "0.3"`" is correctly framed as a new dependency, not a hallucinated "already present" claim.
- `quantile_decomposition.rs:215` `rand::thread_rng()` (τ grid), `:244` `rand::thread_rng()` (row resampling), `:337-354` bootstrap loop, `:344,347` `sample_n_literal` calls — **all confirmed exact**.

**MAJOR**
- **[quantile_decomposition.rs signature anchor is wrong]** The "Signature deltas" section states: *"Quantile `run_single_pass` gains one param... verified current sig `(&df, &all_dummy_names)` at `quantile_decomposition.rs:321`"*, and Build Step 5 says *"Add `rep_master: u64` param (`:~185` sig)"*. Neither line is the function definition. The actual signature is:
  ```
  173: fn run_single_pass(
  174:     &self,
  175:     df: &DataFrame,
  176:     all_dummy_names: &[String],
  177: ) -> Result<SinglePassResult, OaxacaError> {
  ```
  Line 321 is a **call site** (`let point_estimates = self.run_single_pass(&df, &all_dummy_names)?;`), and line 185 falls inside the function **body** (an `InvalidGroupVariable` error string), not the signature. This does not block the build — the two call sites the spec separately names (`:321` sentinel path, `:352` rep_master path) ARE correct, and a worker will locate `fn run_single_pass` by name regardless of the wrong line pointer — but it is a genuine verification miss in a spec whose premise is "verified against source 2026-07-17," and worth correcting so a worker doesn't waste a turn looking at the wrong line.
  - **Fix**: change the citation to `quantile_decomposition.rs:173-177` for the signature; keep `:321`/`:352` as the (correct) call-site references.

**Acceptance criteria**: AC-1/AC-2 (grep, zero matches), AC-3/AC-4/AC-6/AC-7 (exit-code tests with byte/sha256 comparisons) — all objectively checkable. No subjective criteria found.

**Cross-domain note**: D2's `unit_rng(master, purpose, unit)` closed-form design (no thread id, no clock, no atomic counter) is the correct basis for INV-02 bit-identity and is consistent with what memory-budget's D4 assumes for the shared index-vector resampling.

---

## 3. phase4-final-engine-parallel-surface.md

**Anchor verification: the five-entry-point audit table (D1) and all cited line numbers are precise** — `lib.rs:26,34,42,50,59` map exactly to `decompose`/`optimize`/`verify_adjustments`/`calculate_efficient_frontier`/`check_defensibility`; `analysis.rs:40` (`verify_inner`), `:147` (`reps = ... .unwrap_or(100)`), `:201-204` (the two `Vec::new()` placeholders), `:206` (tuple return), `:309` (`optimize_inner`), `:875` (`calculate_efficient_frontier_inner`), `:950` (`optimize_inner` call inside frontier), `:1089-1109` (`compute_t_stat` closure), `:1135-1166` (sequential budget loop) — **all confirmed exact**. `defensibility.rs:9` (`check_defensibility_inner`) — confirmed exact. `math/{ols,rif,kde,normalization,quantile_regression}.rs` all exist with the claimed public functions (`ols::ols`, `rif::calculate_rif`, `normalization::normalize_categorical_coefficients`, `quantile_regression::solve_qr`).

**CRITICAL**
- **[In-Scope 12 duplicates an existing, working, tested RIF-OLS quantile-decomposition path the spec never discovers]** The spec's Summary and D5 state flatly: *"the math does not exist today"* and prescribe ~10 build steps to add a brand-new `rif_quantile()` function to `math/rif.rs`, new detail-algebra reuse of `decomposition.rs`, and a new `QuantileDecompositionDetail` API surface. But `oaxaca_blinder::OaxacaBuilder` **already has this exact capability, live and used**:
  - `oaxaca_blinder/src/math/rif.rs:14` — `pub fn calculate_rif(series: &Series, quantile: f64) -> Result<Series, PolarsError>` already implements the identical formula the spec's D5 §1 states as new work: `RIF(y; q_τ, F) = q_τ + (τ − 1{y ≤ q_τ}) / f_Y(q_τ)` (see `rif.rs:78,83`). It even estimates `f_Y(q_τ)` via its own inline Gaussian KDE with Silverman's-rule bandwidth (`rif.rs:37-72`) — **not** via `math/kde.rs`, which is what the new spec's D5/Build-Step-5 instructs the new `rif_quantile()` to use.
  - `oaxaca_blinder/src/builder.rs:710-766` — `OaxacaBuilder::decompose_quantile(&self, quantile: f64) -> Result<OaxacaResults, OaxacaError>` already: (1) calls `calculate_rif` on both groups (`:730,735`), (2) substitutes RIF for the outcome (`:742-746`), (3) re-runs the standard `OaxacaBuilder::run()` pipeline (`:752-765`) — which **already produces per-predictor `detailed_explained`/`detailed_unexplained`** (`SinglePassResult` fields returned at `builder.rs:695-707`, specifically `:698-699`). This is a complete, working, one-stage RIF-OLS decomposition with per-predictor detail, reachable today via `oaxaca_blinder::OaxacaBuilder::decompose_quantile()`.
  - There is also a pre-existing `oaxaca_blinder/tests/rif_test.rs`, indicating this path already has test coverage the spec does not reference or reconcile against.
  - Neither this spec's Design/Sources sections nor `phase4-final-statistical-trust-layer.md`'s Design/Sources sections mention `decompose_quantile`, `calculate_rif`, or `rif_test.rs` anywhere. The spec's own math (D5 §1-3) is a line-for-line re-derivation of what `calculate_rif` + the standard OB detail machinery already do.
  - **Risk if built as specced**: two independent RIF-OLS code paths coexist in the same crate with **different density estimators** (existing: inline Gaussian KDE/Silverman; new: `math/kde.rs`-based), which can silently diverge numerically on the same input — a defensibility risk for a pay-equity audit tool, plus duplicated build/test effort for capability that (largely) already ships.
  - **Fix-forward**: Phase-0 reconciliation before dispatch — determine whether `QuantileDecompositionBuilder`'s new detail path should call `OaxacaBuilder::decompose_quantile()`/`calculate_rif()` directly (reuse) instead of building a parallel `rif_quantile()` + new detail algebra, or explicitly document why the two need to diverge (e.g., MM-simulation aggregate vs RIF-OLS detail structurally can't share the same call path). Right now the spec is silent on the existing code's existence, which is the failure mode this rule exists to catch (`.claude/rules/build-safety.md` § Spec-Codebase Reconciliation).

**Acceptance criteria**: AC-1 through AC-10 are all objectively checkable (grep, `cargo tree`, byte-compare, exit code) except that AC-6/AC-7/AC-9 depend on the In-Scope-12 math landing — which is the above CRITICAL issue's build surface.

---

## 4. phase4-final-memory-budget.md

**Anchor verification: excellent.** `builder.rs:813` (dummy hstack), `:822-823` (`df_a_global`/`df_b_global`), `:825` (bootstrap `into_par_iter`), `:828-829` (clone pair), `:850-856` (discard warning) — all confirmed exact against the live file. The D4 illustrative code block labels the `sample_n_literal` call as `:831`/`:834` when the actual method-call token sits one line later (`:832`/`:835` — the `let sample_a = df_a` binding is `:831`, the `.sample_n_literal(...)` continuation is `:832`); this is a **MINOR** cosmetic imprecision in a paraphrased code block, not a grep target any AC depends on (AC-M9 only requires "code read confirms the `&self` signature and clone-then-sample pattern," which holds).

**MAJOR** (cross-domain — see § Cross-Domain Seams below for full detail)
- D3/AC-M8 assert this domain "owns `N_max_const` and the ceiling `8`," and that "meridian-integration owns the `initThreadPool(N)` call" consuming a 3-term formula `min(N_max_const, hardwareConcurrency, 8)`. The meridian-integration spec's actual worked code implements only a 2-term formula and attributes the constant's *emission* to a third, non-existent-in-this-set domain name ("engine-parallelization"). See Cross-Domain Seams, Finding B.

**Acceptance criteria**: AC-M1 through AC-M15 are objectively checkable (populated-cell counts, ≤5% delta assertions, formula-vs-literal checks, sha256 byte-match, `git check-ignore`). Good discipline — even the "formula, not a bare literal" requirement (AC-M4) is checkable by inspection of committed values.

**Open Items O1-O4** are correctly routed to the buildability gate / non-blocking research, not silently assumed.

---

## 5. phase4-final-meridian-integration.md

**Anchor verification: excellent** — `analysis.worker.js:1-8` (import block), `:10,13,17` (idempotent latch), `:12-24` (`initialize()`), `:26-27` (`onmessage`/destructure), `:29-30` (try/`await initialize()`), `:53` (success post), `:54-61` (error post) — all confirmed exact. `AnalysisWorkerService.js:24` (consumer destructure), `:26` (`if (!job) return`), `:47,56,66` (`run()`/`terminate()`/`restart()` — confirmed each starts on exactly the cited line), `:53` (`w.postMessage`) — all confirmed exact. `vite.config.js:3` (`vite-plugin-wasm` import), `:4` (`topLevelAwait`), `:10` (`base`), `:11,13` (wasm() usages), `:12-14` (worker block), `:15-22` (esbuild block), `:23-44` (test block) — all confirmed exact, including the resulting-config example correctly preserving the `vue()` import and citing `:10` for the unchanged `base` line. `webui/__init__.py:117-123` (`/`/`/guide` routes), `:125-131` (`_apply_meridian_csp`, with the exact `if`-at-129/CSP-line-at-130/return-at-131 structure), `:138-152` (`_apply_plugin_csp`, `:151` only sets CSP never COOP/COEP) — all confirmed exact.

This is the most anchor-precise spec in the set — genuinely impressive fidelity given the volume of line-level citations.

**CRITICAL** (cross-domain — see § Cross-Domain Seams, Finding B, for full detail)
- The spec's D2 worker-init code imports `MEMORY_THREAD_CAP` from `./thread-cap.js`, a file that **no spec in this set — including this one — has a build step to create**. `pnpm build` (AC-M4.3) will fail to resolve the import unless some other, unspecified process creates `frontend/src/wasm/thread-cap.js`. The spec's own attribution of "who emits it" ("the engine-parallelization domain," lines 87 and 192) does not match any of the 7 domain names in this set, and the domain it evidently means (`engine-parallel-surface`) explicitly disclaims owning "the memory profile / thread-cap formula" in its own Out-of-Scope line. See Cross-Domain Seams for the full three-way contradiction.

**MINOR**
- D2's worked JS computes `cap = Math.min(navigator.hardwareConcurrency || 1, MEMORY_THREAD_CAP)` — a 2-term min — while the same spec's own "Not owned here" line states the formula as 3-term, `min(hardwareConcurrency, memoryCap, 8)`. Even setting aside the ownership question above, the worked code silently drops the explicit `8` ceiling that memory-budget's AC-M8 requires be enforced somewhere. If `MEMORY_THREAD_CAP` is meant to already have `min(_, 8)` baked in, that should be stated; if not, the worker code under-enforces the ceiling.

**Acceptance criteria**: AC-M1.x through AC-M9.x are all Playwright/grep/byte-diff checkable — genuinely well-specified, not "works correctly" language anywhere. AC-M1.1's exact-value assertion (`threads === Math.min(navigator.hardwareConcurrency||1, MEMORY_THREAD_CAP)`) is checkable in principle but currently untestable in practice because `MEMORY_THREAD_CAP` has no defined source (same CRITICAL issue).

---

## 6. phase4-final-statistical-trust-layer.md

**Anchor verification: good**, with one real citation defect and one **build-dependency-driven** issue (not a defect, correctly flagged by the spec itself).

- `oaxaca_blinder/tests/parity_test.rs:24` (`TOLERANCE: f64 = 1e-6`), `:25` (`INTERNAL_TOL: f64 = 1e-9`), `:72` (placeholder guard) — confirmed exact. `Cargo.toml:38-40` (`assert_cmd`, `predicates` only, no `proptest`) — confirmed exact; `:46` (`[dev-dependencies.criterion]`) — confirmed exact. `verification/gen_parity_golden.py` exists as claimed. `ground_truth_verification_test.rs` exists as claimed.
- `quantile_decomposition.rs:267-271` (aggregate `DecomposedEffects{gap,characteristics,coefficients}`) — confirmed exact (this specific anchor is reused correctly across three of the seven specs and matches in every case).

**MINOR**
- **[Wrong intercept-literal anchor]** D-2 states *"Intercept: engine uses `__ob_intercept__` (`builder.rs:325`)."* Line 325 is actually inside an unrelated error-message string (`"Null values found in outcome after cleaning"`); the `__ob_intercept__` literal first appears at `builder.rs:334` (`vec!["__ob_intercept__".to_string()]`). Non-blocking — this citation is background context for the R/Rust design-matrix-alignment note, not a line any build step edits or any AC greps for.

**Correctly-flagged dependency (not a defect)**
- OI-1 explicitly states `quantile_detail_golden_test.rs` (Step 8, AC-6) is gated on the engine's In-Scope 12 RIF-OLS detail math landing first, and sequences "ship Steps 1-7 first, land Step 8 with the engine work." This is good practice — but see the CRITICAL finding in § engine-parallel-surface above: since that math *already substantially exists* via `decompose_quantile`/`calculate_rif`, the golden test in this spec could plausibly be built and run **now**, against the existing code path, well ahead of any new engine-parallel-surface work landing — a build-order opportunity this spec doesn't consider because it inherited the "math doesn't exist" framing uncritically.

**Acceptance criteria**: AC-1 through AC-8 are all exit-code/grep/tolerance-table checkable — no subjective criteria.

---

## 7. phase4-final-verification-benchmark.md

**Anchor verification: excellent.** `.github/workflows/ci.yml:60` (remap), `:62` (nondeterminism comment), `:70-82` (sha256 verify) — confirmed exact (consistent with toolchain-build's identical citations). `verification/gen_parity_golden.py:90` (`%.17g` float format convention) and `quantile_decomposition.rs:267-271` — confirmed exact.

No CRITICAL or MAJOR findings specific to this spec's own anchors.

**MAJOR** (inherited cross-domain dependency, not this spec's own defect)
- OI-1 explicitly and correctly names `declared_max_bytes`/`margin` for AC-4 (memory-ceiling test) as coming from memory-budget's final budget table via "a single shared config location the memory domain owns — not hardcoded here." This is well-handled *dependency sequencing*, but the shared-config-location itself is never named as a concrete file path anywhere in memory-budget's D8 file table either — a minor extension of the same "who emits the shared cross-domain constant, and where" gap seen in Finding B below. Lower severity here because the spec correctly defers rather than hardcoding a guess.

**Acceptance criteria**: AC-1 through AC-10 are all objectively checkable (sha256 four-way compare, exit codes, grep absence checks, `ratio > 1.5` numeric assertion). AC-5's `grep -L 'RuntimeError' <oom_test_file>` is a good concrete, checkable proxy for "no message-parsing."

---

## Cross-Domain Seams

### Finding A — `--max-memory` value ownership: consistent, no contradiction
Toolchain-build provides the buildable-default literal (`536870912` / 512 MiB) in `.cargo/config.toml` and `scripts/build-wasm.sh`, explicitly deferring the *final* value to memory-budget ("the memory-budget domain finalizes both... consumed here as a cross-domain contract"). Memory-budget's D2 independently derives the same default band and states the same handoff in reverse. Toolchain-build's Build Step 6 explicitly re-runs the baseline-record step once memory-budget delivers. **This seam is well-specified — no finding.**

### Finding B — `MEMORY_THREAD_CAP` / `N_max_const` / `thread-cap.js`: three-way ownership contradiction + missing build step (CRITICAL, restated from §3/§5 for visibility)
- **memory-budget** (D3, AC-M8): *"This domain owns `N_max_const` and the ceiling `8`; meridian-integration owns the `initThreadPool(N)` call."* → claims ownership of the value.
- **engine-parallel-surface** (Out-of-Scope line): *"Out of this domain:... the memory profile / thread-cap formula (memory-budget)."* → explicitly disclaims ownership, attributing it back to memory-budget. Consistent with memory-budget's own claim.
- **meridian-integration** (lines 87, 192, 220): *"MEMORY_THREAD_CAP is supplied by the engine-parallelization domain"* / *"`thread-cap.js` (the `MEMORY_THREAD_CAP` constant) is emitted/refreshed alongside by the engine-parallelization domain."* → attributes emission to a **third**, differently-named domain ("engine-parallelization") that does not match the two domains above and, if it means `engine-parallel-surface`, directly **contradicts** that spec's own explicit disclaimer.
- **No spec's numbered Build Steps section anywhere in the 7 specs actually creates `frontend/src/wasm/thread-cap.js`.** memory-budget's D8 file table lists `scripts/build-wasm.sh` values and a fixture generator, but not a JS constant file. meridian-integration's own Build Step 4 only says "confirm `thread-cap.js` (`MEMORY_THREAD_CAP`) is present alongside" — a *verification* step assuming prior existence, not a creation step.
- **Consequence**: `analysis.worker.js`'s `import { MEMORY_THREAD_CAP } from './thread-cap.js'` (meridian-integration D2) has no build step anywhere in this spec set that produces the imported file. `pnpm build` (AC-M4.3) cannot succeed as specced without a founder/orchestrator filling this gap at the buildability gate.
- **Secondary defect**: meridian-integration's own "Not owned here" line states the cap formula as 3-term (`min(hardwareConcurrency, memoryCap, 8)`), but its D2 worked code implements only the 2-term `Math.min(navigator.hardwareConcurrency || 1, MEMORY_THREAD_CAP)` — silently dropping the explicit `8` ceiling unless it is already baked into the constant (undocumented either way).
- **Fix-forward**: at the buildability gate, name one domain (recommend: memory-budget, since it already owns `N_max_const` per D3) to add a Build Step emitting `frontend/src/wasm/thread-cap.js` as `export const MEMORY_THREAD_CAP = <N_max_const-already-clamped-to-8-or-not, state which>;`, and have meridian-integration's D2 code either drop the redundant `,8` from its own prose or add it to the worked `Math.min(...)` call.

### Finding C — RNG seeding shared between memory-budget and deterministic-rng: consistent, no contradiction
Memory-budget's D7 fixture generator explicitly derives its per-`(k,r)` stream "via the same `rand_chacha`/`ChaCha8Rng` family deterministic-rng selects," and both specs agree `rand_chacha = "0.3"` is a direct-dependency addition (not already present — confirmed: `oaxaca_blinder/Cargo.toml:25` currently has only `rand = "0.8.5"`). Memory-budget's D4 index-vector-resampling memory analysis and deterministic-rng's D3 owned-index-vector design are the same refactor viewed from two angles, correctly cross-referenced both ways (each says "seeding correctness owned by deterministic-rng" / "this domain claims only the memory consequence"). **No contradiction found.**

### Finding D — `pkg refresh` path and Strategy B/A: consistent
Toolchain-build (Strategy B, `--target web`), engine-parallel-surface (D6 feature matrix), and meridian-integration (D5 pkg refresh, D7 fallback) all agree on one artifact under Strategy B, with a matching one-paragraph Strategy-A fallback note in each of the affected specs (toolchain-build D7, engine-parallel-surface Open Items, deterministic-rng Open Items, meridian-integration D5). **No contradiction found** — this is a genuinely well-coordinated cross-domain decision, correctly deferred to a single founder ratification point (SC-02) in every spec that touches it.

---

## Summary Table

| Spec | CRITICAL | MAJOR | MINOR |
|---|---|---|---|
| toolchain-build | 0 | 0 | 0 |
| deterministic-rng | 0 | 1 (wrong signature-line anchor) | 0 |
| engine-parallel-surface | 1 (In-Scope-12 duplicates existing `decompose_quantile`/`calculate_rif`) | 0 | 0 |
| memory-budget | 0 | 1 (shared, see Cross-Domain Finding B) | 1 (cosmetic line offset in D4 code block) |
| meridian-integration | 1 (shared, see Cross-Domain Finding B — missing `thread-cap.js` build step) | 0 | 1 (2-term vs 3-term cap formula in own text) |
| statistical-trust-layer | 0 | 0 | 1 (wrong intercept-literal anchor) |
| verification-benchmark | 0 | 1 (inherited: shared-config-location for memory constants never named as a concrete path) | 0 |

Two distinct CRITICAL issues (the RIF-OLS duplication, and the `thread-cap.js` three-way ownership gap) are load-bearing enough that a `/build` run following these specs literally would either (a) build materially duplicate/divergent statistical machinery, or (b) fail at the Vite build step on an unresolved import. Both are fixable at the buildability gate without re-running Phase 3 research — they are reconciliation/ownership gaps, not missing research.

**VERDICT: CRITICAL-PRESENT**

---

## RESOLUTION (orchestrator, 2026-07-18)

- **CRITICAL-1 (In-Scope 12 duplicates existing code) — FIXED.** Verified against ground truth: `OaxacaBuilder::decompose_quantile()` (`builder.rs:720-766`) already implements one-stage RIF-OLS quantile decomposition with full per-predictor detail via `run()`; WASM path was empty only because `analysis.rs:168` calls the MM builder. Rewrote In-Scope 12 in `phase4-final-engine-parallel-surface.md` (summary, scope, D1 table row 1c, D5, RK2/RK4, build steps 5-9, AC-6/8/9, anchors) from "implement new math" to "wire the existing `decompose_quantile()` into the WASM branch + one founder decision (MM→RIF aggregate)". Updated `phase4-final-statistical-trust-layer.md` D-6/OI-1/step-8: golden validates EXISTING code, no longer gated on new math. No divergent RIF/KDE path.
- **CRITICAL-2 (thread-cap.js ownership) — FIXED.** Single-owner assignment: memory-budget owns the VALUE `N_max_const` (D3); meridian-integration M7 WRITES `frontend/src/wasm/thread-cap.js` (`export const MEMORY_THREAD_CAP = <N_max_const>`); worker imports it. Fixed the non-existent "engine-parallelization" misattributions and added the file-creation build step (meridian M7, lines 40/88-90/193/221/226; memory-budget D3).
- **MINOR-1 (`run_single_pass` cited `:321`, actual `:173`) — FIXED** in `phase4-final-deterministic-rng.md` (definition at `:173`; call sites `:321`/`:352` were correct).
- **MINOR-2 (intercept literal cited `builder.rs:325`, actual `:334`) — FIXED** in `phase4-final-statistical-trust-layer.md`.

Post-fix Phase 4 quality gate: 22/0. Both CRITICALs closed; spec set is internally consistent.
