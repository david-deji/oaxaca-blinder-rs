# Council — Statistical Correctness Lens (econometrician)

> Reviewer: adversarial statistical-correctness seat, buildability gate
> Date: 2026-07-18
> Scope: INV-02 bit-identity + In-Scope 12 (RIF quantile detail) + golden references
> Method: every finding cites a file:line Read against the live repo this session.

## Verdict: SHIP_WITH_FIXES

The determinism architecture is sound and the MM→RIF aggregate switch (option a) is the
correct call. But three of the suite's central *statistical-trust* acceptance criteria are
either **unachievable as written**, **testing a tautology**, or **wired against the wrong
aggregate**. None require re-running research; all are resolvable at the buildability gate.

---

## Affirmations (verified sound)

**INV-02 bit-identity design is correct.** The scheme in `phase4-final-deterministic-rng.md`
D2 (`unit_rng(master, purpose, unit) = ChaCha8Rng::seed_from_u64(master).set_stream((purpose<<40)^unit)`)
is a closed-form function of `(master, purpose, unit)` with zero dependence on thread id,
clock, or atomic counter — the SC-03 proof-sketch holds. Combined with (a) owned index-vector
resampling via `DataFrame::take` (order-preserving gather, no float reduction), (b) the
order-preserving `into_par_iter().collect()` + **sequential** `RepOutcome` fold for discard
(D5), and (c) the sequential-summation guard on `bootstrap_stats` (D6, `inference.rs:4`),
the values computed in Rust are genuinely bit-identical across thread counts. The silent
`filter_map(...).ok()` discard at `builder.rs:827/846-847` is real (I confirmed it) and the
D5 deterministic partition is the right fix. **This part of the spec is trustworthy.**

**Option (a) MM→RIF aggregate is more defensible — affirmed on three grounds, with one caveat.**
1. *Coherence*: the RIF aggregate (`decompose_quantile`→`run()`) and the RIF per-predictor
   detail come from one method, so detail sums to aggregate. The MM aggregate
   (`quantile_decomposition.rs:267-271`) is a different estimator; RIF detail cannot sum to it.
2. *Determinism*: the RIF aggregate **point estimate** is closed-form — mean-path
   `run_single_pass` calls no RNG (confirmed: RNG lives only in the bootstrap closure and in
   the MM `run_single_pass` at `quantile_decomposition.rs:215,244`). The MM aggregate point
   estimate is simulation-noise-laden and seed-dependent even after seeding. A deterministic
   closed-form aggregate is strictly more defensible for an audit tool.
3. *Validatability*: RIF-OLS has an independent golden (`ddecompose reweighting=FALSE`); MM
   per-covariate attribution is path-dependent and non-additive (W10) — unvalidatable.
   **Caveat:** option (a) re-baselines every displayed quantile number (MM→RIF), and RIF
   inherits the local-linear `E[RIF|X]` approximation error, worst at extreme τ (W10 limit i).
   The founder must accept a methodology change on already-delivered analyses.

**W5 manual-loop mean golden and the property identities are well-posed.** Shared 0-based
index matrix + R `lm()` on `data[idx+1,]` vs Rust `take(&IdxCa)` gives exact-index SE
comparison (rel 1e-3). Label-swap antisymmetry and scale-invariance are genuine
non-tautological algebraic properties. `ddecompose(reweighting=FALSE, rifreg_statistic="quantiles")`
is the correct *form* of reference (one-stage RIF-OLS = FFL-2009 = what `decompose_quantile`
computes). No finding on these.

---

## Findings (ranked)

### F1 — MAJOR: per-predictor quantile golden tolerance (rel 1e-4) is likely unachievable — density-estimator divergence propagates ~linearly into every RIF coefficient

`statistical-trust-layer.md` tolerance table sets "Per-predictor quantile detail | engine vs
`ddecompose` | rel 1e-4, abs floor 1e-8" (line 100), and AC-6/AC-7 gate on it. This is
optimistic by 1–2 orders of magnitude.

The engine's RIF (`math/rif.rs`) scales inversely with the density estimate `f_Y(q_τ)`:
`RIF = q_τ + (τ − 1{y≤q_τ})/f(q_τ)` (`rif.rs:83`). Because `E[τ − 1{y≤q_τ}] = 0`, the RIF is
`q_τ` plus a mean-zero term scaled by `1/f`, so every regression coefficient — hence every
per-predictor `detailed_explained_k`/`detailed_unexplained_k` — is **proportional to `1/f`**.
Relative error in the detail ≈ relative error in `f` between the two implementations.

The two `f` estimators diverge structurally:
- **Bandwidth constant**: engine `min_spread = min(std_dev, iqr/1.34)` (`rif.rs:52`); R
  `bw.nrd0` (used by `density()`/`rifreg`/`ddecompose`) uses `IQR/1.349`. ~0.7% bandwidth
  gap in the IQR-limited regime.
- **IQR itself**: engine computes q25/q75 by nearest-rank ceil-index (`rif.rs:43-49`), R's
  `IQR()` uses type-7 interpolation. Different IQR → different bandwidth.
- **Evaluation**: engine sums the Gaussian kernel **directly at the single point q_τ**
  (`rif.rs:65-72`); R `density()` builds a 512-point FFT grid with reflection/padding and
  **linear-interpolates** at q_τ. Grid-interp vs exact point eval differ ~1e-3–1e-2.

Each of these alone exceeds rel 1e-4; together the per-predictor detail will diverge from
`ddecompose` by ~1e-3–1e-2 relative. AC-6/AC-7 will fail, forcing either a silent tolerance
loosening (which weakens the very "trust" claim the suite exists to make) or an out-of-scope
change to the engine's density internals.

**Fix (build-gate, no research):** before committing the tolerance, run `ddecompose` on
`employers_trust_fixture.csv` and byte-compare to `decompose_quantile` empirically; set the
tolerance to the *measured* agreement and record it, OR pin the golden to a density estimator
matching `rif.rs` (custom R RIF using the identical Silverman-1.34 + nearest-rank IQR +
point-eval), which turns the golden into a true cross-implementation check at ~1e-6. Do not
ship a 1e-4 tolerance on faith.

### F2 — MAJOR: `decompose_quantile` does not forward the seed to its inner builder — chosen-seed reproducibility and `RunMetadata.seed` silently break on the RIF quantile path that option (a) makes the displayed aggregate

`builder.rs:752-765`: `decompose_quantile` constructs a **fresh** `OaxacaBuilder::new(df_mod,…)`
and sets `.predictors/.categorical_predictors/.bootstrap_reps/.reference_coefficients/
.normalize/.weights` — but **never `.seed(self.seed)`**. Under the determinism refactor the
inner builder's `seed` field defaults to `None` → resolves to `DEFAULT_SEED` at `run()`.

Consequences on the exact path option (a) wires into WASM (`engine-parallel-surface.md` D5):
- INV-02 (thread-count bit-identity) **still holds** — `DEFAULT_SEED` is fixed, so across
  thread counts the quantile output is identical. The master criterion is safe.
- But **user seed control is dropped**: `.seed(s)` on the caller is ignored inside
  `decompose_quantile`; the bootstrap always uses `DEFAULT_SEED`.
- `.seed_from_entropy()` becomes a **no-op** for quantile, and `RunMetadata.seed` on the
  returned `OaxacaResults` records `DEFAULT_SEED`, not the entropy value — so AC-5
  (entropy round-trip) would fail if applied to this path, and D7's defensibility claim
  ("reproducible via the recorded seed") is false for the quantile aggregate.

The deterministic-rng spec's site inventory (three sites) covers the mean bootstrap, the
quantile-bootstrap, and the MM RNG — but **no build step forwards `self.seed` through the
`decompose_quantile` inner-builder construction at `builder.rs:752`.** The spec asserts
"decompose_quantile inherits the fix via run()" — true for thread-count bit-identity, false
for seed *control*.

**Fix (one line):** add `.seed(self.seed.unwrap_or(DEFAULT_SEED))` (or forward `Option<u64>`)
to the inner builder at `builder.rs:752-765`, and add an AC asserting a chosen seed on a
`decompose_quantile` call round-trips into `RunMetadata.seed` and reproduces byte-identically.

### F3 — MAJOR: the per-predictor adding-up identity is wired against the wrong aggregate (stale MM reference survives the RESOLUTION) and, once corrected, is a near-tautology — not the independent trust asset the spec frames it as

`statistical-trust-layer.md` D-6 (line 77) and AC-6 (line 128) still assert
`sum_k(endowment_k) == aggregate_characteristics` "against the engine's own aggregate output
(**the same aggregate `quantile_decomposition.rs:267-271` produces**)". That anchor is the
**MM** aggregate. Under option (a) the aggregate comes from RIF `decompose_quantile`, not MM.
- If a builder tests RIF detail-sum against the MM aggregate (as the text literally says),
  the identity **fails** (different estimators).
- The audit RESOLUTION (line 175) claims D-6 was updated to "validate EXISTING code," but the
  MM-aggregate reference at line 77 **survived** — a genuine residual contradiction with
  `engine-parallel-surface.md` AC-8 (line 177), which correctly says the aggregate is the RIF
  path's own output.

Worse, once the reference is corrected to the RIF run()'s **own** aggregate: in the standard
OB decomposition the aggregate explained equals `Σ_k detailed_explained_k` **by construction**
— both `two_fold_agg` and `detailed_explained` are built from the same `point_estimates`
(`builder.rs:876-893` vs `:921-932`). So `sum_k(detail) == aggregate` at 1e-9 tests only f64
summation order — the engine's arithmetic, not method correctness. It is a fine *regression
guard* but must not be presented as an independent statistical-trust check; the only
independent check of the split is the `ddecompose` golden (F1).

**Fix:** delete the `267-271` MM reference in D-6/AC-6; point the identity at the RIF run()'s
own aggregate and relabel it a "self-consistency regression guard," not a trust assertion;
rely on the (repaired) `ddecompose` golden for independent validation of the split.

### F4 — MINOR→MAJOR: the `ddecompose` quantile golden's reference-coefficient (two-fold weighting) scheme is not pinned to the engine's `Pooled`/Neumark default

The WASM quantile branch defaults to `ReferenceCoefficients::Pooled` (`analysis.rs:154`).
`ddecompose::ob_decompose` uses its own counterfactual-coefficient convention. D-2 pins the
**group** reference (which Gender level is A vs B, line ~51) but does **not** pin the two-fold
*weighting/reference-coefficient* scheme for the quantile golden (D-6). A Pooled-vs-GroupB
mismatch shifts the explained/unexplained split by far more than rel 1e-4 on individual
channels — an apparent golden "failure" that is really a scheme misalignment. D-3 shows the
authors know the scheme dimension for the *mean* oaxaca() golden ("GroupB + Pooled/Neumark
schemes") but it is not restated for the RIF quantile golden.

**Fix:** in `gen_trust_goldens.R`, pin `ddecompose`'s reference/weighting to match the
engine's `Pooled` default and record it in `_meta`; assert `_meta.reference_scheme` in
`quantile_detail_golden_test.rs`.

### F5 — MINOR: two inconsistent quantile estimators coexist in-crate

`math/rif.rs:25-35` computes the sample quantile via R-Type-7 interpolation (correct);
`quantile_decomposition.rs:169` computes it via truncated nearest-rank
(`(data.len() as f64 * quantile) as usize`). Under option (a) the MM path is no longer the
displayed aggregate so this is moot for Meridian, but `QuantileDecompositionBuilder` stays
exported for the CLI (`lib.rs:84`), so the CLI and the WASM path now report quantile numbers
computed with different quantile definitions. Document the divergence, or align the MM path's
`empirical_quantile` to Type-7 for consistency across surfaces.

---

## Bottom line

INV-02 is genuinely achieved and option (a) is the right, more-defensible choice — approve
both. But the "statistical trust layer" that is supposed to *prove* the numbers is, as
specced, resting on a golden tolerance that won't hold (F1), a reproducibility path that
drops the seed (F2), and an adding-up identity pointed at the wrong aggregate that collapses
to a tautology once fixed (F3). These are the load-bearing correctness ACs. Resolve F1–F3 at
the gate before dispatch; F4–F5 can be build-time cleanups.
