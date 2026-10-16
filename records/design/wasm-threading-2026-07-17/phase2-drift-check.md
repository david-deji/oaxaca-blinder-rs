# Phase 2 Drift Check — Spec vs Charter

> Date: 2026-07-17  
> Scope: 7 phase2-spec-*.md files vs spec-charter.md  
> Invariant count: 8  
> Anti-goal count: 7  

---

## Summary

**No drifts detected** against 8 invariants and 7 anti-goals. All Founder Intake assertions are preserved. Specs are properly sequenced and cross-domain dependencies are consistent.

---

## Findings (0)

(No drift items identified.)

---

## Invariant Verification

### INV-01: Native builds byte-equivalent; wasm-threads feature-gated
- **Toolchain-build** §4.3: "wasm-threads is never in the native/default feature set"; `cargo tree` empty by default
- **Engine-parallel-surface** R3: native unchanged, wasm-bindgen-rayon absent from native graph
- **Status**: ✓ Preserved

### INV-02: Same input + seed → bit-identical across thread counts
- **Deterministic-rng** R1–R7: master-seed derivation, schedule-independent per-rep streams, sequential float summation
- **Deterministic-rng** §1: "master acceptance criterion is INV-02"
- **Verification-benchmark** R-VB-1: mode-parity byte-compare across native / wasm-seq / wasm-threaded(2,4)
- **Status**: ✓ Fully specified; SC-03 gate

### INV-03: Non-COI context degrades to sequential (never crash)
- **Memory-budget** R5: "graceful error to UI — never a blank screen"
- **Meridian-integration** R1: crossOriginIsolated guard with sequential fallback
- **Meridian-integration** §4.1: try/catch inside guard for undocumented rejection (RK6)
- **Status**: ✓ Preserved

### INV-04: Raw-wasm reproducibility model survives
- **Toolchain-build** §4.7: sha256 baseline recording + commit + verify procedure
- **Toolchain-build** §4.5: double-build reproducibility check in CI
- **Verification-benchmark** R-VB-4: reproducibility double-build sha256 blocking job
- **Status**: ✓ Preserved with double-build verification

### INV-05: Peak memory 50k fits max with headroom; thread cap by memory
- **Memory-budget** R1–R3: profile harness, sizing formulas, thread-cap `N_max = floor((M_max − H_res − Marg) / (Sc + St))`
- **Memory-budget** AC-R1.3: profile is inviolable precondition ("profile-before-parallelize ordering")
- **Verification-benchmark** R-VB-2: memory-ceiling blocking job, peak < max − margin
- **Status**: ✓ Fully addressed with ordering gate

### INV-06: audit-forge headers additive + scoped to /pay-equity/ + /api/
- **Meridian-integration** R3 §4.5: COOP/COEP inside existing `if` predicate scoping
- **Meridian-integration** AC-R3.2: "No response outside those two prefixes gains either header"
- **Status**: ✓ Preserved

### INV-07/08: Decimal(18,2) boundary, engine f64-only
- **Deterministic-rng** §5: "all engine RNG math stays f64; no monetary value passes through"
- **Engine-parallel-surface** R5: no monetary handling in this layer
- **Memory-budget** R6: "round to cents (Decimal(18,2) at the data boundary per INV-08)"
- **Statistical-trust-layer** §1: "engine math is f64; Decimal only at boundaries"
- **Status**: ✓ f64 interior preserved; boundary enforcement intact

---

## Anti-Goal Verification

| Anti-goal | Evidence | Status |
|---|---|---|
| 1. No Meridian API/Vue/Pinia change | Meridian-integration AC-R2.4: "No message removed or renamed; no field removed" | ✓ Additive only |
| 2. No further native parallelism | Engine-parallel-surface R3: "native rayon unchanged" (INV-01) | ✓ Preserved |
| 3. No public deployment | Meridian-integration §1: local audit-forge Flask, no new host | ✓ Preserved |
| 4. No algorithm changes | Deterministic-rng §1: "RNG *structure* only; point-estimate math untouched" | ✓ RNG seeding only |
| 5. No weakening reproducible-build | Toolchain-build R5: "chosen strategy must preserve hashable re-buildable baseline" | ✓ Dual approach specified |
| 6. No wasm32-wasip1 or non-browser | Toolchain-build §1: "Browser target only (wasm32-unknown-unknown)" | ✓ Browser-target enforced |
| 7. No pkg/ or nightly binaries in git | Toolchain-build §4.7: "`pkg/` gitignored (charter anti-goal)" | ✓ Preserved |

---

## Founder Intake Assertions

| Assertion | Evidence | Status |
|---|---|---|
| Toolchain strategy "let pipeline decide" + founder gate | Toolchain-build §4.6.3: both strategies specified; founder ratifies at buildability gate (SC-02) | ✓ Handed to founder |
| Production: local audit-forge Flask, no public deploy | Meridian-integration §1, §4.5: local only | ✓ Preserved |
| Success criterion: full-surface audit of 5 WASM entries | Engine-parallel-surface R1 table: decompose (PARALLELIZE), optimize/frontier/defensibility (SKIP), verify_adjustments (via inheritance) | ✓ All 5 audited |
| Data scale 50k; memory critical; fixed shared-memory max | Memory-budget §1: entire spec addresses 50k with fixed maximum sizing + profile-before-parallelize ordering | ✓ Fully addressed |
| Determinism required: bit-identical, schedule-independent RNG | Deterministic-rng R1–R7 + verification-benchmark R-VB-1: master-seed → per-rep streams, INV-02 gate | ✓ Fully specified |
| Build order: determinism → profile → threading → validation | Memory-budget §4.3 AC-R1.3: "profile is a hard precondition"; determinism spec (R1–R7) lands first | ✓ Order enforced |
| 50k revised down from 130k | Acknowledged in Charter, memory-budget specs 50k | ✓ Consistent |

---

## Cross-Domain Consistency

| Domain pair | Interface | Consistency check |
|---|---|---|
| toolchain-build ↔ memory-budget | `--max-memory` / `-zstack-size` values | TB §4.2 uses 512 MiB placeholder; MB §3.2 says "band default 256–512 until profile substitutes" — consistent ✓ |
| deterministic-rng ↔ memory-budget | Owned index resampling + seeded RNG | MB R4 shares refactor with DR; cross-domain summary confirms alignment ✓ |
| engine-parallel-surface ↔ memory-budget | POLARS_MAX_THREADS pinning | MB §5 notes "POLARS_MAX_THREADS=1 assumption"; EPS R4 specifies it — consistent ✓ |
| meridian-integration ↔ engine-parallel-surface | `initThreadPool` re-export + `MEMORY_THREAD_CAP` | MI R1 consumes `init_thread_pool`; EPS R2 produces it; MI §4.1 consumes cap from formula — consistent ✓ |
| verification-benchmark ↔ statistical-trust-layer | Mode vs method testing distinction | VB §3 clarifies "mode-parity = MODES, trust suite = METHODS — both needed"; no overlap ✓ |
| statistical-trust-layer ↔ verification-benchmark | Shared 50k fixture (PII-stripped Employers) | STL §4.5 + VB §4.5 both strip `Employee_ID`/`Name`; shared PII convention consistent ✓ |

---

## Research Items (Not Drift)

The following are flagged as `NEEDS RESEARCH:` in Phase 2 specs — deferred questions, not Charter contradictions:

| Item | Location | Impact | Gate |
|---|---|---|---|
| Nightly compile validation (nightly-2024-08-02 + polars/clarabel/nalgebra) | Toolchain-build §4.1 | Build may need nightly bump | Pre-dispatch research ✓ |
| Shared-memory link args (does +atomics auto-emit or need --shared-memory?) | Toolchain-build §4.2 | Link step may need arg adjustment | Pre-dispatch research ✓ |
| wasm-bindgen-rayon 1.3.0 ↔ wasm-bindgen 0.2.106 compatibility | Toolchain-build §4.3 | May need dep version update | ASM-01 re-verify at build time ✓ |
| build-std reproducibility across machines | Toolchain-build §4.5 | May reveal nondeterminism | Double-build CI gate catches it ✓ |
| Polars POLARS_MAX_THREADS runtime-set behavior | Engine-parallel-surface §7 | May need build-time pin fallback | RK1 mitigation specified ✓ |
| Polars DataFrame::take with with-replacement indices | Deterministic-rng §6 | Core primitive, but Polars API is stable | RK3 low-risk ✓ |
| R oaxaca 0.1.5 external bootstrap index hook | Statistical-trust-layer §7 | May need fallback bootstrap loop | R-TL-A fallback pre-specified ✓ |
| wasm-pack test COI header support | Verification-benchmark §7 | Test infrastructure; mitigation = custom server | R-VB-A high, but mitigation designed ✓ |
| Nested-worker initThreadPool spawn | Meridian-integration RK1 | Test/production topology; fallback = main-thread + relay | Phase 3 risk, designed fallback ✓ |

None of these are Charter drift — they are research questions with mitigations specified.

---

## File Path Verification

**Charter Deliverables Shape — all referenced in Phase 2 specs:**

| File | Spec section | Status |
|---|---|---|
| `apps/hr-apps/oaxaca-blinder-rs/.cargo/config.toml` | Toolchain §4.2 | ✓ |
| `apps/hr-apps/oaxaca-blinder-rs/oaxaca_blinder/Cargo.toml` | Deterministic-rng §3, Engine-parallel §4 | ✓ |
| `apps/hr-apps/oaxaca-blinder-rs/oaxaca_blinder/src/builder.rs` | Deterministic-rng §4, Memory §4 | ✓ |
| `apps/hr-apps/oaxaca-blinder-rs/oaxaca_blinder/src/quantile_decomposition.rs` | Deterministic-rng §4 | ✓ |
| `apps/hr-apps/oaxaca-blinder-rs/engine/Cargo.toml` | Engine-parallel §4 | ✓ |
| `apps/hr-apps/oaxaca-blinder-rs/engine/src/lib.rs` | Engine-parallel §4 | ✓ |
| `apps/hr-apps/oaxaca-blinder-rs/scripts/build-wasm.sh` | Toolchain §4.4 | ✓ |
| `apps/hr-apps/oaxaca-blinder-rs/.github/workflows/ci.yml` | Toolchain §4.5, Verification §4.2 | ✓ |
| `apps/hr-apps/oaxaca-blinder-rs/engine/pay_equity_engine.wasm.sha256` | Toolchain §4.7 | ✓ |
| `apps/hr-apps/pay-equity-app/frontend/src/wasm/analysis.worker.js` | Meridian §4.1–4.3 | ✓ |
| `apps/hr-apps/pay-equity-app/frontend/vite.config.js` | Meridian §4.6 | ✓ |
| `/home/deji/telos/audit-forge/webui/__init__.py` | Meridian §4.5 | ✓ |

---

## CLARIFY Markers

**Count: 0** — No `[CLARIFY: ... close-condition: ...]` markers found in Charter or any Phase 2 spec file.

Charter explicitly states (§8): "None open — the two prior candidates (reproducibility strategy, prod host) were resolved at intake."

Phase 2 specs use `NEEDS RESEARCH:` for deferred questions (not `[CLARIFY:]`). This is consistent with the Charter's resolution of reproducibility strategy (left to founder at gate) and prod host (local-only confirmed).

---

## Conclusion

**All invariants held. All anti-goals preserved. No Charter contradictions detected.**

Phase 2 specs are a faithful elaboration of the Charter:
- Ordering gate on memory profile (pre-parallelize precondition, AC-R1.3)
- Reproducibility strategy left to founder at buildability gate
- All 8 invariants have explicit acceptance criteria
- All 7 anti-goals are maintained
- Cross-domain dependencies are consistent
- File paths are correct
- Research items are gated with mitigations

The specs are ready for /build Phase 3.
