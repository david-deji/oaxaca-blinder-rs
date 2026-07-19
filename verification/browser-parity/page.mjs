// Page driver (0014-MERIDIAN verification-benchmark D-6). Reads ?threads=N from the URL, spawns the compute
// Worker, and relays its result to window.__PARITY__ for the Playwright assertion.
//
// Why a Worker and not inline: the Meridian app runs compute inside analysis.worker.js, and wasm-bindgen-rayon
// then spawns the rayon pool FROM that worker — a worker spawning workers (the E3 "nested-worker spawn" path).
// Running the engine in a Worker here reproduces that exact deployment shape, so a nested-spawn regression
// fails HERE rather than only in production. crossOriginIsolated is reported from both scopes; the worker's
// value is the one that actually gates SharedArrayBuffer for the pool.
const params = new URLSearchParams(location.search);
const threads = Number(params.get('threads') || 1);
const log = (m) => { document.getElementById('log').textContent = m; };

const worker = new Worker(new URL('./compute.worker.mjs', import.meta.url), { type: 'module' });

worker.onmessage = (e) => {
  window.__PARITY__ = { pageCrossOriginIsolated: crossOriginIsolated, ...e.data };
  log(
    `threads=${e.data.threads} ok=${e.data.ok} ` +
    `coi(page)=${crossOriginIsolated} coi(worker)=${e.data.workerCrossOriginIsolated} ` +
    `pool=${e.data.poolResolved}` + (e.data.error ? ` err=${e.data.error}` : '')
  );
};
worker.onerror = (e) => {
  window.__PARITY__ = { threads, ok: false, error: String((e && e.message) || e) };
  log('worker error: ' + ((e && e.message) || e));
};

worker.postMessage({ threads });
