import { test, expect } from '@playwright/test';
import { readFileSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { diffNativeWasm, DEFAULT_TOLERANCE } from './native-wasm-diff.mjs';

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
// NATIVE_BASELINE_PATH before this spec runs (`.github/workflows/ci.yml`); `npm run pretest` does the same
// for a local `npm test`. The comparator itself lives in `native-wasm-diff.mjs` (see that file for why).
const NATIVE_BASELINE_PATH = join(__dirname, 'native-baseline.json');

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

  // `pretest` regenerates the baseline, but only `npm test` fires it — a direct
  // `npx playwright test` would silently compare against whatever gitignored
  // baseline happens to be on disk. Refuse a baseline older than the engine
  // source it is supposed to describe, rather than reporting a false pass.
  const baselineStat = statSync(NATIVE_BASELINE_PATH);
  const enginePaths = [
    join(__dirname, '..', '..', 'oaxaca_blinder', 'src'),
    join(__dirname, '..', '..', 'engine', 'src'),
    join(__dirname, '..', '..', 'engine', 'examples', 'native_baseline.rs'),
  ];
  const newestEngineMtime = Math.max(...enginePaths.map((p) => statSync(p).mtimeMs));
  expect(
    baselineStat.mtimeMs,
    `native-baseline.json is older than the engine source it describes — regenerate it with \`npm test\` (which runs the pretest generator) instead of invoking playwright directly.`
  ).toBeGreaterThan(newestEngineMtime);

  const native = JSON.parse(readFileSync(NATIVE_BASELINE_PATH, 'utf8'));
  const wasm = JSON.parse(parity.json);

  const mismatches = diffNativeWasm(native, wasm);
  expect(
    mismatches,
    `native vs wasm(threads=1) exceeded ${DEFAULT_TOLERANCE} tolerance on ${mismatches.length} field(s):\n${mismatches.join('\n')}`
  ).toEqual([]);
});
