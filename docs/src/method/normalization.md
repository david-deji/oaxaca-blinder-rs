# Categorical normalisation

A categorical predictor is split into one column per level with one level dropped. In the detailed result every other
level is then measured against whichever level sorts first. Rename that level and the per-level rows change while
every aggregate stays the same. Normalisation removes the dependence. For how to use it see
[Categorical drivers](../user-guide/categorical-drivers.md).

## The restriction

Normalisation re-expresses the coefficients under the restriction \\( \sum\_k s\_k \tilde\beta\_k = 0 \\) over **all**
\\( k \\) levels, where the dropped level's coefficient is 0 before the transform. Every level, the dropped one
included, becomes a deviation from a share-weighted average of the levels, and the intercept absorbs the average:

\\[ \tilde\beta\_j = \beta\_j - c, \quad \tilde\beta\_0 = -c, \quad
\tilde\beta\_{\text{intercept}} = \beta\_{\text{intercept}} + c, \quad c = \sum\_{j \ne \text{base}} s\_j \beta\_j . \\]

Predictions and the aggregate decomposition do not change, and all \\( k \\) levels are reported.

Two conventions for the shares \\( s\_k \\) (`normalization_convention`):

- **Population share** (default). \\( s\_k \\) is the level's share of the pooled analysed rows of both groups, or of
  the summed weights when a weights column is set. One share vector per run is applied to \\( \beta\_A \\),
  \\( \beta\_B \\), the pooled fit and the weighted mix alike, which keeps explained plus unexplained equal to the gap
  exactly. Adding a small department changes only that department's row. No package implements this restriction
  by hand. It is checked against a base-R weighted-effect-coding refit.
- **Equal share**. \\( s\_k = 1/m \\). This is [Yun (2005)](references.md#yun-2005) and [Gardeazabal and Ugidos (2004)](references.md#gardeazabal-ugidos-2004), and what Stata
  `categorical()`, R `oaxaca` (third formula part) and `ddecompose(normalize_factors = TRUE)` print. It is checked
  against `ddecompose` and R `oaxaca` to \\( 10^{-10} \\) (\\( 2 \times 10^{-10} \\) on the 10,000-row fixture).

`run_metadata.normalization` records the convention, the share basis and the shares of the point-estimate sample.
Bootstrap replicates recompute their shares from each replicate's own pooled resample.

## Which surface normalises

One rule: a result that names a categorical level as a driver is normalised. A result that only predicts, or exposes
the raw terms of a prediction model, is not, and says so.

| Surface | Normalised | Why |
|---|---|---|
| WASM `decompose` and `verify_adjustments`, MCP `forensic_decomposition` and `verify_adjustments` | yes, population share | the per-level rows are the drivers a reader ranks |
| WASM `optimize`, `check_defensibility`, `calculate_efficient_frontier`; MCP `simulate_remediation`, `check_defensibility`, `generate_efficient_frontier` | raw by design | they fit a fair-wage model for prediction, which does not depend on the coding. `model_coefficients` and per-employee `contributions` are raw model terms and are not ranked as drivers |
| Command line `run` and `report` | yes, population share | `--normalization equal-share` reproduces Stata, R `oaxaca` and `ddecompose`; `--normalization none` returns raw coding |
| Library `OaxacaBuilder::run`, `decompose_quantile`, `from_formula` | raw unless you call `.normalize(vars)` or `.normalize_all_categoricals()` | opt in per call |
| Library `QuantileDecompositionBuilder` (Machado-Mata, deprecated) | raw | no external check |

The three-fold aggregate is always computed from the raw coefficient vectors, because it does not depend on the
coding and the normalised one would. With a Heckman selection model, normalisation is skipped on the compared group,
the reference group and the pooled fit together, and `run_metadata.normalization` says so.
