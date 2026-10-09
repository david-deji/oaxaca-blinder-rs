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
import init, {
  initThreadPool, decompose, verify_adjustments, check_defensibility, calculate_efficient_frontier,
} from '/engine/pkg-threaded/pay_equity_engine.js';

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

    // Same request as engine/tests/mode_parity_test.rs (the native leg) so the two legs prove the same run,
    // run twice: three_fold (interaction term, no detail rows) and two_fold (detail rows and the bootstrap
    // standard error). engine/examples/native_baseline.rs runs the same pair; keep them in step.
    const request = (threeFold) => ({
      csv_data: csvBytes,
      outcome_variable: 'log_wage',
      group_variable: 'gender',
      reference_group: 'F',
      predictors: ['education', 'experience', 'tenure'],
      categorical_predictors: null,
      three_fold: threeFold,
      quantile: null,
      reference_coefficients: "Pooled",
      bootstrap_reps: 64, // enough bootstrap work that a non-ordered reduction would diverge
    });
    const result = { three_fold: decompose(request(true)), two_fold: decompose(request(false)) };

    // Serializer coverage (0120-MERIDIAN contract F4): the three entry points that carry the
    // diagnostics blocks, run with a CATEGORICAL predictor, and only their key sets reported.
    // Keys are read from the live JS objects (Object.keys), before JSON.stringify drops anything.
    // `normalised_difference` and `subject` are Option fields: undefined here in JS, null over MCP.
    const dept = ['A', 'B', 'C'];
    const rows = ['gender,dept,salary,years'];
    for (let i = 0; i < 48; i++) {
      const years = 1 + ((i * 7) % 13);
      const d = dept[Math.floor(i / 2) % 3];
      const salary = 40000 + 1500 * years + (d === 'B' ? 2500 : d === 'C' ? 5000 : 0)
        - (i % 2 === 1 ? 3000 : 0) + (((i * 37) % 19) - 9) * 100;
      rows.push(`${i % 2 === 0 ? 'M' : 'F'},${d},${salary},${years}`);
    }
    const catBytes = Array.from(new TextEncoder().encode(rows.join('\n') + '\n'));
    const catRequest = {
      csv_data: catBytes,
      outcome_variable: 'salary',
      group_variable: 'gender',
      reference_group: 'M',
      predictors: ['years'],
      categorical_predictors: ['dept'],
      three_fold: null,
      quantile: null,
      reference_coefficients: 'Pooled',
      bootstrap_reps: 0,
    };
    const keys = (o) => Object.keys(o).sort();
    const verified = verify_adjustments({ ...catRequest, adjustments: [] });
    const defensible = check_defensibility({
      ...catRequest,
      adjustments: [{ index: 1, value: 50000 }],
    });
    const frontier = calculate_efficient_frontier({ ...catRequest, steps: 3, max_budget: 20000 });
    const shapes = {
      verify_adjustments: {
        top: keys(verified),
        support: keys(verified.support),
        predictor: keys(verified.support.predictors[0]),
        normalization: keys(verified.run_metadata.normalization),
        warnings: Array.isArray(verified.warnings),
      },
      check_defensibility: {
        top: keys(defensible),
        support: keys(defensible.support),
        interval: keys(defensible.interval),
        adjustment: keys(defensible.adjustments[0]),
        warnings: Array.isArray(defensible.warnings),
      },
      calculate_efficient_frontier: { point: keys(frontier[0]), points: frontier.length },
    };
    // The engine serializes the u64 seed as a string and all counts as plain JS Numbers, so the
    // result is directly JSON-stringifiable and deterministic across thread counts (byte-parity).
    const json = JSON.stringify(result);
    self.postMessage({ threads, ok: true, poolResolved, workerCrossOriginIsolated: crossOriginIsolated, json, shapes });
  } catch (err) {
    self.postMessage({
      threads, ok: false, poolResolved,
      workerCrossOriginIsolated: typeof crossOriginIsolated !== 'undefined' ? crossOriginIsolated : null,
      error: String((err && err.stack) || err),
    });
  }
};
