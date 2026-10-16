> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (verification-benchmark) — Draft

# Domain Spec — Verification & Benchmark (Mode Parity, Memory Ceiling, Wall-Clock, Repro, Test Server)

Charter items covered: In-Scope 8 (verification + benchmark), In-Scope 13 (Employers_data.csv fixture ×5→50k). Bound invariants: INV-02 (bit-identical across modes — the parity suite is its gate), INV-04 (reproducible-build model survives), INV-05 (50k peak fits the shared-memory maximum with headroom). Success criteria: SC-03 (parity test plan), SC-07 (verification suite with runnable commands). This suite validates the **MODES** (native / wasm-sequential / wasm-threaded). The METHODS suite (R/statsmodels goldens, proptest, QR) is in `phase2-spec-statistical-trust-layer.md` — kept distinct throughout.

---

## 1. Executive Summary

Threading changes the execution mode but must not change a single output bit (INV-02). This suite proves that with five jobs: (1) a **mode-parity** job that byte-compares canonical JSON across native / wasm-seq / wasm-threaded(N=2,4) on a fixed seed — blocking in CI, the SC-03 gate; (2) a **50k-row memory-ceiling** job asserting peak memory fits the declared shared-memory maximum with headroom (INV-05); (3) a **wall-clock benchmark harness** (browser page, `performance.now()`, warmup + median/p95 of ≥10, measured in-worker) plus a CI speedup job asserting relative speedup (>1.5× on 4-core), scheduled/non-blocking; (4) a **reproducibility double-build sha256** job (INV-04); (5) the **COOP/COEP-capable test server + headless-Chromium runner** the browser jobs need.

Two hard constraints shape the design. First, `wasm-pack test` / `wasm-bindgen-test-runner` have no verified cross-origin-isolation (COI) header support for atomics builds — so the browser gate is a tiny header-setting static server + Playwright-driven headless Chromium, not the built-in runner (see §7 gap). Second, perf assertions are **relative** (speedup ratio), never absolute wall-clock — GitHub Actions `ubuntu-latest` is 4-core, adequate for a 1.5× floor but the runner class is documented as a dependency.

Fixture PII: the 50k dataset is built by ×5 perturbation-replication of the **PII-stripped** Employers fixture (`Employee_ID`/`Name` dropped — see §4.5); it reuses the stripped fixture the trust spec commits, not the raw CSV.

---

## 2. Requirements

### R-VB-1 — Mode-parity suite (byte-identical JSON across modes) — SC-03 gate, blocking

The same seeded decompose run in native, wasm-sequential, and wasm-threaded(N=2 and N=4) produces byte-identical canonical JSON.

**Acceptance criteria**
- A canonical-JSON serializer (stable key order, fixed float formatting `%.17g`, seed + algorithm + version recorded) is used by all modes so a byte-compare is meaningful. Verify the serializer is deterministic: same run twice → identical bytes.
- Native reference: `cargo test -p pay-equity-engine --test mode_parity_test` runs the seeded decompose and writes `native.json`.
- Browser modes: the Playwright harness (R-VB-5) loads the threaded pkg, runs the same seeded decompose with the pool uninitialized (sequential fallback) and with `initThreadPool(2)` / `initThreadPool(4)`, emitting `wasm_seq.json`, `wasm_t2.json`, `wasm_t4.json`.
- Gate: `sha256sum native.json wasm_seq.json wasm_t2.json wasm_t4.json` yields four identical hashes; the job fails on any mismatch. This is **blocking** on every engine PR.
- Fixed seed + fixed input (a small committed fixture, e.g. the seed-42 parity fixture or a slice of the stripped Employers fixture); reps default from `analysis.rs:147`.
- Enforces the sequential-float-summation requirement from the cross-domain summary (any parallel reduction that reordered float adds would break this byte-compare — that is the point).

### R-VB-2 — 50k-row memory-ceiling job — INV-05, blocking

Peak memory at 50k rows stays below the declared shared-memory maximum with measured headroom.

**Acceptance criteria**
- Input: the stripped Employers fixture perturbation-replicated ×5 to ~50k rows (R-VB-4.5), committed or generated deterministically at test time from the committed 10k fixture (prefer generate-at-test-time to avoid committing a 50k file; the generator is seeded).
- Peak metric: the engine-side `memory_stats()` (allocated pages / peak via allocator hook) that the memory domain exposes — this suite **consumes** it; it does not respec it (cross-domain summary §Memory / u3: shared-memory `byteLength` reads the maximum, so page-count/allocator stats are the fine-grained metric).
- Assertion: `peak_bytes < declared_max_bytes − margin`, where `declared_max_bytes` is the shared-memory maximum from the memory-budget table (256–512 MiB envelope, cross-domain summary) and `margin` is the documented headroom. Job fails if peak ≥ ceiling.
- Command: a native harness path (`cargo test -p pay-equity-engine --test memory_ceiling_test`) for the CI gate where the engine allocator stats are readable without a browser; the browser `measureUserAgentSpecificMemory()` path (Chromium + COI) is the optional cross-check via the Playwright harness. Blocking on the native-readable metric.

### R-VB-3 — Wall-clock benchmark harness + CI speedup job (relative, non-blocking)

A browser benchmark page measures threaded-vs-sequential wall-clock; a scheduled CI job asserts a relative speedup floor.

**Acceptance criteria**
- Harness: a static HTML page served with COI headers (R-VB-5), running the decompose workload **inside the Web Worker** (less event-loop noise), timed with `performance.now()`.
- Protocol: 3–5 warmup iterations discarded; then ≥10 timed iterations; report **median and p95** for sequential and for threaded(4); compute `ratio = median_seq / median_threaded`.
- CI job `benchmark-speedup`: asserts `ratio > 1.5` on a 4-core runner; **scheduled/manual (`workflow_dispatch` + `schedule`), non-blocking** — never gates a PR (flaky-sensitive). Documents `runs-on: ubuntu-latest` (4-core) as a threshold dependency; a different runner class invalidates the 1.5× floor.
- Never asserts an absolute time. Local reuse: the same page runs on David's corp PC to measure real hardware.

### R-VB-4 — Reproducibility double-build sha256 job — INV-04

The threaded build is byte-reproducible: two clean builds of the raw wasm hash identically.

**Acceptance criteria**
- Extends the existing `wasm-verify` job pattern (`ci.yml:70` already sha256-checks the raw wasm against a committed baseline).
- New `reproducibility` job: build the raw threaded wasm twice from clean (`cargo clean` between), with the pinned toolchain + the `--remap-path-prefix` set already in `ci.yml:60` (covers `~/.cargo`, `$PWD`, `~/.rustup`), and assert the two sha256s match each other AND the committed baseline (`engine/pay_equity_engine.wasm.sha256` or a sibling threaded baseline per the chosen strategy).
- Command literal committed so `/build` runs it without further research (SC-01 spirit). Note: current baseline hashes the RAW cargo wasm (pre-bindgen) because wasm-bindgen 0.2.106 output is nondeterministic (`ci.yml:62`) — the threaded baseline follows the same raw-wasm rule.

### R-VB-5 — COOP/COEP test server + headless-Chromium runner

The browser jobs need a cross-origin-isolated context; the built-in wasm test runners do not provide one verifiably.

**Acceptance criteria**
- A tiny static server sets `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` on every response (Python `http.server` subclass or a ~30-line Node server). Verify `crossOriginIsolated === true` in the served page (a harness assertion that fails the job otherwise).
- Playwright drives headless Chromium against that server, loads the `--target web` threaded pkg, and exposes the run outputs (JSON, timings, memory) back to the CI job. (`--target web` migration is mandatory regardless of strategy — cross-domain summary §Toolchain.)
- The Meridian repo already has vitest; this browser-threaded job is new surface. Reused locally by David (headless or headed) on the corp PC.
- Node `worker_threads` + SAB smoke path is classified **optional supplement**, not the gate — `--target web` glue under Node isn't guaranteed. The authoritative gate is one real headless-Chromium job with headers.

---

## 3. Technical Architecture

```
CI (.github/workflows/ci.yml — extends existing jobs)
├─ quality               [existing, blocking]  cargo test --workspace  ← METHODS suite runs here (trust spec)
├─ wasm-verify           [existing, blocking]  raw-wasm sha256 vs baseline
├─ mode-parity           [NEW, blocking]       native.json ⊕ wasm_{seq,t2,t4}.json → 4× equal sha256   (R-VB-1, SC-03/INV-02)
├─ memory-ceiling        [NEW, blocking]       50k-row peak < declared max − margin                     (R-VB-2, INV-05)
├─ reproducibility       [NEW, blocking]       double clean build → equal raw-wasm sha256               (R-VB-4, INV-04)
└─ benchmark-speedup     [NEW, scheduled/manual, non-blocking]  median_seq/median_threaded > 1.5×       (R-VB-3)

Browser jobs (mode-parity browser modes, benchmark, optional browser memory) run via:
  tiny COI static server (COOP=same-origin, COEP=require-corp)  ──serves──►  --target web threaded pkg page
                                    ▲ Playwright headless Chromium drives, asserts crossOriginIsolated, extracts outputs
```

**Modes vs methods (kept distinct).** This suite never checks whether a number is *statistically correct* — that is the trust spec's job (native goldens/proptest/QR). It checks whether the number is *bit-identical across execution modes* and *fits the memory/perf envelope*. The mode-parity byte-compare is the INV-02 gate; the trust goldens are the correctness gate. A regression in decomposition math would pass mode-parity (all modes wrong-identically) and fail the trust suite — which is why both exist.

**Naming collision (flagged).** `oaxaca_blinder/tests/parity_test.rs` is a METHOD golden (statsmodels), despite "parity". This suite's byte-compare is the MODE parity. New files here use the `mode_parity` prefix to disambiguate (mirrors Risk R-TL-C in the trust spec).

---

## 4. Implementation Details

### 4.1 Canonical JSON serializer (shared by all modes)
- Stable key ordering (BTreeMap or explicit field order), floats as `%.17g`, embedded `_meta`: seed, RNG algorithm + crate version, reps, mode label. The mode label is excluded from the hashed payload (or the harness strips it) so only the numeric content is byte-compared.
- Determinism self-test: serialize the same result twice, assert equal bytes, before any cross-mode compare (guards against map-iteration-order nondeterminism).

### 4.2 CI wiring
- Add the four new jobs to `ci.yml`. `mode-parity`, `memory-ceiling`, `reproducibility` under the same `on: [push, pull_request]` triggers as `quality`. `benchmark-speedup` under `on: {schedule: [cron], workflow_dispatch: {}}` only.
- Browser jobs install Playwright + Chromium (`npx playwright install --with-deps chromium`) and the threaded toolchain (pinned nightly + `--target web`, per cross-domain summary — this suite consumes the toolchain the build spec pins, does not re-pin it).
- Threaded-build wasm-bindgen invocation uses `--target web` (existing `ci.yml:66` uses `--target bundler`; the migration is owned by the build/toolchain domain — referenced, not respecced here).

### 4.3 Benchmark statistics
- Warmup 3–5 (discarded), timed ≥10, report median + p95. Ratio on medians. Workload sized so timer quantization is negligible (`performance.now()` is microsecond-capable only under COI — another reason the COI server is mandatory even for timing).

### 4.4 Memory metric selection
- Primary (CI gate): engine-side `memory_stats()` allocated-page peak (native-readable, deterministic). Secondary (browser cross-check): `measureUserAgentSpecificMemory()` (Chromium + COI). `WebAssembly.Memory.buffer.byteLength` is **not** used as the peak metric for shared memory — it reports the fixed maximum, not usage (cross-domain summary / u3 finding). This suite consumes these metrics; their engine-side implementation is the memory domain's deliverable.

### 4.5 50k fixture + PII handling (explicit)
- Source: the **PII-stripped** Employers fixture the trust spec commits (`oaxaca_blinder/tests/fixtures/employers_trust_fixture.csv`, `Employee_ID`/`Name` dropped). This suite never reads the raw `/home/deji/Downloads/Employers_data.csv`.
- 50k build: ×5 perturbation-replication (seeded jitter on numeric columns, category-preserving) of the 10k stripped fixture → ~50k rows, generated deterministically at test time from the committed 10k file (avoids committing a 50k binary; generator seed recorded). Perturbation strategy: small bounded noise on `Age`, `Experience_Years`, `Salary`; categoricals unchanged — documented in the generator header.
- No direct identifiers ever enter the 50k frame, the memory job, or any emitted artifact. Emitted parity/benchmark JSON contains only aggregate results + timings/memory, no row data.

---

## 5. Dependencies and Integrations
- **New CI surface**: Playwright + headless Chromium; a ~30-line COI static server (Python or Node). Meridian has vitest already; the browser-threaded job is new either way.
- **Consumes (not respecs)**: the threaded toolchain + `--target web` build (build/toolchain domain), the engine `memory_stats()` + shared-memory maximum + thread-cap formula (memory domain), the owned-index seeded RNG (RNG/determinism domain), the sequential-float-summation guarantee (RNG domain). All per the cross-domain summary.
- **Extends existing**: `ci.yml` (`wasm-verify` sha256 pattern → `reproducibility`; `quality` already runs the native METHODS suite).
- **Fixture**: PII-stripped Employers fixture shared with the trust spec (§4.5).
- **Local reuse**: benchmark page + COI server run standalone on David's corp PC.

---

## 6. Risk Assessment
- **R-VB-A (high)** — No verified COI support in `wasm-pack test`/`wasm-bindgen-test-runner`; if the custom server + Playwright path also snags (e.g. nested-worker pool spawn from a page vs a worker), the browser gate slips. Mitigation: the custom header-server + Playwright path is the primary design (not the built-in runner); nested-worker spawn is a cross-domain unknown (Meridian wiring) — reference, don't own. §7 gap.
- **R-VB-B (medium)** — Benchmark flakiness on shared CI runners → false speedup failures. Mitigation: non-blocking + scheduled; relative ratio not absolute; median/p95 of ≥10 with warmup; runner-class documented.
- **R-VB-C (medium)** — Canonical-JSON nondeterminism (map ordering, float formatting) would make byte-compare falsely fail or falsely pass. Mitigation: stable-order serializer + a same-mode double-serialize self-test before cross-mode compare (§4.1).
- **R-VB-D (low)** — wasm-bindgen 0.2.106 output nondeterminism (already known, `ci.yml:62`) means the repro baseline must hash the RAW cargo wasm, not the bindgen output. Mitigation: follow the existing raw-wasm baseline rule for the threaded blob.
- **R-VB-E (low)** — 50k generate-at-test-time perturbation could drift the memory profile run-to-run. Mitigation: seeded generator, seed recorded; peak assertion has documented margin, not a knife-edge.

---

## Gaps Requiring Deeper Research

> NEEDS RESEARCH: Does `wasm-pack test` / `wasm-bindgen-test-runner` support setting COOP/COEP (cross-origin isolation) for atomics builds in any current version, or is the tiny-header-server + Playwright path the only viable browser gate? (single-agent: check the wasm-bindgen/wasm-pack issue trackers + latest release notes for COI header support.)

> NEEDS RESEARCH: Can a wasm-bindgen-rayon thread pool be spawned from inside a dedicated Web Worker (nested workers) under headless Chromium — i.e. does the Playwright harness need the page to host the worker, or can it drive the exact Meridian nested-worker topology? (single-agent: minimal Playwright + nested-worker + initThreadPool experiment. Note: the Meridian-wiring domain owns the production topology; this gap is only about the test harness reproducing it.)

> NEEDS RESEARCH: What is the exact declared shared-memory maximum and headroom margin the memory-budget table settles on (256 vs 384 vs 512 MiB), so R-VB-2's ceiling constant is a literal, not a `[CLARIFY]`? (single-agent: read the memory domain's final budget table once available.)

---

## 8. Spark Notes
- Five jobs: mode-parity (blocking, SC-03/INV-02), memory-ceiling@50k (blocking, INV-05), reproducibility double-build sha256 (blocking, INV-04), benchmark-speedup (scheduled/non-blocking, >1.5× on 4-core), COI test server + Playwright Chromium (enabling infra).
- Mode-parity = byte-identical canonical JSON across native / wasm-seq / wasm-threaded(2,4) on a fixed seed → 4 equal sha256s. This validates MODES; the trust spec validates METHODS. A math bug passes parity (all modes wrong-identically) and fails the trust goldens — both needed.
- Perf assertions are **relative** (ratio), never absolute; median/p95 of ≥10 after 3–5 warmup, measured in-worker; `ubuntu-latest` = 4-core, documented dependency.
- Memory peak = engine `memory_stats()` page count (consumed, not respecced); shared-memory `byteLength` is the max not the usage, so it is not the metric.
- Browser gate = tiny COOP=same-origin/COEP=require-corp static server + headless Chromium via Playwright; `wasm-pack test` COI support unverified (§7); `--target web` mandatory (owned by build domain).
- Repro hashes the RAW cargo wasm (wasm-bindgen 0.2.106 nondeterministic), following the existing `ci.yml:70` baseline rule.
- 50k fixture = ×5 seeded perturbation-replication of the PII-stripped 10k Employers fixture, generated at test time; `Employee_ID`/`Name` never present.
- Naming: `parity_test.rs` is a METHOD golden despite its name; new files here use `mode_parity` to disambiguate.
- Open: wasm-pack COI support, nested-worker pool spawn in the harness, the final declared memory-max constant.


## Phase 1 Sources

- phase1-verification-benchmark-browser-ci.md
- phase1-statistical-trust-layer-golden-proptest.md
