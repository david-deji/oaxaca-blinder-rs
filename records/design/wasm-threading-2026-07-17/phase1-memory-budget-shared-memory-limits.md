# Phase 1 — Memory: Shared-Memory Limits, Growth, Thread Stacks

> Unit: u3 | Domain: memory-budget | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

Threaded wasm shared memory must declare a fixed `maximum`; engines allocate the backing
SharedArrayBuffer at maximum up front and never move it (no detach-on-grow — a reliability win vs
non-shared memory). Practical ceiling: 4 GiB on 64-bit Chrome/Edge/Firefox, but ≤2 GiB is the
portable envelope and browsers can OOM-kill well below that. Per-thread cost = stack
(set via `-C link-arg=-zstack-size=N`, typical 0.5–1 MiB) + small TLS, carved from the shared
linear memory — maximum must budget heap + N×stack + margin. For our 50k-row workload the
research suggests a 256–512 MiB maximum with 4–8 workers is the right envelope, pending the
empirical profile (In-Scope 11).

## Findings

1. [CONSENSUS] Shared `WebAssembly.Memory` requires `{shared: true, maximum}`; growth up to maximum works at runtime; backing SAB is allocated at maximum and never detaches, so existing views stay valid (unlike non-shared grow). (v8.dev/blog/4gb-wasm-memory; wasmtime shared_memory source; WebAssembly/design#1397)
2. [CONSENSUS] Ceilings: 4 GiB wasm32 spec max; Chrome/V8 up to 4 GiB opt-in; Firefox 89+ 64-bit up to 4 GiB wasm memory (8 GB SAB), DOM views capped 2 GB; practical portable envelope ≤2 GiB; real tabs can OOM below 1–2 GiB. (v8.dev; mozilla dev-platform; bugzilla 1392234)
3. [CONSENSUS] Stack per thread: set via wasm-ld `-z stack-size` → Rust: `RUSTFLAGS="-C link-arg=-zstack-size=1048576"` (1 MiB example); each rayon worker's stack lives in shared linear memory; under-provisioned maximum + more threads = mysterious stack-overflow traps. Budget: heap + N×(stack+scratch) + margin. (webassembly-wasm.com threading guide; general toolchain docs)
4. [REPORTED] OOM behavior: `memory.grow` failure → allocator returns null → Rust abort → WebAssembly.RuntimeError catchable in JS (state possibly inconsistent — treat as fatal for the run, surface a graceful error per INV-03/build-safety); hard browser OOM → tab kill, unrecoverable. Detection: no free-memory query exists; catch RuntimeError at the worker boundary and message the UI. (WebAssembly/design#1397; practitioner reports)
5. [REPORTED] Sizing rule of thumb for our shape (50k rows, tens-of-MB working set): base heap 64–128 MiB + N×(scratch 5–20 MiB + stack 1 MiB) + 64 MiB margin → 256–512 MiB maximum; initial small (16–64 MiB) and grow. Thread count: min(hardwareConcurrency, 4–8), memory-capped per INV-05. (synthesis from v8.dev + threading guides)
6. [SINGLE_SOURCE] Whole-heap JS views over shared memory read byteLength = max SAB size; logical heap tracked in wasm pages — worker-side JS that inspects memory must not conflate the two. (red-badger memory-grow writeup + wasmtime source)

## Spec Implications

- The thread-cap formula (Charter INV-05) has its structure: `N_max = floor((M_max − heap_profile_50k − margin) / (stack + scratch_per_thread))`, all terms measured in the pre-threading profile (In-Scope 11), then `N = min(N_max, hardwareConcurrency, 8)`.
- Bootstrap currently CLONES df_a/df_b per rep (builder.rs:828-829) — per-thread scratch for us is a full dataset clone per in-flight rep. At 50k rows this dominates the budget; the spec's engine-parallel-surface domain must consider sample-index-based resampling (share the base frame read-only, materialize only index vectors) as the memory-dominant design choice.
- `--max-memory` link arg + `-zstack-size` become pinned constants in build-wasm.sh, derived from the profile, recorded in the spec.
- Graceful OOM: worker catches RuntimeError → posts ERROR message to UI (existing worker protocol has error path) — never a blank screen (INV-03).

## Sources

- https://v8.dev/blog/4gb-wasm-memory
- https://groups.google.com/a/mozilla.org/g/dev-platform/c/90hVJF-X9c4
- https://bugzilla.mozilla.org/show_bug.cgi?id=1392234
- https://github.com/WebAssembly/design/issues/1397
- https://docs.wasmtime.dev/api/src/wasmtime/runtime/vm/memory/shared_memory.rs.html
- https://www.webassembly-wasm.com/js-wasm-interop-memory-management/sharedarraybuffer-atomics-threading/
- https://awesome.red-badger.com/chriswhealy/memory-grow-and-arraybuffers
- https://web.dev/articles/webassembly-threads

## Research Inventory

- Perplexity ask 2026-07-17 (citations above)
- Session recon 2026-07-17: builder.rs:825-848 (per-rep DataFrame clones — memory-dominant pattern)
