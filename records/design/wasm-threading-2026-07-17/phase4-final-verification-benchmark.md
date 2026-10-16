> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (verification-benchmark) — Phase 4 FINAL (buildable)
> Charter items: In-Scope 8 (verification + benchmark), In-Scope 13 (Employers_data.csv ×5→50k)
> Bound invariants: INV-02 (bit-identical across modes — this suite's gate), INV-03 (sequential fallback never crashes), INV-04 (reproducible-build survives), INV-05 (50k peak fits shared-memory max with headroom)
> Success criteria: SC-03 (parity test plan), SC-07 (verification suite with runnable commands)

# Phase 4 Final Spec — Verification & Benchmark (Mode Parity, Memory Ceiling, Speedup, Repro, COI Test Server)

## Summary

Threading changes the execution mode but must not change a single output bit (INV-02). This suite is the **MODES** gate — it never checks whether a number is statistically correct (that is the METHODS suite, `phase4-final-statistical-trust-layer.md`); it checks that the number is **bit-identical across execution modes** and **fits the memory/perf envelope**. A math bug passes mode-parity (all modes wrong-identically) and fails the trust goldens — which is why both suites exist.

Four CI jobs plus one enabling infra piece:

1. **mode-parity** (blocking, SC-03/INV-02) — canonical JSON on a fixed seed: byte-identical WITHIN each platform across thread counts (native 1/2/4; wasm seq/t2/t4), tolerance-parity 1e-6 ACROSS native↔wasm (council MJ-1 split; cross-ISA libm makes byte-equality across ISAs unachievable).
2. **memory-ceiling@50k** (blocking, INV-05) — peak memory at 50k rows < declared shared-memory max − margin.
3. **reproducibility** (blocking, INV-04) — double clean build → equal raw-wasm sha256, using **two distinct non-default `CARGO_TARGET_DIR`s** (W8) so rust-cache cannot mask a real recompile.
4. **benchmark-speedup** (scheduled/manual, non-blocking) — median-of-10 `median_seq / median_threaded > 1.5×` on a 4-core runner.
5. **COI test server + Playwright headless Chromium** (enabling infra) — a custom ~30-line COOP(same-origin)/COEP(require-corp) static server, because `wasm-pack test` cannot serve those headers for atomics builds (W7). Non-threaded unit tests may stay on `wasm-pack test`.

Fixture: `/home/deji/Downloads/Employers_data.csv` (10k rows) ×5 perturbation-replication → ~50k, `Gender` + categoricals unchanged, `Name`/`Employee_ID` stripped, generated at test time from the PII-stripped 10k fixture the trust spec commits (never the raw CSV). Build order: this validation suite is **last** (determinism → memory profile → threading → validation) — it consumes the threaded toolchain, the seeded RNG, and the engine memory metric; it does not respec them.

---

## In-Scope (this domain)

- **Canonical-JSON serializer** shared by all modes (stable key order, `%.17g` floats, seed/algorithm/version in `_meta`) so a byte-compare is meaningful — with a same-mode double-serialize determinism self-test.
- **mode-parity** CI job (native + browser modes via Playwright) — blocking.
- **memory-ceiling@50k** CI job — blocking; consumes the engine `memory_stats()` peak metric (memory domain's deliverable).
- **reproducibility double-build** CI job — blocking; two distinct `CARGO_TARGET_DIR`s (W8).
- **benchmark-speedup** CI job — scheduled/`workflow_dispatch`, non-blocking; relative ratio only, never absolute wall-clock.
- **COI static server + Playwright Chromium** harness (W7) — the browser gate; asserts `crossOriginIsolated === true`.
- **50k fixture generator** — ×5 seeded perturbation-replication of the PII-stripped 10k fixture.
- **Worker OOM handling reframed** (W4) — budget-prevention + structured engine self-report, not `RuntimeError` message parsing.

Explicitly **not** in this domain: the R/statsmodels goldens, proptest, QR, per-predictor quantile golden (all in `phase4-final-statistical-trust-layer.md`); the threaded toolchain pin, `.cargo/config.toml`, `--max-memory` link arg, `--target web` bindgen migration (toolchain-build domain — consumed here); the engine `memory_stats()` implementation and the declared shared-memory maximum (memory domain — consumed here); the Meridian worker `initThreadPool` wiring (meridian domain).

---

## Design & Decisions

### D-1 — Canonical JSON serializer (shared by all modes)

Stable key ordering (BTreeMap or explicit field order), floats formatted `%.17g` (matching the fixture precision convention `gen_parity_golden.py:90` established), embedded `_meta`: seed, RNG algorithm + crate version, reps, mode label. The mode label is **excluded from the hashed payload** (the harness strips it) so only numeric content is byte-compared. A determinism self-test serializes the same result twice and asserts equal bytes **before** any cross-mode compare — guards against map-iteration-order nondeterminism (R-VB-C). Any parallel float reduction that reordered adds would break this byte-compare — that is the point (enforces the sequential-float-summation requirement from the RNG/determinism domain).

### D-2 — mode-parity: byte-identical across modes (SC-03/INV-02, blocking)

- Native reference: `cargo test -p pay-equity-engine --test mode_parity_test` runs the seeded decompose and writes `native.json`.
- Browser modes via the Playwright harness (D-6): load the `--target web` threaded pkg, run the same seeded decompose with (a) the pool uninitialized (sequential fallback), (b) `initThreadPool(2)`, (c) `initThreadPool(4)` — emitting `wasm_seq.json`, `wasm_t2.json`, `wasm_t4.json`.
- Gate (SPLIT per council MJ-1, pending founder INV-02 reframe): **within-platform** byte-identity — `sha256(native_1t)==sha256(native_2t)==sha256(native_4t)` AND `sha256(wasm_seq)==sha256(wasm_t2)==sha256(wasm_t4)` — is the blocking threading-safety check (achievable). The **native↔wasm** leg is tolerance-parity `|native − wasm| ≤ 1e-6` (cross-ISA libm divergence makes byte-equality across ISAs unachievable — `.exp`/`.powf`/statrs `cdf`; `parity_test.rs:24` already uses 1e-6). Blocking on every engine PR. Founder confirms the split at the buildability gate.
- Fixed seed + fixed small committed input (the seed-42 parity fixture or a slice of the stripped Employers fixture); reps default from the decompose entry point.
- Sequential fallback (pool uninitialized) exercises INV-03 — rayon's wasm fallback runs on the current thread; the wasm `POOL` stub degrades safely (L4).

### D-3 — memory-ceiling@50k (INV-05, blocking) + OOM reframe (W4)

- Input: the PII-stripped 10k fixture perturbation-replicated ×5 to ~50k (D-7), generated deterministically at test time (avoids committing a 50k binary).
- Peak metric: the engine-side `memory_stats()` allocated-page/peak (memory domain's deliverable) — this suite **consumes** it. `WebAssembly.Memory.buffer.byteLength` is **not** the metric: for shared memory it reports the fixed maximum, not usage (L4/u3). The browser `measureUserAgentSpecificMemory()` (Chromium + COI) path is an optional cross-check via the Playwright harness.
- Assertion: `peak_bytes < declared_max_bytes − margin`, where `declared_max_bytes` and `margin` come from the memory domain's final budget table (256–512 MiB envelope). Job fails if peak ≥ ceiling.
- CI gate command: `cargo test -p pay-equity-engine --test memory_ceiling_test` (native path where allocator stats are readable without a browser). Blocking on the native-readable metric.
- **Worker OOM handling (W4, reframe — do NOT parse error messages)**: an in-execution wasm OOM cannot be reliably distinguished from other traps by inspecting the error object (same `WebAssembly.RuntimeError` constructor, engine-defined non-standard message; may hard-crash with no JS exception; a `Memory.grow` past the declared max throws `RangeError`, indistinguishable from OOM — W4 `[CONSENSUS]`). The worker OOM strategy is therefore: (a) **budget-prevention** — enforce the computed thread-cap and the `--max-memory` link budget so OOM is prevented, not caught; (b) **structured engine self-report** — the Rust engine detects its own allocation pressure (checked allocation at rep-batch boundaries) and returns a structured `{error:"OOM", ...}` value rather than trapping. The verification harness asserts the engine emits that structured value under a deliberately-oversized input; it never inspects `RuntimeError.message`.

### D-4 — reproducibility double-build (INV-04, blocking; W8)

- Extends the existing `wasm-verify` sha256 pattern (`ci.yml:70-82` sha256-checks the raw wasm vs a committed baseline).
- New `reproducibility` job: build the raw threaded wasm twice from clean, each into a **distinct non-default `CARGO_TARGET_DIR`** (e.g. `CARGO_TARGET_DIR=target-a` then `target-b`) — Swatinem/rust-cache@v2 caches only `~/.cargo` + `./target`, so a custom target dir is **not** restored from cache and both builds recompile from source including `-Zbuild-std` std artifacts, with no masking (W8 `[CONSENSUS]`). Alternative: disable rust-cache on this job. Use the pinned toolchain and the `--remap-path-prefix` set already at `ci.yml:60` (covers `~/.cargo`, `$PWD`, `~/.rustup`).
- Assert the two sha256s match each other AND the committed baseline. Hash the **RAW cargo wasm** (pre-bindgen) because wasm-bindgen 0.2.106 output is nondeterministic (`ci.yml:62`) — the threaded baseline follows the same raw-wasm rule as `engine/pay_equity_engine.wasm.sha256` (`ci.yml:70-73`).
- Command literals committed so `/build` runs it without further research (SC-01 spirit).

### D-5 — benchmark-speedup (non-blocking, relative)

- Harness: a static HTML page served with COI headers (D-6), running the decompose workload **inside the Web Worker** (less event-loop noise), timed with `performance.now()` (microsecond-capable only under COI — another reason the COI server is mandatory even for timing).
- Protocol: 3–5 warmup iterations discarded; ≥10 timed iterations; report **median and p95** for sequential and threaded(4); `ratio = median_seq / median_threaded`.
- CI job `benchmark-speedup`: asserts `ratio > 1.5` on a 4-core runner; `on: {schedule: [cron], workflow_dispatch: {}}`, **non-blocking** — never gates a PR (flaky-sensitive). Documents `runs-on: ubuntu-latest` (4-core) as a threshold dependency; a different runner class invalidates the 1.5× floor. Never asserts absolute time. The same page reruns on David's corp PC for real-hardware numbers.

### D-6 — COI test server + Playwright headless Chromium (W7, the browser gate)

`wasm-pack test --chrome --headless` / `wasm-bindgen-test-runner` do **not** serve the test page with COOP/COEP headers — `crossOriginIsolated` is false and SharedArrayBuffer/threads are unavailable in that harness (W7 `[CONSENSUS]`; wasm-bindgen#2151, wasm-pack#1355). So the threaded/SAB browser gate is:

- A tiny static server (~30 lines, Python `http.server` subclass or Node) setting `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` on every response. A harness assertion fails the job unless the served page reports `self.crossOriginIsolated === true`.
- Playwright drives headless Chromium against that server, loads the `--target web` threaded pkg (the `--target web` migration is mandatory and owned by the toolchain domain — consumed here), and exposes run outputs (JSON, timings, memory) back to the CI job.
- Non-threaded unit tests may still use `wasm-pack test` (Node) — the COI harness is required only for the threaded/SAB paths.
- Node `worker_threads` + SAB smoke path is an **optional supplement**, not the gate — the authoritative gate is one real headless-Chromium job with the headers.

### D-7 — 50k fixture generator + PII (L4-clean, no POLARS_MAX_THREADS)

- Source: the PII-stripped 10k Employers fixture the trust spec commits (`oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv`) — never the raw `/home/deji/Downloads/Employers_data.csv`.
- 50k build: ×5 perturbation-replication with seeded bounded jitter on numeric columns (`Age`, `Experience_Years`, `Salary`); `Gender` **and all categoricals** (`Department`, `Education_Level`, `Location`) unchanged (W10/L6: preserves the 3-level Education_Level structure). Generated at test time from the committed 10k file; generator seed recorded in a header comment. No direct identifiers (`Name`, `Employee_ID`) ever enter the 50k frame, the memory job, or any emitted artifact.
- **L4 correction (delete POLARS_MAX_THREADS)**: no test setup sets `POLARS_MAX_THREADS`. polars 0.44 never reads it on wasm (the wasm `POOL` is a stub delegating `join`/`scope`/`spawn` to the global rayon registry — polars-utils-0.44.2 `src/wasm.rs`; L4). After `initThreadPool`, polars' wasm stub routes onto **our** single global rayon pool — cooperative work-stealing, not oversubscription. The only residual determinism audit is any polars-internal parallel float reduction on the hot path; our stats run in nalgebra after `take`, so exposure is narrow — but named as an audit item for the mode-parity byte-compare to catch.

### CI architecture

```
.github/workflows/ci.yml (extends existing jobs)
├─ quality           [existing, blocking]  cargo test --workspace  ← METHODS suite runs here (trust spec)
├─ wasm-verify       [existing, blocking]  raw-wasm sha256 vs baseline (ci.yml:70-82)
├─ mode-parity       [NEW, blocking]       native.json ⊕ wasm_{seq,t2,t4}.json → 4× equal sha256   (D-2, SC-03/INV-02)
├─ memory-ceiling    [NEW, blocking]       50k-row peak < declared max − margin                     (D-3, INV-05)
├─ reproducibility   [NEW, blocking]       double clean build, 2 CARGO_TARGET_DIRs → equal raw-wasm sha256  (D-4, INV-04)
└─ benchmark-speedup [NEW, scheduled/manual, non-blocking]  median_seq/median_threaded > 1.5×      (D-5)

Browser jobs (mode-parity browser modes, benchmark, optional browser memory):
  COI static server (COOP=same-origin, COEP=require-corp)  ──serves──►  --target web threaded pkg page
                        ▲ Playwright headless Chromium drives, asserts crossOriginIsolated, extracts outputs  (D-6, W7)
```

### Naming collision (flagged, both specs)

`oaxaca_blinder/tests/parity_test.rs` is a METHOD golden (statsmodels) despite the word "parity". This suite's byte-compare is the MODE parity; new files here use the `mode_parity` prefix to disambiguate.

---

## Build Steps (ordered, buildable)

1. **Canonical JSON serializer** (D-1) in the engine, shared by native + wasm entry points; with the same-mode double-serialize determinism self-test.
2. **`mode_parity_test.rs`** native reference (`cargo test -p pay-equity-engine --test mode_parity_test` writes `native.json`).
3. **COI static server + Playwright harness** (D-6): the ~30-line COOP/COEP server, a `crossOriginIsolated === true` assertion, and the Playwright driver that runs seq / t2 / t4 and emits `wasm_seq.json` / `wasm_t2.json` / `wasm_t4.json`.
4. **50k fixture generator** (D-7): seeded ×5 perturbation-replication of the committed 10k stripped fixture; categoricals unchanged; no `POLARS_MAX_THREADS`.
5. **`memory_ceiling_test.rs`** (D-3): consumes engine `memory_stats()` peak; asserts `peak_bytes < declared_max_bytes − margin`; plus the structured-OOM self-report assertion (W4) under an oversized input.
6. **Add CI jobs** to `.github/workflows/ci.yml`: `mode-parity`, `memory-ceiling`, `reproducibility` under the same `on: [push, pull_request]` triggers as `quality` (`ci.yml:3-7`); `benchmark-speedup` under `on: {schedule: [cron], workflow_dispatch: {}}`. Browser jobs install Playwright + Chromium (`npx playwright install --with-deps chromium`) and the threaded `--target web` toolchain (consumed from the toolchain domain).
7. **`reproducibility` job** (D-4): double clean build into `target-a` / `target-b`, pinned toolchain + `--remap-path-prefix` (mirror `ci.yml:60`), assert equal raw-wasm sha256 vs each other and the committed threaded baseline.
8. **`benchmark-speedup` job + benchmark page** (D-5): in-worker `performance.now()`, 3–5 warmup + ≥10 timed, median/p95, `ratio > 1.5`, non-blocking.

---

## Acceptance Criteria (objectively checkable)

- **AC-1 (serializer determinism)**: a native test serializes one result twice and asserts equal bytes; `cargo test -p pay-equity-engine --test mode_parity_test` exits 0 including that self-test.
- **AC-2 (mode-parity gate — split per council MJ-1)**: the `mode-parity` CI job runs native (1/2/4) + browser (seq, t2, t4); asserts `sha256(native_1t)==native_2t==native_4t` AND `sha256(wasm_seq)==wasm_t2==wasm_t4` (within-platform byte-identity, blocking) AND `|native − wasm_seq| ≤ 1e-6` per numeric field (across-ISA tolerance-parity). Exits non-zero on any within-platform mismatch or any across-platform field exceeding 1e-6. Blocking on push/PR. (Pre-reframe wording asserted native==wasm byte-equality, which cross-ISA libm makes unachievable.)
- **AC-3 (COI proven)**: the served harness page asserts `self.crossOriginIsolated === true`; the Playwright job fails if it is false (proves the COOP/COEP server works and `wasm-pack test` was correctly bypassed — W7).
- **AC-4 (memory ceiling)**: `cargo test -p pay-equity-engine --test memory_ceiling_test` exits 0 with `peak_bytes < declared_max_bytes − margin` at ~50k rows; exits non-zero if peak ≥ ceiling. Blocking.
- **AC-5 (structured OOM, no message parsing)**: a test feeds a deliberately-oversized input and asserts the engine returns the structured `{error:"OOM"}` value; the test contains **no** reference to `RuntimeError` message text (W4). `grep -L 'RuntimeError' <oom_test_file>` confirms absence.
- **AC-6 (reproducibility)**: the `reproducibility` job builds twice into `CARGO_TARGET_DIR=target-a` and `target-b` (neither `./target`), and `sha256sum target-a/.../pay_equity_engine.wasm target-b/.../pay_equity_engine.wasm` plus the committed baseline are **all equal**; job exits non-zero otherwise. Blocking.
- **AC-7 (no POLARS_MAX_THREADS)**: `grep -rn 'POLARS_MAX_THREADS' .github/ engine/ oaxaca_blinder/ scripts/` returns zero matches in test/CI setup (L4).
- **AC-8 (speedup, non-blocking)**: the `benchmark-speedup` job (`schedule` + `workflow_dispatch` only) computes `median_seq / median_threaded` over ≥10 timed iterations after 3–5 warmup and asserts `> 1.5` on `ubuntu-latest` (4-core); it never runs on push/PR and never gates a merge.
- **AC-9 (INV-03 fallback)**: the `wasm_seq.json` mode (pool uninitialized) runs to completion and byte-matches the threaded modes — proving sequential fallback works and is bit-identical, not a crash or blank output.
- **AC-10 (50k PII-clean)**: the 50k generator reads only `oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv`; no emitted artifact contains `Name` or `Employee_ID` columns; categoricals (`Gender`, `Department`, `Education_Level`, `Location`) are unchanged across the ×5 replication (assertable by category-set equality pre/post).

---

## Open Items (route to buildability gate)

- **OI-1 (cross-domain constant)**: the exact `declared_max_bytes` and `margin` for AC-4 come from the memory domain's final budget table (256 vs 384 vs 512 MiB). Until that lands, the memory-ceiling test reads the constant from a single shared config location the memory domain owns — not hardcoded here. Non-blocking on this spec's structure; blocking on the number before the memory-ceiling job is green.
- **OI-2 (harness topology, verify at build)**: whether the wasm-bindgen-rayon pool spawns from inside a dedicated Web Worker (nested workers) under headless Chromium vs. must be hosted by the page — the production Meridian topology is owned by the meridian domain; this harness must reproduce whichever it uses. W2 confirms the COI/SAB *permission* holds for nested isolated workers (`crossOriginIsolated === true`); the *spawn mechanics* are a build-preflight check (E3), not a research gap. **Strategy A (dual artifact) RATIFIED** (2026-07-18, ruling 1): the mode-parity matrix runs the stable seq blob AND the threaded blob (seq/t2/t4); the reproducibility double-build runs on the threaded blob, baseline generated in the pinned container.
- All other phase-2 gaps resolved: `wasm-pack test` COI support → W7 (no support; custom server is the design); rust-cache masking → W8 (two `CARGO_TARGET_DIR`s); OOM taxonomy → W4 (budget-prevention + structured self-report).

---

## Sources

- Phase 1: `phase1-verification-benchmark-browser-ci.md`, `phase1-statistical-trust-layer-golden-proptest.md`
- Phase 3 web: `phase3-web-w1-w2-w3-w4-w5-w6-w7-w8-w9-w10-findings.md` — W4 (OOM taxonomy, no message parsing), W7 (custom COOP/COEP server + Playwright, not `wasm-pack test`), W8 (two non-default `CARGO_TARGET_DIR`s), W2 (nested-worker COI inheritance)
- Phase 3 local: `phase3-local-l1-l2-l3-l4-l5-l6-findings.md` — L4 (drop `POLARS_MAX_THREADS`; wasm POOL stub delegates to global rayon; polars-utils-0.44.2 `src/wasm.rs`), L6 (Education_Level 3-level — preserved unchanged across replication)
- Charter: `spec-charter.md` — In-Scope 8/13, INV-02/03/04/05, SC-03/SC-07
- Repo anchors (verified this phase): `.github/workflows/ci.yml:3-7,25-28,37,53-54,56-68,60,62,64-68,70-82`; `verification/gen_parity_golden.py:90`; `oaxaca_blinder/src/quantile_decomposition.rs:267-271`
- W7 URLs: https://rustwasm.github.io/docs/wasm-pack/commands/test.html ; https://github.com/rustwasm/wasm-bindgen/issues/2151 ; https://github.com/rustwasm/wasm-pack/issues/1355
- W8 URL: https://github.com/Swatinem/rust-cache ; W4 URL: https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/RuntimeError ; W2 URLs: https://developer.mozilla.org/en-US/docs/Web/API/WorkerGlobalScope/crossOriginIsolated ; https://web.dev/articles/coop-coep
