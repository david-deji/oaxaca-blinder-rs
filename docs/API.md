# API Reference

This document summarizes the primary interfaces and schemas for the `oaxaca-blinder-rs` components.

## 📦 Core Library: `oaxaca_blinder`

### `OaxacaBuilder`
The main entry point for mean-based decompositions.

| Method | Description |
| :--- | :--- |
| `new(df, outcome, group)` | Initialize with Polars DataFrame and variables. |
| `predictors(&[&str])` | Add continuous predictors. |
| `categorical_predictors(&[&str])` | Add categorical variables (auto-dummy encoded). |
| `reference_coefficients(ReferenceCoefficients)` | Set target coefficients (GroupA, GroupB, Pooled, PooledNoIndicator, Weighted). Required (exact name) at every engine boundary. |
| `normalize(vars)` / `normalize_all_categoricals()` | Re-express categorical contributions as deviations from a share-weighted average of ALL levels. Opt-in in the library; on for every engine and CLI run. |
| `normalization_convention(c)` | `PopulationShare` (default) or `EqualShare` (Stata / R `oaxaca` / `ddecompose`). |
| `weights(col)` + `weights_kind(WeightsKind)` | Sample weights. The kind is required: `Frequency` (whole-number counts, `w = 2` is the row twice) or `Relative` (rescaled to the row count). See `docs/DIAGNOSTICS.md`. |
| `bootstrap_reps(usize)` | Number of iterations for standard error estimation. |
| `run()` | Execute the decomposition. |

### `QuantileDecompositionBuilder`
Used for Machado-Mata/RIF quantile decompositions.

| Method | Description |
| :--- | :--- |
| `quantile(f64)` | The target quantile (e.g., 0.5 for median). |
| `simulations(usize)` | Number of simulations for Machado-Mata method. |

---

## 🤖 Meridian MCP Server Tools

The MCP server exposes the following tools for use by AI agents.

### `forensic_decomposition`
Performs a full Oaxaca-Blinder pay equity audit.
- **Parameters**: 
    - `csv_content`: String data.
    - `outcome_variable`, `group_variable`, `reference_group`: Field names.
    - `predictors`, `categorical_predictors`: List of strings.
    - `three_fold`: Boolean (standard 2-fold if false).
    - `reference_coefficients`: REQUIRED, exactly one of `GroupA`, `GroupB`, `Pooled`, `PooledNoIndicator`, `Weighted` (see the README for what each prices).
- **Result**: the per-level rows of categorical predictors in `detailed_explained` / `detailed_unexplained` are deviations from the pooled-sample share-weighted average of all levels (every level, the alphabetically first included). The intercept is the entry named `__ob_intercept__` (`pay_equity_engine::intercept_token()`); it is not a driver. A consumer must match it by that token (the WASM package exports the same function) and never by the string `intercept`: an HR export can carry a column of that name, and a filter on the string misses the real token, so the intercept, which now absorbs the share-weighted level average and can be the largest entry, would be ranked as a driver. Publishing this engine to the app waits on the app importing `intercept_token()` and passing its grep gate for the literal string. `run_metadata` records the scheme, the engine version, the method and the normalisation shares.

### `simulate_remediation`
Costs a remedy: it raises each compared employee below the chosen pay line up to it, never above, within the budget. It is a scenario that prices a set of raises. `docs/DIAGNOSTICS.md` ("The remedy's figures") defines every result field.
- **Parameters**:
    - `budget`: the most the remedy may spend in total, over everyone paid. `0` means no cap: every eligible shortfall is paid in full. A negative or non-finite value is refused (`INVALID_BUDGET`).
    - `target_gap`: the compared group's mean gap to the pay line the remedy should reach, on the sign and scale of `original_unexplained_gap` (dollars per compared employee, negative while below the line); it is not the raw `total_gap`. It derives the budget: the least that reaches it. Already met pays nothing; out of reach pays every eligible shortfall (on a pooled roster where paying someone widens the gap, up to the budget where the gap is highest) and sets `target_gap_reachable: false` with `best_reachable_gap` (the highest the gap gets as the budget grows) and `shortfall_to_target`. Refused with `adjust_both_groups` (`TARGET_GAP_WITH_REFERENCE_RAISES`) and when not finite (`INVALID_TARGET_GAP`).
    - `strategy`: "Greedy" pays the largest shortfalls first; "Equitable" pays every employee the same share of their own shortfall. Both pay at most a shortfall; the strategy only decides who is paid first when the money falls short.
    - `target`: "Reference" (the reference group's own pay line) or "Pooled" (the pooled line with a target-group indicator, read at indicator 0: the decomposition's `Pooled` line). Under Pooled the interval, `extrapolated` and `few_residual_df` (subject `pooled`) come from that pooled fit.
    - `range_target`: "Midpoint" (default), "LowerBound" or "UpperBound": the point of each employee's fair range the raise reaches. Reference employees raised with `adjust_both_groups` are raised to the same point.
    - `min_gap_pct`: smallest shortfall worth paying, as a fraction of CURRENT pay (shortfall / current pay). Finite and not negative (`INVALID_MIN_GAP_PCT`).
    - `adjust_both_groups`: also raise reference employees below the line. The line is refitted on the raised pay, so it moves; `cost_target` and `cost_reference` are reported apart and `closure` counts the compared group only.
    - Enumerated arguments are exact and case-sensitive. The MCP tool refuses an unknown value (`UNKNOWN_TARGET`, `UNKNOWN_STRATEGY`, `UNKNOWN_RANGE_TARGET`); it used to run Greedy or Reference.
- **Result**: `model_coefficients` and each adjustment's `contributions` are RAW terms of the fair-wage model fitted on the reference group (treatment-coded: each categorical level against the alphabetically first one, whose level has no row). They are not drivers and must not be ranked or labelled as findings; the constant is the entry named `__ob_intercept__`.

- **Result** (all tools above that return a decomposition or a remedy): `support` and `warnings` say how far the compared group's characteristics sit from the baseline group's and how many residual degrees of freedom each fitted regression has; each remedy row carries `extrapolated`; `interval` states the Student t prediction interval behind the bounds. `docs/DIAGNOSTICS.md` maps every field. With `quantile` set, `quantile_report` holds the actual percentile gap beside `rif_total` (the model total `total_gap` carries) and the tie diagnostics.

### `check_defensibility`
Scores each proposed adjustment against the prediction range of comparable reference-group employees (`confidence_level`, default 0.95, Student t on the residual degrees of freedom of the fit the fair wage is read off; one cent of slack). `target` is `Reference` (default: the baseline group's own regression) or `Pooled` (the pooled regression with a target-group indicator, read at indicator 0): the line the remedy was priced against, so the bounds, `extrapolated` flags and `few_residual_df` warning describe the same fit as the remedy's rows. It reports the share of adjusted wages inside that range; it does not certify legal compliance. Each row carries `source` and where it sits against its range before and after (`range_position_before`, `range_position`). `position_counts` counts every analysed compared employee below, inside and above their range before and after the schedule (an unnamed row counts at adjustment 0; reference employees are in no count), and `group_test` is the exact test of the compared group on the schedule's wages, always on the pooled line (`group_test.line`): under `target` `Reference` its `group_coefficient` is not `new_unexplained_gap`, and the two can differ in sign. It describes the adjusted roster; it is not evidence that the schedule is fair. Gaps carry `simulate_remediation`'s sign (compared minus the line, negative while below); `new_unexplained_gap` is refitted on the schedule, and `required_budget` is the compared group's need to the midpoint line at threshold 0 (`target_line`); it is the figure `simulate_remediation` returns on that basis, and differs when `simulate_remediation` is given a `min_gap_pct` or a `range_target`.

### `generate_efficient_frontier`
Simulates the trade-off between Remediation Budget and remaining Statistical Significance of the gap. Each point carries the pooled regression's group coefficient, its t statistic and its two-sided p-value on the pooled residual degrees of freedom (`confidence_level` sets `is_significant`), on the wages the remedy's schedule produces at that budget. The curve follows the remedy: `strategy`, `target`, `range_target`, `min_gap_pct` and `adjust_both_groups` mean what they mean in `simulate_remediation` (absent: Reference, Greedy, Midpoint, threshold 0, compared group only). `confidence_level` also sets the interval a `LowerBound` or `UpperBound` remedy pays to. The budget axis ends at that remedy's full cost; `max_budget` must be 0 or more and `steps` 1 or more (`INVALID_BUDGET`, `INVALID_STEPS`).

---

## 🛠 Extension Engines

### AKM (Fixed Effects)
Available via `AkmBuilder`. Requires high-density longitudinal data with `worker_id` and `firm_id`.

### Matching Engine
Supports Nearest Neighbor and Propensity Score Matching (PSM) for causal audit workflows.
