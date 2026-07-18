# Council — Adversarial Verifier (red-team)

> Lens: refute the specs' load-bearing claims. Every finding cites a file:line Read this session.
> Date: 2026-07-18. Spec set: `_specify-wasm-rayon-multithreading-2026-07-17/` (7 final specs + audit + reconciliation).

Verdict: **SHIP_WITH_FIXES.** The two build-readiness CRITICALs are genuinely resolved and the fixes are sound in the direction they claim (structure-compatible reuse of `decompose_quantile`, single-owner `thread-cap.js`). But the reconciliation over-corrected into a new blind spot: it verified that the *point estimates* of the reused RIF path are correct and stopped there. The RIF path's **bootstrap inference is methodologically understated and unvalidated**, and the **seed does not propagate** into the newly-wired path. Both are silent-failure (#6) shaped on a `client-facing` defensibility tool. Neither blocks the build mechanically; both should be closed before the numbers reach a real filing.

---

## Claim-by-claim red-team

### Claim 3 — "decompose_quantile already produces correct per-predictor detail" → **REFUTED in part (inference), HELD for point estimates.**

The reconciliation (`phase4.5-reconciliation.md:18`) and audit RESOLUTION (`phase4-build-readiness-audit.md:175`) sell the reuse as *"numerically complete and tested."* Verified against source, that is true **only for point estimates.**

- `OaxacaBuilder::decompose_quantile()` (`builder.rs:720-766`) calls `calculate_rif` on each group **exactly once on the full sample** (`builder.rs:730-739`), substitutes RIF for the outcome (`:742-746`), then runs the standard bootstrap (`builder.rs:825-848`) over the **already-RIF-transformed** frames `df_a_global`/`df_b_global`.
- `calculate_rif` (`math/rif.rs:14-88`) estimates the quantile `q_τ` (`rif.rs:29-35`) and the density `f(q_τ)` (`rif.rs:65-72`) **inside that one call**. The bootstrap at `builder.rs:825` resamples the fixed RIF column and re-runs OLS; it **never re-invokes `calculate_rif`.** So each replicate treats `q_τ` and `f(q_τ)` as fixed constants.
- This is the exact subtlety the lens flagged. In the RIF-regression literature (FFL 2009; Rios-Avila `oaxaca_rif` 2020; R `rifreg`/`ddecompose`) the correct bootstrap **recomputes the RIF each replicate**, because `q_τ` and especially the KDE density `f(q_τ)` are estimated quantities with their own sampling variance. Holding them fixed **systematically understates the standard errors** → overstates t-stats/significance. `builder.rs:858-864` (`process_component`) computes `std_err`, `t_stat`, `p_value`, and CI from exactly these understated replicate estimates.
- **The trust-layer golden does not catch it.** `phase4-final-statistical-trust-layer.md` AC-6 (line 128) and the tolerance table (line 100) compare the RIF path to `ddecompose` on **per-predictor detail point estimates only** (rel 1e-4) plus a self-consistency adding-up identity (abs 1e-9). The shared-index bootstrap-SE golden (D-3, line 57-61) is scoped to the **mean** `oaxaca()` path, not the RIF quantile path. **No AC anywhere compares the RIF path's SEs/CIs against any reference.** Even if it did, it would fail, because the engine holds RIF fixed while `ddecompose bootstrap=TRUE` recomputes it.

For a tool whose Charter blast-radius is *"wrong numbers = defensibility harm"* (`spec-charter.md:16`), shipping quantile CIs that are known-narrow and unvalidated is the finding. Point estimates are fine; **inference is not, and the specs assert completeness they did not verify.**

*Robustness sub-note (MINOR):* `rif.rs:75` floors the density at `1e-8`. At extreme τ on 50k rows a small-but-real tail density yields large finite RIF values that dominate the OLS; a truly clamped `1e-8` yields RIF ≈ ±1e8 outliers. Not the primary issue, but the golden should exercise q10/q90 (D-6 uses `rifreg_probs=c(0.10,0.50,0.90)`, good) and assert finiteness.

### Claim 4 — "MM→RIF switch is coherent; nothing downstream depends on MM aggregate" → **HELD structurally, with one honest caveat.**

Verified the switch is structure-compatible. The WASM `decompose` entry already collapses quantile output into the **standard Oaxaca shape**: `analysis.rs:166-206` maps the quantile result into `(total, explained, unexplained, None, d_exp, d_unexp, None)` — the same 7-tuple the mean branch produces. `decompose_quantile` returns `OaxacaResults` (`builder.rs:720`), the *same type* the mean branch consumes, so `two_fold().detailed_explained()` mapping (`engine-parallel-surface.md:119`, `analysis.rs:261-269`) applies verbatim. The `{type,payload}` postMessage contract (`analysis.worker.js:53`) and `lib.rs:26` signature are untouched. Meridian's worker → dashboard.store → compiler/report consume `explained/unexplained/detailed`; the RIF path fills the same fields. **No structural break.** Option (a) is correctly the coherent choice; (b) is correctly rejected.

Caveat I could not fully refute but flag: the **numbers change** (MM and RIF-OLS are different estimators answering different questions — MM is a full distributional counterfactual, RIF is a local first-order approximation). Any Meridian analysis persisted to Supabase under the MM estimator will silently disagree with a re-run under RIF. That is expected for a methodology upgrade, and Meridian-frontend snapshot coverage was explicitly deferred to a separate MERIDIAN issue (`spec-charter.md:144`), so it is out of this blast surface — but it should be a founder-visible line at the gate, not buried in D5.

### Claim 1 — "bit-identical across thread counts" → **HELD.** I tried hard and could not break it for the `decompose` path.

- Every reduction in the `decompose` call graph is pinned sequential or order-preserving: bootstrap collect is indexed `into_par_iter().collect()` (order-preserving); `bootstrap_stats` is pinned sequential with a guard comment and an AC grep (`deterministic-rng.md` D6 + AC-2); the MM per-τ `par_iter().filter_map().collect()` is order-preserving and its failure set is deterministic given fixed τ (`quantile_decomposition.rs:221-230`); `empirical_quantile` sorts in place (`quantile_decomposition.rs:168`).
- The polars POOL-stub concern is addressed: `take` runs `try_apply_columns_par` collected **in column order** and the wasm POOL stub routes onto the single global rayon pool (`deterministic-rng.md` D3 L1-note; `engine-parallel-surface.md:81`, L4) — one pool, structured fork-join, deterministic placement.
- nalgebra is sequential by default (no `rayon` feature) so inner OLS is deterministic regardless of how reps are stolen across threads. clarabel (the only heavy solver that could carry a multithreaded BLAS reduction) is in the **optimize/frontier** path, **not** `decompose` — consistent with INV-02 being asserted only for `decompose`.

One honest scope observation (not a refutation): INV-02 byte-identity is asserted and tested **only for `decompose`** (deterministic-rng AC-6 "mean and quantile requests"; the mode-parity suite). `optimize`, `calculate_efficient_frontier`, and `check_defensibility` are **not** claimed bit-identical. If a client re-runs an optimization across thread counts and the adjustment ledger shifts in the last digits, that is a defensibility surprise the parity suite does not cover. Worth one sentence in the verification spec's scope.

### Claim 2 — "COOP/COEP on the document is sufficient; nested rayon workers inherit isolation" → **LARGELY HELD; one residual.**

The serving story checks out for the production (audit-forge loopback) path: `vite.config.js:10` sets `base: '/pay-equity/'`, so all subresources (worker JS, `.wasm`, and the wasm-bindgen-rayon-spawned worker helpers) are **same-origin under `/pay-equity/`**. Flask's `_apply_meridian_csp` is a **global `@app.after_request` keyed on the path prefix** (`webui/__init__.py:125-131`), so once COOP/COEP are added there they reach **every** response under `/pay-equity/` and `/api/`, including static asset responses — same-origin satisfies COEP `require-corp`. The nested-worker-from-`analysis.worker.js` spawn (the genuinely hard part) is already the spec's top-flagged unknown E3 with a specified main-thread-relay fallback (`phase4.5-reconciliation.md:31`) and works on the target evergreen browsers (ASM-05). I could not land a refutation here.

Residual (MINOR, gate-worthy): the specs say "COOP/COEP" but I did not find a stated choice of **`require-corp` vs `credentialless`**. Meridian is a Supabase app (`pay-equity-app/CLAUDE.md`: "Auth + file sync via Supabase", "AI narrative via a single Deno edge function"). Under `require-corp`, cross-origin **no-cors** subresources (e.g. a Supabase-storage avatar/logo `<img>` without `crossorigin`, a cross-origin font) are blocked; CORS `fetch()` to Supabase survives, but no-cors embeds do not. In the air-gapped offline deployment (esbuild comment, `vite.config.js:16`) this is moot, but the **Vite-dev / online** path could regress silently. Name `credentialless` (or audit cross-origin embeds) explicitly.

---

## Two coordination seams the reconciliation missed

### Seam 1 (MAJOR) — the seed does not propagate into the now-wired RIF quantile path.

`decompose_quantile` constructs a **fresh** `OaxacaBuilder::new(df_mod, ...)` at `builder.rs:752` and sets predictors/categoricals/bootstrap_reps/reference/normalize/weights (`:754-763`) — but **never `.seed(...)`**. After the deterministic-rng refactor adds `seed: Option<u64>` resolving to `DEFAULT_SEED` at `run()` (`deterministic-rng.md` D1, build step 3), the inner builder's `seed` is `None` → `DEFAULT_SEED`, **regardless of any seed set on the outer builder.** So `OaxacaBuilder::new(...).seed(X).decompose_quantile(τ)` bootstraps with `DEFAULT_SEED`, not `X` — the `.seed()`/`.seed_from_entropy()` API **silently no-ops on the RIF quantile path.**

- No build step covers this. deterministic-rng build steps 3-8 touch `builder.rs:825-848` (mean `run()`) and `quantile_decomposition.rs` (MM) — **never `builder.rs:720-766`.** engine-parallel-surface wires `decompose_quantile` in (step 6, line 162) but says "seeding comes from the deterministic-rng spec's rep stream" (line 121) — which is true for cross-thread identity but false for **seed-value control**, and neither spec adds the one-line `.seed(self.seed.unwrap_or(DEFAULT_SEED))` forward at `:759`.
- AC coverage misses it: AC-4/AC-5 (`deterministic-rng.md:212-213`) test `OaxacaBuilder::run()`, not `decompose_quantile`. A worker following the specs literally ships this and no test fails.
- Practical impact: INV-02's *cross-thread* clause still holds (fixed default seed → deterministic), so the build's headline criterion passes. But INV-02's *"same seed"* clause and D1's defensibility rationale ("`seed_from_entropy` records the drawn seed so the run stays reproducible after the fact") are **broken on the wired quantile path.** For an audit tool, an explicit seed that is silently ignored is exactly the kind of latent defect that surfaces in a deposition, not in CI.
- Fix: add `.seed(self.seed.unwrap_or(DEFAULT_SEED))` (or `seed_from_entropy` forwarding) at `builder.rs:759`; add an AC that `decompose_quantile` with `.seed(1)` vs `.seed(2)` yields non-equal bytes and `.seed(X)` reproduces.

### Seam 2 (MINOR) — deterministic-rng's quantile sections describe a path In-Scope 12 removes from WASM.

deterministic-rng D4/D5 (lines 104-148) seed the **MM simulation** (`quantile_decomposition.rs:215,244`) and its AC-6 asserts byte-identity for "quantile requests" assuming the WASM `decompose` quantile branch runs `QuantileDecompositionBuilder`. But engine-parallel-surface step 6 (line 162) **replaces `analysis.rs:166-206` so the WASM quantile branch runs `decompose_quantile` (RIF), and step 9 leaves the MM builder called only by CLI** (`engine-parallel-surface.md:165`). So deterministic-rng's AC-6 "quantile" byte-compare now exercises the RIF path (seeded via the mean-path D3/D5 fix — still deterministic, so it passes) while the spec's own prose (D4) describes seeding MM. Not a build-breaker — both paths get seeded — but a spec-drift the reconciliation left behind: a worker reading deterministic-rng in isolation seeds MM and expects AC-6 to exercise it, when the engine runs RIF. Add a one-line note in deterministic-rng that under In-Scope-12 option (a) the WASM quantile parity target is the RIF `decompose_quantile` path (mean-path-seeded), and the MM seeding serves CLI/native only.

---

## What held (credit where due)

- CRITICAL-1 fix is real: `decompose_quantile` (`builder.rs:720-766`) + `calculate_rif` (`math/rif.rs:14`) exist, are RIF-OLS, return the mean-path type, and are tested by `rif_test.rs`. The "reimplement RIF with a divergent KDE" risk is genuinely closed.
- CRITICAL-2 fix is real and single-owner: memory-budget owns `N_max_const`; meridian M7 writes `thread-cap.js`; worker imports it. The three-way "engine-parallelization" misattribution is gone.
- Bit-identity engineering (deterministic-rng) is unusually careful about float non-associativity — sequential-sum guard, order-preserving collects, owned-index resampling replacing the unseeded polars sampler, deterministic discard partition. This is the strongest spec in the set.
- Claims 1 and 4 survived a deliberate attempt to break them.
