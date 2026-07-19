import { test, expect } from '@playwright/test';

// Browser byte-parity: threaded WASM produces byte-identical decompose output across thread counts in a
// REAL cross-origin-isolated Chromium context (0014-MERIDIAN, AC-2 browser leg + AC-3 + E3).
//
// This is the deployment-truth complement to engine/tests/mode_parity_test.rs. The native test proves the
// bootstrap reduction is rep-index-ordered (thread-count-invariant math). This test proves the SAME run,
// compiled to wasm and driven through COI + a nested worker pool, still yields identical bytes AND that the
// threads genuinely engaged (crossOriginIsolated === true) rather than silently falling back to serial.

const THREAD_COUNTS = [1, 2, 4];

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
