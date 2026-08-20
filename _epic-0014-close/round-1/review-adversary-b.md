## Adversary B Report — Lane B (pay-equity-app/frontend), Epic 0014-close Round-1

**Verdict: Lane B's code is clean. One MAJOR cross-lane gap found — not a bug in this diff, but a precondition that isn't met, which would let AC-5 be signed off against dead code.**

### Diff scope confirmed
`git -C .../pay-equity-app status --short` / `diff --stat`: only `frontend/src/components/dashboard/GapAnalysisResults.vue` (+64/-4), `frontend/src/locales/{en,fr}.json` (+5/-1 each), and new `frontend/src/components/dashboard/__tests__/GapAnalysisResults.spec.js`. No engine files, no `src/wasm*` touched.

---

### 1. Sibling specs (saveSurface/dataSource/rehydration) — CLEAN
Ran directly:
```
StatisticalDashboard.dataSource.spec.js    ✓ 6 tests
rehydration.spec.js                        ✓ 57 tests
StatisticalDashboard.saveSurface.spec.js   ✓ 15 tests
```
78/78 pass. These mount `StatisticalDashboard.vue` with hand-built store stubs that predate `run_metadata`, and `GapAnalysisResults.vue` is a real (non-stubbed) child in that tree — `store.results?.run_metadata ?? null` (`GapAnalysisResults.vue:369`) and `store.decompositionError` (`:393`) both resolve safely to `undefined`/falsy against a stub lacking the field. No crash.

### 2. sessionPassword.spec.js
`StatisticalDashboard.vue` was **not** touched in this diff, so this wasn't a required run, but ran it anyway: `sessionPassword.spec.js ✓ 27 tests`. Clean.

### 3. `RunMetadata` field names vs frontend usage — CLEAN, but see the MAJOR finding below
Grepped the actual struct (`oaxaca_blinder/src/rng.rs:91-111`): `seed`, `rng_algorithm`, `rand_chacha_version`, `bootstrap_reps_requested`, `bootstrap_reps_succeeded`, `bootstrap_reps_discarded`, `fixed_rif`. Frontend reads (`GapAnalysisResults.vue:165-166,174,384`) use exactly `bootstrap_reps_succeeded`, `bootstrap_reps_requested`, `bootstrap_reps_discarded`, `seed`. Traced the wire format end-to-end: `types.rs:50` → `analysis.rs:183/256` → `lib.rs:43` (`JsValue::from_str`, no wrapping) → `analysis.worker.js:94-102` (`error.message || String(error)`) → `AnalysisWorkerService.js:42` (`new Error(payload.message)`) → `analysisResults.store.js:214`. No wrapping anywhere on this path. Field names match exactly — no silent-mismatch risk here.

**However**: `strings -a src/wasm/pay_equity_engine_bg.wasm | grep run_metadata` **does** find `run_metadata`/`RunMetadata`/all its field names baked into the currently-shipped binary — so D5 is live and functional. But the same search for `EMPTY_LEVEL_IN_GROUP` / `missing_from_group` returns **zero hits** in either `src/wasm/pay_equity_engine_bg.wasm` or the glue. Confirmed why: the app's WASM was last republished at `git -C pay-equity-app log -- frontend/src/wasm*/pay_equity_engine_bg.wasm` → commit `a1c9b451`, built from engine HEAD `f81feca` (matches spec anchor A8 exactly). The engine's D1 work (`error.rs`, `builder.rs`, `integration_test.rs`) is **uncommitted working-tree state** in `oaxaca-blinder-rs` right now (`git -C oaxaca-blinder-rs status --short` shows those three files modified, HEAD at `ec60b1c`, two commits past `f81feca`, with D1 itself not yet committed at all).

**MAJOR — D6's localization is currently unreachable in the live app.** `GapAnalysisResults.vue:389-398`'s `EMPTY_LEVEL_IN_GROUP_RE` mapping is correctly written and correctly tested against a mocked store, but the shipped engine cannot yet produce that token — a level-confined dataset today still throws the old opaque "Failed to perform Cholesky decomposition... multicollinearity" (A4), which D6 correctly (and silently) leaves verbatim. AC-5 ("the D1 token renders localized in the UI") and AC-7's closing-walk check ("crafted level-confined CSV → localized named refusal, no raw Cholesky text") **cannot pass today** — not because of a code defect, but because D7 (`bash scripts/build-wasm.sh`) hasn't run yet. This is exactly the class of bug this app's own `CLAUDE.md` calls out by name ("Nothing in this repo catches a stale WASM... a blob that lags the Rust source keeps every test green while the browser runs old numbers") and exactly what D7's own text warns against ("The round is not done on green cargo tests alone"). Fix: lane A must commit D1-D4, then run `bash scripts/build-wasm.sh` from `oaxaca-blinder-rs`, then re-verify AC-5/AC-7 live in browser before the round is called RESOLVED. Do not sign off D6 as done from the frontend diff alone.

### 4. D6 parsing — CLEAN
- Malformed token (missing `missing_from_group=`): test at `GapAnalysisResults.spec.js:221-230` passes — falls to verbatim, no crash.
- Regex `^EMPTY_LEVEL_IN_GROUP: column=(.+?), level=(.+?), missing_from_group=(.+)$` (`:390`): `^`/`$` in JS match only string start/end (no multiline exception on trailing newline, unlike Python), so a token appearing mid-message would correctly NOT match and fall to verbatim — but I confirmed this never actually happens: the Display impl (`error.rs:51-55`) is the *only* producer and emits nothing before the token on every call path that can construct `EmptyLevelInGroup` (`decompose_inner`'s both branches at `analysis.rs:207,276` use unwrapped `e.to_string()`; I also traced the one place that DOES wrap with a prefix — `analysis.rs:479`, `format!("Oaxaca Error: {}", e)` inside `optimize_inner` — and confirmed `get_data_matrices()` at `builder.rs:396-435` never calls `check_level_confinement` (`builder.rs:161`), so that wrapped path structurally cannot emit this token; the one call inside `optimize_inner` that *can* — `gap_builder.run()` at `analysis.rs:425` — uses unwrapped `.to_string()`. No prefix-collision exists on any live path).
- Lazy-group greediness against embedded commas in column/level values (e.g. a department named "Region, Zone"): traced by hand — the lazy quantifiers correctly expand to the first literal `, level=` / `, missing_from_group=` occurrence, so embedded commas in values are handled correctly; only a value containing the literal separator substring itself would break it, which is not a realistic data shape. Not flagging.
- Unrecognized error stays verbatim: test `GapAnalysisResults.spec.js:211-219` passes; also confirmed `StatisticalDashboard.vue` only reads `store.decompositionError` to gate tab visibility (`:153,375`), never renders it raw itself — no second leak surface.

### 5. Locale parity + i18n:lint + warning tone — CLEAN
`npm run i18n:lint` → `en leaf keys: 2988   fr leaf keys: 2988 ... ✓ i18n lint clean — every referenced key exists, locales at parity`. Warning-tone class `text-[var(--color-warning)]` (`GapAnalysisResults.vue:172`) is a distinct CSS var from the parent's `text-[var(--color-text-muted)]` (`:160`) in all three themes (`src/assets/main.css:25/35` light, `:78/86` dark, `:150/126` high-contrast — different hue angle in every theme, not just different lightness). The discarded `<span>` is a sibling with its own explicit color rule, so it overrides the parent's inherited muted color unconditionally. Ran the enforced `designTokens.spec.js` (catches exactly this class of bug) — `✓ 5 tests`.

### 6. Old saved projects (no `run_metadata`) — CLEAN
`v-if="runMetadata"` (`:158`) gates the entire block; `runMetadata` computed (`:369`) is `store.results?.run_metadata ?? null` — both "key absent" and "key explicitly null" normalize to `null`, tested explicitly (`GapAnalysisResults.spec.js:157-171`, both pass). Confirmed via the 78 sibling-spec passes above that this doesn't crash when mounted inside real legacy-shaped stubs.

---

### Full suite
`npx vitest run` (whole frontend): **266 files, 4321 passed, 2 skipped** (AC-4 claims "default 4311+" — satisfied). New spec file: `GapAnalysisResults.spec.js ✓ 10/10`.

### Bottom line
Lane B's diff (D5 + D6) is correct, well-tested (mutation-coupled succeeded/requested pair, both locales, both falsy shapes of legacy data, both branches of the error mapping), and doesn't regress anything. The one thing worth stopping the round for is not in this diff: **the shipped WASM blob has no idea `EMPTY_LEVEL_IN_GROUP` exists**, so D6 is unverifiable and AC-5/AC-7 are not actually met yet — that's on D7 (lane A republish), still pending, and it should be called out explicitly before this round is marked RESOLVED.