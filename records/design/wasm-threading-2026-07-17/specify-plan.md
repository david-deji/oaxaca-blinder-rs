# Orchestration Plan: WASM Rayon Multithreading for pay-equity-engine

> Created: 2026-07-17
> Tier: balanced
> Output: apps/hr-apps/oaxaca-blinder-rs/_specify-wasm-rayon-multithreading-2026-07-17/
> Estimated agents: ~10 Phase-1 research units (orchestrator-direct Perplexity), 4 Phase-2 spec writers (opus), ~12 Phase-3 units (sonnet), 4 Phase-4 writers (opus), optional 7-seat council

```yaml
# Workflow State
deliverable_type: app
phase: completed
completed:
  - phase-0-brief
  - plan-confirmation-gate
  - phase-1-research   # gate 19/0 pass
  - phase-1.5-citation-verifier   # fix-in-place, 0/12 post-fix
  - council-gate-post-phase-1     # no council
  - phase-2-spec-drafts           # 7 drafts, gate 16/0, drift 0 findings
  - phase-2.5-gap-extraction      # phase3-research-brief.md
  - council-gate-post-phase-2     # founder: NO COUNCIL. Plus In-Scope 12 ruling: FULL per-predictor
                                  # quantile math (scope expansion APPROVED, Charter Scope-Delta logged,
                                  # anti-goal carved out, W10 research unit added, H1 resolved)
  - phase-3-gap-fill-research   # 6 LOCAL (L1-L6) + 10 WEB (W1-W10) all resolved
  - phase-3-quality-gate        # 4/0 PASS (files renamed to embed slugs; brief E/H/DEP demoted to h4;
                                # provenance URLs added. Friction logged: gate assumes 1-file-per-gap + web URLs)
  - phase-3.5-citation-verifier # PASS 3/3 verified, 0 mismatch (<10%). W10 academic anchors confirmed
                                # verbatim: FFL-2018 Econometrics 6(2):28 DOI 10.3390/econometrics6020028;
                                # Rios-Avila 2020 Stata Journal 20(1):51-94 oaxaca_rif; ddecompose ob_decompose() sig captured.
  - phase-4-refined-specs       # 5 opus writers → 7 phase4-final-*.md; quality gate 22/0
  - phase-4.5-reconciliation-and-gates   # SaaS-creep clean, drift clean, no [CLARIFY]
  - phase-4-build-readiness-audit         # Sonnet, VERDICT: CRITICAL-PRESENT (2 CRITICAL, 2 MINOR)
  - phase-4-critical-fixes                # BOTH CRITICALs FIXED + 2 MINOR citations. gate re-run 22/0.
                                          # C1: In-Scope 12 = WIRE existing decompose_quantile (builder.rs:720,
                                          #     already does RIF per-predictor detail via run()), NOT new math.
                                          #     Surfaces founder decision: MM->RIF quantile aggregate (client-facing).
                                          # C2: thread-cap.js single-owner (memory-budget=value, meridian M7=file).
  - phase-4.6-mandatory-adversarial-council   # 5 opus seats, ALL SHIP_WITH_FIXES (no BLOCK).
                                              # council-synthesis.md + 5 council-*.md written.
  - phase-4.7-council-fixes                    # ALL code-grounded fixes applied, gate 22/0:
                                              #  CV-1 seed-propagation seam (decompose_quantile must forward self.seed) - TOP FIX
                                              #  CV-2 adding-up re-pointed at RIF aggregate + CLI-divergence surfaced
                                              #  MJ-3 golden tolerance -> measured-then-pinned + track-2 custom-R
                                              #  MJ-5 decompose_quantile fixed-RIF SE disclosure + AC
                                              #  MJ-6 thread-cap 8-clamp baked into N_max_const
                                              #  MJ-4 E1 nightly subgraph-trim (default-features=false, cfg-gate askama)
                                              #  entropy scoped native; MJ-1 native<->wasm split annotated (pending founder)
  - buildability-gate                          # 4 founder rulings (2026-07-18), recorded in buildability-gate-rulings.md:
                                              #   1. Reproducibility = Strategy A (dual artifact; container baseline)
                                              #   2. INV-02 = the split (within-platform byte-identical + native<->wasm <=1e-6)
                                              #   3. In-Scope 12 aggregate = a-1 (both surfaces -> RIF)
                                              #   4. Quantile SE = recompute RIF per bootstrap replicate (fixed_rif:false)
                                              #   H2 memory margins: decide-and-proceed (band defaults; revisit at profile).
                                              #   ALL 4 baked into Charter (INV-02 reframe + 3 Scope-Delta rows) + 7 phase4-final specs.
                                              #   Phase 4 gate re-passed 22/0 after all edits.
  - phase-4.8-saas-creep-scan                  # clean (AI-native N/A: Rust engine + browser plumbing, no SaaS creep)
  - phase-4.9-final-synthesis                  # synthesis.md (build handoff, required preamble present; canonical Bundle-C name)
  - phase-4.9-distillation                     # distilled.md (<=200w) + tldr.md (<=50w) + quickstart.md (app-type) + spec.yaml (machine contract)
                                              # NOTE canonical names synthesis.md/distilled.md (NOT final-synthesis*): directory-contract
                                              # check + /build Phase 0.1 both read canonical; /specify SKILL Handoff prose names final-synthesis* (drift, friction-logged)
  - phase-4.10-final-synthesis-review-gate     # APPROVED (A) by founder 2026-07-18. Handoff accepted.
  - phase-4.11-telemetry-emit                  # specify-telemetry.json written (INV-21 non-blocking)
  - phase-4.11-knowledge-registration          # 0014-MERIDIAN issue -> SPECIFIED, spec dir linked
current: completed  # /specify pipeline DONE. /build is a separate founder-triggered invocation — NOT auto-started.
# FOUR founder decisions (council-sharpened), presenting as AskUserQuestion:
#  1. Strategy A vs B (council: A more competitive than spec claimed; B needs container/2-host baseline)
#  2. INV-02 reframe (within-platform byte-identical + native<->wasm tolerance) - touches "bit-identical" ruling
#  3. In-Scope 12 aggregate: (a-1) MM->RIF both surfaces / (a-2) accept+doc CLI divergence / (c) keep MM+expose both
#  4. In-Scope 12 SE rigor: fixed-RIF+disclose vs recompute-RIF-per-replicate (client-facing defensibility)
# H2 memory margins: DECIDE-AND-PROCEED (low-stakes, memory non-binding at 50k; band defaults, revisit at profile).
# ultracode ON (xhigh + workflow orchestration). blast_radius client-facing => council floor.
# Running post-reconciliation adversarial council as a Workflow over the CORRECTED specs.
# FOUNDER GATE PENDING (NOT answered — /effort + task-notif are NOT user input): Strategy B ratify,
#   H2 memory margins, In-Scope 12 MM->RIF aggregate decision (NEW, from C1 fix).
# RECONCILIATION WIN (build-safety.md): math/rif.rs:14 calculate_rif ALREADY implements the
# exact unconditional-quantile RIF form In-Scope 12 needs (Q_tau + (tau - I(y<=Q_tau))/f, own
# Silverman KDE). math/kde.rs:20, math/normalization.rs:5 (G-U norm), math/ols.rs, math/
# quantile_regression.rs all exist. In-Scope 12 is WIRING existing primitives, not new math →
# de-risks the scope expansion (writer's OI-1/MAJOR risk resolved safe). Engine-parallel-surface
# spec build-step 5 is "reuse calculate_rif" not "implement rif_quantile".
# Pending: build-readiness audit (dispatched), SaaS-creep scan, drift/[CLARIFY] scan,
# mandatory post-recon council (blast_radius client-facing floor — surface at gate), buildability gate.
# Phase 3 COMPLETE. Outputs: phase3-local-findings.md, phase3-web-findings.md.
# Six Phase-4 corrections queued (see phase3-web-findings.md § Phase 4 corrections):
#  1. W4 worker OOM: budget-prevention + structured self-report, not RuntimeError parsing
#  2. W5 mean golden: bespoke manual-loop R script, not oaxaca(R=)
#  3. W7 threaded browser CI: custom COOP/COEP server + Playwright, not wasm-pack test
#  4. W8 double-build: two non-default CARGO_TARGET_DIRs (or disable rust-cache)
#  5. L4 drop POLARS_MAX_THREADS=1 (no-op on wasm); reframe one-parallel-layer via polars wasm stub
#  6. W10 In-Scope 12: one-stage RIF-OLS detail (reuse mean OB detail + G-U norm), golden=R ddecompose,
#     MM stays aggregate-only, two-stage reweighting = documented v2
# Key DECISIONS locked: worker.format:'es' REQUIRED (W3); page-level COOP/COEP sufficient, no per-worker (W2);
#   clarabel thread-safe default build (L5); take() bounds-checked gather (L1); rand_chacha 0.3 direct dep (L2).
# phase-2 COMPLETE: 7 drafts, quality gate 16/0 (after mechanical header/provenance normalization),
# drift check 2.4: ZERO findings (report: phase2-drift-check.md), no [CLARIFY] markers.
# phase-2.5 COMPLETE: phase3-research-brief.md written — 6 LOCAL + 9 WEB + 3 EXPERIMENT(→build preflight) + 2 HUMAN + dependents.
# Toolchain recommendation on table: Strategy B (single threaded artifact). Founder ratifies at buildability gate.
# Phase 2 DISPATCHED (2026-07-17): 5 opus document-analyst writers, 7 expected outputs:
#   A → phase2-spec-toolchain-build.md
#   B → phase2-spec-deterministic-rng.md + phase2-spec-engine-parallel-surface.md
#   C → phase2-spec-memory-budget.md
#   D → phase2-spec-meridian-integration.md
#   E → phase2-spec-statistical-trust-layer.md + phase2-spec-verification-benchmark.md
# Inputs per writer: own phase1 files + phase2-cross-domain-summary.md + spec-charter.md + Founder Intake + code anchors.
# On resume: check disk for the 7 files; re-dispatch only missing ones (GUPP).
# council-gate-post-phase-1: founder chose NO COUNCIL (2026-07-17)
# phase-1.5-citation-verifier: FAILED mechanically (2/12 = 16.7%), founder chose FIX-IN-PLACE:
#   tlsn issue 524 citation excised from u1 (claim stands on crate README, verbatim-confirmed);
#   sinanpl/OaxacaBlinder excised from u9 Sources (claim stands on CRAN, verbatim-confirmed).
#   Post-fix effective mismatch: 0/12 relevant. Proceeding to Phase 2.
side_effects:
  files_written:
    - _specify-wasm-rayon-multithreading-2026-07-17/specify-plan.md
    - _specify-wasm-rayon-multithreading-2026-07-17/spec-charter.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-toolchain-build-nightly-reproducibility.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-toolchain-build-dual-artifact-patterns.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-memory-budget-shared-memory-limits.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-memory-budget-polars-allocator.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-deterministic-rng-parallel-seeding.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-engine-parallel-surface-clarabel-nalgebra.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-meridian-integration-vite-worker.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-statistical-trust-layer-golden-proptest.md
    - _specify-wasm-rayon-multithreading-2026-07-17/phase1-verification-benchmark-browser-ci.md
  issues_filed: [0014-MERIDIAN, 0015-MERIDIAN]
safe_to_retry: true
next: founder-synthesis-review-gate -> (on approve) telemetry-emit + knowledge-registration -> /build handoff
started_at: 2026-07-17T00:00:00Z
updated: 2026-07-18T00:00:00Z
phase_durations:
  phase-1-research: 1
retry_counts:
  phase-1-research: 0
# Phase 1: 10 logical units in 9 files (u5+u6 merged into phase1-deterministic-rng-parallel-seeding.md).
# Key Phase-3 gaps flagged in files: nested-worker initThreadPool semantics (u8, TOP),
# clarabel internals local inspection (u7), R/Stata primary docs (u5/6), Vite prod worker config (u8),
# COOP inheritance by workers (u8), wasm-bindgen-test-runner COI support (u10),
# existing gen_parity_golden.py extend-vs-replace (u9), MM golden R routine (u9).
```

## Deliverable Type

Type: app
Rationale: deployable code change across a Rust workspace (oaxaca-blinder-rs) + browser app plumbing (Meridian frontend, audit-forge Flask server). No skill/rule/pipeline signals.

## Prior Research Context

No formal /research output files ingested (Research Inventory hard-fail N/A — recon was live code-reading + one cited Perplexity call this session). Session recon findings, all code-verified:

- Rayon 1.11 is an unconditional dep of `oaxaca_blinder`; call sites: `builder.rs:826` (bootstrap), `quantile_decomposition.rs:222,227,338` (per-tau QR + MM simulation). Native consumers already parallel; WASM runs rayon's built-in sequential fallback.
- WASM surface (`engine/src/lib.rs`): decompose / optimize / verify_adjustments / calculate_efficient_frontier / check_defensibility. `decompose` defaults to 100 bootstrap reps (`engine/src/analysis.rs:147`) — the hot path.
- Toolchain (verified via Perplexity 2026-07-17, sources: github.com/RReverser/wasm-bindgen-rayon README, docs.rs, MDN, web.dev): threads on wasm32-unknown-unknown still require pinned nightly + `build-std` with `+atomics,+bulk-memory`; `wasm-bindgen-rayon` latest = 1.3.0, compatible with wasm-bindgen 0.2.x (repo pins 0.2.106); COOP `same-origin` + COEP `require-corp`/`credentialless` still required for SharedArrayBuffer; rayon sequential fallback confirmed (rayon-rs/rayon#1122).
- Reproducibility model: `rust-toolchain.toml` pins stable 1.90.0 (Track-0 SC-04); raw wasm sha256 baseline at `engine/pay_equity_engine.wasm.sha256`, verified by `.github/workflows/ci.yml` wasm-verify job; `scripts/build-wasm.sh` builds with `--target bundler`, wasm-bindgen-cli 0.2.106 pinned, wasm-bindgen output documented nondeterministic (pkg/ not hashed).
- Sole consumer: Meridian (`apps/hr-apps/pay-equity-app`) — Vue 3 client-side, engine in one Web Worker (`frontend/src/wasm/analysis.worker.js`), Vite + vite-plugin-wasm, base `/pay-equity/`.
- Serving surface (verified on disk): audit-forge Flask app at `/home/deji/telos/audit-forge` (loopback-only, port 5137, no-egress test suite). `webui/__init__.py:125` `_apply_meridian_csp` after_request hook already scopes headers to `/pay-equity/` + `/api/` — COOP/COEP insertion point. Loopback = secure context; no cross-origin subresources by design → `require-corp` safe.

Open decisions carried into pipeline: (1) nightly re-pin vs dual-artifact reproducibility strategy — pipeline researches, founder approves at buildability gate; (2) deterministic parallel RNG design; (3) memory ceiling at 50k rows.

## Founder Intake

- **Toolchain/reproducibility decision**: "Let pipeline decide" — research both nightly-re-pin and dual-artifact; spec proposes one with rationale; founder approves at buildability gate.
- **Production environment**: Meridian runs locally on David's corporate PC for day-job use, served by the audit-forge Flask offline bundle from `/home/deji/telos/audit-forge`. No public deployment in scope.
- **Success criterion**: **Full-surface parity** — not just decompose bootstrap; audit and parallelize (where profitable) all five WASM entry points: decompose, optimize, verify_adjustments, calculate_efficient_frontier, check_defensibility.
- **Scope**: Engine + Meridian wiring in one spec (worker initThreadPool, Vite headers, audit-forge Flask headers).
- **Data scale**: **~50,000 employee rows in scope** (founder revised down from an initial 130k estimate). **"Memory is of utmost importance"** (direct quote). Memory budget is a first-class constraint, not a nice-to-have. Threaded wasm requires a fixed shared-memory maximum — sizing analysis mandatory.
- **Determinism**: **Bit-identical required** across modes — same input → same numbers regardless of thread count. Requires schedule-independent per-rep seeded RNG streams.
- **Launch path**: Flask offline bundle (headers go in audit-forge `_apply_meridian_csp` region). Vite dev server headers also in scope for development.

## Domains

- **toolchain-build** — nightly pin + build-std + atomics flags, wasm-bindgen-rayon 1.3.0 integration, build-wasm.sh + CI rework, reproducibility strategy (re-pin vs dual-artifact) with recommendation.
- **memory-budget** — 50k-row memory profile (Polars frames, nalgebra matrices, bootstrap copies), shared linear memory maximum + per-thread stacks, thread-count vs memory tradeoff, allocator behavior under threads.
- **deterministic-rng** — audit current rand usage in bootstrap/MM simulation; design schedule-independent per-rep seeding (bit-identical threaded vs sequential vs native); reproducibility test design.
- **engine-parallel-surface** — full-surface audit of the 5 WASM entry points: what parallelizes profitably (bootstrap reps, per-tau QR, MM simulation, frontier grid, clarabel solves), thread-pool init API export, feature-gating so native builds are untouched.
- **meridian-integration** — analysis.worker.js initThreadPool + feature-detect fallback, Vite dev/build config + headers, audit-forge COOP/COEP insertion (verified hook at `webui/__init__.py:125`), pkg/ artifact refresh flow, fallback UX signal.
- **verification-benchmark** — threaded-vs-sequential parity tests (bit-identical gate), 50k-row memory ceiling test (seeded from Employers_data.csv ×5 replication), wall-clock benchmark harness, CI wasm-verify job updates, sha256 baseline handling per chosen strategy.
- **statistical-trust-layer** — golden-file parity vs R `oaxaca`/Stata at realistic n (Employers_data.csv), property-based adding-up identity tests, heteroskedastic/skewed QR cases, quantile detailed-components API exposure (analysis.rs:201 gap).

## Build Order & Pre-Gate Findings

Build-order constraint (founder-directed): determinism fix (domain 3) → memory profile of current build (domain 2 prerequisite deliverable) → threading (domains 1,4,5) → validation/benchmark suites (domains 6,7). The spec must sequence its acceptance criteria accordingly.

Verified pre-gate findings (founder review items, code-confirmed 2026-07-17):
- Bootstrap not run-to-run reproducible today: `sample_n_literal(..., None)` builder.rs:832 (None = seed), `thread_rng()` quantile_decomposition.rs:215,244.
- Silent rep discard: builder.rs:827 `filter_map(.ok())` — failed reps dropped with only a stderr warning; rep count nondeterministic.
- No memory profiling artifact exists anywhere in the repo.
- QR unit tests tau-insensitive (linear data); integration tests n=12–20, reps 2–5.
- Meridian coverage thresholds 30/25/25/30 (vite.config.js) — compiler.js coverage deferred to separate MERIDIAN issue.
- Engine is f64 throughout (engine/src/types.rs) vs monorepo Decimal(18,2) monetary rule — founder ruling pending at plan gate.
- Test fixture directive: /home/deji/Downloads/Employers_data.csv (10,001 lines; Gender/Salary + 6 predictors) — seed for realistic-n and 50k-row tests.

## Deliverable Shape

Files to modify (engine repo `apps/hr-apps/oaxaca-blinder-rs/`):
| File | Change |
|---|---|
| `rust-toolchain.toml` | strategy-dependent: nightly re-pin OR untouched (dual-artifact adds a second toolchain file) |
| `.cargo/config.toml` | NEW — wasm target rustflags + build-std (possibly scoped per-profile/strategy) |
| `oaxaca_blinder/Cargo.toml` | wasm-thread feature gate; rayon stays unconditional for native |
| `oaxaca_blinder/src/builder.rs`, `src/quantile_decomposition.rs` | seeded per-rep RNG streams (bit-identical requirement) |
| `engine/Cargo.toml` | `wasm-bindgen-rayon = 1.3.0` behind `wasm-threads` feature |
| `engine/src/lib.rs` | export `init_thread_pool` (re-export from wasm-bindgen-rayon) |
| `scripts/build-wasm.sh` | threaded build path, `--max-memory` link arg, artifact strategy |
| `.github/workflows/ci.yml` | wasm-verify job: new toolchain steps, baseline handling |
| `engine/pay_equity_engine.wasm.sha256` | re-baseline or dual-baseline per strategy |

Files to modify (Meridian `apps/hr-apps/pay-equity-app/`):
| File | Change |
|---|---|
| `frontend/src/wasm/analysis.worker.js` | crossOriginIsolated feature-detect + initThreadPool before first compute |
| `frontend/vite.config.js` | `server.headers` COOP/COEP for dev |
| `frontend/src/wasm/pkg files` | regenerated engine artifact(s) |

Files to modify (audit-forge `/home/deji/telos/audit-forge/` — separate repo, coordinate):
| File | Change |
|---|---|
| `webui/__init__.py` | COOP/COEP on `/pay-equity/` responses beside existing `_apply_meridian_csp` (line 125) |

Component graph:
```
audit-forge Flask (COOP/COEP) ──serves──> Meridian frontend
Meridian analysis.worker.js ──initThreadPool──> pay_equity_engine.wasm (threaded)
pay_equity_engine ──wasm-threads feature──> oaxaca_blinder (rayon + seeded RNG)
scripts/build-wasm.sh ──produces──> pkg/ artifact(s) + sha256 baseline(s)
ci.yml wasm-verify ──verifies──> baseline(s)
```

Trigger surface: `bash scripts/build-wasm.sh` (build); worker postMessage API unchanged (no Meridian store/UI changes).

## Council Perspective Selection (Phase 0.3 pre-selection)

Balanced default 7 (adaptive selection re-runs at Phase 3 per Bundle C; static fallback = this roster). Substitutions from default:
- systems-architect (kept)
- end-user-advocate (kept — David as day-job operator: fallback UX, failure legibility)
- domain-expert → **WASM/browser-platform expert** (SharedArrayBuffer, managed-browser policy, memory growth)
- product-visionary → **memory/performance engineer** (50k rows, "memory of utmost importance")
- founder-proxy (kept)
- pragmatic-integration-specialist (kept)
- integration-strategist → **statistical-correctness auditor** (bit-identical bootstrap, defensibility of numbers)
Plus mandatory: NBJ thinker + Adversarial Verifier (per council.md; blast-radius floor makes post-reconciliation council MANDATORY, see Charter).

## Scope Baseline (Phase 0.5b)

Snapshot of Charter In-Scope at plan CONFIRMATION (2026-07-17, after founder pre-gate review — 13 items; this is the scope-delta baseline):
1. Threaded WASM build path for pay-equity-engine (wasm-bindgen-rayon 1.3.0)
2. Reproducibility strategy decision + implementation (re-pin OR dual-artifact)
3. Bit-identical seeded RNG refactor of bootstrap/MM paths
4. Full-surface parallelization audit of 5 WASM entry points (implement where profitable)
5. Memory budget + shared-memory maximum sized for 50k rows
6. Meridian worker initThreadPool + feature-detect fallback
7. COOP/COEP on audit-forge `/pay-equity/` + Vite dev server
8. Parity/benchmark/memory-ceiling verification + CI updates
9. Statistical trust layer (golden vs R/Stata, proptest identities, heteroskedastic QR)
10. Determinism as prerequisite + standalone feature (incl. silent rep-discard fix)
11. Pre-threading memory profile of current single-threaded build at 50k rows
12. Quantile detailed-components API exposure (analysis.rs:201)
13. Test fixture: Employers_data.csv seed (×5 replication strategy)

## Phase Plan

### Phase 1: ~10 research units (sonnet, orchestrator-direct Perplexity) — toolchain-basics unit SKIPPED (covered by session recon, cited above, dated 2026-07-17)
### Phase 2: 4 spec writers (opus) — A: toolchain-build; B: engine-parallel-surface + deterministic-rng; C: memory-budget + verification-benchmark; D: meridian-integration
### Phase 2.5: orchestrator gap extraction
### Council after Phase 1: PENDING (founder gate)
### Phase 3: ~12 units (sonnet) — from Phase 2 gap briefs
### Phase 4: 4 spec writers (opus) — same grouping, refined
### Council after Phase 2: PENDING (founder gate)
### Reconciliation
### Council after Reconciliation: MANDATORY (blast-radius floor: production)

## Status

- [ ] Issue filed (MERIDIAN prefix, pay-equity-app/issues/) — before Phase 1 dispatch
- [ ] Plan confirmed by founder
- [ ] Phase 1 complete
- [ ] Council after Phase 1
- [ ] Phase 2 dispatched
- [ ] Phase 2 complete
- [ ] Phase 2.4 drift check
- [ ] Phase 2.5 complete
- [ ] Council after Phase 2
- [ ] Phase 3 dispatched
- [ ] Phase 3 complete
- [ ] Phase 4 dispatched
- [ ] Phase 4 complete
- [ ] Phase 4.5 drift check
- [ ] Build-readiness audit
- [ ] Reconciliation complete
- [ ] Council after Reconciliation (MANDATORY)
- [ ] Buildability gate passed
- [ ] SaaS-creep scan
- [ ] Final synthesis complete
- [ ] Founder review: PENDING

## Citation Verifier Log

| Date | Phase | Cluster | Sample Size | Verified | Mismatched | Status |
|---|---|---|---|---|---|---|
| 2026-07-17 | phase-1.5 | phase1-all | 12 | 10 | 2 | FAILED |

### Mismatch Details

**Citation #2 (TLSNotary issue 524):** Specific quote about nightly-2022-12-12 unverified. Only indirect evidence (repo dependency on wasm-bindgen-rayon) found; primary source text not retrieved.

**Citation #12 (sinanpl/OaxacaBlinder):** Claim "part of OB reference/benchmark ecosystem" too vague and minimally verified. Repo confirmed as R implementation but not established as reference ecosystem member.
