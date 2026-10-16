# RIF quantile decomposition

The mean decomposition cannot see where in the pay distribution a gap sits. A quantile decomposition can. This
library uses the recentered influence function (RIF) of [Firpo, Fortin and Lemieux (2009)](references.md#firpo-fortin-lemieux-2009).

## Method

For a chosen percentile \\( \tau \\), the RIF of the outcome \\( y \\) is

\\[ \mathrm{RIF}(y; q\_\tau) = q\_\tau + \frac{\tau - \mathbf{1}\\{y \le q\_\tau\\}}{f\_Y(q\_\tau)} , \\]

where \\( q\_\tau \\) is the sample percentile and \\( f\_Y \\) the density of the outcome at it. The density is
a Gaussian kernel estimate, evaluated exactly at \\( q\_\tau \\), with the bandwidth
\\( h = 0.9 \min(s, \mathrm{IQR}/1.34) n^{-1/5} \\) (Silverman's rule of thumb; \\( s \\) is the standard deviation
and IQR the interquartile range of the group's outcome). The mean of the RIF
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
solver. The two agree to about five significant figures, not more. The bandwidth is the same as R's `bw.nrd0`, but R's
`density()`, which `ddecompose` reaches through `rifreg`, bins the data onto a 512-point grid and interpolates, while
this crate evaluates the kernel sum exactly at the percentile. On the sample file the two densities differ by 0.003%,
which moves the explained part of the median gap from 1498.361 to 1498.398. The crate's figure is the exact one. Details
in [Reproducing results in R](../reproducing/r.md).

## Machado-Mata

The library also has a simulation-based quantile decomposition (Machado and Mata), `QuantileDecompositionBuilder`.
It is deprecated, has no external check, and is not reachable from the command line, the engine or the MCP server.
