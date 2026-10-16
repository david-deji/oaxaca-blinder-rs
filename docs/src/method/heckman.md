# Heckman selection correction

Pay is observed only for people who work. If who works is not random, for example if better-educated people are more
likely to be employed, the regression on the employed is biased by selection. The Heckman two-step correction adjusts
for it.

1. A **selection equation**, a probit of employment on the selection predictors, gives for each person the inverse
   Mills ratio \\( \lambda = \phi(z'\gamma) / \Phi(z'\gamma) \\).
2. The **outcome equation** is the usual regression on the people with observed pay, with \\( \lambda \\) added as a
   regressor:

   \\[ y = X\beta + \rho\sigma\_u \lambda(Z\gamma) + \epsilon . \\]

The decomposition then reports a **selection** component beside the explained and unexplained parts
(`detailed_selection`).

## Use

```rust,ignore
builder.heckman_selection("employed", ["age", "education", "marital_status"]);
```

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience \
    --selection-outcome employed \
    --selection-predictors age,education,marital_status
```

The selection predictors should include at least one variable that affects employment and not pay (an exclusion
restriction). Without one the correction is identified only through the shape of the normal distribution.

With a selection model, categorical normalisation is skipped on the compared group, the reference group and the
pooled fit together, and `run_metadata.normalization` says so.

A Heckman selection model cannot be combined with `Pooled` or `PooledNoIndicator`: the run panics with a matrix
dimension mismatch. Use `GroupA`, `GroupB`, `Weighted` or `Cotton`.

## Status

The correction is tested on simulated data with a known answer. There is no R or Stata run to compare it with in
`verification/`. Treat its numbers as less checked than the mean and quantile decompositions.
