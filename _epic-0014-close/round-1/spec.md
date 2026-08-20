# Epic 0014-close · Round 1 — Named level-confinement refusal, INV-02 tolerance leg, run_metadata surfaced

> Issue: 0014-MERIDIAN (stages 1-4 shipped; this round closes the audited punch-list)
> Maps: `../map/a-spec-vs-shipped.md` (10 verdicts), `../map/b-ref-absent-edge.md` (10 verdicts)
> Repos: engine `oaxaca-blinder-rs` (lane A) + app `pay-equity-app/frontend` (lane B)

## §0 Anchors (ground truth from the maps — cite, don't re-derive)

- A1. All 4 build stages + follow-ups shipped (`356faab`→`3858b43`); issue header `SPECIFIED` is stale (map A v1).
- A2. Zero-rows-of-a-group in a resample is **structurally impossible** — resampling is stratified per group at fixed size (`builder.rs:828-830,863-866`; `quantile_decomposition.rs:390-419`) (map B v1).
- A3. Per-replicate level-vanishing is handled: caught → `RepOutcome::Failed` → deterministic `discarded` count (`builder.rs:879-891,1051-1063`) (map B v3).
- A4. **The defect**: a categorical level confined to one group in the FULL data hard-fails `.run()`/`.decompose_quantile()` with opaque `"Failed to perform Cholesky decomposition... multicollinearity"` naming neither column, level, nor group. Empirically confirmed (map B v4, §1c run). Rejection happens in `math/ols.rs:100-105` (correct, deterministic — just unnamed).
- A5. `integration_test.rs:332-334` `test_detailed_components_with_rare_category` is targeted coverage for exactly this class, is `#[ignore]`d, and panics if un-ignored (map B v5).
- A6. INV-02's native↔wasm ≤1e-6 tolerance leg has **zero implementing test**; within-platform byte-identity legs are tested and green (native `mode_parity_test.rs:52-63`; wasm `verification/browser-parity/parity.spec.mjs:34-38`) (map A v3).
- A7. `RunMetadata` (seed, algorithm, crate version, requested/succeeded/discarded reps) is computed and serialized into every WASM result but has **zero consumers** in the Meridian frontend (map B v9). The walk saw `run_metadata` among results keys (w36-05f).
- A8. Published WASM blobs are **current** with engine HEAD `f81feca` (app publish commit `a1c9b451`) — no republish precondition (map B v8).
- A9. FOLLOWUPS.md's RIF `n<2` narrative describes a "singleton resample" the stratified architecture cannot produce; the guard actually protects whole-group-too-small density estimation (`math/rif.rs:14-34`) (map B v6).
- A10. Known-accepted deferred gaps, NOT this round's work: OOM self-report (item H), R-blocked golden items (E/G), Playwright infra in the app, sequential-fallback automated test (map A v4-v7). Recorded at close, not built.
- A11. Engine errors reach the UI verbatim: worker `catch` → `{status:'ERROR', payload:{message}}` (`analysis.worker.js:94-102`) → `store.decompositionError` → interpolated into the localized wrapper (`GapAnalysisResults.vue:41-56`).
- A12. Ship path for any engine change: `bash scripts/build-wasm.sh` (dual artifact, sha256-verified publish into the app). A green cargo test alone ships nothing.

## §1 Deliverables

### Lane A — engine (Rust)

**D1 — Named pre-flight refusal for level-confinement.** Before estimation in both public entries (`OaxacaBuilder::run()` and `decompose_quantile()`), detect any categorical predictor level that is present in the data but absent from one group (the level-confined case, A4) and return a named error instead of reaching Cholesky. Error Display carries a stable machine token and the specifics:
`EMPTY_LEVEL_IN_GROUP: column=<col>, level=<level>, missing_from_group=<group>` (first offending level is enough; deterministic scan order — column order then level sort — so the same data always names the same offender).
Implementation freedom: the check may live in the shared design-matrix/validation path if both entries route through it; it must NOT change behavior for data that estimates today (only the case that already hard-fails gets a better name). Per-replicate handling (A3) stays untouched.

**D2 — Un-ignore and fix `test_detailed_components_with_rare_category`** (A5): rewrite its expectation to the new named error if its fixture is level-confined, or to a passing estimation if it isn't — whichever the fixture actually is; the test must run un-ignored and pin D1's message shape.

**D3 — INV-02 native↔wasm ≤1e-6 tolerance leg** (A6). Smallest honest implementation: the existing browser-parity CI job generates a native baseline JSON (CLI run on the shared fixture, same seed) before Playwright, and `parity.spec.mjs` adds a per-numeric-field `|native − wasm_seq| ≤ 1e-6` comparison (recursive walk; non-numeric fields byte-equal except fields the spec's own baselines already exempt). Must run in the same CI job (no new job) and locally via the existing harness entry point.

**D4 — FOLLOWUPS.md narrative correction** (A9): rewrite the item's bootstrap-path rationale to what the guard actually protects. One paragraph, no code.

### Lane B — app (frontend)

**D5 — run_metadata trust line in the results region** (A7). In `GapAnalysisResults.vue` (or a sibling block in the results card), when `store.results?.run_metadata` exists, render a small muted line: seed (hex), succeeded/requested reps; when `bootstrap_reps_discarded > 0`, a warning-toned suffix with the discarded count. Localized (fr/en), falsy-safe (`?.`), absent entirely for results without the field (old saved projects). Test: presence with metadata, absence without, warning tone on discard>0 — mutation-coupled.

**D6 — Localized mapping for the D1 refusal.** In the decomposition error path (store or component — implementer's choice, cite A11), detect the `EMPTY_LEVEL_IN_GROUP:` prefix and render a localized message naming the column, level, and group (« Le niveau « {level} » de « {column} » est absent du groupe « {group} » : la décomposition ne peut pas comparer ce niveau entre groupes. ») instead of interpolating the raw token. Unrecognized errors keep today's verbatim path. Test both branches.

**D7 — Republish.** After lane A merges: `bash scripts/build-wasm.sh` publishes the refusal + any engine change into the app (A12). The round is not done on green cargo tests alone.

### Close-out (orchestrator)

**D8 — Issue 0014 honest close**: retroactive log rows for stages 1-4 (one row each, commit-linked, from map A), a row for this round, status → RESOLVED with the accepted-gap list (A10) recorded verbatim in the log; REGISTER row moved.

## §2 Out of scope (named, with reasons)

- OOM self-report, R-blocked goldens E/G, app Playwright infra, sequential-fallback automated test — founder-reviewed deferrals (A10); recorded at close, not built.
- DFL two-stage reweighting (map B v10) — separate surface.
- Toolchain-pin spec-literal refresh (map A v9) — spec archive is historical record; the live pin is the truth.
- Any threading/memory work — shipped and audited (A1).

## §3 Acceptance

- AC-1: level-confined fixture → named error with column/level/group on BOTH entries (mean + quantile); data that estimates today still estimates (regression: existing suites green).
- AC-2: `test_detailed_components_with_rare_category` runs un-ignored and passes.
- AC-3: CI browser-parity job (and local harness) computes the native↔wasm ≤1e-6 per-field diff and fails on violation; green at HEAD.
- AC-4: Meridian results region shows seed + rep counts when run_metadata present; discard>0 gets warning tone; absent for legacy results. Suites green (default 4311+, sqlite untouched).
- AC-5: the D1 token renders localized in the UI; unrecognized errors unchanged.
- AC-6: `build-wasm.sh` republished blobs; sha256 stamps updated in the app.
- AC-7 (round 2): closing walk — same analysis twice → identical numbers + visible seed line; crafted level-confined CSV → localized named refusal, no raw Cholesky text.

## §5 Dispositions

- Stream-scheme `(rep+1)` deviation (map A, Domain 1 note): deliberate improvement, keep — no action.
- CI topology delta (map A v10): equivalent coverage, cosmetic — no action.
- `seed_from_entropy` native-only scoping: per spec D1 decision — no action.
