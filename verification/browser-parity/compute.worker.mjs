// Compute worker (module Worker) — 0014-MERIDIAN verification-benchmark D-6, AC-2 browser leg + AC-3 + E3.
//
// Runs the threaded WASM engine end-to-end:
//   init()               → instantiate the wasm module (fetches *_bg.wasm relative to the glue)
//   initThreadPool(N)     → spawn N rayon workers over SharedArrayBuffer (the NESTED spawn — a Worker
//                           spawning workers; requires crossOriginIsolated, else it throws / silently serial)
//   decompose(req)        → run the seeded bootstrap decomposition; par_iter is backed by the pool above
//
// Returns JSON.stringify(result) for the cross-thread-count byte comparison. INV-02 guarantees identical
// f64 bits across thread counts; identical bits → identical JS Number→string → identical JSON. So a byte
// mismatch across N means the bootstrap reduction is not rep-index-ordered (a real threading defect),
// not float noise.
import init, { initThreadPool, decompose } from '/engine/pkg-threaded/pay_equity_engine.js';

self.onmessage = async ({ data }) => {
  const threads = Number(data.threads || 1);
  let poolResolved = false;
  try {
    await init();
    await initThreadPool(threads); // nested worker spawn (E3); throws if SharedArrayBuffer is unavailable
    poolResolved = true;

    const csvResp = await fetch('/oaxaca_blinder/tests/fixtures/parity_fixture.csv');
    if (!csvResp.ok) throw new Error('fixture fetch ' + csvResp.status);
    const csvBytes = Array.from(new Uint8Array(await csvResp.arrayBuffer()));

    // Same request as engine/tests/mode_parity_test.rs (the native leg) so the two legs prove the same run.
    const req = {
      csv_data: csvBytes,
      outcome_variable: 'log_wage',
      group_variable: 'gender',
      reference_group: 'F',
      predictors: ['education', 'experience', 'tenure'],
      categorical_predictors: null,
      three_fold: true,
      quantile: null,
      reference_coefficients: null,
      bootstrap_reps: 64, // enough bootstrap work that a non-ordered reduction would diverge
    };
    const result = decompose(req);
    // The engine serializes the u64 seed as a string and all counts as plain JS Numbers, so the
    // result is directly JSON-stringifiable and deterministic across thread counts (byte-parity).
    const json = JSON.stringify(result);
    self.postMessage({ threads, ok: true, poolResolved, workerCrossOriginIsolated: crossOriginIsolated, json });
  } catch (err) {
    self.postMessage({
      threads, ok: false, poolResolved,
      workerCrossOriginIsolated: typeof crossOriginIsolated !== 'undefined' ? crossOriginIsolated : null,
      error: String((err && err.stack) || err),
    });
  }
};
