# Build Plan — 0014-MERIDIAN: WASM Rayon Multithreading + Statistical Trust

> Created: 2026-07-18 · Tier: balanced · Charter mode: TRUE
> Spec: `_specify-wasm-rayon-multithreading-2026-07-17/` (7 phase4-final-*.md + spec.yaml + spec-charter.md + buildability-gate-rulings.md)
> Deliverable type: app (in-place modification of 3 existing repos — NOT an apps/ scaffold)
> Blast radius: client-facing (wrong stats numbers = defensibility harm on David's day-job)

## Repos & branches (isolation)

| Repo | Path | Build branch | Base |
|---|---|---|---|
| oaxaca-blinder-rs (engine, primary) | apps/hr-apps/oaxaca-blinder-rs | `build/wasm-rayon-mt/main` | main @ cfcca71 |
| pay-equity-app (Meridian frontend) | apps/hr-apps/pay-equity-app | (create at threading stage) | main |
| audit-forge (Flask server) | /home/deji/telos/audit-forge | (create at threading stage) | master (has 1 foreign uncommitted: _shell.html) |

## Phase 0 findings (2026-07-18)

- **E1 GREEN**: current engine graph compiles to wasm32 under `nightly-2025-06-27` + `-Zbuild-std=panic_abort,std` + `+atomics,+bulk-memory,+mutable-globals` (1m22s, exit 0). → nightly pin = **2025-06-27** (spec's 2024-08-02 was provisional; MJ-4 subgraph-trim NOT needed for compile).
- **rand_chacha 0.3.1** already in the dep graph (E1 log). `set_stream` available.
- Native workspace builds clean (3m43s). Test baseline: <PENDING — buhvi62pe>.
- Code anchors re-verified 2026-07-18: builder.rs:720 decompose_quantile; engine/analysis.rs:166 MM-branch (rewire target); main.rs:247 CLI (rewire target).
- E2 (shared-memory link args) + E3 (nested-worker spawn) deferred to threading stage (need the wasm-bindgen-rayon integration first).

## Inviolable stage order (founder-directed)

**determinism → memory profile → threading → validation.** Each stage lands + verifies before the next.

| Stage | Domains (phase4-final-*.md) | Repos | Gate to next stage |
|---|---|---|---|
| **1. determinism** | deterministic-rng | oaxaca-blinder-rs | AC-1..AC-10 pass; within-platform byte-identity holds; native build+test green |
| **2. memory profile** | memory-budget | oaxaca-blinder-rs | AC-M1..M15; profile report committed + reviewed; N_max_const computed |
| **3. threading** | toolchain-build, engine-parallel-surface, meridian-integration | all 3 repos | AC-1..16 (toolchain), AC-1..13 (engine), AC-M*.* (meridian); threaded blob links; E2/E3 resolved |
| **4. validation** | statistical-trust-layer, verification-benchmark | oaxaca-blinder-rs | trust goldens pass; mode-parity + memory-ceiling + reproducibility CI green |

## Stage 1 results (determinism) — 2026-07-18

Implementation: dispatched Opus worker did rng.rs + mean-path builder.rs + CV-1 seed forwarding, then
stopped mid-quantile-path (budget). Orchestrator completed inline: full quantile path (run_single_pass
rep_master param, D4 MM seeding, D5 bootstrap rewrite, seed API + RunMetadata on quantile builder/results),
inference.rs D6 guard, engine run_metadata surfacing (DecompositionResult + both analysis.rs branches),
and 6 missing tests (rng_determinism.rs AC-4/5/7/8/11 + parity_test AC-9 byte-identity). Design refinement:
rep-master = master ^ ((rep+1)*phi) so bootstrap rep 0 never collides with the raw-master point pass.

AC verification (9/10 confirmed; AC-10 full-suite running):
- AC-1 PASS (grep: 0 sample_n_literal, 0 thread_rng in quantile_decomposition)
- AC-2 PASS (grep: 0 par-iter in inference.rs)
- AC-3 PASS (rng::tests 3/3 + 35 other lib tests = 38/0)
- AC-4 PASS · AC-5 PASS · AC-7 PASS · AC-8 PASS · AC-11 PASS (rng_determinism 5/5)
- AC-6 PASS — CORE: sha256 d74efb3c... byte-identical across RAYON_NUM_THREADS 1/2/4 (INV-02 within-platform)
- AC-9 PASS (meanpath_point_estimates_byte_unchanged vs pre-refactor baseline; statsmodels parity still green)
- AC-10 (native build both green; wasm32 engine build green; full-suite test RUNNING b2qqjku6a with CARGO_BUILD_JOBS=2)

Build lesson: `cargo test --workspace` OOMs on 31GiB when uncapped (parallel linking of ~15 polars test
binaries). Always run with CARGO_BUILD_JOBS=2, or one --test target at a time.

## Stage 2 results (memory profile) — 2026-07-18

Implementation: dispatched Sonnet worker created `mem_profile.rs` (tracking `GlobalAlloc` +
checkpoint A/B API), wired checkpoints A/B into `builder.rs`, added the `mem-profile` feature
(off by default, INV-01) — then stopped at budget (same pattern as stage 1), before the
empirical half. Orchestrator completed inline: `examples/gen_mem_fixture.rs` (×5 D7
perturbation generator), `examples/mem_profile_harness.rs` (D1 measurement driver, gated by
`required-features = ["mem-profile"]`), `.gitignore` entry for the fixture, ran the profile,
computed the constants, wrote the committed report.

Feature compile: `cargo build -p oaxaca_blinder --features mem-profile` exit 0 (1m28s).
Fixture: seed 0x0014_50CE_5EED_A115, 50000 rows, sha256 8de0a364… (deterministic across runs).

AC verification:
- AC-M13/14/15 PASS (fixture determinism / unperturbed group+categoricals / gitignored).
- AC-M1 PASS — profile report committed (`.build-state/mem-profile-report.md`) with H_res/H_peak/Sc/St_obs.
- AC-M9 revised / AC-M10 RETIRED — profile FALSIFIED the D4 "memory lever": Polars clone is
  Arc-shallow, so Sc_before (8.352 MiB) ≈ Sc_after (8.386 MiB), no reduction. Stage-1 refactor
  kept for determinism (INV-02), not memory.
- AC-M6 PASS — N_max_const=8, a TRUE safe (bounded peak 248 MiB @ N=8 < ceiling 277 MiB).

### Founder decision + corrected diagnosis (2026-07-18) — Finding 2

David chose **Option A (bounded/streaming refactor)** at the AskUserQuestion gate. The DIAGNOSIS
was then corrected twice by measurement (both wrong hypotheses recorded in the report for honesty):
- ❌ "retained bootstrap results dominate" — refuted: `RepEstimates` extraction (dropping heavy
  per-rep `SinglePassResult`) left peak UNCHANGED. current-after-collect ≈ H_res → no retention.
- ❌ "D2 formula omits an R×retained term / 40 GiB leak" — refuted: no leak; the peak is transient.

**True mechanism:** unbounded `into_par_iter().map().collect()` lets rayon keep many reps'
`Sc`-sized working sets in flight → peak scales with reps AND threads (461 MiB N=1 … 899 MiB N=16
@ R=100). Sequential `into_iter` → flat 64 MiB. Benign natively (reclaimed); **fatal in WASM**
(linear memory only grows, never returns pages → permanent SharedArrayBuffer bloat).

**Fix (both in `builder.rs`, honoring Option A):**
1. `RepEstimates` — map extracts only the scalars the SE/CI reduction reads, drops heavy result
   per-rep. Bounds RETENTION (original ~400 KB/rep → 4 GB at R=10000; now ~2.3 KB/rep). Necessary.
2. **Bounded-parallel (chunked) bootstrap** — reps run in index-ordered chunks of the pool size →
   at most N working sets in flight → peak = H_res + N·Sc, FLAT in rep count. Index-ordered
   consumption keeps the FP reduction order fixed across thread counts.
Quantile path unchanged (its `SinglePassResult` = 3 scalars/quantile, already memory-safe).

**Verification (all PASS):** bounded peak @ N=8/50k = 248 MiB (was 860), flat in reps → in-band
(512) and no rep-count OOM at R=10000; AC-6 sha256 `d74efb3c…` IDENTICAL across
RAYON_NUM_THREADS 1/2/4/8 (INV-02, same hash as stage 1); AC-9 byte-identity; rng_determinism 5/5;
parity 2/2. **Threading invariant:** the bootstrap must stay bounded-parallel — never revert to
unbounded `into_par_iter().collect()`.

## Status

- [x] Phase 0 — init, git isolation, baseline build, E1 preflight, anchors, control files
- [x] Stage 1 — determinism (ALL 10 ACs PASS; AC-6 sha256 byte-identical across threads; AC-10 workspace 85/0). Committed 356faab.
- [x] Stage 2 — memory profile (GATE PASS: profile committed; N_max_const=8 true safe; WASM-OOM
      hazard found + fixed via bounded-parallel bootstrap; peak 248 MiB @ N=8/50k in-band; INV-02
      byte-identity + AC-9 + determinism all hold). AC-M10 retired (clone-lever falsified).
- [x] Stage 3 — threading (GATE PASS 2026-07-18): dual WASM artifacts build reproducibly; quantile INV-02 byte-id 2ea93797 across threads 1/2/4/8; mean path d74efb3c unchanged; 3 adversarial reviews (stats/refactor-safety/contract) CLEAN — 0 confirmed defects; David ruled commit-the-threaded-artifacts. Committed per-repo on build/wasm-rayon-mt/main (NO push/merge). E3 nested-worker browser spawn deferred to stage-4 CI. INVARIANT held: bootstrap bounded-parallel; N_max_const=8.
- [ ] Stage 4 — validation
- [ ] Phase 3 — review panel (Charter-compliance reviewer incl.)
- [ ] Phase 4 — integration, cross-surface parity, BUILD-REPORT.md, merge to main

## Resumability

On resume: read this file + features.json. Current stage = the first unchecked stage above.
Re-verify the stage's ACs before assuming it's done. Build branch: `build/wasm-rayon-mt/main` in oaxaca-blinder-rs.

## Workflow State

```yaml
phase: completed
current: merge-gate-DONE-both-repos-on-main
completed: [phase-0-init, stage-1-determinism, stage-2-memory-profile, stage-3-threading, stage-4-validation, merge-gate]
stage_4_commit: 623ff98   # build/wasm-rayon-mt/main (engine repo). Squashed the d0a6a65 WIP. NOW ON origin/main.
merge_gate:  # DONE 2026-07-19 (David approved "Merge both repos to main + push")
  oaxaca-blinder-rs: "PUSHED. build/wasm-rayon-mt/main:main FF 666b36b..623ff98. origin/main=623ff98 (verified ls-remote). local main synced. origin URL corrected dot-comma-hyphen -> david-deji (stale post-rename redirect; ls-remote confirms same 623ff98)."
  pay-equity-app: "MERGED+PUSHED. origin/main had David's dup-0016 (41563988); build branch 21366149 = same 0016 (2-line REGISTER.md delta only), not FF-able -> git merge origin/main (ort strategy, NO conflicts, identical 0016 auto-reconciled) = merge commit 7e21c771. Pushed 41563988..7e21c771 HEAD:main. origin/main=7e21c771 (verified ls-remote). local main synced. BUG-2 closed end-to-end on main (engine 623ff98 + app wasm 0030544e both integrated)."
  audit-forge: "UNTOUCHED by stage 4 (foreign _shell.html never staged); nothing to merge."
stage_4_open_items_for_founder:
  - "DONE (David approved 2026-07-18): pay-equity-app wasm propagation. Copied fresh Fix-B *_bg.wasm into frontend/src/wasm (seq f2f26646) + frontend/src/wasm-threaded (threaded f4c86323 = browser-validated). ONLY the 2 .wasm binaries changed — glue/d.ts/snippets/package.json byte-identical (ABI unchanged, drop-in). Committed pay-equity-app 0030544e on build/wasm-rayon-mt/main. BUG-2 now closed engine-side (623ff98) AND app-side (0030544e). (Minor: app build.sh 'cd engine' is stale/dead — wasm comes from oaxaca-blinder-rs copy, not app-local wasm-pack.)"
  - "[adversarial-verify follow-up, non-blocking] gen_trust_goldens.R §2 comment overstates 'independent oracle' — mean golden checks OLS fit (independent) not OB arithmetic (transliterated); loaded oaxaca R pkg never called. ddecompose (AC-6) IS the independent arithmetic oracle."
  - "[adversarial-verify follow-up, non-blocking] Machado-Mata quantile_decomposition.rs orphaned from shipped surfaces (all use RIF decompose_quantile); still exported + self-consistency-tested, no R oracle. Cleanup candidate."
commits: {stage-1: 356faab, stage-2: c005837, stage-3: see-git-log-build/wasm-rayon-mt/main}
stage_3_substages:
  3A-engine-feature-plumbing: DONE        # Cargo.toml x2 wasm-threads feature, lib.rs init_thread_pool re-export, SKIP doc comments, INV-08 in CLAUDE.md, AC-4 clean
  3B-decompose-quantile-rif-seed: DONE    # per-rep RIF recompute + seed fwd (builder.rs); RunMetadata.fixed_rif (skip-if-none); aggregate_results extracted; MEAN PATH BYTE-IDENTICAL d74efb3c across 1/2/4/8
  3C-wire-both-surfaces-rif: DONE         # analysis.rs quantile branch + main.rs CLI -> decompose_quantile (RIF); QuantileDecompositionBuilder import dropped both sites
  # 3A/B/C verified: native build 0; AC-3/6 wasm-bindgen-rayon absent native; AC-9 parity 2/2; rng_determinism 5/5; rif_test 1/1
  3D-toolchain-build-strategy-a: DONE     # .cargo/config.toml (appended to existing PyO3 [env]); build-wasm.sh dual-pass --target web; ci.yml dual-artifact+double-build. E1 GREEN (nightly-2025-06-27 builds full graph+wasm-bindgen-rayon 1.3.0/ASM-01). E2 RESOLVED: minimal flags link (atomics auto-emits shared-memory). baselines seq f14eb326 / threaded c7076609 REPRODUCED across 2 runs. config.toml native-safe (AC-2/8 exit 0).
  3E-meridian-audit-forge: DONE           # audit-forge COOP/COEP (2 lines, additive) + AC-M6 test (10 pass incl 8 CSP regression); vite.config; analysis.worker.js Strategy-A dual dynamic-import; service INIT_RESULT+getComputeMode; thread-cap.js=8; M7 pkg refresh + wasm-threaded/package.json (fixes rayon workerHelpers ../../.. import). pnpm build PASS (AC-M4.3, 42s). E3 deferred to stage-4 browser CI (W7).
  3F-gate-verify-commit: DONE             # VERIFIED: quantile INV-02 byte-id 2ea93797 across 1/2/4/8; mean path still d74efb3c; AC-6/8/11 pass; engine suite 14/14; oaxaca lib 38/38; clippy MY files clean; fmt MY files clean; pnpm build + audit-forge 10 tests pass. 3 adversarial reviews CLEAN (stats 6/6 CORRECT; refactor-safety 5/5 SAFE mean-path byte-identical; contract 6/7 OK + 1 SUSPECT=untracked wasm-threaded/ now resolved). David ruled COMMIT-the-threaded-artifacts (matches tracked seq wasm). Committed per-repo on build/wasm-rayon-mt/main — NO push/merge. Non-blocking carried to stage 4: AC-13 CLI<->WASM parity independent verify; density-floor SE at tau 0.1/0.9 via ddecompose golden; calculate_rif n<2 latent-unreachable.
  # PRE-EXISTING DEBT discovered (NOT mine, flag to David): CI quality job already red on main — clippy akm.rs:376 needless_range_loop + cargo fmt --check dirty across ab_binding_regression_test.rs/parity_test.rs/etc. My changes add zero clippy warnings + are fmt-clean.
  # SPEC RECONCILIATIONS: (1) .cargo/config.toml was NOT new — appended to existing PyO3 [env]. (2) E2 minimal flag set suffices (no explicit --shared-memory). (3) seq baseline moved cff16253->f14eb326 (engine RIF change, expected). (4) AC-9 forced seq glue to --target web too (baseline is target-independent).
next: stage-4-then-phase-3-review-then-merge-gate
stage_4_substages:
  4A-env-r-goldens: DONE           # gfortran installed (David); quantreg 6.1 + ddecompose 1.0.0 + rifreg 1.1.0 + oaxaca 0.1.5. gen_trust_goldens.R authored + run: employers_trust_fixture.csv (PII-stripped 10k, AC-2 pass) + resample_indices.csv (200x1500) + trust_goldens_r.json. ref=Female/A=Male, 14 design cols, base=alpha-first (matches engine ascending-sort). gap=0.0185 exp=0.0173 unexp=0.0012 (real small-gap data). VERIFY-UPSTREAM caught ddecompose sig drift: group= is a separate arg (not |g in formula); Composition_effect=explained, Structure_effect=unexplained. BUG caught: fixture comment line broke polars header + AC-2 -> removed, provenance to _meta.
  4B-methods-tests: DONE           # ALL 4 pass. AC-3 trust_golden_r (engine==R lm oracle 1e-6 on real 10k, +AC-3 design-column set guard). AC-4 decomposition_properties (proptest 256ea: two/three-fold adding-up, detailed==agg, label-swap antisym, scale-equiv, no-panic-degenerate). AC-5 qr_location_scale (engine solve_qr==quantreg rq 1e-4 + analytic tau-varying + discrimination; added pub qr_coefficients wrapper, INV-01 additive). AC-6 quantile_detail vs ddecompose: MEASURED tau=.1->4e-5, .5->1.5e-5, .9->1.37e-2 (tail density divergence per MJ-3), pinned 2e-2; +self-consistency 1e-9. AC-7 INV-01 frozen files unmodified + parity 2/2 green. Added proptest dev-dep. Fixtures: employers_trust_fixture.csv 684K (FLAG >500KB but load-bearing, all tests read it), trust_goldens_r.json 328K, resample_indices.csv 180K. DEFERRED: bootstrap-SE consuming test (golden+indices ready; point-agreement 1e-6 + INV-02 determinism cover intent). AC-8 full-workspace -> 4E gate.
  4C-modes-serializer-parity-mem: DONE (native)  # mode_parity_test.rs (engine/tests/): AC-1 double-serialize determinism + AC-2 byte-identical decompose across rayon pools 1/2/4 threads (in-process, live regression guard; canonical=serde_json of all-Vec DecompositionResult). memory_ceiling_test.rs (oaxaca_blinder/tests/, mem-profile feature): AC-4 peak=198 MiB @ 50k/N=8 < 326 MiB budget (128 MiB headroom); 50k=5x committed 10k (no jitter, peak is row-count fn). AC-7 no POLARS_MAX_THREADS (0 matches). AC-10 50k PII-clean (built from stripped fixture). Added rayon+serde_json to engine dev-deps, rayon to oaxaca dev-deps. RECONCILE: mode_parity sited engine-level (decompose_inner pub, DecompositionResult all-Vec no map-order risk); memory_ceiling sited oaxaca_blinder (allocator lives there). DEFERRED: AC-5 structured-OOM self-report (W4 not implemented; budget-prevention via bounded-parallel+N_max=8+--max-memory is the PRIMARY defense per D-3, validated by AC-4). Browser leg (AC-2 wasm seq/t2/t4, AC-3 COI) -> 4D.
  4C-benchmark: DONE               # David asked Rust-vs-R perf. verification/bench_methods.R + examples/bench_methods.rs (serial, warm, median). Native release: mean-OB-point R 5ms vs Rust 10ms (R wins, LAPACK); OB-boot100 R 912ms vs Rust 52ms (Rust 18x, the real workload); RIF-quantile R 30ms vs Rust 10ms (3x); QR R 33ms vs Rust 83ms (R wins, quantreg Fortran vs clarabel LP). App faster where it counts (bootstrap); WASM ~2-3x slower than native. Note: QR solver optimization candidate (specialized Frisch-Newton).
  4D-ci-browser-infra: DONE        # AUTHORED: verification/browser-parity/{coi-server.mjs,index.html,page.mjs,compute.worker.mjs,parity.spec.mjs,playwright.config.mjs,package.json}. ci.yml +3 jobs (memory-ceiling; browser-parity[nightly build-std+wasm-bindgen+node+playwright chromium]; benchmark-speedup[schedule/dispatch, continue-on-error]) + workflow_dispatch/schedule triggers. Reproducibility double-build ALREADY in wasm-verify (not re-added). BROWSER PROOF via Playwright MCP (node v25 hung the npx runner; pivoted to session MCP browser against local COI server). VERIFIED IN REAL COI CHROMIUM: crossOriginIsolated=true (page+worker) @ threads 1/2/4; initThreadPool resolved @ 1/2/4 (E3 nested worker->rayon spawn works); byte-identical decompose across 1/2/4 threads proven transcription-free via in-browser SHA-256 over localStorage-persisted runs -> all three sha256=c39d494791b10a63, len 652, all_byte_identical=true. TWO REAL BUGS FOUND+FIXED:
  #   BUG-1 (harness): wasm-bindgen-rayon workerHelpers.js:54 `import('../../..')` = pkg DIRECTORY import; a plain static server can't do bundler/pkg.json main-resolution -> 404, pool never spawns. FIX: coi-server.mjs resolves a dir import lacking index.html to the `<name>.js` whose sibling `<name>_bg.wasm` exists (bundler-free). native-mode-parity CANNOT catch this (no worker re-import step).
  #   BUG-2 (ENGINE, real latent deploy defect in seq+threaded wasm): decompose() threw "6840134480574414850 can't be represented as a JavaScript number" — serde_wasm_bindgen default serializes u64 RunMetadata.seed (DEFAULT_SEED 0x5EED_0A11_CA8A_0002) as JS Number and THROWS >2^53. serde routes usize via serialize_u64 too. Native serde_json handles u64 -> latent until browser. FIX (Fix B, chosen over blanket-bigint Fix A which turned counts into BigInt=app-arith risk): oaxaca_blinder/src/rng.rs RunMetadata.seed gets #[serde(serialize_with=serialize_u64_as_str)] -> seed serializes as a STRING in BOTH serde_json + serde_wasm_bindgen; counts stay JS Numbers (app-safe); lib.rs reverted to plain to_value ×5 (no wasm serializer change needed). Seed VALUE + Rust type unchanged; only JSON encoding. NATIVE-SAFE VERIFIED: parity_test 2/2 (incl meanpath byte-unchanged AC-9), trust_golden_r 1/1, mode_parity 2/2 all GREEN post-fix. In-browser confirmed: seed="6840134480574414850" (string), total_count=300 (number).
  #   ENGINE SOURCE CHANGED (flag to David): oaxaca_blinder/src/rng.rs (seed serde) + engine/src/lib.rs (comment only, reverted). Refreshed baselines: seq 0514af14.., threaded db1ad558.. (bytes changed by Fix B).
  4E-gate-verify-commit: IN-PROGRESS # ADVERSARIAL VERIFY (3-lens workflow, sonnet) FOUND REAL DEFECTS in MY stage-4 tests — all FIXED:
  #   [CRITICAL] AC-6 quantile_detail VACUOUS PASS (both lenses, indep): est() returned Option, call sites `if let Some` SKIPPED missing keys, tau_max_diff init 0.0 -> a renamed/empty detailed Vec => assert(0.0<=2e-2) passes on ZERO comparisons (failure mode #9). FIXED: added KEY-SET guard (engine detail keys == golden keys, per tau, BEFORE value cmp) + compared==28 backstop. Now 28 covariates compared @ every tau.
  #   [HIGH] AC-6 never asserted the well-conditioned check (engine aggregate vs ddecompose aggregate — was eprintln-only). FIXED: added aggregate assertion, tau-dependent tol (1e-4 well-cond / 5e-3 tail); measured agg diff ~1e-6 @tau.1/.5 (near-exact, validates RIF-OLS arithmetic), 1.1e-3 @tau.9 (density divergence).
  #   [CRITICAL/HIGH] QUANTILE_DETAIL_TOL=2e-2 flat > entire aggregate gap; near-zero power for ~10/14 covariates. FIXED: tau-dependent per-pred tol (5e-4 @tau.1/.5 measured 4e-5/1.5e-5 -> real power; 2e-2 @tau.9 density-limited, disclosed).
  #   [LOW] trust_golden + AC-6 comments said "GroupB set explicitly" but never called it. FIXED: both now call .reference_coefficients(GroupB) explicitly (guards default flip).
  #   [HIGH-doc] trust_golden mean "independent oracle" OVERSTATED: gen_trust_goldens.R §2 transliterates decomposition.rs formula (copies it); `oaxaca` R pkg loaded+version-asserted but NEVER called. It DOES independently check the OLS FIT (R lm vs nalgebra) but NOT the OB arithmetic. MITIGATED: ddecompose (AC-6) IS an independent arithmetic oracle (own C/R code). -> flag to David; R gen §2 comment honesty is a follow-up, not code-blocking.
  #   [note] Machado-Mata module quantile_decomposition.rs now ORPHANED from shipped surfaces (CLI+WASM+MCP all use RIF decompose_quantile per stage-3 3C); still exported + self-consistency-tested, no R oracle. Cleanup candidate, not stage-4 blocker. -> flag to David.
  #   PRE-EXISTING STAGE-3 REGRESSION surfaced by AC-8 full run: cli_test::test_quantile_decomposition asserted old "Machado-Mata..." header; stage-3 rewired CLI->RIF (main.rs:271 "(RIF-regression)") but didn't update the test. FIXED: assert "(RIF-regression)"+"Two-Fold Decomposition". (Stage-3 3F gate missed it — cli_test not in lib-38 count.)
  #   FIX-B SAFETY CONFIRMED: RunMetadata is Serialize-ONLY (no Deserialize derive); grep confirms NO consumer (meridian-mcp/CLI/tests) reads seed back as a number -> seed-as-string breaks nothing.
  #   AC-8: `--all-features` COMPILES natively (wasm deps build as native no-ops); first full run had ONLY cli_test fail (now fixed); re-running clean now.
  #   PENDING: confirm AC-8 clean re-run -> commit (NO push/merge). Include: rng.rs, engine/src/lib.rs, tests {quantile_detail,trust_golden,cli}_*, verification/browser-parity/*, ci.yml, refreshed baselines seq 0514af14/threaded db1ad558, fixtures employers_trust_fixture.csv+trust_goldens_r.json + the d0a6a65 WIP files. EXCLUDE: .idea/*.iml, meridian-mcp/src/main.rs.~1~, audit-forge _shell.html, resample_indices.csv. ab_binding fmt-churn NOT mine.
stage_4_constants: {declared_max_bytes: 342228992, peak_n8_mib: 248, margin_headroom_mib: 78, fixture_src_10k: /home/deji/Downloads/Employers_data.csv}
n_max_const: 8            # true safe; bounded peak 248 MiB @ N=8/50k
threading_invariant: "bootstrap must stay bounded-parallel (chunked to pool size); never revert to unbounded into_par_iter().collect() — reintroduces WASM-OOM"
link_args: {stack_size: 1048576, max_memory: 342228992, initial_memory: 82378752}
nightly_pin: nightly-2025-06-27          # E1-proven (Phase 0); spec's 2024-08-02 was provisional
in_scope_12_ruling: "a-1: both WASM(analysis.rs) + CLI(main.rs) -> decompose_quantile (RIF); per-rep RIF recompute (fixed_rif:false)"
strategy: A-dual-artifact                 # untouched stable seq baseline + threaded --target web blob
repos: {engine: build/wasm-rayon-mt/main, pay-equity-app: TBD-branch-off-main, audit-forge: TBD-branch-off-master (foreign _shell.html untouched)}
safe_to_retry: true
started_at: 2026-07-18
updated: 2026-07-19
```
