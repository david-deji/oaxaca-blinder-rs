# Categorical drivers

A categorical predictor such as `sector` is split into one column per level, with one level dropped. Without any
further step, every other level's row in the detailed result is measured against the dropped level, which is
whichever level sorts first. Rename that level and the per-level rows change, while every aggregate stays the same.

**Normalisation** removes that dependence. Each level, the dropped one included, is shown as a deviation from a
share-weighted average of all the levels, and the intercept absorbs the average. The aggregates do not change. The
rows then answer "how much does this level contribute" without reference to a level you did not choose.

| Where | Normalised | How to change it |
|---|---|---|
| Library | no, unless you ask | `normalize_all_categoricals()` or `normalize(["sector"])` |
| Command line | yes, population share | `--normalization equal-share` or `none` |
| Engine and MCP server | yes, population share | not configurable |

Two conventions exist. **Population share** (the default) weights each level by its share of the analysed rows of
both groups, which keeps explained plus unexplained equal to the gap exactly and means a small new department changes
only its own row. **Equal share** weights every level the same. It is what Stata `categorical()`, R `oaxaca` and
`ddecompose(normalize_factors = TRUE)` print. Choose it to compare with those packages:

```rust,ignore
builder
    .normalization_convention(NormalizationConvention::EqualShare)
    .normalize_all_categoricals();
```

The intercept appears in every detailed result under the reserved name `__ob_intercept__` (the constant
`oaxaca_blinder::INTERCEPT_NAME`). It is not a driver. A column in your data may be called `intercept`, so compare
against the constant, not the word. The printed tables and the HTML report show the row as `(intercept)`.

A worked example with a categorical and a weights column:

```rust
{{#include ../../../oaxaca_blinder/examples/weights_and_categoricals.rs}}
```

```text
{{#include ../outputs/weights_and_categoricals.txt}}
```

Which surfaces normalise, and why the engine's remedy tools return raw model terms instead, is on the
[normalisation method page](../method/normalization.md).
