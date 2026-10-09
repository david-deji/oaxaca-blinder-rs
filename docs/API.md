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
- **Result**: the per-level rows of categorical predictors in `detailed_explained` / `detailed_unexplained` are deviations from the pooled-sample share-weighted average of all levels (every level, the alphabetically first included). The intercept is the entry named `__ob_intercept__` (`pay_equity_engine::intercept_token()`); it is not a driver. `run_metadata` records the scheme, the engine version, the method and the normalisation shares.

### `simulate_remediation`
Calculates required wage adjustments to close identified gaps.
- **Parameters**:
    - `budget`: Maximum available budget.
    - `strategy`: "Greedy" (max gap first) or "Equitable" (shared distribution).
    - `target`: "Reference" or "Pooled" coefficients.
- **Result**: `model_coefficients` and each adjustment's `contributions` are RAW terms of the fair-wage model fitted on the reference group (treatment-coded: each categorical level against the alphabetically first one, whose level has no row). They are not drivers and must not be ranked or labelled as findings; the constant is the entry named `__ob_intercept__`.

### `generate_efficient_frontier`
Simulates the trade-off between Remediation Budget and remaining Statistical Significance of the gap.

---

## 🛠 Extension Engines

### AKM (Fixed Effects)
Available via `AkmBuilder`. Requires high-density longitudinal data with `worker_id` and `firm_id`.

### Matching Engine
Supports Nearest Neighbor and Propensity Score Matching (PSM) for causal audit workflows.
