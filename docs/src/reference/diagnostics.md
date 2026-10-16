# Diagnostics and warnings

Every field below is documented where it is defined (`engine/src/types.rs`); this page is the map. The fields
are additive: a consumer that ignores them reads the same numbers. Wording shown to end users belongs to the
consuming application. The engine reports what was measured and which line it crossed.

## Support and small samples

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

Warning codes and their lines (pinned in `engine/tests/support_diagnostics_test.rs`):

| `code` | Fires when | `subject` |
|---|---|---|
| `outside_range` | more than 5% of target rows lie outside the baseline [min, max] of a continuous predictor | the predictor |
| `normalised_difference` | the absolute normalised difference of a continuous predictor exceeds 0.25 (Imbens and Rubin 2015) | the predictor |
| `few_residual_df` | a FITTED regression has fewer than 10 residual degrees of freedom | `reference` or `target`; `pooled` for the Pooled optimise target's pooled fit |
| `tie_share`, `ecdf_offset` | percentile mode only, see below | `reference` or `target` |

The engine emits all four warnings. A consumer decides which to show. `normalised_difference` fires on ordinary
data (on the `parity` fixture, education and experience are 0.45 and 0.39 pooled standard deviations apart, with no
amount resting on an extension), so treat it as information and read `outside_range`, extrapolated rows and
`few_residual_df` as the caveats.

Normalised difference is `(mean_target - mean_reference) / sqrt((var_target + var_reference) / 2)` with
sample variances. `ddecompose` prints the same quantity without the 1/2, so it is `1/sqrt(2)` of this one;
the test checks the relation to 1e-12.

Which groups are "fitted": decompose and verify fit both; optimize and defensibility fit the baseline
only (the compared group enters the remedy as rows, not as a regression), so they judge and warn on
the baseline's degrees of freedom alone.

A fitted group with zero or fewer residual degrees of freedom is REFUSED on every entry point with
`INSUFFICIENT_RESIDUAL_DF: group=reference|target, rows=N, model_columns=K, residual_df=D`. This used to be
a zero-width interval (optimize, defensibility) or an anonymous estimator error (decompose).

## Intervals and p-values

| Field | On | What it is |
|---|---|---|
| `interval.confidence_level`, `interval.degrees_of_freedom`, `interval.critical_value` | optimize, defensibility | the prediction interval behind `fair_wage_lower_bound` / `fair_wage_upper_bound` |
| `confidence_level` (request) | optimize (existing), check_defensibility, calculate_efficient_frontier | a fraction in [0.50, 0.999], default 0.95; anything else (95, NaN, 0.4) is refused: `INVALID_CONFIDENCE_LEVEL: confidence_level=`; for the frontier it sets `is_significant` to `p < 1 - level` |
| `FrontierPoint.group_coefficient`, `FrontierPoint.degrees_of_freedom` | frontier | the pooled regression's group-indicator coefficient after the budget is paid, and its residual df |
| `FrontierPoint.confidence_level` | frontier | the level `is_significant` was held against, echoed on every point |

The interval is exact `predict.lm(interval = "prediction")` on the fit the fair wage is read off, for both
optimise targets. Reference: the baseline group's own regression, `n - k` residual df. Pooled: the pooled
regression with a target-group indicator, read at indicator 0, with that regression's sigma squared, `(Z'Z)^-1` and
`n_reference + n_target - k - 1` residual df; `interval.degrees_of_freedom` and `critical_value` report that fit.
`extrapolated` under Pooled compares the leverage `(x, 0)' (Z'Z)^-1 (x, 0)` with the largest among the reference rows
of the pooled design. `check_defensibility` takes the same `target` (default Reference): under Pooled its fair wages,
bounds, `extrapolated` flags and `few_residual_df` warning (subject `pooled`) come from the pooled fit, held to the same R
golden as the optimiser's, so a remedy and the check that scores its amounts mark the same people.

Pooled sigma squared assumes ONE residual variance for both groups: the regression has a group indicator and no
group-specific slope or variance, so a baseline group much noisier or quieter than the compared group widens or narrows
the compared group's prediction range by the pooled average. "Exact `predict.lm`" holds for that fit as written, not
for a model with variance by group.

## Percentile mode

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
`1/n` floor keeps that from reading as a step grid. On gridded pay both warnings are
large. `tie_share` is the step-grid signal.

## Weights

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

## The remedy's figures

The remedy raises each compared employee below the chosen pay line up to it, never above, within the budget. It
is a scenario that costs a set of raises. It uses no solver: for these amounts the cost of reaching a gap is
fixed, and the strategy decides only who is paid first when the money falls short.

SIGN. Every gap, from `optimize` and from `check_defensibility`, is the compared group's figure minus the line (or
minus the reference group's): negative while the compared group sits below, rising with every dollar paid.
`original_unexplained_gap` is the mean of (pay - fair pay) per compared employee on the MIDPOINT of the fitted line,
whatever `range_target` pays to. `new_unexplained_gap` is the same figure on the line REFITTED to the schedule's
wages: raising reference employees moves the line, and under `Pooled` the compared group's own raises move it too,
so it is not `original_unexplained_gap + total_cost / n`. `check_defensibility` used to report all four gap fields
(`original_gap`, `new_gap`, `original_unexplained_gap`, `new_unexplained_gap`) with the opposite sign.

Per-person figures are dollars per compared employee. `total_cost`, `cost_*`, `need_*`, `required_budget` and
`unfunded_amount` are totals. Fields marked "optimise only" are `null`/absent from `check_defensibility`, and
"defensibility only" fields from `optimize`; a zero is never used for "not applicable".

| Field | Entry point | Meaning |
|---|---|---|
| `required_budget`, `need_target` | both | sum of the compared employees' shortfalls to `target_line` that pass the threshold; the same number in both on the default basis (threshold 0, `Midpoint`): `check_defensibility` takes neither `min_gap_pct` nor `range_target`, so with either set the two differ. The screen reads it from `optimize` |
| `need_reference` | optimise | what raising the reference employees would cost; 0 with the toggle off |
| `cost_target`, `cost_reference` | both | money paid to each group; `total_cost` is their sum |
| `target_line` | both | the line the shortfalls are measured to (`Midpoint` from `check_defensibility`) |
| `best_reachable_gap` | optimise | the highest the compared group's gap gets as the budget grows along the strategy's own order (honours `min_gap_pct` and `range_target`). On the `Reference` line, and wherever every payment raises the gap, that is the gap after paying every eligible shortfall in full. On the `Pooled` line a compared employee far beyond the reference group's characteristics can lower the gap when paid, the path peaks before its end, and this is the peak. With `adjust_both_groups` it is the gap after the full schedule |
| `overshoot_mean` | optimise | mean of `max(0, pay - fair pay)` over the compared group on the midpoint line; with `Reference`, `Midpoint`, threshold 0 and no reference raises it equals `best_reachable_gap` |
| `target_gap_reachable` | optimise | with `target_gap`: `true` when already met or within reach, `false` beyond `best_reachable_gap` |
| `shortfall_to_target` | optimise | `target_gap - best_reachable_gap` when out of reach |
| `target_budget` | optimise | the budget the target rule set: 0 when already met, the least that reaches it, or when out of reach the budget that gets nearest (the full need, unless paying someone lowers the gap, then the budget where the gap peaks) |
| `budget_binding` | optimise | `true` when the caller's `budget` is below both the need and the target's budget |
| `closure` | optimise | `cost_target / need_target`, 0 to 1, `null` when `need_target` is 0. It counts money, not the target: 0 when a target is already met, 1 when one is out of reach |
| `unfunded_amount` | optimise | `need_target - cost_target` |
| `unfunded_count` | optimise | eligible compared employees paid less than their shortfall (all of them under `Equitable` while the budget is short). Counted against the pay line, so it is not `position_counts.below`, which is counted against the range's lower bound |
| `threshold_excluded_count` | optimise | compared employees below the line by less than `min_gap_pct`; still below the line, left out on purpose |
| `adjustments[].source` | both | `Compared` or `Reference` |
| `adjustments[].range_position`, `range_position_before` | both | `Below` / `Inside` / `Above` for `new_wage` and `current_wage` against the row's own range; `Below` is exactly "not `is_defensible`", a wage within one cent of a bound is `Inside` |
| `position_counts` | defensibility | `below`, `inside`, `above` after the schedule and `*_before` before it, over EVERY analysed compared employee (a row the schedule does not name counts at adjustment 0); `newly_above` = above after and not before; reference employees are in no count |
| `group_test` | defensibility | the pooled regression with a group indicator on the schedule's wages: `line` (always `"Pooled"`), `group_coefficient`, `t_statistic`, `p_value`, `degrees_of_freedom`, `is_significant`. The frontier's computation at the same budget; not the bootstrap of `verify_adjustments`. Under `target: Reference` its `group_coefficient` is on the pooled line while `new_unexplained_gap` is on the reference line: they differ and can differ in sign. The raises are functions of the fitted line and remove the group's negative residuals, so the p-value describes the adjusted roster and is not evidence that the schedule is fair |

`target_gap` is on the sign and scale of `original_unexplained_gap`, not the raw `total_gap`. It sets the budget: the
least that brings the group's gap to it. A figure already met pays nothing; one out of reach pays every eligible
shortfall (the same amounts as no target; on a pooled roster where paying someone lowers the gap, up to the budget where
the gap peaks) and sets `target_gap_reachable: false`. On the `Reference` line the cost is
`n_compared * (target_gap - original_unexplained_gap)`. Under `Pooled` a dollar to employee `i` moves the group
coefficient by `d~_i / (d~' d~)` (Frisch-Waugh-Lovell), not by `1/n`, so the budget is found along the strategy's own
order: a walk to the first segment that crosses the target for `Greedy`, a share of every shortfall for `Equitable`; the
two strategies then cost slightly different amounts. A compared employee's weight is the residual of the group
indicator on the model columns, negative for one far beyond the reference group's characteristics: paying that employee
widens the gap. The path then peaks before its end (`best_reachable_gap` is the peak), and a target between where the
full schedule ends and the peak is still reached, at a smaller budget. `target_gap` with `adjust_both_groups` is refused
(`TARGET_GAP_WITH_REFERENCE_RAISES`): a raise to the reference group moves the line, so no single budget reaches it.

`budget` is the most the remedy may spend in total, over both groups; 0 means no cap. A negative or non-finite
value is refused (`INVALID_BUDGET`); it used to fund the full need. `min_gap_pct` is a fraction of the employee's
CURRENT pay (shortfall / current pay) and must be finite and not negative (`INVALID_MIN_GAP_PCT`).

How each figure is checked against R is on [Reproducing results in R](../reproducing/r.md).
