# Oaxaca-Blinder decomposition

## Setting

The decomposition is due to [Oaxaca (1973)](references.md#oaxaca-1973) and [Blinder (1973)](references.md#blinder-1973).
Two groups, A and B. B is the **reference group** you name. A is every other row. For each group, an ordinary least
squares regression of the outcome on the predictors gives coefficients \\( \beta\_A \\) and \\( \beta\_B \\). The
average outcome is the average predictor vector times the coefficients, so the gap in means is

\\[ \bar Y\_A - \bar Y\_B = \bar X\_A' \beta\_A - \bar X\_B' \beta\_B . \\]

`total_gap` in a result is this quantity: the compared group's mean minus the reference group's mean.

## Two-fold decomposition

Pick a coefficient vector \\( \beta^\ast  \\) that stands for pay without a gap. Then

\\[ \bar Y\_A - \bar Y\_B = \underbrace{(\bar X\_A - \bar X\_B)' \beta^\ast }\_{\text{explained}} +
  \underbrace{\bar X\_A' (\beta\_A - \beta^\ast ) + \bar X\_B' (\beta^\ast  - \beta\_B)}\_{\text{unexplained}} . \\]

The explained part is the gap that the groups' different average characteristics produce at the coefficients
\\( \beta^\ast  \\). The unexplained part is the rest. Both are reported with a bootstrap standard error, p-value and
percentile interval. The choice of \\( \beta^\ast  \\) is the subject of
[Reference coefficients](reference-coefficients.md).

Each part also splits by predictor into a **detailed** decomposition. Variable \\( k \\) contributes
\\( (\bar X\_{A,k} - \bar X\_{B,k}) \beta^\ast \_k \\) to the explained part and
\\( \bar X\_{A,k} (\beta\_{A,k} - \beta^\ast \_k) + \bar X\_{B,k} (\beta^\ast \_k - \beta\_{B,k}) \\) to the unexplained part. The
intercept appears as a row under the reserved name `__ob_intercept__`; the printed tables show it as `(intercept)`.

## Three-fold decomposition

The three-fold form uses no \\( \beta^\ast  \\). It splits the gap into

\\[ \underbrace{(\bar X\_A - \bar X\_B)' \beta\_B}\_{\text{endowments}} +
  \underbrace{\bar X\_B' (\beta\_A - \beta\_B)}\_{\text{coefficients}} +
  \underbrace{(\bar X\_A - \bar X\_B)' (\beta\_A - \beta\_B)}\_{\text{interaction}} . \\]

It is always computed from the raw coefficient vectors, because it does not depend on how categorical variables are
coded and the normalised version would.

## Bootstrap inference

Every estimate in a result (the two parts, each detailed row, each percentile gap) is repeated on bootstrap resamples
of the rows. With \\( B \\) replicates \\( \hat\theta\_1, \dots, \hat\theta\_B \\):

- The **standard error** is the sample standard deviation of the replicates (denominator \\( B - 1 \\)).
- The **95% interval** is the pair of replicates at the 2.5th and 97.5th percentile ranks: sort them and take the
  ones at positions \\( \lfloor 0.025B \rfloor \\) and \\( \lfloor 0.975B \rfloor \\), counting from 0, with no
  interpolation. It is not centred on the estimate and not symmetric. It needs at least 41 replicates; below that it
  is `NaN`.
- The **p-value** tests that the quantity is zero. It is twice the smaller of two shares, the share of replicates
  that are zero or above and the share that are zero or below, capped at 1. A replicate equal to zero counts on both
  sides. If every replicate has the same sign the p-value is `0.0000`.

The p-value is read off the replicates themselves, not from a normal approximation or a recentred null
distribution. See [Bootstrap and seeds](../user-guide/bootstrap-and-seeds.md) for replicate counts and seeds.

## What is checked

The aggregate and detailed arithmetic is compared with `ddecompose::ob_decompose()` in R. The regressions are
compared with R `lm()` and statsmodels. Bootstrap standard errors are compared with an R resampling run on the same
indices. See [Reproducing results in R](../reproducing/r.md) for the packages and tolerances.

## Limits to keep in mind

The decomposition describes two groups' averages in a linear model. The unexplained part is the part the model's
predictors do not explain. It collects the effect of every omitted variable as well as any difference in treatment,
and the data cannot tell those apart. A different set of predictors gives a different split.
