# Phase 1 — Meridian: wasm-bindgen-rayon + Vite + Worker Integration

> Unit: u8 | Domain: meridian-integration | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

`initThreadPool(n)` is an async export of the bindgen glue (enabled by wasm-bindgen-rayon) that
must be awaited after `init()` and before any parallel call. The documented model runs the
wasm module + pool inside a dedicated Worker (matching Meridian's existing architecture) — BUT the
docs phrase initialization as "right after instantiating your module on the main thread," and
**nested-worker spawning (pool workers spawned from within our dedicated worker) has no published
compatibility guarantee** — this is the highest-priority Phase 3 verification (browser reality:
nested Workers are standard-supported; wasm-bindgen-rayon's helper specifically needs testing).
Failure mode without crossOriginIsolated is not documented as a clean rejection — the guard must
be `self.crossOriginIsolated === true` BEFORE deciding to load/init the threaded path, not
try/catch around initThreadPool. COOP/COEP: top-level document headers are the documented
requirement; worker-script header needs and Flask static-serving specifics are unsourced — Phase 3.

## Findings

1. [CONSENSUS] API: `await init(); await initThreadPool(navigator.hardwareConcurrency);` — async, must complete before first par_iter-backed call. Lazy/conditional init is the endorsed progressive-enhancement pattern (feature-detect first, only then load threaded build + init pool). (docs.rs wasm-bindgen-rayon; web.dev wasm-threads)
2. [CONSENSUS] Supported deployment model: run wasm + pool in a dedicated Worker; main thread must not be blocked (GoogleChromeLabs/wasm-bindgen-rayon#22 — "must do all the work in a dedicated Worker"). Meridian's analysis.worker.js already matches this shape. (github issue #22; web.dev)
3. [UNVERIFIED → Phase 3, top priority] Nested-worker spawning: pool workers created from INSIDE our dedicated worker — no published compatibility matrix. Verify by minimal local experiment during spec (cheap: serve a COI test page) or primary-source dig into workerHelpers.js implementation. Blocks: whether initThreadPool call site is analysis.worker.js (preferred, no API change) or main thread (would change Meridian's layering).
4. [SINGLE_SOURCE] Feature-detect: guard on `self.crossOriginIsolated === true` before selecting the threaded path (webassembly-wasm.com guide) — do NOT rely on initThreadPool rejecting cleanly (undocumented). Optional wasm-feature-detect lib; hand-rolled check preferred for offline bundle (zero new deps).
5. [REPORTED] Vite: bundler/no-bundler modes exist; Vite-specific production config (worker.format 'es', asset URL survival for workerHelpers) is undocumented in primary sources — Phase 3: pull working Vite configs from community examples (github tlsnotary, rollup-plugin examples) or specify a verification build step.
6. [SINGLE_SOURCE + gap] COOP/COEP scope: top-level document must send the pair; whether pool worker scripts need explicit headers is unsourced (browser spec: dedicated workers inherit COI from owner context — verify in Phase 3 with MDN primary source). Flask: header injection point exists (audit-forge `_apply_meridian_csp` after_request, session-verified) — mechanism is per-response headers which that hook already demonstrates.

## Spec Implications

- Meridian wiring design (draft): analysis.worker.js `initialize()` becomes: `await init(); if (self.crossOriginIsolated) { try { await initThreadPool(cap); mode='threaded' } catch { mode='sequential' } } else { mode='sequential' }`; post an INIT_RESULT message including mode so the UI can display compute mode (defensibility metadata: record mode in run outputs).
- Thread cap: `min(navigator.hardwareConcurrency, memoryCap)` per INV-05 formula.
- audit-forge: add COOP `same-origin` + COEP `require-corp` on `/pay-equity/` responses in the existing after_request region (webui/__init__.py:125-131); no-egress design makes require-corp safe (no cross-origin subresources).
- Vite dev: `server.headers` with the same pair for local dev parity.

## Sources

- https://docs.rs/wasm-bindgen-rayon
- https://docs.rs/crate/wasm-bindgen-rayon/latest/source/README.md
- https://github.com/GoogleChromeLabs/wasm-bindgen-rayon/issues/22
- https://web.dev/articles/webassembly-threads
- https://www.webassembly-wasm.com/js-wasm-interop-memory-management/sharedarraybuffer-atomics-threading/
- https://github.com/rustwasm/wasm-bindgen/discussions/3769

## Research Inventory

- Perplexity ask 2026-07-17 (citations above)
- Session recon 2026-07-17: analysis.worker.js (initialize guard), vite.config.js (worker.plugins), audit-forge webui/__init__.py:117-159
