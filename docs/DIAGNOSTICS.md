# Result diagnostics and weight semantics (0120-MERIDIAN S6-S9)

Every field below is documented where it is defined (`engine/src/types.rs`); this page is the map.
All of them are ADDITIVE: an existing consumer that ignores them reads the same numbers it read
before, except the places marked CHANGED. Words a reader sees (copy, captions) belong to the
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
| `few_residual_df` | a FITTED regression has fewer than 10 residual degrees of freedom | `reference` or `target`; `pooled` for the Pooled optimise target's pooled fit |
| `tie_share`, `ecdf_offset` | percentile mode only, see below | `reference` or `target` |

Decision (V6, orchestrator call): the engine keeps emitting all four warnings. The app shows the plain caveat
(D3) only for `outside_range`, extrapolated rows and `few_residual_df`. `normalised_difference` is shown as
information, because it fires on the ordinary `parity` fixture (education and experience are 0.45 and 0.39 pooled
SDs apart) where no amount rests on an extension; `parity` carries two `normalised_difference` warnings and no
`outside_range`, `few_residual_df` or extrapolation (pinned in `support_diagnostics_test`). The 50 000-row memory-profile file is git-ignored, so its silence check
runs on `employers_trust_fixture.csv` (same shape, 10 000 rows) and is not tested on the 50k file.

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
| `confidence_level` (request) | optimize (existing), check_defensibility, calculate_efficient_frontier | a fraction in [0.50, 0.999], default 0.95; anything else (95, NaN, 0.4) is refused: `INVALID_CONFIDENCE_LEVEL: confidence_level=`; for the frontier it sets `is_significant` to `p < 1 - level` |
| `FrontierPoint.group_coefficient`, `FrontierPoint.degrees_of_freedom` | frontier | the pooled regression's group-indicator coefficient after the budget is paid, and its residual df |
| `FrontierPoint.confidence_level` | frontier | the level `is_significant` was held against, echoed on every point |

The interval is exact `predict.lm(interval = "prediction")` on the fit the fair wage is read off, for both
optimise targets. Reference: the baseline group's own regression, `n - k` residual df. Pooled (T8): the pooled
regression with a target-group indicator, read at indicator 0, with that regression's sigma squared, `(Z'Z)^-1` and
`n_reference + n_target - k - 1` residual df; `interval.degrees_of_freedom` and `critical_value` report that fit.
`extrapolated` under Pooled compares the leverage `(x, 0)' (Z'Z)^-1 (x, 0)` with the largest among the reference rows
of the pooled design. `check_defensibility` still reads the Reference line (its `target` is a later change).

CHANGED: intervals are Student t on the baseline regression's residual degrees of freedom
(`predict.lm(interval = "prediction")`), not Normal. The frontier's `p_value` is `2 * pt(-|t|, df)`. At
5 residual df the 95% multiplier is 2.5706, not 1.9600. `is_defensible` allows ONE CENT below the lower
bound (`DEFENSIBLE_TOLERANCE`), not one dollar. Frontier with no residual df is a named refusal, not
`(t = 0, p = 1)`.

CHANGED: under `range_target` `LowerBound` / `UpperBound` the remedy PAYS the interval bound, so the t-based
bound moves remedy dollars: `adjustment`, `new_wage`, `total_cost`, `required_budget`, `new_gap`,
`original_unexplained_gap` and `new_unexplained_gap`. The half-width grows by t/z: about +2% at 58 residual df, about
+14% at 10, +31% at 5. A saved scenario re-run on this engine returns different dollars; `Midpoint` is unchanged.
The bound is checked against `predict.lm` row by row (`engine/tests/intervals_test.rs`).

## Percentile mode (S8, T16)

`quantile_report` on `decompose` when `quantile` is set (absent otherwise):

| Field | What it is |
|---|---|
| `tau` | the requested percentile |
| `quantile_gap` | `Q_tau(target) - Q_tau(reference)`, R `quantile(type = 7)` per group, same orientation as `total_gap` |
| `rif_total` | the RIF model total, equal to `total_gap` on this path |
| `reference`, `target` | per group: `count`, `quantile_value`, `ecdf_at_quantile` (`F_n(q_tau)`), `ecdf_offset` (`F_n(q_tau) - tau`), `tie_share` (rows exactly equal to `quantile_value`) |

`tie_share` fires above 5%; `ecdf_offset` fires above `max(0.01, 1/n)` in absolute value, `n` being the group's
row count (the warning's own `threshold` carries the line that applied). A type-7 percentile lies between two
order statistics, so a group with no tied value is off by up to `1/n` by discreteness alone (0.043 at 23 rows); the
`1/n` floor keeps that from reading as a step grid. T16 said 0.01; this amends it. On gridded pay both warnings are
large. `tie_share` is the step-grid signal.

## Weights (S9, T17)

A weights column always carries a kind. `OaxacaBuilder::weights_kind(WeightsKind)`, CLI
`--weights-kind frequency|relative`; each without the other is an error
(`WEIGHTS_KIND_REQUIRED` from the library, a clap error from the CLI). The engine and Meridian send no
weights.

| Kind | Meaning | Estimators | Quantile |
|---|---|---|---|
| `frequency` | whole-number counts; `w = 2` is the row twice, and so is the bootstrap: a replicate draws `sum(w)` employees per group, so standard errors, intervals and p-values match the repeated rows | used as given; a fractional, negative or non-finite value is refused: `INVALID_WEIGHT: column=, row=, value=` with the original 0-based data-row ordinal | type 7 on the expanded sample; the RIF density bandwidth reads `n = sum(w)`, so the whole run equals the run on the repeated rows |
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
| T8 Pooled target: `original_unexplained_gap` = group coefficient, 7 cases | `lm(y ~ x + group)`, `oaxaca` weight -2 | 1e-9 | 2.3e-10 |
| T8 Pooled target: `optimize` against `decompose` `Pooled` unexplained, 5 cases | the same file through the decomposition | 1e-9 | 2.2e-10 |
| T8 Pooled target: fair wage and bounds, three levels, 7 cases | `predict.lm(pooled_fit, interval = "prediction")` at indicator 0 | 1e-9 | 6.4e-12 |
| T8 Pooled target: `extrapolated` ordinals | `hatvalues` of the pooled fit, leverage of `(x, 0)` | exact set | exact |
| V8 percentile gap, ECDF, tie counts | `quantile(type = 7)` | 1e-12 | exact |
| V9 relative quantile | `Hmisc::wtd.quantile(normwt = TRUE)`, 5 weight patterns x 5 taus | 1e-9 | see `weights_kind_test` |
| V9 frequency quantile | `quantile(rep(y, w), type = 7)` | 1e-9 | exact |

Generator: `verification/gen_diag_goldens.R` (R 4.6, `ddecompose` 1.0.0, `Hmisc` 5.2.6). Golden:
`oaxaca_blinder/tests/fixtures/diag_goldens_r.json`, with the sha256 of the generator and of every fixture;
the Rust tests refuse a stale golden. No expected value comes from engine output.
