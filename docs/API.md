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
Calculates required wage adjustments to close identified gaps.
- **Parameters**:
    - `budget`: Maximum available budget.
    - `strategy`: "Greedy" (max gap first) or "Equitable" (shared distribution).
    - `target`: "Reference" (the reference group's own pay line) or "Pooled" (the pooled line with a target-group indicator, read at indicator 0: the decomposition's `Pooled` line). Under Pooled the interval, `extrapolated` and `few_residual_df` (subject `pooled`) come from that pooled fit.
- **Result**: `model_coefficients` and each adjustment's `contributions` are RAW terms of the fair-wage model fitted on the reference group (treatment-coded: each categorical level against the alphabetically first one, whose level has no row). They are not drivers and must not be ranked or labelled as findings; the constant is the entry named `__ob_intercept__`.

- **Result** (all tools above that return a decomposition or a remedy): `support` and `warnings` say how far the compared group's characteristics sit from the baseline group's and how many residual degrees of freedom each fitted regression has; each remedy row carries `extrapolated`; `interval` states the Student t prediction interval behind the bounds. `docs/DIAGNOSTICS.md` maps every field. With `quantile` set, `quantile_report` holds the actual percentile gap beside `rif_total` (the model total `total_gap` carries) and the tie diagnostics.

### `check_defensibility`
Scores each proposed adjustment against the prediction range of comparable reference-group employees (`confidence_level`, default 0.95, Student t on the reference regression's residual degrees of freedom; one cent of slack). It reports the share of adjusted wages inside that range; it does not certify legal compliance.

### `generate_efficient_frontier`
Simulates the trade-off between Remediation Budget and remaining Statistical Significance of the gap. Each point carries the pooled regression's group coefficient, its t statistic and its two-sided p-value on the pooled residual degrees of freedom (`confidence_level` sets `is_significant`).

---

## 🛠 Extension Engines

### AKM (Fixed Effects)
Available via `AkmBuilder`. Requires high-density longitudinal data with `worker_id` and `firm_id`.

### Matching Engine
Supports Nearest Neighbor and Propensity Score Matching (PSM) for causal audit workflows.
