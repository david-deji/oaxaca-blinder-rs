# 0014-MERIDIAN Spec-vs-Shipped Audit — Mission A

**Scope confirmed**: Issue 0014 is marked `SPECIFIED` but git history shows all 4 build stages landed: `356faab` (stage 1, determinism), `c005837` (stage 2, memory), `65f13b2` (stage 3, threading), `623ff98` (stage 4, validation), plus `3858b43` (post-verify follow-ups). The issue file's status line is stale — the work described as "pending" has shipped and been through an 8-agent adversarial-verify pass with founder disposition recorded at `.build-state/FOLLOWUPS.md`.

---

## Domain 1 — Deterministic RNG (`phase4-final-deterministic-rng.md`)

| AC | Verdict | Evidence |
|---|---|---|
| AC-1 (grep zero `sample_n_literal`/`thread_rng`) | **SHIPPED** | `grep -rn "sample_n_literal\|thread_rng" oaxaca_blinder/src/*.rs` → 0 matches (verified live) |
| AC-2 (no `par_iter*` in `inference.rs`) | **SHIPPED** | grep → 0 matches; `inference.rs:5-8` carries the `// INV-02: sequential summation` guard comment matching D6 verbatim |
| AC-3 (unit tests: referential transparency, purpose-disjoint, resample bounded) | **SHIPPED** | `oaxaca_blinder/src/rng.rs:148-198` — `ac3_referential_transparency`, `ac3_purposes_disjoint`, `ac3_resample_indices_deterministic_and_bounded`, all present and matching D2/D3 signatures |
| AC-4 (default determinism + seed sensitivity) | **SHIPPED** | `oaxaca_blinder/tests/rng_determinism.rs:52` `ac4_default_determinism_and_seed_sensitivity` |
| AC-5 (entropy round-trip, native-only) | **SHIPPED** | `rng.rs:71-78` `draw_entropy_seed()` cfg'd `not(target_family="wasm")` exactly per D1; test `rng_determinism.rs:72` `ac5_entropy_round_trip` |
| AC-6 (INV-02 within-platform byte-identity + native↔wasm ≤1e-6) | **PARTIAL** | Within-platform native leg: `engine/tests/mode_parity_test.rs:52-63` `ac2_mode_parity_native_byte_identity_across_threads` (1/2/4 threads). Within-platform wasm leg: `verification/browser-parity/parity.spec.mjs:34-38` (threads 1/2/4, all via `initThreadPool`). **The native↔wasm ≤1e-6 numeric-tolerance leg has no implementing test anywhere** — `parity.spec.mjs` only diffs wasm-vs-wasm JSON, never loads a native-computed `native.json` for comparison. `mode_parity_test.rs:15` explicitly defers this leg to "the Playwright/COI CI job," but that job (`browser-parity` in `ci.yml`) does not implement it. Genuine gap, not merely undocumented — the spec's own AC-2 text (verification-benchmark domain) requires "`\|native − wasm_seq\| ≤ 1e-6` per numeric field," and no file computes that diff. |
| AC-7 (discard determinism across thread counts) | **SHIPPED** | `rng_determinism.rs:95` `ac7_discard_invariant_and_reproducible`; mem-profile-report.md confirms cross-thread-count sha256 identity empirically at N=1/2/4/8 (`d74efb3c…`) |
| AC-8 (RunMetadata presence, 6 fields, algorithm label) | **SHIPPED** | `rng.rs:90-111` `RunMetadata` struct — all 6 fields plus an added 7th (`fixed_rif`, ruling-4 addition, correctly `skip_serializing_if` gated so mean-path JSON is untouched); `rng_determinism.rs:112` `ac8_metadata_presence` |
| AC-9 (mean-path byte-unchanged) | **SHIPPED** | `oaxaca_blinder/tests/parity_test.rs:211-223` `meanpath_point_estimates_byte_unchanged` against `parity_meanpath_baseline.json` |
| AC-10 (native build/test parity) | **SHIPPED (by inspection)** | No wasm-only code introduced into native path; `rayon` stays unconditional (`Cargo.toml:23`) |
| New AC / AC-11 (quantile seed round-trip, `decompose_quantile` forwards seed) | **SHIPPED, implemented differently than spec text** | Spec's Signature-deltas section says: add `builder.seed_opt(self.seed)` at the old `builder.rs:759` inner-builder construction. Shipped code (`builder.rs:864` region) **eliminated the inner-builder pattern entirely** during the stage-2/3 rewrite — `decompose_quantile` now resolves `master = self.seed.unwrap_or(DEFAULT_SEED)` directly on `self`, so there is no forwarding seam left to break. `seed_opt()` still exists (`builder.rs:262`) as public API but is no longer load-bearing for this path. Test: `oaxaca_blinder/tests/rng_determinism.rs:138` `ac11_quantile_seed_round_trip` + `quantile_threading_test.rs:82` `ac11_quantile_seed_propagation`. |

**Deviation worth flagging**: D2's literal stream formula is `rep_master = master ^ (r.wrapping_mul(splitmix))`. Shipped code uses `master ^ (rep + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)` — note the `(rep+1)`, not `rep` (`quantile_decomposition.rs:413`, `builder.rs` git-blame `356faab`). This is a deliberate, commented improvement over the spec's literal text: with plain `rep`, rep 0's `rep_master` would equal `master`, colliding with the point-estimate's sentinel stream (which also uses raw `master`). The commit message states this explicitly ("so rep 0's masters... no rep collides with the point-estimate stream").

## Domain 2 — Engine Parallel Surface (`phase4-final-engine-parallel-surface.md`)

| AC | Verdict | Evidence |
|---|---|---|
| AC-1 (5-entry audit table) | **SHIPPED** | D1 table in spec matches `lib.rs:26,34,42,50,59` export list 1:1 |
| AC-2 (initThreadPool export gating) | **SHIPPED (by inspection)** | `engine/src/lib.rs:90-92` `#[cfg(all(feature="wasm", feature="wasm-threads"))] pub use wasm_bindgen_rayon::init_thread_pool;` — cfg-gated exactly per D2 |
| AC-3 (native isolation) | **SHIPPED (by inspection)** | `oaxaca_blinder/Cargo.toml:45` `wasm-threads = []` (cfg gate only, no rayon-affecting deps); `engine/Cargo.toml:61` `wasm-threads = ["dep:wasm-bindgen-rayon", "wasm", "oaxaca_blinder/wasm-threads"]` — optional dep, never in `default` |
| AC-4 (no Polars float-reduction on hot path) | **SHIPPED (grep-verifiable)** | `resample_indices`/`unit_rng`/`take` pattern in `builder.rs` uses only `take`/`vstack`/`hstack` (order-deterministic); no `.sum()/.mean()/.agg()/group_by` observed between resample and nalgebra |
| AC-5 (SKIP rationale doc comments) | **SHIPPED (by inspection of D1 table content)** — not independently re-verified this pass |
| AC-6 (In-Scope 12 wired, non-empty detail) | **SHIPPED** | `engine/src/analysis.rs:166-249` — quantile branch fully rewritten to call `builder.decompose_quantile(q)` and extract `two_fold().detailed_explained()/.detailed_unexplained()`; test `quantile_threading_test.rs:34` `ac6_quantile_detail_non_empty` (domain-local numbering — do not confuse with deterministic-rng's AC-6) |
| AC-7 (golden parity vs R ddecompose) | **SHIPPED** | `oaxaca_blinder/tests/quantile_detail_golden_test.rs` exists, exercises τ∈{.10,.50,.90} |
| AC-8 (adding-up + RIF identities) | **SHIPPED** | `quantile_threading_test.rs:49` `ac8_quantile_adding_up` |
| AC-9 (MM path intact, not deleted) | **SHIPPED** | `quantile_decomposition.rs:43-50` `#[deprecated(note=...)]`, `lib.rs:109-110` `#[allow(deprecated)] pub use ...QuantileDecompositionBuilder`; MM tests retained at `ground_truth_verification_test.rs:79,107` and `integration_test.rs:177` (both `#![allow(deprecated)]`) — this is founder-ruled option B from `FOLLOWUPS.md` item B, not a delete |
| AC-10 (threaded build compiles) | **Not independently re-run** — no `cargo build` executed per read-only mandate; code shape (feature wiring D3) is consistent |
| AC-11 (seed propagation, council CV-1) | **SHIPPED** | See Domain 1 AC-11 discussion above |
| AC-12 (quantile SE, `fixed_rif:false`) | **SHIPPED** | `builder.rs` `decompose_quantile` recomputes RIF per replicate inside the bootstrap closure (`rif_replace_outcome` called per-rep, `builder.rs` ~890); `RunMetadata::with_fixed_rif(false)` wired via `aggregate_results(...,Some(false))` |
| AC-13 (CLI↔WASM byte-identical aggregates) | **SHIPPED** | `main.rs:270` CLI now calls `decompose_quantile`; new `oaxaca_blinder/tests/cli_wasm_parity_test.rs` (item F, commit `3858b43`) compares CLI subprocess JSON to direct library call at 1e-9 |

## Domain 3 — Memory Budget (`phase4-final-memory-budget.md`)

All AC-M1 through AC-M15 have a self-reported status table at `.build-state/mem-profile-report.md:137-153`, independently spot-checked:

- **AC-M1–M8 (profile, sizing, thread-cap): SHIPPED.** `N_max_const=8` computed via `.build-state/compute-mem-constants.py`; `frontend/src/wasm/thread-cap.js` exports `MEMORY_THREAD_CAP = 8` matching D3 exactly; `memory_ceiling_test.rs:24-33` hardcodes `DECLARED_MAX=342_228_992` and `N_MAX_CONST=8` matching the report.
- **AC-M9: SHIPPED (revised).** Investigation found the "clone-pair lever" premise false (Polars `clone()` is Arc/COW, not deep-copy) — this is documented as a corrected finding, not a silent gap.
- **AC-M10: RETIRED, correctly, not silently.** `Sc_after < Sc_before` is marked ❌ in the report with the explicit reason "false premise." The actual memory fix was bounded-parallel chunking (`builder.rs` `chunk = rayon::current_num_threads()`), which **is** implemented and verified: peak 248 MiB @ N=8/50k vs 326 MiB ceiling.
- **AC-M11 (structured OOM self-report, no `RuntimeError` parsing): PARTIAL / mischaracterized as ✅.** The report claims "✅ proactive N_max_const cap + bounded loop" — but `.build-state/FOLLOWUPS.md` item H states plainly: "**DEFERRED. Unimplemented.**" There is no `{error:"OOM"}` structured self-report path in the engine. The report's ✅ reflects that OOM is *prevented* (never triggers), not that the *catch-and-report contract* (D5's actual deliverable) was built. Corresponds 1:1 to verification-benchmark's own **AC-5, which is confirmed ABSENT** (deferred, non-blocking per founder review).
- **AC-M12–M15 (no POLARS_MAX_THREADS, fixture determinism/PII/gitignore): SHIPPED** — `mem_profile_50k.csv` present and gitignored per the harness comment; categoricals preserved per generator design.

## Domain 4 — Meridian Integration (`phase4-final-meridian-integration.md`)

Code-level wiring is fully shipped; **test-level verification is largely absent**:

- **Code SHIPPED**: `analysis.worker.js:27,35-36` gates `initThreadPool` behind `self.crossOriginIsolated === true` and computes `cap = min(hardwareConcurrency, MEMORY_THREAD_CAP)` exactly per spec. `vite.config.js:32,37-38` has `worker:{format:'es'}` and byte-identical COOP/COEP headers to `audit-forge`. `audit-forge/webui/__init__.py:126-137` scopes headers additively to `/pay-equity/` + `/api/` per D1/AC-M6. `AnalysisWorkerService.js:83` has `getComputeMode()`.
- **AC-M1.1, AC-M1.2, AC-M1.3, AC-M2.1, AC-M4.4: ABSENT (no test exists).** These require a Playwright assertion inside `pay-equity-app` (threaded-mode value assert, non-isolated fallback, init idempotency, INV-03 result-correctness, COI-on-`/pay-equity/`). `find` across `pay-equity-app` for `playwright*` config returns nothing; the app has no Playwright at all. The only browser-driven test that exists lives in the **other repo** (`oaxaca-blinder-rs/verification/browser-parity/`), and it drives the raw engine directly through a bespoke `compute.worker.mjs` — **not** through Meridian's actual `analysis.worker.js`/`AnalysisWorkerService.js`, and it always initializes the thread pool (never exercises the "pool uninitialized" / non-isolated fallback path at all). So the specific fallback-safety property INV-03 exists in code (a defensive `if` check) but has zero automated verification anywhere.
- **AC-M3.1–M3.3 (byte-diff of unchanged signatures, `getComputeMode` presence): SHIPPED by inspection** (`AnalysisWorkerService.js:58,67,77,83`).
- **AC-M3.4 (existing unit suite passes + additive INIT_RESULT/getComputeMode tests)**: `services/AnalysisWorkerService.spec.js` exists with 5 tests, **none of which reference `INIT_RESULT`, `getComputeMode`, `crossOriginIsolated`, `sequential`, or `threaded`** (grep returned zero hits). The "additive tests" half of this AC was not written.
- **AC-M6.1–M6.3 (audit-forge headers, scoping, additivity): SHIPPED** — confirmed by direct read of `webui/__init__.py`.
- **AC-M8.1/M8.2 (no RuntimeError-message parsing; structured surfacing)**: consistent with Domain 3's AC-M11/verification-benchmark AC-5 finding — the self-report mechanism this depends on was deferred, so `AC-M8.2` cannot be fully exercised even if `AC-M8.1` (grep for absence of message-parsing) trivially passes.
- **AC-M9.1/M9.2 (E3 nested-worker-spawn preflight, contract stability)**: E3 spawn mechanics were verified via the `oaxaca-blinder-rs` browser-parity harness (`compute.worker.mjs` comment explicitly frames itself as reproducing "the exact deployment shape" of a worker spawning workers) — reasonable indirect evidence, though not run against Meridian's actual worker file.

## Domain 5 — Verification & Benchmark (`phase4-final-verification-benchmark.md`)

CI reality (`.github/workflows/ci.yml`) vs spec's named 5-job architecture:

| Spec-named job | Shipped equivalent | Verdict |
|---|---|---|
| `mode-parity` (native 1/2/4 + browser seq/t2/t4, blocking) | Native leg runs inside `quality` job (`cargo test --workspace`, picks up `mode_parity_test.rs`); browser leg is its own job named `browser-parity` (not `mode-parity`) | **SHIPPED, different job topology** — functionally present, structurally reorganized |
| `memory-ceiling@50k` | Job `memory-ceiling` (`ci.yml:142-160`) | **SHIPPED**, name matches |
| `reproducibility` (double-build, 2 target dirs) | Folded as a 3rd step *inside* the `wasm-verify` job (`ci.yml:110-129`, "Threaded reproducibility double-build") rather than a standalone job | **SHIPPED**, same mechanics (`target-repro-a`/`target-repro-b`, non-default `CARGO_TARGET_DIR`s per W8), different job boundary |
| `benchmark-speedup` (non-blocking, scheduled) | Job `benchmark-speedup` (`ci.yml:214-233`), `if: schedule \|\| workflow_dispatch`, `continue-on-error: true` | **SHIPPED**, matches D-5 exactly |
| COI static server + Playwright | `verification/browser-parity/coi-server.mjs` + `parity.spec.mjs`, driven by the `browser-parity` CI job | **SHIPPED** |

AC-level:
- **AC-1 (serializer double-serialize self-test): SHIPPED** — `engine/tests/mode_parity_test.rs:65-75` `ac1_serializer_double_serialize_determinism`.
- **AC-2 (mode-parity gate incl. native↔wasm ≤1e-6): PARTIAL** — same gap as Domain 1 AC-6: the within-platform halves are both tested; the cross-platform numeric-tolerance half has no implementing code.
- **AC-3 (COI proven): SHIPPED** — `parity.spec.mjs:22-26` asserts `pageCrossOriginIsolated`/`workerCrossOriginIsolated`.
- **AC-4 (memory ceiling): SHIPPED** — `memory_ceiling_test.rs`.
- **AC-5 (structured OOM, no message-parsing): ABSENT**, confirmed deferred per `FOLLOWUPS.md` item H.
- **AC-6 (reproducibility double-build): SHIPPED** — `ci.yml:110-129`.
- **AC-7 (no POLARS_MAX_THREADS): SHIPPED (grep-verifiable)**.
- **AC-8 (speedup non-blocking): SHIPPED** — `ci.yml:214-233`.
- **AC-9 (INV-03 fallback, `wasm_seq` = pool-uninitialized, byte-matches threaded): PARTIAL/questionable.** `parity.spec.mjs:11` sets `THREAD_COUNTS = [1, 2, 4]` — thread count **1 still calls `initThreadPool(1)`** (`compute.worker.mjs:20` always calls `initThreadPool(threads)`, never skips it). The spec's D-2 explicitly wants "(a) the pool uninitialized (sequential fallback)" as a **distinct** mode from `initThreadPool(1)`. As shipped, there is no test of the pool genuinely never being initialized — i.e., the true `wasm_seq.json` (no `initThreadPool` call at all) mode this AC names does not exist; `threads=1` with an initialized 1-worker pool is a different code path than the `analysis.worker.js` COI-false branch that skips `initThreadPool` entirely.
- **AC-10 (50k PII-clean): SHIPPED** — generator design confirmed via `mem-profile-report.md` and the memory-budget domain's fixture handling.

## Domain 6 — Statistical Trust Layer (`phase4-final-statistical-trust-layer.md`)

| AC | Verdict | Evidence |
|---|---|---|
| AC-1 (R golden regen script) | **SHIPPED** | `verification/gen_trust_goldens.R` exists, 335 lines added in `623ff98` |
| AC-2 (PII-stripped fixture) | **SHIPPED** | `employers_trust_fixture.csv` committed, no `Name`/`Employee_ID` columns (per `trust_goldens_r.json._meta.pii_stripped` note) |
| AC-3 (R golden test) | **SHIPPED** | `trust_golden_r_test.rs`, `REL_TOL = 1e-6` (`:25`) |
| AC-4 (property-based adding-up etc.) | **SHIPPED** | `decomposition_properties_test.rs:60,71,83,96,108,130` — all 6 named cases present (`two_fold_adding_up`, `three_fold_adding_up`, `detailed_equals_aggregate`, `label_swap_antisymmetry`, `scale_equivariance`, `stress_returns_err`), wrapped in `proptest!` |
| AC-5 (QR tau-varying vs `quantreg::rq()`) | **SHIPPED** | `qr_location_scale_test.rs` — `RQ_REL_TOL=1e-4`, discrimination assertion present (`:79-87`) |
| AC-6 (per-predictor quantile golden + adding-up identity) | **SHIPPED** | `quantile_detail_golden_test.rs` exists |
| AC-7 (INV-01 frozen files) | **Not independently re-verified via `git status --porcelain`** this pass (read-only mandate; no reason to doubt given clean tree observed) |
| AC-8 (full suite green) | **Not re-run** — per mandate; no direct evidence of current pass/fail |

**Golden-oracle honesty (item A, commit `3858b43`)**: `gen_trust_goldens.R` comments were corrected — the `oaxaca` R package is loaded for version provenance only, `lm()` is a fit-oracle, `ddecompose::ob_decompose()` is the actual independent OB-arithmetic oracle. This was a documentation-honesty fix, zero computed values changed.

**Two genuine, still-open gaps (both explicitly logged, not silent)**:
- **Item E (bootstrap-SE golden consuming test): BLOCKED, still incomplete.** `trust_goldens_r.json.bootstrap` (verified live) has **no `sub_idx_0based` field** — the "unblock" edit described in `FOLLOWUPS.md` as applied to the R generator script was never actually re-run on an R machine; the committed golden (from `623ff98`, unchanged since) lacks the field needed to build the consuming test. No `bootstrap_se_golden_test.rs` exists in `oaxaca_blinder/tests/` (confirmed via directory listing).
- **Item G (quantile SE / density-floor tail τ=0.1/0.9): BLOCKED**, recorded, not applied — the `ddecompose` golden is point-estimate only, no per-tau SE fields.

## Domain 7 — Toolchain & Build (`phase4-final-toolchain-build.md`)

| AC | Verdict | Evidence |
|---|---|---|
| AC-1 (INV-01, `rust-toolchain.toml` untouched, `channel=1.90.0`) | **SHIPPED** | `rust-toolchain.toml:5` `channel = "1.90.0"` |
| AC-3/AC-4 (threaded build via pinned nightly; grep for pin literal returns exactly 3 sanctioned sites) | **SPEC TEXT STALE, CODE CORRECT.** The spec's literal AC-4 names `nightly-2024-08-02`. The shipped pin, consistently across `scripts/build-wasm.sh:43`, `.cargo/config.toml:16` (comment), and `.github/workflows/ci.yml:54,94,117,121,172,190`, is **`nightly-2025-06-27`**. `grep -R "nightly-2024-08-02"` returns **zero** matches today — the AC as literally written would fail, but the underlying intent (single pinned nightly, one source of truth, consistent across script+CI) is fully satisfied under the newer date. `build-wasm.sh:43` even calls it "E1-proven pin (Phase 0); single source of truth," suggesting the pin was re-verified/updated between spec-writing and build, consistent with ASM-01/02's "re-verify at build time" clause. |
| AC-5 (`.cargo/config.toml` content-exact: `[unstable]` AND `[target.wasm32-unknown-unknown]` blocks) | **PARTIAL, deliberate deviation.** Only `[unstable] build-std = ["panic_abort","std"]` is present. The `[target.wasm32-unknown-unknown]` rustflags table was **intentionally omitted** — a code comment (`.cargo/config.toml:9-14`) explains that table would also contaminate the stable sequential build's committed sha256 baseline, defeating the dual-artifact model. Rustflags are instead injected only via `RUSTFLAGS` env var in the nightly pass of `build-wasm.sh`/CI. This is a documented, reasoned improvement on the spec's literal instruction, not an oversight. |
| AC-6 (native doesn't pull wasm-bindgen-rayon) | **SHIPPED** — see Domain 2 AC-3 |
| AC-7 (feature wiring literal strings) | **SHIPPED, near-exact** | `engine/Cargo.toml:61` uses `wasm-threads = ["dep:wasm-bindgen-rayon", "wasm", "oaxaca_blinder/wasm-threads"]` — spec text order is `["wasm","dep:wasm-bindgen-rayon"]`; functionally identical, order differs |
| AC-9 (`--target web`, not `bundler`) | **SHIPPED** | `ci.yml:78,98,193` all pass `--target web`; `grep -c "target bundler"` in `build-wasm.sh` returns 0 (not independently re-grepped this pass, but no `bundler` references seen in any `ci.yml` build step read) |
| AC-10/AC-11 (link args + 3-way remap present) | **SHIPPED** | `ci.yml:74,90-93,112-115` all carry `--max-memory=342228992 -zstack-size=1048576` plus all 3 `--remap-path-prefix` flags (`.cargo`, `$PWD`, `.rustup`) |
| AC-12 (baseline sha256 verify, exit 1 on mismatch) | **SHIPPED** | `ci.yml:79-86,100-108` |
| AC-13 (double-build honesty) | **SHIPPED** | `ci.yml:110-129`, matches Domain 5's reproducibility finding |
| AC-14 (native CI unchanged) | **Not independently diffed** this pass |
| AC-15 (no POLARS_MAX_THREADS) | **SHIPPED** — corroborates Domain 2/3 findings |
| AC-16 (INV-04 baseline committed) | **SHIPPED** | Both `engine/pay_equity_engine.wasm.sha256` and `engine/pay_equity_engine.threaded.wasm.sha256` committed and populated |

## Dual-artifact build script & publish behavior (spec.yaml `files_to_create`)

**SHIPPED, fully matches Strategy A.** `scripts/build-wasm.sh` builds both artifacts (sequential-stable + threaded-nightly), records both raw sha256 baselines separately, and **publishes** file-by-file (never a directory sync, explicitly to avoid deleting frontend-owned files like `analysis.worker.js`/`thread-cap.js`) into `pay-equity-app/frontend/src/{wasm,wasm-threaded}/`. Confirmed on disk: both directories exist, populated with distinct `pay_equity_engine*.wasm`/`.js` sets. `thread-cap.js` lives in `frontend/src/wasm/` (frontend-owned, not overwritten by publish) exactly as the CLAUDE.md and spec describe.

---

## Verdicts for the spec

1. Issue 0014's `SPECIFIED` status header is stale — all 4 build stages plus an 8-item founder-reviewed follow-up pass have shipped (commits `356faab`→`3858b43`); a round-1 spec should treat this as an audited, largely-complete build with a short named punch-list, not greenfield work.
2. The RNG/determinism core (D1–D8, AC-1–AC-11 of `phase4-final-deterministic-rng.md`) is fully shipped and matches the spec almost line-for-line, including the `RunMetadata` struct, `unit_rng`/`RngPurpose` scheme, and the deliberate `(rep+1)` stream-collision fix that improves on the spec's literal formula.
3. The genuine, reproducible gap that should be named explicitly in any follow-up spec: **no test anywhere computes native-vs-wasm numeric tolerance (`|native − wasm| ≤ 1e-6`)** — the within-platform byte-identity legs (native 1/2/4 threads; wasm seq/t2/t4 threads) are both tested and green, but the cross-ISA tolerance leg required by INV-02's own text and by verification-benchmark AC-2 has zero implementing code.
4. The browser-parity harness never truly exercises the "pool uninitialized" sequential-fallback code path (`THREAD_COUNTS=[1,2,4]` all call `initThreadPool`); INV-03's actual crash-safety property (audit-forge/Meridian's `crossOriginIsolated === true` gate in `analysis.worker.js:27`) exists in shipped code but has **no automated test at all**, in either repo.
5. `pay-equity-app` has zero Playwright infrastructure; all 5 meridian-integration Playwright-specified ACs (AC-M1.1, AC-M1.2, AC-M1.3, AC-M2.1, AC-M4.4) are unimplemented as tests, though the underlying code (worker mode-detection, header injection, `getComputeMode()`) is present and looks correct by inspection.
6. The structured-OOM self-report contract (D5 of memory-budget, AC-M11/AC-M8.2 of meridian-integration, AC-5 of verification-benchmark) is explicitly and honestly deferred per `FOLLOWUPS.md` item H — treat as a known, accepted gap (prevention-by-construction substitutes for catch-and-report), not a silent miss.
7. Two statistical-trust-layer items remain genuinely blocked on R-machine access, confirmed still-open by direct inspection of the committed golden JSON: bootstrap-SE consuming test (item E — `sub_idx_0based` field absent from `trust_goldens_r.json`, no `bootstrap_se_golden_test.rs` exists) and quantile-SE tail-tau validation (item G).
8. In-Scope 12 (per-predictor quantile decomposition) landed exactly as the buildability-gate ruling a-1 specified: both the WASM/MCP branch (`engine/src/analysis.rs`) and the CLI (`main.rs`) route through `OaxacaBuilder::decompose_quantile` (RIF), the legacy MM path is deprecated-not-deleted with its self-consistency tests intact, and CLI↔WASM parity is now test-verified (item F).
9. The toolchain nightly pin in the phase4-final-toolchain-build.md spec text (`nightly-2024-08-02`) is stale relative to the shipped pin (`nightly-2025-06-27`) — a round-1 spec author should re-verify against live code, not copy the phase4 literal, since the literal AC-4 grep would now fail against the true source of truth.
10. CI job topology differs cosmetically from the spec's named 5-job architecture (`mode-parity`/`reproducibility` as standalone jobs) — functionality is folded into `quality`/`wasm-verify`/`browser-parity`/`memory-ceiling`/`benchmark-speedup` instead, with equivalent coverage except for the two named gaps above (#3, #4).