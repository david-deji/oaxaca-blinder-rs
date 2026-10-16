# Phase 4 Final Spec — Meridian Integration (worker init, COOP/COEP, Vite prod config, pkg refresh)

> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Pipeline: /specify balanced — Phase 4 refined spec (build handoff)
> Domain slug: meridian-integration
> Strategy: **B** (single threaded `--target web` artifact, rayon sequential fallback). One-paragraph Strategy-A note in § Design.

---

## Summary

This domain wires the threaded `pay_equity_engine` WASM build into Meridian's client-side runtime **without changing the worker's inbound `{id,type,payload}` postMessage contract** (Charter Anti-Goal; `AnalysisWorkerService.js:53`, `analysis.worker.js:27`). It owns exactly three surfaces:

1. **`analysis.worker.js` init redesign** — a positive `self.crossOriginIsolated === true` feature-detect selects the threaded path: `await init()` → `await initThreadPool(cap)` → record `mode` → post it additively to the UI. A non-isolated context runs `await init()` only and degrades to working sequential compute (INV-03) — never a crash, never a blank screen.
2. **Vite production config** — `worker.format: 'es'` (REQUIRED; rayon spawns nested ES-module workers), `vite-plugin-wasm` removed for the `--target web` JS-glue import, `new URL(..., import.meta.url)` asset resolution left to the generated glue, plus dev `server.headers` COOP/COEP parity.
3. **audit-forge Flask COOP/COEP** — two header lines added inside the existing scoped `_apply_meridian_csp` `if` block on the `/pay-equity/` + `/api/` document response (`webui/__init__.py:130`), which is **sufficient** to make the top document and every nested worker cross-origin-isolated (W2 [CONSENSUS]).

Two Phase-3 corrections are folded in: **W4** — the worker must NOT parse `WebAssembly.RuntimeError` messages to detect OOM (engine-defined, indistinguishable from other traps); OOM is handled by budget-prevention (thread-cap + `--max-memory`, owned upstream) plus a structured engine self-report, and the worker's catch stays a generic pass-through. **E3** (nested-worker `initThreadPool` spawn) is the **top /build Phase 0 preflight experiment**, with a main-thread-relay fallback that keeps the postMessage contract byte-identical.

This spec preserves the cross-spec build order (determinism → memory profile → threading → validation): the wiring here is part of the **threading** stage and consumes the completed determinism (INV-02, deterministic-rng domain) and memory-profile (INV-05, `N_max_const`, memory-budget domain) outputs. It does not reorder or re-spec them.

---

## In-Scope (this domain)

| # | Item | Charter trace |
|---|---|---|
| M1 | `analysis.worker.js` crossOriginIsolated feature-detect + `await initThreadPool(cap)` + `mode` recorded and posted | In-Scope 6; SC-06 |
| M2 | Sequential fallback that never crashes / never blanks (INV-03) | In-Scope 6; INV-03 |
| M3 | Additive UI surfacing of `mode` (new `INIT_RESULT` message + `mode` field on every result); inbound contract byte-compatible | In-Scope 6; Anti-Goal (postMessage API) |
| M4 | Vite production config: `worker.format:'es'`, drop `vite-plugin-wasm`, glue asset-URL handling | In-Scope 6, 7; SC-06 |
| M5 | Vite dev-server COOP/COEP parity (`server.headers`) | In-Scope 7; SC-06 |
| M6 | audit-forge COOP/COEP on `/pay-equity/` + `/api/`, additive + scoped (INV-06) | In-Scope 7; SC-06; INV-06 |
| M7 | pkg artifact refresh flow into `frontend/src/wasm/` (Strategy B single artifact) | In-Scope 6 |
| M8 | Worker OOM handling reframed to budget-prevention + structured self-report pass-through (W4) | In-Scope 8 (verification surface); INV-05 |
| M9 | E3 nested-worker spawn preflight + main-thread-relay fallback design | In-Scope 6; SC-06 |

**Not owned here** (consumed cross-domain, not re-spec'd): the threaded build toolchain + `--target web` migration + `--max-memory` link arg (toolchain-build); RNG determinism, INV-02 (deterministic-rng); the `MEMORY_THREAD_CAP` **value** = `N_max_const` and the cap formula `min(hardwareConcurrency, N_max_const, 8)` (memory-budget D3); the engine's structured allocation-pressure self-report (memory-budget D5 / engine); the pkg *contents* (toolchain-build). This spec DOES write the `thread-cap.js` file that surfaces `N_max_const` to the worker (M7) — it owns the file, not the value.

---

## Design & Decisions (code anchors file:line)

### D1 — Isolation chain: page-level headers are sufficient (W2 locked)

Setting `Cross-Origin-Opener-Policy: same-origin` + `Cross-Origin-Embedder-Policy: require-corp` on the **`/pay-equity/` document response** makes the top browsing context `crossOriginIsolated`. A dedicated worker created by a cross-origin-isolated document is itself cross-origin isolated — `self.crossOriginIsolated === true` inside `analysis.worker.js`, and `SharedArrayBuffer` is available (W2 [CONSENSUS]; MDN `WorkerGlobalScope.crossOriginIsolated`). The worker script is same-origin (served by audit-forge / the Vite build), so it loads fine under COEP with no CORP/CORS work and **no per-worker header**. Rayon's nested pool workers, spawned from inside our worker, inherit the same isolated worker-global state (W2). Therefore the header work is exactly two lines at one insertion point plus dev-server parity.

`require-corp` is safe here because the offline bundle is loopback + no-egress and `_MERIDIAN_CSP` already forbids cross-origin subresources (`connect-src 'self'` / `default-src 'self'`; phase2 anchor `webui/__init__.py:20-23`). `worker-src 'self' blob:` in `_MERIDIAN_CSP` (phase2 anchor `webui/__init__.py:21`) already covers the dedicated worker and any blob-URL pool workers.

**Insertion point** — inside the existing `if` block at `webui/__init__.py:129-130`, joining the CSP line under the same predicate (`webui/__init__.py:129`), additive, before `return response` at `:131`:

```python
    @app.after_request
    def _apply_meridian_csp(response):
        # Scoped to the Meridian integration surface only — the 9 pre-existing blueprints'
        # HTML surfaces are untouched by this header (spec Sec.5).
        if request.path.startswith("/pay-equity/") or request.path.startswith("/api/"):
            response.headers["Content-Security-Policy"] = _MERIDIAN_CSP
            # wasm-bindgen-rayon SharedArrayBuffer threads need cross-origin isolation on the
            # DOCUMENT response; the dedicated worker + rayon's nested pool workers inherit it
            # (W2). require-corp is safe: loopback/no-egress + _MERIDIAN_CSP already blocks
            # cross-origin subresources (connect-src/default-src 'self'). Same two prefixes (INV-06).
            response.headers["Cross-Origin-Opener-Policy"] = "same-origin"
            response.headers["Cross-Origin-Embedder-Policy"] = "require-corp"
        return response
```

The plugin-CSP `after_request` (`webui/__init__.py:138-152`) and the 9 pre-existing blueprints are untouched (INV-06). `_apply_plugin_csp` only sets `Content-Security-Policy` (`:151`), never COOP/COEP, so it cannot strip the isolation headers even when it runs after this hook.

### D2 — Worker init redesign (positive feature-detect, then await init → await initThreadPool)

Replaces `analysis.worker.js:12-24`. The import line (`analysis.worker.js:1-8`) gains `initThreadPool` (wasm-bindgen emits `initThreadPool(numThreads) → Promise` from the engine's `pub use init_thread_pool`; W1 [CONSENSUS]). Under Strategy B there is one pkg, so the import is static at module top; `initThreadPool` is simply never called in sequential mode (rayon falls back to sequential when the pool is uninitialized; L4).

```js
import init, {
  decompose,
  optimize,
  verify_adjustments,
  calculate_efficient_frontier,
  check_defensibility,
  init_panic_hook,
  initThreadPool,           // added — async initThreadPool(numThreads): Promise (W1)
} from './pay_equity_engine.js'

// MEMORY_THREAD_CAP = N_max_const, computed by the memory-budget domain (D3) from the 50k
// memory profile, WITH the <=8 ceiling already folded in (memory-budget MJ-6 clamp). This
// domain does NOT derive it; it writes thread-cap.js in M7 (below) and imports it here. The
// worker's 2-term N = min(hardwareConcurrency, MEMORY_THREAD_CAP) is therefore complete — no
// separate ,8 term (the clamp lives in the value).
import { MEMORY_THREAD_CAP } from './thread-cap.js'   // written by M7 pkg-refresh (this spec)

let isInitialized = false
let computeMode = 'sequential'   // 'threaded' | 'sequential'
let threadCount = 1

async function initialize() {
  if (isInitialized) return       // preserves the idempotent latch (analysis.worker.js:10,13,17)
  await init()                    // no manual URL arg — glue resolves *_bg.wasm via new URL(import.meta.url) (D4)
  init_panic_hook()

  // Positive feature-detect BEFORE choosing the threaded path — initThreadPool has no documented
  // clean rejection on non-isolated contexts, so we never enter it speculatively.
  if (self.crossOriginIsolated === true) {
    try {
      const cap = Math.min(navigator.hardwareConcurrency || 1, MEMORY_THREAD_CAP)
      await initThreadPool(cap)   // awaited before any par_iter-backed compute call (W1)
      computeMode = 'threaded'
      threadCount = cap
    } catch (e) {
      // Undocumented isolated-context failure => degrade, never crash (INV-03). Pool stays
      // uninitialized; rayon runs sequential (L4).
      console.warn('initThreadPool failed; running sequential:', e)
      computeMode = 'sequential'
      threadCount = 1
    }
  }

  isInitialized = true
  // Additive, unsolicited INIT_RESULT (id:null). The existing consumer drops unknown ids
  // (AnalysisWorkerService.js:26); the additive handler (D3) captures it.
  self.postMessage({
    id: null, type: 'INIT_RESULT', status: 'SUCCESS',
    payload: { mode: computeMode, threads: threadCount }, mode: computeMode,
  })
}
```

The `onmessage` dispatch (`analysis.worker.js:26-51`) is unchanged; `await initialize()` (`analysis.worker.js:30`) still runs on the first message and is idempotent thereafter.

### D3 — Additive `mode` surfacing (contract stays byte-compatible)

The inbound destructure (`analysis.worker.js:27`) is untouched. Outbound gains **one new message type** (`INIT_RESULT`) and **one new top-level field** (`mode`) — both additive. The consumer destructures only `{ id, status, payload }` (`AnalysisWorkerService.js:24`) and drops unknown `id`s (`AnalysisWorkerService.js:26`), so both additions are invisible to unchanged code.

Success post (`analysis.worker.js:53`) and error post (`analysis.worker.js:55-60`) gain a trailing `mode`:

```js
self.postMessage({ id, type, status: 'SUCCESS', payload: result, mode: computeMode })
// ...and in the catch (analysis.worker.js:54-61):
self.postMessage({ id, type, status: 'ERROR', payload: { message: error.message || String(error) }, mode: computeMode })
```

`AnalysisWorkerService.js` gains an `INIT_RESULT` branch **before** the `if (!job) return` drop (`AnalysisWorkerService.js:26`), and a read-only `getComputeMode()`. `run()`/`terminate()`/`restart()` signatures (`AnalysisWorkerService.js:46-70`) are unchanged:

```js
worker.addEventListener('message', (e) => {
  const { id, type, status, payload } = e.data
  if (type === 'INIT_RESULT') {          // additive: capture compute mode, then stop
    computeMode = payload?.mode ?? 'sequential'
    return
  }
  const job = pending.get(id)
  if (!job) return
  // ...unchanged (AnalysisWorkerService.js:27-32)...
})
```

Displaying `mode` in the dashboard is a Meridian-frontend UI task (defensibility run metadata), out of this engine-spec's build blast surface — `mode` is optional metadata, never a blocking or error surface (INV-03; R6 of the draft).

### D4 — Vite production config (W3 locked)

- **`worker.format: 'es'` is REQUIRED** (Vite default is `'iife'`). Rayon spawns nested ES-module workers using `import` / `import.meta.url`; IIFE workers break code-splitting and dynamic import inside workers (W3(a) [CONSENSUS]; vitejs/vite#18585, #17483). Replace the current `worker: { plugins: () => [wasm()] }` (`vite.config.js:12-14`) with `worker: { format: 'es' }`.
- **Drop `vite-plugin-wasm`** (currently imported `vite.config.js:3`, used at `:11` and `:13`). For `--target web` we import the wasm-bindgen **JS glue** (`./pay_equity_engine.js`), not the raw `.wasm`; the rayon README confirms Vite has the needed plugins built-in, and `vite-plugin-wasm` is unnecessary and potentially harmful if it reinterprets `.wasm` handling (W3(c) [CONSENSUS]). Remove it from both the main `plugins` array and the `worker` block.
- **Asset URLs**: use bundler mode (do NOT enable rayon's `no-bundler` feature). The generated glue locates the binary via `new URL("pay_equity_engine_bg.wasm", import.meta.url)`; Vite emits a hashed asset and rewrites the reference in prod, and bundles rayon's internal `workerHelpers.js` as ES-module worker chunks (W3(b)). **Do not pass a manual URL string to `init()`** and do not hand-concatenate the wasm path — Vite will not emit/hash a string-concat URL.
- `topLevelAwait` (`vite.config.js:4`) is retained as harmless (it only rewrites genuine top-level `await`); its retention is verified by the build-succeeds check (AC-M4.3), not assumed.

Resulting config shape:

```js
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import topLevelAwait from 'vite-plugin-top-level-await'
// vite-plugin-wasm removed — not needed for --target web JS-glue import (W3c)

export default defineConfig({
  base: '/pay-equity/',                       // unchanged (vite.config.js:10)
  plugins: [vue(), topLevelAwait()],
  worker: { format: 'es' },                   // REQUIRED for rayon nested ES-module workers (W3a)
  server: {
    headers: {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  esbuild: { drop: ['debugger'], pure: ['console.log', 'console.debug'] },  // unchanged (vite.config.js:15-22)
  test: { /* unchanged (vite.config.js:23-44) */ },
})
```

Dev `server.headers` reproduce the isolated context under `pnpm dev`, byte-for-byte matching the audit-forge production values (D1), so `mode === 'threaded'` in both dev and the offline bundle (M5).

### D5 — pkg artifact refresh (Strategy B)

Strategy B ships **one** threads-capable `--target web` pkg that replaces `frontend/src/wasm/pay_equity_engine.js` (+ `pay_equity_engine_bg.wasm` + the rayon `workerHelpers` glue the bindgen output emits). `initThreadPool` is exported but only called in threaded mode; in sequential mode rayon runs on the calling thread with no pool (L4), so the same artifact serves both modes. The refresh is a documented copy of the toolchain-build domain's `scripts/build-wasm.sh` output into `frontend/src/wasm/` (M7 build step); this domain does not author the build script. `pkg/` stays gitignored (Charter Anti-Goal); only the committed `frontend/src/wasm/` copies are refreshed. **This domain's M7 step also WRITES `frontend/src/wasm/thread-cap.js`** — a one-line `export const MEMORY_THREAD_CAP = <N_max_const>;` using the integer the memory-budget domain computes in its D3 (`N_max_const`). Single owner: memory-budget owns the value, meridian M7 owns the file, the worker imports it. (Resolves build-readiness audit CRITICAL-2 — previously three specs disagreed on ownership and no step created the file.)

**Strategy A — RATIFIED (2026-07-18, ruling 1), the build path.** Dual artifact (untouched sequential pkg + sibling threaded pkg): the worker's `init`/`initThreadPool` imports are a conditional dynamic `import()` **inside** the `crossOriginIsolated` branch — resolve the threaded glue only in isolated contexts, the sequential glue otherwise. Conditional dynamic `import()` produces a clean code-split chunk under Vite given `worker.format:'es'` (W3(d) [CONSENSUS]). Every AC holds; AC-M7 = two pkg dirs (`frontend/src/wasm/` seq + `frontend/src/wasm-threaded/` or equivalent).

### D6 — Worker OOM handling reframed (W4 correction)

The draft's OOM tagging (parse `WebAssembly.RuntimeError`, prefix `"Out of memory..."`) is **removed**. W4 [CONSENSUS]: an in-execution OOM cannot be reliably distinguished from other traps by inspecting the error — same `WebAssembly.RuntimeError` constructor, engine-defined non-standard message, and the engine may hard-crash with no JS exception at all. Message-substring classification is a false-confidence silent-failure surface (agentic-workflow-principles #6).

OOM is handled by two mechanisms this domain **consumes**, not invents:

1. **Budget-prevention (primary).** The thread-cap (`MEMORY_THREAD_CAP`, passed to `initThreadPool`) and the `--max-memory` link arg (engine-build) bound peak shared-linear-memory so OOM is prevented, not caught (INV-05). This domain's contribution is passing the cap unchanged (D2).
2. **Structured engine self-report (secondary).** When the Rust engine detects its own allocation pressure at a checked boundary, it returns a structured error value (e.g. `{ error: "OOM", ... }`) rather than trapping. That value arrives at the worker as a normal thrown JS `Error` (from the wasm-bindgen `Result` unwrap) or a structured payload; the worker's catch surfaces it verbatim through the existing `status: 'ERROR'` envelope (`analysis.worker.js:55-60`) with the `mode` stamp — no message parsing, no `instanceof WebAssembly.RuntimeError` branch.

The worker's catch therefore stays a generic pass-through (D3 error post). It never classifies the error type; whatever message the engine self-report or the runtime provides flows through unchanged to `AnalysisWorkerService.js:31`'s reject path — the console being the only diagnostic channel on the air-gapped box (`vite.config.js:15-22`). A hard-crash with no JS exception is caught by the worker-`error` restart path (`AnalysisWorkerService.js:34-43`), unchanged.

### D7 — E3 nested-worker spawn: top preflight + main-thread-relay fallback

W2 confirms the **isolation/SAB permission** question ([CONSENSUS]: nested workers inherit `crossOriginIsolated`), but theoretical isolation ≠ proof the nested pool actually **spawns** from inside a dedicated worker. **E3 is the top /build Phase 0 preflight experiment**: serve a COOP/COEP page from a ~30-line static server, load `analysis.worker.js`, and assert (a) `self.crossOriginIsolated === true` in the worker and (b) `await initThreadPool(cap)` resolves and a `par_iter`-backed compute call returns with `threadCount > 1`.

- **If E3 passes** (expected): `initThreadPool` stays in `analysis.worker.js` (D2), zero API-layer change.
- **If E3 fails** (nested spawn unsupported): fall back to `initThreadPool` on the **main thread** + a relay that forwards `{id,type,payload}` to/from the compute worker. The postMessage contract to the Meridian app stays byte-identical (Charter Anti-Goal); only an internal main-thread pool + relay is added. The relay does not change any message shape the app sees.

E3 is a preflight because it decides the init call site before the worker code is finalized; it does not block the header/config work (M4–M6), which are correct under either outcome.

---

## Build Steps (ordered, buildable)

> Precondition (cross-spec build order — do not reorder): the **determinism** (INV-02) and **memory-profile** (INV-05) stages are complete, so `N_max_const` (the value written into `thread-cap.js` at M7) exists, and the toolchain-build `--target web` threaded pkg is available to copy. This domain is the **threading-wiring** stage (and writes `thread-cap.js` in M7); **validation** follows.

1. **E3 preflight (do first).** Run the nested-worker spawn experiment (D7) against a COOP/COEP static server + Playwright headless Chromium. Record pass/fail in the buildability-gate record. On fail, switch M-steps to the main-thread-relay layout (D7 fallback) before proceeding.
2. **audit-forge headers (M6).** Add the two COOP/COEP lines inside `_apply_meridian_csp` at `webui/__init__.py:130`, under the existing predicate (`:129`), before `return response` (`:131`). No other blueprint touched (INV-06).
3. **Vite config (M4, M5).** In `vite.config.js`: remove the `vite-plugin-wasm` import (`:3`) and its uses (`:11`, `:13`); set `worker: { format: 'es' }`; add `server.headers` COOP/COEP; keep `base`, `esbuild`, `test` unchanged (D4).
4. **pkg refresh (M7).** Copy the toolchain-build `scripts/build-wasm.sh` `--target web` output (`pay_equity_engine.js`, `pay_equity_engine_bg.wasm`, rayon `workerHelpers` glue) into `frontend/src/wasm/`. **Then write `frontend/src/wasm/thread-cap.js`** with `export const MEMORY_THREAD_CAP = <N_max_const>;` where `<N_max_const>` is the integer from the memory-budget domain's D3 (available once the memory profile lands). `pkg/` stays gitignored; `thread-cap.js` + the copied `*.js`/`*.wasm` are committed under `frontend/src/wasm/`.
5. **Worker init (M1, M2, M3, D2).** Rewrite `analysis.worker.js:1-24`: add the `initThreadPool` + `MEMORY_THREAD_CAP` imports, replace `initialize()` with the feature-detect + `await init()` → `await initThreadPool(cap)` + `mode` recording + additive `INIT_RESULT` post. Preserve the `isInitialized` latch and the unchanged `onmessage` dispatch (`:26-51`).
6. **Mode stamp + OOM pass-through (M3, M8, D3, D6).** Add the trailing `mode` field to the success post (`analysis.worker.js:53`) and the error post (`:55-60`). Ensure the catch is a generic pass-through — remove any `WebAssembly.RuntimeError`/OOM-prefix classification.
7. **Service capture (M3, D3).** In `AnalysisWorkerService.js`: add the `INIT_RESULT` branch before `if (!job) return` (`:26`) and a read-only `getComputeMode()`. Do not change `run()`/`terminate()`/`restart()` (`:46-70`).
8. **Validation handoff.** Emit the served-preview COOP/COEP + `crossOriginIsolated` checks (AC-M6, AC-M4) into the verification-benchmark suite (custom COOP/COEP server + Playwright per W7 — not `wasm-pack test`).

---

## Acceptance Criteria (objectively checkable)

**Worker init (M1, M2)**

- **AC-M1.1** — In a cross-origin-isolated served context, a Playwright assertion reads `INIT_RESULT.payload.mode === 'threaded'` and `INIT_RESULT.payload.threads === Math.min(navigator.hardwareConcurrency||1, MEMORY_THREAD_CAP)`. *(Playwright test, exact-value assert)*
- **AC-M1.2** — In a non-isolated context (headers stripped), the same assertion reads `mode === 'sequential'` and `threads === 1`, and `initThreadPool` is never referenced (verified: no `initThreadPool` call on the non-isolated path — grep `analysis.worker.js` shows the call only inside the `if (self.crossOriginIsolated === true)` block). *(Playwright test + static grep)*
- **AC-M1.3** — `initialize()` is idempotent: a second inbound message does not re-run `init()`/`initThreadPool` (assert `init` call count === 1 after two `DECOMPOSE` messages). *(Playwright/unit spy)*
- **AC-M2.1 (INV-03)** — A non-isolated context returns a correct `DECOMPOSE` result (SUCCESS payload deep-equals the sequential-mode golden) with no thrown error, no worker `error` event, and no blank screen. *(Playwright test: result compare + zero error events)*

**Contract additivity (M3, Anti-Goal)**

- **AC-M3.1** — `git diff analysis.worker.js` shows the inbound destructure at `:27` unchanged (`const { id, type, payload } = e.data`). *(byte diff of that line)*
- **AC-M3.2** — Every outbound message carries a top-level `mode`; no existing message type is removed or renamed; the only new type is `INIT_RESULT`. *(schema diff test enumerating message shapes)*
- **AC-M3.3** — `AnalysisWorkerService.js` `run()`, `terminate()`, `restart()` signatures at `:47`, `:56`, `:66` are unchanged (byte diff); a new `getComputeMode()` is present. *(byte diff + export presence check)*
- **AC-M3.4** — With the worker emitting `INIT_RESULT` (`id:null`) + trailing `mode`, the existing consumer resolves/rejects jobs correctly (existing `AnalysisWorkerService` unit suite passes unmodified except the additive `INIT_RESULT`/`getComputeMode` tests). *(`pnpm test` exit 0)*

**Vite config (M4, M5)**

- **AC-M4.1** — `vite.config.js` contains `worker: { format: 'es' }` and does NOT import or reference `vite-plugin-wasm`. *(grep: `format: 'es'` present; `vite-plugin-wasm` absent)*
- **AC-M4.2** — `vite.config.js` `server.headers` sets `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`, byte-equal to the audit-forge values (D1). *(string compare across both files)*
- **AC-M4.3** — `pnpm build` exits 0 and the built worker chunk is emitted as an ES module (the rayon `workerHelpers` chunk is present in `dist/` and references the hashed `*_bg.wasm` via an emitted asset URL, not a raw string). *(build exit code + `dist/` asset grep)*
- **AC-M4.4** — Serving `dist/` behind the COOP/COEP static server, a Playwright check reads `window.crossOriginIsolated === true` on the `/pay-equity/` document. *(Playwright)*

**audit-forge headers (M6, INV-06)**

- **AC-M6.1** — A request to `/pay-equity/` and to `/api/` returns `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`. *(HTTP header assert)*
- **AC-M6.2** — A request to `/` and to `/guide` (`webui/__init__.py:117-123`) returns neither COOP nor COEP (INV-06). *(HTTP header absence assert)*
- **AC-M6.3** — The change is additive: `_apply_meridian_csp` still returns `response`, the `_MERIDIAN_CSP` assignment line (`webui/__init__.py:130`) is unchanged, and `_apply_plugin_csp` (`:138-152`) is untouched. *(git diff scoped to the two added lines)*

**OOM handling (M8, W4)**

- **AC-M8.1** — `analysis.worker.js` contains no `instanceof WebAssembly.RuntimeError` check and no OOM message-substring/prefix classification. *(grep returns zero matches)*
- **AC-M8.2** — When the engine returns its structured allocation-pressure self-report, the worker surfaces it verbatim through `{ id, type, status: 'ERROR', payload: { message }, mode }` with the `id` of the failing job, and the worker remains able to serve the next (smaller) job (a subsequent `DECOMPOSE` succeeds). *(Playwright test with a forced-pressure fixture from verification-benchmark)*

**E3 preflight (M9)**

- **AC-M9.1** — The E3 preflight has a recorded pass/fail in the buildability-gate record; on pass, `initThreadPool` is called inside `analysis.worker.js`; on fail, the main-thread-relay layout is in place and a `par_iter`-backed compute call still returns with `threadCount > 1`. *(gate record + Playwright threadCount assert)*
- **AC-M9.2** — Under either E3 outcome, the app-visible postMessage contract is byte-identical (the `AnalysisWorkerService` inbound `w.postMessage({ id, type, payload })` at `:53` and the resolve/reject shape are unchanged). *(byte diff + contract test)*

---

## Open Items (route to buildability gate)

1. **Strategy A — RATIFIED (2026-07-18, ruling 1).** Dual artifact; worker uses conditional dynamic `import()` (D5); AC-M7 = two pkg dirs. No longer open. *(Charter In-Scope 2)*
2. **E3 preflight outcome.** The nested-worker `initThreadPool` spawn result (D7) decides whether the init call site stays in-worker or moves to a main-thread relay. Preflight is the first build step; outcome recorded at the gate. No API-shape risk either way (AC-M9.2).

No `[CLARIFY]` markers remain. Both open items are decisions with fully-specified branches, not missing research.

---

## Sources

**Phase 3 findings (this pipeline):**
- `phase3-web-w1-w2-w3-w4-w5-w6-w7-w8-w9-w10-findings.md` — W1 (`initThreadPool` re-export/JS signature), W2 (worker isolation inheritance; page-level headers sufficient — locked), W3 (Vite `worker.format:'es'` required, omit `vite-plugin-wasm`, `new URL`/bundler mode — locked), W4 (OOM taxonomy; do-not-parse-RuntimeError correction), W7 (threaded browser CI harness ≠ `wasm-pack test`)
- `phase3-local-l1-l2-l3-l4-l5-l6-findings.md` — L4 (polars wasm `POOL` stub delegates to global rayon; sequential-fallback degrades safely)
- `phase2-spec-meridian-integration.md` — this domain's Phase-2 draft (starting point)
- `spec-charter.md` — In-Scope 6, 7, 8, 12; INV-02, INV-03, INV-05, INV-06; ASM-05; Anti-Goals (postMessage API, local-only host)
- `phase4-writer-brief.md` — six Phase-3 corrections, locked decisions, output rules

**Code anchors verified (Read, this session):**
- `apps/hr-apps/pay-equity-app/frontend/src/wasm/analysis.worker.js:1-8,10,12-24,26-27,29-30,53,54-61`
- `apps/hr-apps/pay-equity-app/frontend/src/services/AnalysisWorkerService.js:1,23-24,26,28-32,34-43,46-70`
- `apps/hr-apps/pay-equity-app/frontend/vite.config.js:1-4,10,11,12-14,15-22,23-44`
- `/home/deji/telos/audit-forge/webui/__init__.py:117-123,125-131,138-152`

**External URLs (Perplexity-returned, via Phase 3):**
- W2: https://developer.mozilla.org/en-US/docs/Web/API/WorkerGlobalScope/crossOriginIsolated , https://web.dev/articles/coop-coep
- W3: https://vite.dev/config/worker-options , https://github.com/vitejs/vite/issues/18585 , https://github.com/RReverser/wasm-bindgen-rayon
- W4: https://www.w3.org/TR/wasm-js-api-2/ , https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/RuntimeError
- W1: https://github.com/GoogleChromeLabs/wasm-bindgen-rayon/blob/main/src/lib.rs
</content>
</invoke>
