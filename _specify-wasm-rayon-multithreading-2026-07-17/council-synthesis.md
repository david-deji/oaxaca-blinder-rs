# Post-Reconciliation Adversarial Council — Synthesis

> Orchestrator synthesis, 2026-07-18. 5 Opus seats (Systems Architect, Statistical Correctness, Build-Feasibility & Cost, NBJ, Adversarial Verifier), run over the CORRECTED 7-spec set + Charter + audit. Council files are audit trail only — this synthesis + the applied fixes are the authoritative output. All seats: **SHIP_WITH_FIXES** (no BLOCK).

## Convergence (multiple seats found the same thing → high confidence)

### CV-1 — Seed-propagation seam [CRITICAL/NBJ, MAJOR/Statistical, MAJOR/Adversarial, NIT/SysArch]
My two corrections interact badly. `OaxacaBuilder::decompose_quantile` (`builder.rs:752-765`) constructs a **fresh inner `OaxacaBuilder`** and never forwards `self.seed`. So wiring the RIF quantile path (In-Scope 12 fix) means the deterministic-rng spec's new `.seed()`/`.seed_from_entropy()` API **silently no-ops on the quantile path** — chosen-seed reproducibility, entropy round-trip, and `RunMetadata.seed` all break there. **Fix:** add `.seed(self.seed)` (+ entropy forwarding) to the inner builder at `builder.rs:759`; state it in BOTH engine-parallel-surface AND deterministic-rng specs; add an AC that `.seed(1)` vs `.seed(2)` on `decompose_quantile` yields non-equal bytes and `.seed(X)` reproduces. → APPLIED.

### CV-2 — In-Scope 12 option (a) causes cross-surface method divergence [MAJOR/NBJ, MAJOR/Statistical adding-up, NIT/SysArch]
Under option (a) (WASM quantile → RIF), `oaxaca-cli` (`main.rs:247`) stays on the MM builder → the **same tool returns different quantile numbers per surface** (browser vs CLI/MCP). AND the trust-spec adding-up identity was still pointed at the MM aggregate (`:267-271`); under (a) the aggregate is the RIF run()'s own output, making the identity a tautology or a contradiction. **Fix (spec):** re-point the adding-up identity at the RIF run's own aggregate, relabel it a self-consistency guard (not a trust check), rely on the ddecompose golden for independent validation → APPLIED. **Fix (founder):** decision (a) must ALSO decide the CLI — switch `main.rs:247` to `decompose_quantile` too (one method everywhere) or explicitly accept + document CLI-vs-browser divergence. → ADDED TO GATE.

## Standalone MAJOR findings

### MJ-1 — INV-02 native↔wasm byte-identity is likely UNACHIEVABLE [SysArch]
Cross-ISA floating-point: transcendentals differ between native glibc libm and wasm32 libm — `.exp()` (`rif.rs:69`), `.powf` (`rif.rs:59`), statrs `Normal::cdf` (`probit.rs:41,68`). So `sha256(json_native) == sha256(json_wasm)` cannot hold. Internal inconsistency: `parity_test.rs:24` already uses `TOLERANCE=1e-6`, not byte-equality. The founder's "bit-identical required" ruling is achievable and correct for THREADING (across thread counts, same platform); across ISA it is a different, likely-impossible claim. **Proposed reframe (founder confirms — touches the ruling):** INV-02 = byte-identical *within a platform across thread counts* (the real threading-safety property) + *tolerance-parity* (1e-6) for the native↔wasm leg. → GATE (NEW decision d).

### MJ-2 — Strategy B cross-machine build-std reproducibility is fragile [SysArch, corroborated Build-Feasibility]
`toolchain AC-12/AC-16` require CI (ubuntu-latest) raw-wasm sha256 to equal the dev-box-committed baseline — a cross-machine `-Zbuild-std` equality (codegen-unit order, embedded std paths, toolchain identity) that Open Item 5 admits is unproven. Undercuts D7's "B is lower-risk." For a client-facing tool, **Strategy A's untouched-stable baseline is arguably lower reproducibility-risk than the spec stated.** **Mitigation if B:** generate the baseline inside a pinned CI/container (not the dev box), or a two-host preflight. → GATE (sharpens decision a; council leans A).

### MJ-3 — In-Scope 12 golden tolerance 1e-4 likely unachievable [Statistical]
RIF detail ∝ 1/f(q_τ). The engine's inline density (`rif.rs`: Silverman min(std, IQR/1.34), nearest-rank IQR, single-point kernel) structurally differs from R `ddecompose`'s (`bw.nrd0`, 512-point FFT grid) → per-predictor coefficients diverge ~1e-3 to 1e-2, not 1e-4. **Fix:** don't ship a 1e-4 tolerance on faith — empirically measure agreement on the fixture and pin the tolerance to it, OR pin a custom-R golden matching `rif.rs`'s exact density for a true ~1e-6 cross-check. → APPLIED (tolerance reframed to measured-then-pinned).

### MJ-4 — E1 nightly pin will likely fail the locked dep graph [Build-Feasibility, corroborated SysArch]
`nightly-2024-08-02` (~13 months older than the `1.90.0` stable pin) probably cannot compile the *locked* wasm dep graph; the "~2-week forward-bump" is under-estimated. **Fix:** trim the wasm subgraph (`default-features=false` on the `oaxaca_blinder` dep to drop comfy-table; cfg-gate askama off wasm32) for a low-MSRV wasm graph the tested nightly can build with `--locked` intact; OR run E1 empirically against the actual wasm subgraph and pin to whatever nightly first satisfies deps + `-Zbuild-std(wasm32)`. → APPLIED (subgraph-trim added to toolchain spec; E1 hardened).

### MJ-5 — decompose_quantile bootstrap inference is understated + untested [Adversarial]
The RIF path is tested only for POINT estimates (`rif_test.rs` smoke). Its bootstrap SEs/CIs use a **fixed RIF** (RIF computed once from the full sample, then the RIF column is bootstrapped) rather than recomputing the RIF quantile+density inside each replicate → SEs are understated, and no AC validates them. For a client-facing defensibility tool, unvalidated quantile CIs should not ship. **Fix:** document the fixed-RIF understatement + add an AC comparing RIF-path per-predictor SEs against `ddecompose` bootstrap SEs; note "recompute `calculate_rif` inside each replicate" as the rigorous upgrade; exercise q10/q90 for finiteness (density floor 1e-8). → APPLIED (documented + AC + upgrade noted); rigor level is a founder quality call → GATE note.

### MJ-6 — thread-cap 8-ceiling asserted but enforced nowhere [NBJ, MINOR/SysArch]
memory-budget D3 writes the UNCLAMPED `N_max_const`; the worker computes `min(hardwareConcurrency, MEMORY_THREAD_CAP)` — dropping the `8`. **Fix:** bake the clamp into the value — `N_max_const := min(floor(...), 8)` at emission; AC-M6 asserts the `min(_,8)` is present. → APPLIED.

## MINOR (applied or noted)
- **seed_from_entropy on wasm** — `oaxaca_blinder/Cargo.toml` has no `getrandom` dep; `getrandom::getrandom` in the new `rng.rs` won't link on wasm without it. Fix: scope `seed_from_entropy()` to native (default path uses `DEFAULT_SEED`) OR add the dep+`wasm_js` feature. → APPLIED (scoped to native + dep note).
- **inconsistent in-crate quantile estimators** — `rif.rs` R-Type-7 vs `quantile_decomposition.rs` nearest-rank; under option (a) the CLI (MM) and browser (RIF) use different quantile definitions. → noted in gate decision (c).
- **COEP require-corp vs credentialless** — offline/loopback bundle → cross-origin subresource breakage is N/A here, but state `require-corp` explicitly. → APPLIED (one line in meridian).
- **INV-02 scope** — bit-identity tested only for `decompose`, not optimize/frontier/defensibility. → noted (decompose is the parallel hot path; others are single-solve/scalar, lower risk).
- **RIF total_gap vs empirical quantile gap the UI labels** — nothing asserts the RIF-approximated gap ≈ the actual empirical quantile gap. → noted (documented FFL local-approx limitation).

## Net
No BLOCK. The corrected specs are buildable. The seed-propagation seam (CV-1) is the one genuine defect introduced by my own corrections — now fixed. The council's biggest contribution is sharpening the founder decisions: INV-02 must be reframed (MJ-1), Strategy A is more competitive than stated (MJ-2), and In-Scope 12 option (a) has a CLI-divergence consequence (CV-2) and an SE-rigor sub-decision (MJ-5).
