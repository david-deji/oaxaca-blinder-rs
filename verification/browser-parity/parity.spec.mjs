import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { diffNativeWasm, coverageProblems, DEFAULT_TOLERANCE } from './native-wasm-diff.mjs';
import { BASELINE_FILE, STAMP_FILE, stampProblems } from './baseline-stamp.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Browser byte-parity: threaded WASM produces byte-identical decompose output across thread counts in a
// REAL cross-origin-isolated Chromium context (0014-MERIDIAN, AC-2 browser leg + AC-3 + E3).
//
// This is the deployment-truth complement to engine/tests/mode_parity_test.rs. The native test proves the
// bootstrap reduction is rep-index-ordered (thread-count-invariant math). This test proves the SAME run,
// compiled to wasm and driven through COI + a nested worker pool, still yields identical bytes AND that the
// threads genuinely engaged (crossOriginIsolated === true) rather than silently falling back to serial.

const THREAD_COUNTS = [1, 2, 4];

// D3 (0014-close round-1) / INV-02 cross-platform leg: native decompose_inner() vs wasm decompose() must
// agree within a small float tolerance (compiling to a different target is allowed to perturb the last few
// ULPs; it must not diverge). `engine/examples/native_baseline.rs` emits the native side for the SAME
// fixture/request/seed this file already drives at threads=1; the browser-parity CI job writes it to
// NATIVE_BASELINE_PATH (with its stamp) before this spec runs (`.github/workflows/ci.yml`, job
// native-baseline); `npm run pretest` does the same for a local `npm test`. The comparator itself lives in
// `native-wasm-diff.mjs` (see that file for why), and is unit-tested by `native-wasm-diff.test.mjs`.
const REPO_ROOT = join(__dirname, '..', '..');
const NATIVE_BASELINE_PATH = join(__dirname, BASELINE_FILE);
const NATIVE_STAMP_PATH = join(__dirname, STAMP_FILE);

test('COI + byte-identical decompose across thread counts (AC-2/AC-3, E3 nested spawn)', async ({ page }) => {
  const json = {};
  for (const n of THREAD_COUNTS) {
    await page.goto(`/verification/browser-parity/?threads=${n}`);
    const parity = await page
      .waitForFunction(() => window.__PARITY__ || null, null, { timeout: 90000 })
      .then((h) => h.jsonValue());

    expect(parity.ok, `threads=${n} compute failed: ${parity.error || '(no error text)'}`).toBe(true);
    expect(parity.pageCrossOriginIsolated, `page not crossOriginIsolated at threads=${n}`).toBe(true);
    expect(
      parity.workerCrossOriginIsolated,
      `worker not crossOriginIsolated at threads=${n} — SharedArrayBuffer unavailable → silent serial fallback`
    ).toBe(true);
    expect(
      parity.poolResolved,
      `initThreadPool(${n}) did not resolve — nested worker spawn (E3) failed`
    ).toBe(true);
    json[n] = parity.json;
  }

  expect(
    json[2],
    'browser 1-thread vs 2-thread byte mismatch — wasm bootstrap reduction is not rep-index-ordered'
  ).toBe(json[1]);
  expect(json[4], 'browser 2-thread vs 4-thread byte mismatch').toBe(json[2]);
});

test('native <-> wasm(threads=1) decompose agree within 1e-6 per numeric field (D3, INV-02 cross-platform leg)', async ({
  page,
}) => {
  await page.goto('/verification/browser-parity/?threads=1');
  const parity = await page
    .waitForFunction(() => window.__PARITY__ || null, null, { timeout: 90000 })
    .then((h) => h.jsonValue());

  expect(parity.ok, `wasm threads=1 compute failed: ${parity.error || '(no error text)'}`).toBe(true);

  // The generator step stamps the baseline with a hash of the engine source it was built from and the
  // sha256 of its bytes. A baseline from other source, or edited after generation, is refused here
  // instead of being compared (this replaces the old mtime check, which a checkout can rewrite).
  const stale = stampProblems({ root: REPO_ROOT, baselinePath: NATIVE_BASELINE_PATH, stampPath: NATIVE_STAMP_PATH });
  expect(stale, `native baseline is not trustworthy:\n${stale.join('\n')}`).toEqual([]);

  const native = JSON.parse(readFileSync(NATIVE_BASELINE_PATH, 'utf8'));
  const wasm = JSON.parse(parity.json);

  const result = diffNativeWasm(native, wasm);
  expect(
    result.mismatches,
    `native vs wasm(threads=1) exceeded ${DEFAULT_TOLERANCE} tolerance on ${result.mismatches.length} field(s):\n${result.mismatches.join('\n')}`
  ).toEqual([]);

  // An agreeing comparison only counts if it compared real numbers: floor on the leaf count and a named
  // list of paths that must have been compared.
  const coverage = coverageProblems(result);
  expect(coverage, `comparison covered too little:\n${coverage.join('\n')}`).toEqual([]);
  console.log(`native <-> wasm: ${result.numericLeafCount} numeric leaves compared within ${DEFAULT_TOLERANCE}`);
});

// 0120-MERIDIAN contract F4: serde-wasm-bindgen is a different serializer from serde_json, and only
// `decompose` was ever driven through it here. verify_adjustments (with a categorical),
// check_defensibility and calculate_efficient_frontier must carry the same diagnostic keys in the browser.
// `normalised_difference` and `subject` are Option fields: undefined in JS, null over MCP, so the
// assertions read key presence from the live object and never compare those two values.
test('verify_adjustments, check_defensibility and the frontier carry their diagnostic keys through the WASM serializer (F4)', async ({
  page,
}) => {
  await page.goto('/verification/browser-parity/?threads=1');
  const parity = await page
    .waitForFunction(() => window.__PARITY__ || null, null, { timeout: 90000 })
    .then((h) => h.jsonValue());
  expect(parity.ok, `wasm compute failed: ${parity.error || '(no error text)'}`).toBe(true);
  const { verify_adjustments: v, check_defensibility: c, calculate_efficient_frontier: f } = parity.shapes;

  const supportKeys = [
    'extrapolated_target_count', 'model_columns', 'predictors', 'reference_count',
    'reference_residual_df', 'target_count', 'target_residual_df',
  ];
  expect(v.support).toEqual(supportKeys);
  expect(c.support).toEqual(supportKeys);
  expect(v.top).toEqual(expect.arrayContaining(['support', 'warnings', 'run_metadata']));
  expect(c.top).toEqual(expect.arrayContaining(['support', 'warnings', 'interval', 'adjustments']));
  expect(v.warnings).toBe(true);
  expect(c.warnings).toBe(true);
  expect(v.predictor).toEqual(expect.arrayContaining(['name', 'normalised_difference', 'target_outside_range_share']));
  expect(v.normalization).toEqual(
    expect.arrayContaining(['applied', 'convention', 'share_basis', 'variables'])
  );
  expect(c.interval).toEqual(['confidence_level', 'critical_value', 'degrees_of_freedom']);
  expect(c.adjustment).toContain('extrapolated');
  expect(f.points).toBeGreaterThan(1);
  expect(f.point).toEqual([
    'budget', 'confidence_level', 'degrees_of_freedom', 'group_coefficient',
    'is_significant', 'p_value', 't_statistic',
  ]);
});
