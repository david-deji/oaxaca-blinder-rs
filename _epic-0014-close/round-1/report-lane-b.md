## Report: Lane B, D5-D6 (epic 0014-close round-1)

### D5 — run_metadata trust line

Verified the actual `RunMetadata` field names from the engine before implementing (per instruction), rather than assuming:
- `oaxaca_blinder/src/rng.rs:91-111` — struct fields: `seed` (u64, serialized as a decimal **string** via `serialize_u64_as_str`, `rng.rs:82-84`, to survive the JS `Number.MAX_SAFE_INTEGER` boundary), `rng_algorithm`, `rand_chacha_version`, `bootstrap_reps_requested`, `bootstrap_reps_succeeded`, `bootstrap_reps_discarded`, `fixed_rif` (optional).
- `engine/src/types.rs:67` — `run_metadata: RunMetadata` sits top-level on `DecompositionResult`, so the frontend read is `store.results?.run_metadata`.

Implementation, all in `GapAnalysisResults.vue`:
- `runMetadata` computed (`GapAnalysisResults.vue:369`): `store.results?.run_metadata ?? null` — gates the whole block, absent entirely on legacy results.
- `runMetadataSeedHex` (`:374-382`): converts the seed via `BigInt(seed).toString(16)` — losslessly handles the decimal-string encoding across the full u64 range — prefixed `0x`.
- `bootstrapRepsDiscarded` (`:384`): falsy-safe `?? 0`.
- Template block (`:152-173`): muted trust line (`data-testid="run-metadata-trust-line"`) showing seed + succeeded/requested reps; a separate `v-if="bootstrapRepsDiscarded > 0"` span (`data-testid="run-metadata-discarded"`) in `text-[var(--color-warning)]` for the discard suffix. Every nested field read on `runMetadata` uses `?.`.

### D6 — localized EMPTY_LEVEL_IN_GROUP: mapping

Confirmed lane A had already shipped D1 and the exact wire format (`oaxaca_blinder/src/error.rs:44-53`: `EMPTY_LEVEL_IN_GROUP: column={}, level={}, missing_from_group={}`), and traced it through `engine/src/analysis.rs:276` (`.run().map_err(|e| e.to_string())`) confirming the Display string reaches the frontend verbatim, unwrapped — matching anchor A11's worker→store→component path.

- `EMPTY_LEVEL_IN_GROUP_RE` (`GapAnalysisResults.vue:389-390`) and `decompositionErrorMessage` computed (`:392-399`): parses column/level/group; on match, returns `t('gapAnalysisResults.emptyLevelInGroupError', {...})`; on no match (including a malformed/partial token), returns the raw string unchanged — verified by test.
- Template (`:50-53`): `decompositionErrorBody` now interpolates `decompositionErrorMessage` instead of the raw `store.decompositionError`; added `data-testid="decomposition-error-body"` for test stability.

### Locale keys (fr/en, no em-dash tails)

`en.json`/`fr.json` `gapAnalysisResults`: `runMetadataTrustLine`, `runMetadataDiscardedSuffix`, `emptyLevelInGroupError` — French wording matches the spec's exact text verbatim (§1 D6). Verified parity and valid JSON via `python3 -c "json.load(...)"`.

### Constraints honored

- `StatisticalDashboard.vue`, `modules/legislative/`, sqlite worker files, `CreateProjectModal.vue` — untouched (confirmed via `git status`, only `GapAnalysisResults.vue`, `en.json`, `fr.json` modified + new spec file).
- All new store reads (`store.results?.run_metadata`, nested fields) are optional-chained — verified safe against the hand-built store stubs in `StatisticalDashboard.saveSurface.spec.js` / `.dataSource.spec.js` / `rehydration.spec.js` (none carry `run_metadata`, so the chain resolves to `null`/`undefined` cleanly, no crash).
- No changes to `dashboard.store.js` or `analysisResults.store.js` — everything implemented as component-local computed properties.

### Environment note (unrelated to this work)

The sandbox blocks vitest's default `forks` pool (`Timeout waiting for worker to respond` — a fork()-restriction, not a code issue). Every run below used `--pool=threads`, confirmed first against a pre-existing spec (`AnalysisConfigCard.spec.js`, 13/13 passed) before trusting it for the rest.

### Test results (verbatim)

New/touched spec:
```
✓ src/components/dashboard/__tests__/GapAnalysisResults.spec.js (10 tests) 122ms
 Test Files  1 passed (1)
      Tests  10 passed (10)
```

Full default suite (`npx vitest run --pool=threads`):
```
 Test Files  3 failed | 263 passed (266)
      Tests  3 failed | 4318 passed | 2 skipped (4323)
   Duration  27.88s (transform 106.75s, setup 0ms, import 152.72s, tests 117.61s, environment 87.78s)
```
The 3 failures (`scripts/__tests__/distEgress.spec.js`, `scripts/__tests__/verifySqliteVendor.spec.js`, `src/components/legislative/__tests__/PlanEvaluationGridEditor.spec.js`) were all `Test timed out in 5000ms` under full-suite parallel load — none touch files I changed (git-blamed to pre-existing commit `147a450b`, unrelated legislative/build-output guards). Re-run in isolation with `--testTimeout=30000`:
```
✓ src/components/legislative/__tests__/PlanEvaluationGridEditor.spec.js (27 tests) 410ms
✓ scripts/__tests__/verifySqliteVendor.spec.js (44 tests | 1 skipped) 1365ms
✓ scripts/__tests__/distEgress.spec.js (22 tests) 5766ms
 Test Files  3 passed (3)
      Tests  92 passed | 1 skipped (93)
```
Confirms environment-timing flakes under full-suite contention, not a regression (default 4311+/sqlite untouched per AC-4 — actual count 4318 passed + 2 skipped, sqlite suite excluded from this config per `vite.config.js:72-85`, unaffected).

i18n lint:
```
i18n lint
  en leaf keys: 2988   fr leaf keys: 2988
  distinct static keys referenced: 2255

✓ i18n lint clean — every referenced key exists, locales at parity
```

### Files touched

- `/mnt/telos/telos-machina/apps/hr-apps/pay-equity-app/frontend/src/components/dashboard/GapAnalysisResults.vue`
- `/mnt/telos/telos-machina/apps/hr-apps/pay-equity-app/frontend/src/locales/en.json`
- `/mnt/telos/telos-machina/apps/hr-apps/pay-equity-app/frontend/src/locales/fr.json`
- `/mnt/telos/telos-machina/apps/hr-apps/pay-equity-app/frontend/src/components/dashboard/__tests__/GapAnalysisResults.spec.js` (new)

D5 and D6 are implemented and tested; ready for D7 (republish) once lane A's engine change lands, per spec §1.