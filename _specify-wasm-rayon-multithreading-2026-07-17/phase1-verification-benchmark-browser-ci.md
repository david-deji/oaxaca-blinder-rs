# Phase 1 — Verification: Browser Benchmarking, Memory Measurement, CI

> Unit: u10 | Domain: verification-benchmark | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

performance.now() (microsecond-capable under COI) is the timer; measureUserAgentSpecificMemory()
(Chromium-only, requires COI) gives page-level memory; WebAssembly.Memory.buffer.byteLength tracks
linear-memory size — but for SHARED memory byteLength reads the full maximum (per u3), so the
in-engine allocator stats or logical page count must be the fine-grained metric. CI: the hard
requirement is a test server that sets COOP/COEP; wasm-pack test / wasm-bindgen-test-runner have
no verified COI header support — the pragmatic stack is a tiny header-setting server + Playwright
(or vitest+playwright) driving headless Chromium. Node worker_threads can smoke-test threading
logic without COI but --target web output under Node is not guaranteed — treat as supplement only.
Perf assertions in CI: relative speedup thresholds (e.g. >1.5× on 4 cores), median-of-N with
warmup, never absolute times.

## Findings

1. [CONSENSUS] Timing: performance.now(), monotonic, up-to-microsecond under COI; workloads sized so timer noise is negligible; measure inside the worker (less event-loop noise). (MDN performance.now; wasmhub perf tips)
2. [CONSENSUS] Memory: measureUserAgentSpecificMemory() Chromium-only + COI-gated (SecurityError otherwise) for page-level; linear-memory tracking via logical wasm page count (shared memory byteLength = maximum, per u3 finding 6); consider exposing an engine-side `memory_stats()` (allocated pages, peak via allocator hook) for precise budget verification. (web.dev monitor-total-page-memory; WICG performance-measure-memory; cross-ref u3)
3. [SINGLE_SOURCE + gap] CI runners: no verified COOP/COEP support in wasm-pack test/wasm-bindgen-test-runner for atomics builds — Phase 3: confirm wasm-bindgen-test-runner status (issue tracker) OR specify the custom path: tiny static server with headers (Python http.server subclass or node) + Playwright headless Chromium. Meridian already has vitest; audit-forge test suite is pytest — the browser-threaded job is new surface either way. (web.dev coop-coep + inference)
4. [REPORTED] Node shortcut: worker_threads + SAB works without COI; --target web glue under Node not guaranteed (browser globals) — classify as optional smoke, not the gate. The authoritative gate is one real headless-Chromium job with headers. (inference from wasm-bindgen docs orientation)
5. [CONSENSUS] Perf assertion design: relative not absolute; warmup 3-5 runs; median (and p95) of ≥10 iterations; assert threaded/sequential wall-clock ratio > threshold (1.5× on 4 cores is the sane floor); GitHub Actions standard runners are 4-core (ubuntu-latest) — adequate for a 1.5× gate, document runner-class dependency. (wasmhub perf tips; CI practice)
6. [CONSENSUS] Bit-identical parity job is platform-independent and cheap: run the same seeded decompose in (a) native, (b) wasm sequential, (c) wasm threaded N=2/4 → byte-compare canonical JSON. This is the SC-03 gate and belongs in CI on every engine PR; the perf job can be scheduled/manual (flaky-sensitive). (design synthesis from u5)

## Spec Implications

- CI additions: (1) parity job (blocking) — native vs wasm-seq vs wasm-threaded byte-compare; (2) memory-ceiling job at 50k rows (blocking, asserts peak < declared budget); (3) speedup job (non-blocking/scheduled, median-of-10, >1.5× on 4-core). Reproducibility double-build job per u1.
- Benchmark harness lives in the engine repo (headless, Playwright-driven page served with headers) — reusable locally by David to measure on the corp PC.
- Float caveat for parity: per u5, any parallel reduction must be avoided in engine statistics; the parity byte-compare test is exactly what enforces this.

## Sources

- https://developer.mozilla.org/en-US/docs/Web/API/Performance/now
- https://web.dev/articles/coop-coep
- https://web.dev/articles/monitor-total-page-memory-usage
- https://github.com/WICG/performance-measure-memory
- https://wasmhub.dev/blog/webassembly-performance-tips
- https://rustwasm.github.io/docs/wasm-bindgen/examples/performance.html

## Research Inventory

- Perplexity ask 2026-07-17 (citations above)
- Session recon 2026-07-17: .github/workflows/ci.yml wasm-verify job; cross-refs to phase1-memory-shared-memory-limits.md (u3) and phase1-rng-parallel-seeding.md (u5)
