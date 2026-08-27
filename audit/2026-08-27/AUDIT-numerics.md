# Audit — numerical core and inference

Lane: the arithmetic that produces shipped numbers. Read at the working tree, 2026-08-27.
Severity is judged against one standard: **would this produce a wrong number that looks right?**

---

## HIGH — Sample weights are silently ignored inside the RIF transform

The quantile decomposition path honours weights everywhere except the one place that defines the
outcome it regresses.

`OaxacaBuilder` carries `weights_col: Option<String>` (`builder.rs:87`). It is honoured in
`clean_dataframe` (`:1042-1044`), in `weighted_levels_present` (`:185-186`), and in the regression
itself — `ols()` takes `weights: Option<&DVector<f64>>` and forms the Gram matrix on
`sqrt(weight)`-scaled data (`math/ols.rs:52-72`).

It is not honoured in the RIF transform:

```rust
// builder.rs:1028
fn rif_replace_outcome(&self, g: &DataFrame, quantile: f64) -> Result<DataFrame, OaxacaError> {
    let rif = calculate_rif(g.column(&self.outcome)?.as_materialized_series(), quantile)
```

`calculate_rif(series, quantile)` (`math/rif.rs:14`) takes no weights and has exactly one call
site, the one above. Inside it, all three quantities are computed unweighted:

- the sample quantile `q_tau`, from an unweighted sort (`rif.rs:38-49`)
- the density `f(q_tau)`, from an unweighted Gaussian KDE (`rif.rs:76-85`)
- `RIF = q_tau + (τ − I(y ≤ q_tau)) / f(q_tau)`, per row (`rif.rs:90-97`)

So on a weighted quantile decomposition the outcome is transformed as though every row weighed 1,
and is then regressed with weights. The result is coherent-looking and computed on an incoherent
basis. Nothing errors.

**Reach:** the RIF path is the shipped quantile path on all three surfaces — WASM (`quantile:
Option<f64>`), CLI (`--quantiles`), MCP. `decompose_quantile` is the only quantile method any
shipped surface calls.

**Why no test caught it:** `oaxaca_blinder/tests/weights_test.rs` contains exactly one test,
`test_weighted_decomposition` (`:5`), and it never calls `decompose_quantile`. Weights are verified
on the mean path and unverified on the quantile path — the feature is tested where it works.

**Who hits it:** anyone using frequency weights, which in pay-equity data is the ordinary case
where one row stands for N incumbents, plus any sampling-weight correction.

---

## MEDIUM — A t-statistic of exactly 0.0 is also the "could not compute" value

`builder.rs:1229-1233`:

```rust
let t_stat = if std_err.abs() > 1e-9 { point / std_err } else { 0.0 };
```

Every IEEE-754 comparison against NaN is false, so a NaN standard error takes the `else` branch and
reports `t_stat = 0.0`. A t of exactly zero is the strongest possible statement of "no effect".
The failure state and the most confident null result are the same number.

`std_err` is NaN whenever the bootstrap distribution has fewer than two usable replicates:
`bootstrap_stats` guards the empty case (`inference.rs:10-12`) but not `n == 1`, where
`sum/(n-1.0)` is `0.0/0.0` (`inference.rs:16-22`).

**Partial mitigation, and it is real:** `RunMetadata` carries `bootstrap_reps_discarded`
(`rng.rs:103`), so a reader who inspects the metadata can see replicates failed. Nothing correlates
the two for them.

---

## MEDIUM — Nothing asserts the confidence interval contains the estimate

`ComponentResult` ships `estimate`, `std_err`, `t_stat`, `p_value`, `ci_lower`, `ci_upper` together
(`types.rs:176-184`). The interval is the raw percentile interval of the bootstrap distribution,
with no bias correction — and the ingredient for one is passed in and thrown away:

```rust
// inference.rs:9
pub fn bootstrap_stats(estimates: &[f64], _point_estimate: f64) -> (f64, f64, (f64, f64)) {
```

The underscore is the whole finding. `builder.rs:1227` calls it as `bootstrap_stats(&estimates,
point)`; the difference between the bootstrap mean and `point` is exactly the bias estimate the
percentile method omits, and it is discarded at the parameter.

For a skewed statistic — and RIF quantile estimates at extreme τ are the textbook case — the
percentile interval can sit off the point estimate far enough to exclude it. The audit-visible
outcome is a report stating a gap of $X with a 95% interval that does not contain $X.

**Why no test would catch it:** the only appearance of `ci_lower`/`ci_upper` anywhere in either
test suite is `cli_wasm_parity_test.rs:124`, which compares the two surfaces field by field. Two
surfaces computing the same interval the same way will agree perfectly whether or not it contains
the estimate. That is failure mode #9 — the check verifies agreement, not correctness.

The missing assertion is one line: `ci_lower <= estimate <= ci_upper`.

---

## MEDIUM — Two Gaussian KDE implementations, and the shipped one is the weaker

`math/kde.rs::kde()` accepts `weights: Option<&[f64]>` and pairs with a `silverman_bandwidth`
helper. Its only caller is `dfl.rs:1`.

`math/rif.rs` hand-rolls both: Silverman inline (`:53-74`) and the Gaussian kernel inline
(`:76-85`), without weight support. This is the root cause of the HIGH finding above — the module
built for the job supports weights, and the shipped path does not use it.

Two implementations of one estimator drift. One already has.

---

## LOW–MEDIUM — The density floor converts an undefined estimate into a finite wrong one

`rif.rs:87`:

```rust
// Avoid division by zero or extremely small density
let density = if density < 1e-8 { 1e-8 } else { density };
```

`RIF = q_tau + (τ − I)/density`. At the clamp, the RIF values reach ±1e8 scale, the RIF-OLS returns
enormous coefficients, and the decomposition returns a large finite number with no signal that the
density estimate failed.

What makes this worth naming is the contrast three lines earlier in the same file. The `n < 2` case
is refused loudly, with a comment that states the principle exactly:

> *"Sample variance and the KDE bandwidth are both undefined for n<2, so silently returning the
> caller's own series as its 'RIF' would be a silent wrong result (agentic failure mode #6) rather
> than a safe no-op. Fail loudly instead."*

The module knows the rule and applies it to one degenerate case while clamping the other.

---

## LOW / latent — WLS degrees of freedom use row count, not the sum of weights

`math/ols.rs:112` computes `sigma_squared = sse / (n_obs - k)` where `n_obs` is `x.nrows()`
(`:66`). For frequency weights the denominator should be `Σw − k`; for analytic weights the row
count is right. The code accepts either kind without distinguishing them.

**Not live.** `OlsResult.vcov` has no consumer outside a zeroed test fixture in
`normalization.rs:76`; only `coefficients` and `residuals` are read, and uncertainty ships from the
bootstrap. This becomes a real defect the moment anyone surfaces analytic standard errors.

---

## Checked and clean

- **OLS solve** — Cholesky rather than explicit inversion, fails closed on a singular Gram matrix
  instead of pseudo-inverting, `n > k` guard with a named error, negative weights rejected
  (`ols.rs:59-63, 88-101`).
- **Bootstrap summation order** — `inference.rs:5-8` pins sequential summation with the reason
  stated: float addition is non-associative and a rayon reduce would break bit-identity across
  thread counts. The invariant is named `INV-02` and enforced by comment at the one site that
  matters.
- **Discard accounting** — failed replicates are counted and surfaced rather than silently reducing
  the sample (`rng.rs:103`).
