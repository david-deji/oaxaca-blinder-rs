# Spec Charter — wasm-rayon-multithreading-pay-equity-engine

> Issue: TBD — MERIDIAN issue filed at plan-confirmation (pay-equity-app/issues/)
> Filed: 2026-07-17
> Author: David via /specify Phase 0.5b
> Status: DRAFT — founder review before plan-confirmation gate

---

## Classification

| Field | Value | Notes |
|---|---|---|
| `deliverable_type` | app | Rust workspace + browser app plumbing; Phase 4 uses app fork |
| `complexity_class` | system | 3 repos touched (engine workspace, Meridian frontend, audit-forge Flask), build pipeline + CI rework |
| `blast_radius` | client-facing | Engine output feeds real day-job pay-equity deliverables on David's corp PC; wrong numbers = defensibility harm. Not `production` (no live traffic/users). Floor: mandatory council + adversarial verifier + dual-founder approval (satisfied by David's direction per `~/.claude/CLAUDE.md` founder-coordination rule). |
| `reversibility` | committable | git revert restores prior state in all 3 repos; sha256 baseline re-recordable; no data migration |

---

## In Scope

1. **Threaded WASM build path** — pay-equity-engine compiled with atomics/bulk-memory + wasm-bindgen-rayon 1.3.0 behind a `wasm-threads` feature; native builds untouched *(Source: Founder goal statement + Intake round 1)*
2. **Reproducibility strategy** — research and decide nightly-re-pin vs dual-artifact; implement the chosen one incl. sha256 baseline + CI handling *(Source: Intake round 1 — "Let pipeline decide", founder approves at buildability gate)*
3. **Bit-identical seeded RNG** — refactor bootstrap (`builder.rs`) and MM simulation (`quantile_decomposition.rs`) to schedule-independent per-rep seeding; same input → same numbers at any thread count *(Source: Intake round 2 — "Bit-identical required")*
4. **Full-surface parallelization audit** — all 5 WASM entry points (decompose, optimize, verify_adjustments, calculate_efficient_frontier, check_defensibility) audited; parallelized where profitable, documented where not *(Source: Intake round 1 — "Full-surface parity")*
5. **Memory budget for 50k rows** — measured memory profile, shared-linear-memory maximum, per-thread stack sizing, thread-count cap derivation *(Source: Intake round 2 — "50k, memory is of utmost importance")*
6. **Meridian worker wiring** — `analysis.worker.js` crossOriginIsolated feature-detect + `initThreadPool`, graceful sequential fallback, regenerated pkg artifact(s) *(Source: Intake round 1 — "Engine + Meridian wiring")*
7. **Serving-surface headers** — COOP/COEP on audit-forge `/pay-equity/` (beside `_apply_meridian_csp`, `webui/__init__.py:125`) and Vite dev `server.headers` *(Source: Intake round 2 — "Flask offline bundle"; wrap-up — audit-forge path)*
8. **Verification + benchmark** — bit-identical parity tests, 50k-row memory-ceiling test, wall-clock benchmark, CI wasm-verify update *(Source: Intake round 1 success criterion + round 2 scale)*
9. **Statistical trust layer** — golden-file parity vs R `oaxaca`/Stata at realistic n, property-based tests of adding-up identities on randomized data, heteroskedastic/skewed-distribution QR cases (current QR tests are tau-insensitive linear-only) *(Source: Founder pre-gate review item 1, 2026-07-17; verified: integration tests n=12–20, reps 2–5)*
10. **Determinism as prerequisite + standalone feature** — the seeded-RNG fix (item 3) lands and is verified BEFORE threading work builds on it; includes fixing silent bootstrap-rep discard (`builder.rs` filter_map `.ok()` — rep count itself is nondeterministic today) and the unseeded `sample_n_literal(..., None)` / `thread_rng()` sites *(Source: Founder pre-gate review item 2; verified builder.rs:832, quantile_decomposition.rs:215,244)*
11. **Pre-threading memory profile** — measure the CURRENT single-threaded memory curve at 50k rows before any parallel design is finalized; profile output is an input to the thread-cap formula (In-Scope 5) *(Source: Founder pre-gate review item 3 — "profile before parallelizing, or threading optimizes speed into an OOM")*
12. **Quantile detailed-components API exposure** — close the admitted public-API gap (`engine/src/analysis.rs:201`) so quantile results expose detailed components like the mean path does *(Source: Founder pre-gate review item 5a)*
13. **Test fixture: Employers_data.csv** — `/home/deji/Downloads/Employers_data.csv` (10k rows: Gender group, Salary outcome, Age/Experience/Education/Department/Location predictors) is the seed for realistic-n tests; replicate ~5× (with perturbation strategy defined in spec) for 50k-row memory/benchmark cases *(Source: Founder directive 2026-07-17 mid-session)*

---

## Out of Scope (Anti-Goals)

| Anti-goal | Preferred alternative |
|---|---|
| Changing the Meridian worker postMessage API or any Vue/Pinia layer | Keep `{type, payload}` contract byte-compatible; all changes stay inside worker init + engine |
| Parallelizing native (non-wasm) code paths further | Native already uses rayon; only the wasm enablement layer changes |
| Public/hosted deployment of Meridian or new serving infrastructure | Local-only: audit-forge Flask (loopback) + Vite dev; a future host inherits the headers checklist in the spec |
| Algorithm changes to decomposition/optimization mathematics | Only RNG seeding structure changes; point-estimate math untouched, verified by parity tests |
| Dropping or weakening the reproducible-build model (SC-04, Track-0) | The chosen strategy must preserve a hashable, re-buildable raw-wasm baseline (single or dual) |
| Adopting wasm32-wasip1-threads or non-browser thread targets | Browser target only (wasm32-unknown-unknown + wasm-bindgen-rayon); WASI is a different runtime |
| Committing pkg/ or nightly-toolchain binaries to git | pkg/ stays gitignored (existing convention); toolchains pinned by config files |

---

## Success Criteria

- **SC-01**: Spec specifies the complete threaded build (exact nightly pin or dual-toolchain layout, `.cargo/config.toml` rustflags, build-std list, `--max-memory` link arg, wasm-bindgen invocation) such that `/build` can execute it without further research — verify by build-readiness audit: every command literal, no `[CLARIFY]` remaining
- **SC-02**: Spec contains the reproducibility decision (re-pin vs dual-artifact) with rationale, founder-approved at buildability gate, incl. sha256 baseline procedure + CI diff — verify by presence of approved decision record + ci.yml change plan
- **SC-03**: Spec contains a schedule-independent RNG design with a written proof-sketch that per-rep streams are independent of thread scheduling, plus the parity test plan (threaded N-thread vs 1-thread vs current-sequential on fixed seed → byte-identical JSON output) — verify by statistical-correctness council seat + adversarial verifier sign-off
- **SC-04**: Spec audits all 5 WASM entry points with a parallelize/skip verdict and rationale each — verify by table present covering decompose, optimize, verify_adjustments, calculate_efficient_frontier, check_defensibility
- **SC-05**: Spec contains a memory budget table for 50k rows × realistic column count (input frame, per-thread clones, bootstrap resample copies, shared-memory maximum, thread cap formula) with cited measurement or estimation method — verify by memory/performance council seat review
- **SC-06**: Spec specifies Meridian worker changes (feature-detect, initThreadPool, fallback signal to UI) and exact COOP/COEP insertions for audit-forge + Vite with file:line anchors — verify by file:line references resolving against actual files
- **SC-07**: Spec defines the verification suite (parity, memory ceiling, benchmark) with pass thresholds and CI integration — verify by acceptance-criteria section listing runnable commands

Acceptance bar for Founder Review: all SC items pass; all Invariants hold; Scope-Delta Log shows only approved deltas.

---

## Invariants

- **INV-01**: Native builds (CLI, meridian-mcp, engine rlib) remain byte-equivalent in behavior — wasm-threads changes are feature-gated off the native path *(Source: Founder Intake — scope; apps-file-caution.md deploy-shape discipline)*
- **INV-02**: Same input + same seed → bit-identical output across thread counts and across threaded/sequential modes *(Source: Founder Intake round 2 — "Bit-identical required")*
- **INV-03**: A non-cross-origin-isolated context must degrade to working sequential execution — never a crash, never a blank screen *(Source: build-safety.md error-handling default; Intake — corp PC environment)*
- **INV-04**: The raw-wasm reproducibility model survives: every shipped blob has a committed sha256 baseline + a pinned toolchain that rebuilds it byte-identically *(Source: Track-0 SC-04, scripts/build-wasm.sh header)*
- **INV-05**: Peak memory at 50k rows must fit the declared shared-memory maximum with measured headroom; thread count is capped by the memory budget, not just hardwareConcurrency *(Source: Intake round 2 — "memory is of utmost importance")*
- **INV-06**: audit-forge header changes are additive and scoped to `/pay-equity/` + `/api/` — the 9 pre-existing blueprints' surfaces stay untouched *(Source: /home/deji/telos/audit-forge/webui/__init__.py:127 scoping comment)*
- **INV-07**: Monetary values remain Decimal(18,2) — no Float64 introduction during refactors *(Source: comp-audit-suite rule via oaxaca-blinder-rs CLAUDE.md)* — SCOPED BY INV-08:
- **INV-08**: **Founder ruling 2026-07-17 (David, plan gate)**: the statistical engine is EXEMPT from Decimal(18,2) — f64 is the correct tool for regression/decomposition math. Any monetary value leaving the engine for display or adjustment ledgers rounds through Decimal(18,2) at the boundary. The spec must name the boundary points; the build must record this ruling in the engine repo's CLAUDE.md *(Source: AskUserQuestion plan gate, 2026-07-17)*

---

## Assumptions

- **ASM-01**: wasm-bindgen-rayon 1.3.0 is current and compatible with wasm-bindgen 0.2.106 — valid until 2026-10-17 *(re-verify via Perplexity at build time)*
- **ASM-02**: No stable-Rust threaded std for wasm32-unknown-unknown exists; nightly + build-std remains required — valid until 2026-10-17
- **ASM-03**: audit-forge serves Meridian at `/pay-equity/` with the `_apply_meridian_csp` hook at `webui/__init__.py:125` — verify via Read of that file before dispatch (verified 2026-07-17)
- **ASM-04**: Day-job dataset ceiling in scope is ~50k employee rows (founder revised down from an initial 130k estimate, 2026-07-17) — INVALIDATED if founder reports larger consolidated analyses
- **ASM-05**: The corp-PC browser supports SharedArrayBuffer when cross-origin-isolated (evergreen Chrome/Edge/Firefox) — INVALIDATED if a managed-browser policy disables it; fallback (INV-03) covers this case regardless
- **ASM-06**: Meridian's built bundle + engine pkg are regenerable from source on this machine (pnpm + cargo toolchains present) — verify via `cargo build -p pay-equity-engine --features wasm --target wasm32-unknown-unknown` before build dispatch

---

## Deliverable Shape (Phase 0.4b output)

### Files to modify

| Path | Change |
|---|---|
| `apps/hr-apps/oaxaca-blinder-rs/rust-toolchain.toml` | strategy-dependent (nightly re-pin) or untouched (dual-artifact) |
| `apps/hr-apps/oaxaca-blinder-rs/oaxaca_blinder/Cargo.toml` | wasm-threads feature plumbing |
| `apps/hr-apps/oaxaca-blinder-rs/oaxaca_blinder/src/builder.rs` | per-rep seeded RNG streams |
| `apps/hr-apps/oaxaca-blinder-rs/oaxaca_blinder/src/quantile_decomposition.rs` | per-rep/per-tau seeded RNG streams |
| `apps/hr-apps/oaxaca-blinder-rs/engine/Cargo.toml` | wasm-bindgen-rayon 1.3.0 behind wasm-threads |
| `apps/hr-apps/oaxaca-blinder-rs/engine/src/lib.rs` | init_thread_pool export |
| `apps/hr-apps/oaxaca-blinder-rs/scripts/build-wasm.sh` | threaded build path + --max-memory + baseline handling |
| `apps/hr-apps/oaxaca-blinder-rs/.github/workflows/ci.yml` | wasm-verify toolchain + baseline updates |
| `apps/hr-apps/oaxaca-blinder-rs/engine/pay_equity_engine.wasm.sha256` | re-baseline (or sibling threaded baseline) |
| `apps/hr-apps/pay-equity-app/frontend/src/wasm/analysis.worker.js` | feature-detect + initThreadPool |
| `apps/hr-apps/pay-equity-app/frontend/vite.config.js` | dev-server COOP/COEP headers |
| `/home/deji/telos/audit-forge/webui/__init__.py` | COOP/COEP beside _apply_meridian_csp (line 125) |

### Files to create

| Path | Purpose |
|---|---|
| `apps/hr-apps/oaxaca-blinder-rs/.cargo/config.toml` | wasm target rustflags + build-std (strategy-scoped) |
| parity/benchmark test files (paths per Phase 4 spec) | bit-identical + memory-ceiling + wall-clock verification |

### Component graph (ASCII)

```
audit-forge Flask (COOP/COEP) ──serves──► Meridian frontend (Vite build)
Meridian analysis.worker.js ──initThreadPool──► pay_equity_engine.wasm (threaded)
pay_equity_engine (wasm-threads) ──feature──► oaxaca_blinder (rayon + seeded RNG)
scripts/build-wasm.sh ──produces──► pkg/ + sha256 baseline(s) ◄──verifies── ci.yml
```

### Composition

- `composes`: [] (no TM skills invoked)
- `composed_by`: [/build (consumes this spec)]

### Trigger surface

- New: none user-facing; `bash scripts/build-wasm.sh` gains threaded path (flag or default per strategy)
- Unchanged: Meridian worker postMessage API; oaxaca-cli; meridian-mcp; native cargo build/test

---

## Scope-Delta Log

| Date | Round | Change | Source | Decision |
|---|---|---|---|---|
| 2026-07-17 | Draft | Initial Charter authored | Claude (orchestrator) from Founder Intake | n/a |
| 2026-07-17 | Draft (pre-confirmation) | +In-Scope 9–13: statistical trust layer, determinism-as-prerequisite (incl. silent rep-discard fix), pre-threading memory profile, quantile API gap, Employers_data.csv fixture. Ordering constraint: determinism → memory profile → threading → validation suite. compiler.js coverage (founder item 5b) deferred to a separate MERIDIAN issue (Meridian frontend testing, out of engine-spec blast surface). Decimal-vs-f64 (5c) pending founder ruling at plan gate. | Founder pre-gate review, 5 items | pre-confirmation edit (no ceremony required) |

---

## [CLARIFY] Markers

None open — the two prior candidates (reproducibility strategy, prod host) were resolved at intake: strategy is In-Scope item 2 with a designed decision point (buildability gate); serving surface is local-only (audit-forge + Vite).

---

## Self-Validation Checklist

- [x] All four classification fields populated with allowed values
- [x] In Scope: bounded, numbered, each item traces to Founder Intake
- [x] Anti-Goals: every prohibition paired with a preferred alternative
- [x] Success Criteria: binary, each with verification method
- [x] Invariants: atomic, indexed, sourced
- [x] Assumptions: each carries a validity window or invalidation condition
- [x] ASM-expiry check: all `valid until` dates (2026-10-17) are future as of 2026-07-17 — none expired
- [x] Deliverable Shape: files tables + component graph + composition present
- [x] Scope-Delta Log: present with draft row
- [x] Tier is balanced (not lite) — Charter required, emitted
- [x] [CLARIFY] markers: none open
