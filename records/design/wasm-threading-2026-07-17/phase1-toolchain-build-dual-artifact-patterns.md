# Phase 1 — Toolchain: Dual-Artifact Ship Patterns + Bundler Target

> Unit: u2 | Domain: toolchain-build | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

The dual-artifact pattern (single-threaded + threaded wasm, selected at load time) is the
reference design documented by web.dev for Rust + wasm-bindgen-rayon, and is production practice
in Apryse WebViewer and Qt/Emscripten ecosystems. Cost: two bindgen output dirs, two build passes,
loader branching, doubled test surface. Critical constraint discovered: **threaded wasm-bindgen
code requires `--target web` (or no-modules) — `--target bundler` is NOT supported for threads.**
Our current build uses `--target bundler`, so the threaded artifact forces a target migration in
Meridian's loading path regardless of strategy chosen.

## Findings

1. [CONSENSUS] Reference dual-artifact design (web.dev "Using WebAssembly threads from C, C++ and Rust"): feature-detect via wasm-feature-detect (`threads`) + crossOriginIsolated; load threaded build in a module Worker; initThreadPool; Comlink optional for API exposure. (web.dev; wasm-bindgen-rayon README)
2. [CONSENSUS] `--target bundler` does not support threaded code — wasm-bindgen guide: "Currently it's required to use the --target no-modules or --target web flag ... to run threaded code." Meridian currently consumes `--target bundler` output via vite-plugin-wasm. Migration of the worker's import path to `--target web` glue (manual init(url) call) is required for the threaded artifact. (rustwasm guide; wasm-bindgen discussion #3769)
3. [CONSENSUS] One glue cannot drive both blobs (glue is generated per-module; imports/exports differ between atomics and non-atomics builds). Dual-artifact = two wasm-bindgen runs → `pkg/` and `pkg-threaded/` (or similar). Sharing one wasm across multiple glue targets is a hand-edit hack (issue #3790), not the supported path. (wasm-bindgen issues/discussions)
4. [REPORTED] Production dual-shippers: Apryse WebViewer (threaded wasm when COOP/COEP present, else single-threaded/asm.js fallback); Qt WebAssembly (threaded vs non-threaded variants); Emscripten pthreads ecosystem generally. (docs.apryse.com; qt.io blog)
5. [REPORTED] Vite integration: wasm-bindgen-rayon workerHelpers work under Vite's module-worker model IF the worker entry stays a true module worker (`new Worker(new URL(...), {type:'module'})`) and the wasm asset URL survives bundling. Known bundler failures: workers inlined into main bundle, rewritten asset URLs. Meridian's vite.config already has `worker.plugins: [wasm()]` — favorable starting point. (wasm-bindgen discussions #3769/#3720; session recon of vite.config.js)
6. [REPORTED] Reported downsides of dual-artifact: nightly toolchain management in CI, doubled build time (std rebuild), loader branching complexity, threaded-only bug classes (deadlocks, worker lifecycle), benchmark both modes. (web.dev; qt.io; tonbo.io)

## Spec Implications

- Strategy comparison for the buildability-gate decision:
  - **Dual-artifact** preserves the stable sha256 baseline untouched (existing SC-04 evidence chain intact); costs: 2 builds, 2 pkg dirs, loader branch — but the loader branch is REQUIRED ANYWAY for the crossOriginIsolated fallback, so much of the "extra" complexity is already mandatory.
  - **Nightly re-pin single-artifact** simplifies to one build but: the single blob is threads-capable-with-fallback (rayon seq fallback still works when initThreadPool is never called), one baseline re-record, all consumers move to nightly-built code even in sequential mode.
- Either way the worker migrates from bundler-glue import to `--target web` init — spec should treat that migration as a shared prerequisite task.
- wasm-feature-detect (tiny, MIT) is the standard detection library — evaluate vs a hand-rolled `crossOriginIsolated && SharedArrayBuffer` check (offline bundle prefers zero new deps; hand-roll is ~5 lines).

## Sources

- https://web.dev/articles/webassembly-threads
- https://docs.rs/crate/wasm-bindgen-rayon/latest/source/README.md
- https://rustwasm.github.io/wasm-bindgen/examples/raytrace.html
- https://github.com/rustwasm/wasm-bindgen/discussions/3769
- https://github.com/rustwasm/wasm-bindgen/issues/3790
- https://github.com/wasm-bindgen/wasm-bindgen/discussions/3720
- https://docs.apryse.com/web/faq/wasm-threads
- https://www.qt.io/blog/2019/06/26/qt-webassembly-multithreading
- https://tonbo.io/blog/threads-with-webassembly

## Research Inventory

- Perplexity ask 2026-07-17 (citations above)
- Session recon 2026-07-17: scripts/build-wasm.sh (--target bundler), pay-equity-app vite.config.js (worker.plugins wasm())
