# Result diagnostics and weight semantics (0120-MERIDIAN S6-S9)

Every field below is documented where it is defined (`engine/src/types.rs`); this page is the map.
All of them are ADDITIVE: an existing consumer that ignores them reads the same numbers it read
before, except the two places marked CHANGED. Words a reader sees (copy, captions) belong to the
app. The engine says what was measured and which line it crossed.

## Support and small samples (S6, T12, T13)

"Baseline" is the group whose pay line the fair wage extends: the `reference_group`. "Target" is
the compared group.

| Field | On | What it is |
|---|---|---|
| `support.reference_count`, `support.target_count` | decompose, verify, optimize, defensibility | analysed rows per group |
| `support.model_columns` | same | design columns (intercept and every level dummy included) |
| `support.reference_residual_df`, `support.target_residual_df` | same | `count - model_columns`, signed |
| `support.predictors[]` | same | per continuous predictor: baseline min, max, type-7 p01, p99; target min, max; share of target rows outside the baseline range and outside its [p01, p99]; Imbens-Rubin normalised difference |
| `support.extrapolated_target_count` | same | target rows whose leverage `x'(X'X)^-1 x` (X = the baseline design) exceeds the largest leverage among baseline rows |
| `adjustments[].extrapolated` | optimize, defensibility | the same test for one remedy row; always `false` for a baseline row |
| `warnings[]` | same | `{code, subject, value, threshold}` |

Warning codes and their lines (restated as literals in `engine/tests/support_diagnostics_test.rs`):

| `code` | Fires when | `subject` |
|---|---|---|
| `outside_range` | more than 5% of target rows lie outside the baseline [min, max] of a continuous predictor | the predictor |
| `normalised_difference` | the absolute normalised difference of a continuous predictor exceeds 0.25 (Imbens and Rubin 2015, section 14.2) | the predictor |
| `few_residual_df` | a FITTED group has fewer than 10 residual degrees of freedom | `reference` or `target` |
| `tie_share`, `ecdf_offset` | percentile mode only, see below | `reference` or `target` |

Normalised difference is `(mean_target - mean_reference) / sqrt((var_target + var_reference) / 2)` with
sample variances. `ddecompose` prints the same quantity without the 1/2, so it is `1/sqrt(2)` of this one;
the test checks the relation to 1e-12.

Which groups are "fitted": decompose and verify fit both; optimize and defensibility fit the baseline
only (the compared group enters the remedy as rows, not as a regression), so they judge and warn on
the baseline's degrees of freedom alone.

A fitted group with zero or fewer residual degrees of freedom is REFUSED on every entry point with
`INSUFFICIENT_RESIDUAL_DF: group=reference|target, rows=N, model_columns=K, residual_df=D`. This used to be
a zero-width interval (optimize, defensibility) or an anonymous estimator error (decompose).

CHANGED: `optimize` no longer fits a regression to the compared group to obtain `original_gap`. It is
the difference of the analysed group means, which is what the decomposition's `total_gap` is, so a
compared group too small to fit its own regression no longer blocks a remedy. Numbers are unchanged.

## Intervals and p-values (S7, T14)

| Field | On | What it is |
|---|---|---|
| `interval.confidence_level`, `interval.degrees_of_freedom`, `interval.critical_value` | optimize, defensibility | the prediction interval behind `fair_wage_lower_bound` / `fair_wage_upper_bound` |
| `confidence_level` (request) | optimize (existing), check_defensibility, calculate_efficient_frontier | clamped to [0.50, 0.999], default 0.95; for the frontier it sets `is_significant` to `p < 1 - level` |
| `FrontierPoint.group_coefficient`, `FrontierPoint.degrees_of_freedom` | frontier | the pooled regression's group-indicator coefficient after the budget is paid, and its residual df |

CHANGED: intervals are Student t on the baseline regression's residual degrees of freedom
(`predict.lm(interval = "prediction")`), not Normal. The frontier's `p_value` is `2 * pt(-|t|, df)`. At
5 residual df the 95% multiplier is 2.5706, not 1.9600. `is_defensible` allows ONE CENT below the lower
bound (`DEFENSIBLE_TOLERANCE`), not one dollar. Frontier with no residual df is a named refusal, not
`(t = 0, p = 1)`.

## Percentile mode (S8, T16)

`quantile_report` on `decompose` when `quantile` is set (absent otherwise):

| Field | What it is |
|---|---|
| `tau` | the requested percentile |
| `quantile_gap` | `Q_tau(target) - Q_tau(reference)`, R `quantile(type = 7)` per group, same orientation as `total_gap` |
| `rif_total` | the RIF model total, equal to `total_gap` on this path |
| `reference`, `target` | per group: `count`, `quantile_value`, `ecdf_at_quantile` (`F_n(q_tau)`), `ecdf_offset` (`F_n(q_tau) - tau`), `tie_share` (rows exactly equal to `quantile_value`) |

`tie_share` fires above 5%; `ecdf_offset` fires above 0.01 in absolute value. On gridded pay both are large;
on small groups the offset is at least `1/n`-sized by construction.

## Weights (S9, T17)

A weights column always carries a kind. `OaxacaBuilder::weights_kind(WeightsKind)`, CLI
`--weights-kind frequency|relative`; each without the other is an error
(`WEIGHTS_KIND_REQUIRED` from the library, a clap error from the CLI). The engine and Meridian send no
weights.

| Kind | Meaning | Estimators | Quantile |
|---|---|---|---|
| `frequency` | whole-number counts; `w = 2` is the row twice | used as given; a fractional, negative or non-finite value is refused: `INVALID_WEIGHT: column=, row=, value=` with the original 0-based data-row ordinal | type 7 on the expanded sample; the RIF density bandwidth reads `n = sum(w)`, so the whole run equals the run on the repeated rows |
| `relative` | FTE or design weights | each regression's weights are rescaled to sum to its row count; scale-free | `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)`; the bandwidth reads Kish's effective n |

Uniform weights of any size are a no-op under `relative`. Zero-weight rows carry no mass under both.
`oaxaca_blinder::weighted_quantile(values, weights, tau, kind)` is the public function both RIF paths use.

CHANGED: the RIF bandwidth under `frequency` weights used Kish's effective n (so `w = 2` differed from the
row twice in the density); it now reads `sum(w)`. Unweighted runs are untouched.

## Oracles

| Check | Oracle | Tolerance | Measured |
|---|---|---|---|
| V6 support block, normalised difference, leverage, extrapolated count, 9 fixtures | base R, `ddecompose:::get_normalized_difference` (x sqrt 2) | 1e-12 | 3.3e-16 |
| V7 prediction intervals, three levels, 10 000 rows and 5 df | `predict.lm(interval = "prediction")` | 1e-9 | 2.9e-12, 2.1e-15 |
| V7 t quantile and `pt` | `qt`, `pt` | 1e-10 | 2.4e-12 |
| V7 frontier group test | pooled `lm` | 1e-9 | see `intervals_test` |
| V8 percentile gap, ECDF, tie counts | `quantile(type = 7)` | 1e-12 | exact |
| V9 relative quantile | `Hmisc::wtd.quantile(normwt = TRUE)`, 5 weight patterns x 5 taus | 1e-9 | see `weights_kind_test` |
| V9 frequency quantile | `quantile(rep(y, w), type = 7)` | 1e-9 | exact |

Generator: `verification/gen_diag_goldens.R` (R 4.6, `ddecompose` 1.0.0, `Hmisc` 5.2.6). Golden:
`oaxaca_blinder/tests/fixtures/diag_goldens_r.json`, with the sha256 of the generator and of every fixture;
the Rust tests refuse a stale golden. No expected value comes from engine output.
