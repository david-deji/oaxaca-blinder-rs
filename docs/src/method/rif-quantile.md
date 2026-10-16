# RIF quantile decomposition

The mean decomposition cannot see where in the pay distribution a gap sits. A quantile decomposition can. This
library uses the recentered influence function (RIF) of Firpo, Fortin and Lemieux.

## Method

For a chosen percentile \\( \tau \\), the RIF of the outcome \\( y \\) is

\\[ \mathrm{RIF}(y; q\_\tau) = q\_\tau + \frac{\tau - \mathbf{1}\{y \le q\_\tau\}}{f\_Y(q\_\tau)} , \\]

where \\( q\_\tau \\) is the sample percentile and \\( f\_Y \\) the density of the outcome at it. The mean of the RIF
is the percentile itself. Regressing the RIF on the predictors in each group and applying the
[Oaxaca-Blinder decomposition](oaxaca-blinder.md) to those regressions gives an explained and an unexplained part of
the gap at that percentile. The reference-coefficient schemes and categorical normalisation work as in the mean case.

## What this implementation does

- The percentile is R `quantile(type = 7)`.
- With weights, `Frequency` weights use type 7 on the expanded sample and `Relative` weights use
  `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)`. See [Weights](../user-guide/weights.md).
- The bootstrap recomputes the percentile and the density inside every replicate, so intervals include the
  uncertainty of the percentile.
- The quantile path is reached from the library with `decompose_quantile(tau)`, one call per percentile, from the
  command line with `--analysis-type quantile --quantiles`, and from the engine with the `quantile` argument.

## Percentile warnings

A percentile of pay that sits on a grid of round values is not well defined. The engine reports `tie_share` when more
than 5% of a group's rows equal the percentile value, and `ecdf_offset` when the sample distribution function at the
percentile is far from \\( \tau \\). See [Diagnostics and warnings](../reference/diagnostics.md).

## Checked against

R `ddecompose::ob_decompose()` with `rifreg_statistic = "quantiles"`, and R `quantreg` for the quantile-regression
solver. Details in [Reproducing results in R](../reproducing/r.md).

## Machado-Mata

The library also has a simulation-based quantile decomposition (Machado and Mata), `QuantileDecompositionBuilder`.
It is deprecated, has no external check, and is not reachable from the command line, the engine or the MCP server.
