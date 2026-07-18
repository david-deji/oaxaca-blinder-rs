# Phase 2 Spec — Meridian Integration (worker init, COOP/COEP, pkg refresh, fallback UX)

> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (meridian-integration) — Draft

---

## 1. Executive Summary

This domain wires the threaded `pay_equity_engine` WASM build into Meridian's client-side runtime without changing the worker's postMessage contract. Five deliverables:

1. **`analysis.worker.js` `initialize()` redesign** — a `self.crossOriginIsolated === true` guard selects the threaded path, awaits `init()` then `initThreadPool(cap)`, records a `mode` (`'threaded' | 'sequential'`), and surfaces it via an additive `INIT_RESULT` message plus a `mode` field on every result message. The existing `{ id, type, status, payload }` shape is untouched (Source: `frontend/src/wasm/analysis.worker.js:26-61`).
2. **COOP/COEP headers** — two `response.headers[...]` lines added inside the existing `_apply_meridian_csp` `if` block, scoped to `/pay-equity/` + `/api/` only (Source: `/home/deji/telos/audit-forge/webui/__init__.py:125-131`; INV-06), plus `server.headers` in `vite.config.js` for dev parity (Source: `frontend/vite.config.js:7-11`).
3. **pkg artifact refresh flow** into `frontend/src/wasm/` — single vs dual pkg-dir layout, resolved by the strategy decision owned by the engine-build domain (cross-domain summary line 12).
4. **Fallback UX contract (INV-03)** — sequential mode works silently; `mode` is optional run metadata the UI may display, never a blocking or error surface.
5. **Worker-level `RuntimeError`/OOM catch** — an OOM during a compute call is surfaced through the existing `status: 'ERROR'` message shape (Source: `analysis.worker.js:54-61`), not a worker crash.

The **nested-worker question** — whether `initThreadPool` can spawn its pool workers from *inside* the dedicated worker — is carried as a Phase 3 risk with a designed fallback (initThreadPool on the main thread + a relay), not resolved here (phase1 finding #3).

**What this domain does NOT own:** the threaded build toolchain, RNG determinism, memory/thread-cap math, and the pkg *contents* (engine-build + engine-parallelization domains). This spec consumes their outputs (`init_thread_pool` export, the `cap` formula, the pkg dir(s)) via the cross-domain summary and does not re-spec them.

---

## 2. Requirements (with acceptance criteria)

### R1 — crossOriginIsolated-guarded threaded init

`initialize()` selects the threaded path only when `self.crossOriginIsolated === true`; otherwise it runs sequential init. The guard is a positive feature-detect *before* choosing the path — not a try/catch around `initThreadPool` (phase1 finding #4: undocumented rejection behavior).

- **AC-R1.1**: In a cross-origin-isolated context, `initialize()` calls `await init()`, then `await initThreadPool(cap)`, sets `mode = 'threaded'`, and completes before the first compute dispatch.
- **AC-R1.2**: In a non-isolated context, `initialize()` calls `await init()` only, sets `mode = 'sequential'`, and never references `initThreadPool`.
- **AC-R1.3**: `initialize()` remains idempotent — the existing `isInitialized` latch (`analysis.worker.js:10,13,17`) still guards against re-init on subsequent messages.
- **AC-R1.4**: `cap` is supplied by the memory/thread-cap formula owned by the engine-parallelization domain (`min(hardwareConcurrency, memoryCap, 8)`, cross-domain summary line 13). This domain passes the value; it does not compute it.

### R2 — mode surfaced via additive messages only

The worker emits an `INIT_RESULT` message after init and stamps a `mode` field on every SUCCESS/ERROR message. Both are additive; the existing `{ id, type, status, payload }` destructure keeps working (Source: `AnalysisWorkerService.js:24`).

- **AC-R2.1**: After a successful `initialize()`, the worker posts `{ id: null, type: 'INIT_RESULT', status: 'SUCCESS', payload: { mode, threads } }` where `threads` is `cap` in threaded mode and `1` in sequential mode.
- **AC-R2.2**: Every existing result message gains a top-level `mode` field: `self.postMessage({ id, type, status, payload, mode })`. Consumers that ignore unknown fields are unaffected (`AnalysisWorkerService.js:24` destructures only `{ id, status, payload }`).
- **AC-R2.3**: `AnalysisWorkerService.js` additively captures `INIT_RESULT` (an `id === null` message not in `pending`, currently dropped at `AnalysisWorkerService.js:26 if (!job) return`) and exposes `computeMode` for the dashboard store. No existing `run()`/`terminate()`/`restart()` signature changes.
- **AC-R2.4**: No message removed or renamed; no field removed from any existing message. Verified by diffing the message shapes against `analysis.worker.js:53,55-60`.

### R3 — COOP/COEP on audit-forge, scoped (INV-06)

Two headers added inside the existing scoped `if` block; the 9 pre-existing blueprints stay untouched.

- **AC-R3.1**: `COOP: same-origin` and `COEP: require-corp` are set on responses where `request.path.startswith("/pay-equity/") or request.path.startswith("/api/")` — the same predicate already guarding `_MERIDIAN_CSP` (`webui/__init__.py:129`).
- **AC-R3.2**: No response outside those two prefixes gains either header (INV-06). Verified by a request to `/` and `/guide` (`webui/__init__.py:117-123`) showing neither header present.
- **AC-R3.3**: The change is additive — `_apply_meridian_csp` still returns `response`, the `_MERIDIAN_CSP` line is unchanged, and the plugin-CSP `after_request` (`webui/__init__.py:138-152`) is untouched.
- **AC-R3.4**: `require-corp` is safe because `_MERIDIAN_CSP` already enforces `connect-src 'self'` / `default-src 'self'` (no cross-origin subresources) (`webui/__init__.py:20-23`; phase1 spec-implication line 32).

### R4 — Vite dev-server COOP/COEP parity

- **AC-R4.1**: `vite.config.js` `server.headers` sets `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`, so `pnpm dev` reproduces the isolated context locally (Source: `vite.config.js:7`, charter In-Scope 7).
- **AC-R4.2**: The dev headers match the audit-forge production headers byte-for-byte (same two values), so `mode` is `'threaded'` in both dev and the offline bundle.

### R5 — pkg artifact refresh flow

The threaded `--target web` bindgen output(s) land in `frontend/src/wasm/`; the loader branch resolves which to load. Layout is strategy-dependent (cross-domain summary line 12).

- **AC-R5.1** (dual-artifact strategy): two pkg dirs — the existing sequential `pay_equity_engine.js` (untouched baseline) and a sibling threaded dir; `analysis.worker.js` imports the threaded glue only inside the `crossOriginIsolated` branch (conditional `import()`), the sequential glue otherwise.
- **AC-R5.2** (single-artifact strategy): one threads-capable pkg replacing `frontend/src/wasm/pay_equity_engine.js`; `initThreadPool` is simply never called in sequential mode (rayon falls back to sequential when the pool is uninitialized, cross-domain summary line 12).
- **AC-R5.3**: The refresh flow is documented as build-step commands (which pkg dir(s) the build-wasm.sh output copies into `frontend/src/wasm/`), consuming the engine-build domain's `scripts/build-wasm.sh` output; this domain does not author the build script.
- **AC-R5.4**: `pkg/` stays gitignored per existing convention (charter Anti-Goal); only the committed `frontend/src/wasm/` copies are refreshed.

### R6 — Fallback UX contract (INV-03)

- **AC-R6.1**: A non-isolated context produces working results — sequential mode never crashes, never blanks the screen (INV-03; `build-safety.md` error-handling default).
- **AC-R6.2**: `mode` is optional run metadata. The UI MAY display "compute mode: threaded/sequential" as run metadata (defensibility record), but no UI element is *required* and no error/toast fires on sequential mode.
- **AC-R6.3**: The mode value is available for inclusion in run outputs (defensibility metadata, cross-domain summary line 16) via the `INIT_RESULT` payload and the per-result `mode` field.

### R7 — Worker-level OOM/RuntimeError catch

- **AC-R7.1**: A WASM `RuntimeError` (OOM / memory.grow failure) thrown from any of the 5 compute calls is caught by the existing `try/catch` (`analysis.worker.js:29,54`) and surfaced as `{ id, type, status: 'ERROR', payload: { message }, mode }`.
- **AC-R7.2**: The error message includes an OOM-identifying hint when the caught error is a `RuntimeError` (e.g. prefix `"Out of memory (WASM RuntimeError): "`) so the dashboard store's reject path (`AnalysisWorkerService.js:31`) produces a diagnosable message on the air-gapped box (`vite.config.js:15-21` — console is the only diagnostic channel).
- **AC-R7.3**: An OOM does not kill the worker silently — the catch posts the ERROR message for the specific `id`, leaving the worker able to serve the next (smaller) job; it does not rely on the `AnalysisWorkerService.js:34-43` worker-`error` restart path.

---

## 3. Technical Architecture

```
audit-forge Flask  ──COOP/COEP + CSP on /pay-equity/,/api/──►  browser (crossOriginIsolated=true)
        │  (webui/__init__.py:125-131, additive)                        │
        ▼                                                               ▼
Vite build (base /pay-equity/)  ──serves──►  Meridian Vue app  ──owns worker──►  dashboard.store.js
   server.headers parity (dev)                                                        │
                                                                                       ▼
                                              AnalysisWorkerService.js  ──postMessage──►  analysis.worker.js
                                              (captures INIT_RESULT, mode)                    │
                                                                                              ▼
                                              init() → [crossOriginIsolated?] → initThreadPool(cap)
                                                                                              │
                                                                                              ▼
                                                                          pay_equity_engine.wasm (threaded | sequential)
```

**Isolation chain**: audit-forge sends COOP `same-origin` + COEP `require-corp` on the two Meridian prefixes → the top-level document becomes `crossOriginIsolated` → `SharedArrayBuffer` is available → `self.crossOriginIsolated` reads `true` inside the dedicated worker (workers inherit isolation from the owner context — Phase 3 verification, phase1 finding #6).

**Message-contract invariant**: the worker's inbound contract (`{ id, type, payload }`, `analysis.worker.js:27`) is unchanged. Outbound gains one new message type (`INIT_RESULT`) and one new top-level field (`mode`) — both additive; the consumer (`AnalysisWorkerService.js:23-33`) is forward-compatible because it destructures only known fields and drops unknown `id`s.

**Strategy dependency**: the pkg layout (R5) and whether `initThreadPool` is conditionally imported branch on the engine-build domain's strategy decision. This spec provides both branches; the engine-build spec picks one at the buildability gate.

---

## 4. Implementation Details

### 4.1 `analysis.worker.js` `initialize()` redesign

Replaces `analysis.worker.js:12-24`. Imports (`analysis.worker.js:1-8`) stay; `initThreadPool` import is conditional per strategy (R5).

```js
let isInitialized = false
let computeMode = 'sequential'   // 'threaded' | 'sequential'
let threadCount = 1

async function initialize() {
  if (isInitialized) return
  await init()
  init_panic_hook()

  // Positive feature-detect BEFORE choosing the threaded path (phase1 finding #4).
  if (self.crossOriginIsolated === true) {
    try {
      // cap supplied by the engine memory/thread-cap formula (cross-domain summary L13)
      const cap = Math.min(navigator.hardwareConcurrency || 1, MEMORY_THREAD_CAP)
      await initThreadPool(cap)      // awaited before any par_iter-backed call (phase1 finding #1)
      computeMode = 'threaded'
      threadCount = cap
    } catch (e) {
      // Undocumented failure => degrade, never crash (INV-03). Pool stays uninitialized;
      // rayon runs sequential.
      console.warn('initThreadPool failed; falling back to sequential:', e)
      computeMode = 'sequential'
      threadCount = 1
    }
  }

  isInitialized = true
  // Additive: unsolicited INIT_RESULT (id:null). Existing consumer drops unknown ids
  // (AnalysisWorkerService.js:26); the additive handler (4.4) captures it.
  self.postMessage({
    id: null, type: 'INIT_RESULT', status: 'SUCCESS',
    payload: { mode: computeMode, threads: threadCount }, mode: computeMode,
  })
}
```

- `MEMORY_THREAD_CAP` is a constant this domain receives from the engine-parallelization spec (cross-domain summary line 13); it is not derived here.
- Under the dual-artifact strategy (R5/AC-R5.1) the `init` / `initThreadPool` imports resolve from the threaded pkg via a conditional `import()` inside the guard; under single-artifact they resolve from the one pkg at module top.

### 4.2 Per-result `mode` stamp

`analysis.worker.js:53` and `:55-60` gain a trailing `mode` field:

```js
self.postMessage({ id, type, status: 'SUCCESS', payload: result, mode: computeMode })
// ...and in the catch:
self.postMessage({ id, type, status: 'ERROR', payload: { message: msg }, mode: computeMode })
```

### 4.3 OOM/RuntimeError catch (R7)

The existing `catch (error)` at `analysis.worker.js:54` is refined to tag WASM `RuntimeError`:

```js
} catch (error) {
  const isOom = error instanceof WebAssembly.RuntimeError
  const message = isOom
    ? `Out of memory (WASM RuntimeError): ${error.message || String(error)}`
    : (error.message || String(error))
  self.postMessage({ id, type, status: 'ERROR', payload: { message }, mode: computeMode })
}
```

No new message shape — same `status: 'ERROR'` envelope the consumer already rejects on (`AnalysisWorkerService.js:31`).

### 4.4 `AnalysisWorkerService.js` additive capture

The message listener (`AnalysisWorkerService.js:23-33`) gains an `INIT_RESULT` branch *before* the `if (!job) return` drop:

```js
worker.addEventListener('message', (e) => {
  const { id, type, status, payload, mode } = e.data
  if (type === 'INIT_RESULT') {            // additive: capture compute mode
    computeMode = payload?.mode ?? 'sequential'
    return
  }
  const job = pending.get(id)
  if (!job) return
  // ...unchanged...
})
```

`workerService` gains a read-only `getComputeMode()` (or a `computeMode` getter). `run()`, `terminate()`, `restart()` signatures unchanged (AC-R2.3). The dashboard store (`dashboard.store.js:22,500,533` worker-lifecycle owner) may read it for run metadata (R6) — that surfacing is a Meridian-frontend UI task, out of this engine-spec's build blast surface (charter Scope-Delta note on compiler.js/frontend deferral).

### 4.5 audit-forge COOP/COEP (additive, scoped)

Inside `_apply_meridian_csp` (`webui/__init__.py:125-131`), the two headers join the existing CSP line under the same predicate:

```python
    @app.after_request
    def _apply_meridian_csp(response):
        # Scoped to the Meridian integration surface only — the 9 pre-existing blueprints'
        # HTML surfaces are untouched by this header (spec Sec.5).
        if request.path.startswith("/pay-equity/") or request.path.startswith("/api/"):
            response.headers["Content-Security-Policy"] = _MERIDIAN_CSP
            # Cross-origin isolation for wasm-bindgen-rayon SharedArrayBuffer threads.
            # require-corp is safe: _MERIDIAN_CSP already blocks cross-origin subresources
            # (connect-src/default-src 'self'). Scoped to the same two prefixes (INV-06).
            response.headers["Cross-Origin-Opener-Policy"] = "same-origin"
            response.headers["Cross-Origin-Embedder-Policy"] = "require-corp"
        return response
```

`worker-src 'self' blob:` already present in `_MERIDIAN_CSP` (`webui/__init__.py:21`) — covers the dedicated worker and any blob-URL pool workers wasm-bindgen-rayon may create.

### 4.6 Vite dev headers (R4)

`vite.config.js:7-11` gains a `server` block (and, if Phase 3 confirms it necessary for worker-format survival, `worker.format: 'es'`):

```js
export default defineConfig({
  base: '/pay-equity/',
  plugins: [vue(), wasm(), topLevelAwait()],
  worker: { plugins: () => [wasm()] },   // may add `format: 'es'` per Phase 3 (finding #5)
  server: {
    headers: {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  // ...esbuild / test blocks unchanged...
})
```

---

## 5. Dependencies and Integrations

| Dependency | Owner domain | What this spec consumes | Reference |
|---|---|---|---|
| `init_thread_pool` / `initThreadPool` export | engine-build | The awaitable pool-init export in the `--target web` pkg | cross-domain summary L11-12; charter files-to-modify `engine/src/lib.rs` |
| `MEMORY_THREAD_CAP` value + formula | engine-parallelization | The integer cap passed to `initThreadPool` | cross-domain summary L13 (`min(hwConcurrency, memCap, 8)`) |
| pkg dir(s) contents + strategy (single vs dual) | engine-build | Which pkg to import and from where | cross-domain summary L12 (buildability gate) |
| `scripts/build-wasm.sh` threaded output | engine-build | The command that copies pkg into `frontend/src/wasm/` | charter files-to-modify |
| `--target web` migration | engine-build | Required by threaded glue (current build is `--target bundler`) | cross-domain summary L11 |

**This domain provides downstream:** the `mode` run-metadata field (defensibility outputs, cross-domain summary L16) and the COOP/COEP header checklist a future host inherits (charter Anti-Goal — "a future host inherits the headers checklist").

**Unchanged by this domain:** worker postMessage *inbound* contract; Vue/Pinia layer (charter Anti-Goal); the 9 audit-forge blueprints (INV-06); the plugin-CSP `after_request` (`webui/__init__.py:138-152`).

---

## 6. Risk Assessment

| # | Risk | Severity | Fallback / mitigation |
|---|---|---|---|
| RK1 | **Nested-worker spawn** — `initThreadPool` spawns pool workers from *inside* the dedicated worker; no published compatibility guarantee (phase1 finding #3) | HIGH — decides whether `initThreadPool` stays in `analysis.worker.js` (no API change) or moves to the main thread (changes Meridian's layering) | **Phase 3 item — do not resolve here.** Plan: run a minimal local COI experiment (serve a COOP/COEP page, call `initThreadPool` from a dedicated worker). If it fails, fall back to `initThreadPool` on the **main thread** + a Comlink-style relay that forwards `{id,type,payload}` to/from the compute worker — the postMessage contract to the app stays byte-identical; only an internal relay is added. Preferred path (in-worker) is attempted first because it needs zero API change. |
| RK2 | Worker context does not inherit `crossOriginIsolated` from the owner document | MEDIUM | Spec-verified in Phase 3 (MDN primary source, phase1 finding #6). If not inherited, `mode` degrades to `'sequential'` cleanly (INV-03) — no crash; threading simply unavailable until fixed. |
| RK3 | Vite production build drops the worker-helpers asset URL under `--target web` | MEDIUM | Phase 3: pull a working Vite+wasm-bindgen-rayon config or add a verification build step (phase1 finding #5). `worker.format: 'es'` is the leading candidate. |
| RK4 | Managed corp browser disables `SharedArrayBuffer` despite isolation (policy) | LOW | ASM-05 + INV-03: fallback to sequential covers it regardless; no code change needed. |
| RK5 | `require-corp` breaks a currently-loaded cross-origin subresource on `/pay-equity/` | LOW | `_MERIDIAN_CSP` already forbids cross-origin subresources (`connect-src 'self'`, `webui/__init__.py:20-23`); no-egress design makes require-corp safe (AC-R3.4). |
| RK6 | `initThreadPool` rejects/hangs in an isolated context for an undocumented reason | LOW | try/catch in `initialize()` (4.1) degrades to sequential; the guard is positive-detect so the common non-isolated case never enters the try. |

---

## Gaps Requiring Deeper Research

> NEEDS RESEARCH: Can `initThreadPool` (wasm-bindgen-rayon 1.3.0) spawn its pool workers from inside a *dedicated* Web Worker in evergreen Chrome/Edge, or must it run on the main thread? (RK1 — the single highest-priority unknown; decides whether the init call site changes Meridian's layering.)

> NEEDS RESEARCH: Does a dedicated Web Worker read `self.crossOriginIsolated === true` when its owner document is cross-origin-isolated, per the HTML spec / MDN — i.e. is the guard reliable inside `analysis.worker.js`? (RK2)

> NEEDS RESEARCH: What exact Vite 5/6 config (`worker.format`, asset-URL handling, `vite-plugin-wasm` interaction) produces a working `--target web` wasm-bindgen-rayon build that survives `pnpm build`, given the current `worker: { plugins: () => [wasm()] }` at `vite.config.js:12-14`? (RK3)

> NEEDS RESEARCH: Under the dual-artifact strategy, does a conditional `import()` of the threaded glue inside the `crossOriginIsolated` branch tree-shake and bundle correctly under Vite, or does it need a static import guarded at runtime? (blocks R5/AC-R5.1 implementation shape)

> NEEDS RESEARCH: Does `WebAssembly.RuntimeError` reliably distinguish OOM/`memory.grow` failure from other traps in the target browsers, or is a message-substring check needed for AC-R7.2's OOM tagging?

---

## 8. Spark Notes

- **The contract is safe by construction.** The consumer (`AnalysisWorkerService.js:24`) destructures only `{ id, status, payload }` and drops unknown `id`s (`:26`). So an `id:null` `INIT_RESULT` and a trailing `mode` field are both invisible to unchanged code — additive-only holds without touching the inbound `{id,type,payload}` shape.
- **Guard is positive-detect, not try/catch-first.** phase1 finding #4: `initThreadPool` has no documented clean rejection on non-isolated contexts. So `self.crossOriginIsolated === true` decides the path; the try/catch is only a second-layer safety inside the isolated branch (RK6).
- **require-corp is free here.** The existing `_MERIDIAN_CSP` already blocks all cross-origin subresources, so COEP `require-corp` adds no new failure surface (AC-R3.4, RK5) — the no-egress offline bundle is what makes this trivially safe.
- **Nested workers stay Phase 3.** Do not let the build assume in-worker `initThreadPool` works — the fallback (main-thread pool + relay, contract-preserving) is designed but only used if the local experiment fails (RK1).
- **One header predicate, reused.** COOP/COEP ride the *same* `if request.path.startswith(...)` that already scopes `_MERIDIAN_CSP` (`webui/__init__.py:129`), so INV-06 scoping is inherited, not re-derived.
- **`mode` is defensibility metadata, not UX.** INV-03 means sequential is a silent success; the UI *may* show compute mode as run metadata but nothing requires or errors on it (R6).

### Research Inventory

- `phase1-meridian-integration-vite-worker.md` (findings #1-6, spec implications)
- `phase2-cross-domain-summary.md` (lines 11-16 toolchain/memory/wiring; L12 strategy gate)
- `spec-charter.md` (In-Scope 6,7; INV-03, INV-06, ASM-05; Anti-Goals)
- Code anchors read: `frontend/src/wasm/analysis.worker.js:1-62`, `frontend/src/services/AnalysisWorkerService.js:1-71`, `frontend/vite.config.js:1-45`, `/home/deji/telos/audit-forge/webui/__init__.py:17-23,100-160`, `apps/hr-apps/pay-equity-app/CLAUDE.md` (worker interaction conventions)
