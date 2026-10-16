# Quantile analysis

The mean decomposition explains the gap between the two groups' averages. A quantile decomposition explains the gap
at one point of the distribution: the 10th, 50th or 90th percentile. A pay gap is often larger among the best paid
than at the median, and the mean hides that.

The method is a decomposition of the **recentered influence function** (RIF) of the chosen percentile, following
Firpo, Fortin and Lemieux. See [RIF quantile decomposition](../method/rif-quantile.md).

## Library

Call `decompose_quantile` once for each percentile. Each call is a separate decomposition:

```rust
{{#include ../../../oaxaca_blinder/examples/quantile_rif.rs}}
```

```text
{{#include ../outputs/quantile_rif.txt}}
```

## Command line

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --analysis-type quantile --quantiles 0.1,0.5,0.9
```

## What to read in the result

- The bootstrap recomputes the percentile and its density inside every replicate, so the intervals include the
  uncertainty of the percentile itself.
- `total_gap` of a quantile run is the total of the RIF model at that percentile. It is close to the difference of
  the two groups' sample percentiles and is not the same number. In the engine's result, `quantile_report` sets the
  actual percentile gap (`quantile_gap`, from R `quantile(type = 7)` in each group) beside the model total
  (`rif_total`, equal to `total_gap`). In the example above the 90th percentile gap in the data is 4,750 and the
  model total is 4,463.
- Two warnings, `tie_share` and `ecdf_offset`, flag pay that sits on a grid of round values, where a percentile is
  not well defined. See [Diagnostics and warnings](../reference/diagnostics.md).
- Machado-Mata simulation (`QuantileDecompositionBuilder`) is deprecated and has no external check.
