use crate::types::*;
use nalgebra::DVector;
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;
use statrs::distribution::{ContinuousCDF, Normal};
use std::collections::HashMap;
use std::io::Cursor;

/// Parallelization audit verdict: **SKIP** (engine-parallel-surface D1, entry point 5).
/// Scalar defensibility scoring over the decompose output (the scoring arithmetic below) —
/// trivial and serial; no solve, no fan-out site.
pub fn check_defensibility_inner(req: VerificationRequest) -> Result<OptimizationResult, String> {
    // 1. Load Data
    let cursor = Cursor::new(&req.decomposition_params.csv_data);
    let mut df = CsvReader::new(cursor).finish().map_err(|e| e.to_string())?;

    // Cast to Float64
    let cast_cols = [&req.decomposition_params.outcome_variable]
        .into_iter()
        .chain(req.decomposition_params.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                let new_s = s
                    .cast(&DataType::Float64)
                    .map_err(|_| format!("Column '{}' contains non-numeric data.", col))?;
                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    // 2. Apply Predictor Overrides (Before Model Building)
    let mut overrides_map: HashMap<usize, HashMap<String, f64>> = HashMap::new();

    for adj in &req.adjustments {
        if let Some(ovr) = &adj.predictor_overrides {
            let mut row_overrides = HashMap::new();
            for (k, v) in ovr {
                // Try parsing value as f64
                if let Ok(val) = v.parse::<f64>() {
                    row_overrides.insert(k.clone(), val);
                }
            }
            if !row_overrides.is_empty() {
                overrides_map.insert(adj.index, row_overrides);
            }
        }
    }

    if !overrides_map.is_empty() {
        for col_name in &req.decomposition_params.predictors {
            if let Ok(s) = df.column(col_name) {
                if let Ok(ca) = s.f64() {
                    let mut vec: Vec<Option<f64>> = ca.into_iter().collect();
                    let mut changed = false;

                    for (row_idx, row_ovrs) in &overrides_map {
                        if let Some(new_val) = row_ovrs.get(col_name) {
                            if *row_idx < vec.len() {
                                vec[*row_idx] = Some(*new_val);
                                changed = true;
                            }
                        }
                    }

                    if changed {
                        let new_s = Series::new(col_name.as_str().into(), &vec);
                        df.with_column(new_s).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }

    // Initialize Pay Equity Problem
    let predictors: Vec<&str> = req
        .decomposition_params
        .predictors
        .iter()
        .map(|s| s.as_str())
        .collect();
    let cats_vec: Option<Vec<&str>> = req
        .decomposition_params
        .categorical_predictors
        .as_ref()
        .map(|c| c.iter().map(|s| s.as_str()).collect());

    let mut problem_builder = OaxacaBuilder::new(
        df.clone(),
        &req.decomposition_params.outcome_variable,
        &req.decomposition_params.group_variable,
        &req.decomposition_params.reference_group,
    );
    problem_builder.predictors(predictors.iter().copied());
    problem_builder.reference_coefficients(ReferenceCoefficients::Pooled);

    if let Some(cats) = &cats_vec {
        problem_builder.categorical_predictors(cats.iter().copied());
    }

    // Get Matrices.
    // get_data_matrices() returns (X_A = NON-reference, y_A, X_B = reference, y_B) — see
    // builder.rs:73 (group_b_name = reference_group). This binding deliberately routes the
    // reference (advantaged) group into local x_a/y_a so beta_fair is solved from it.
    // Correct as committed — do NOT "fix" it. Guardrail: ab_binding_regression_test.
    let (raw_x_b, _, raw_x_a, y_a, mut feature_names) = problem_builder
        .get_data_matrices()
        .map_err(|e| format!("Oaxaca Error: {}", e))?;

    let cols_a = raw_x_a.ncols();
    let predictors_count = predictors.len();

    // Strategy for Intercept
    let (x_a, x_b) = if cols_a > predictors_count {
        (raw_x_a.clone(), raw_x_b.clone())
    } else {
        feature_names.push("Base Rate (Intercept)".to_string());
        (
            raw_x_a.clone().insert_column(cols_a, 1.0),
            raw_x_b.clone().insert_column(raw_x_b.ncols(), 1.0),
        )
    };

    while feature_names.len() < x_b.ncols() {
        feature_names.push(format!("Feature {}", feature_names.len()));
    }

    // Calculate Fair Beta (Reference Target for "Defensibility")
    let beta_fair = x_a
        .clone()
        .svd(true, true)
        .solve(&y_a, 1e-9)
        .map_err(|e| format!("SVD Solve Error: {}", e))?;

    // --- Variance Calculation ---
    let predicted_y_a_fair = &x_a * &beta_fair;
    let residuals_a = &y_a - &predicted_y_a_fair;
    let rss = residuals_a.dot(&residuals_a);
    let degrees_of_freedom = (y_a.len() as f64) - (x_a.ncols() as f64);
    let sigma_squared = if degrees_of_freedom > 0.0 {
        rss / degrees_of_freedom
    } else {
        0.0
    };

    let xt_x = x_a.transpose() * &x_a;
    let _r = xt_x.nrows();
    let _c = xt_x.ncols();
    let cov_matrix = xt_x
        .try_inverse()
        .ok_or("Covariance matrix is singular, likely due to perfect multicollinearity.")?;

    // Confidence Level (Default 95%)
    let confidence = 0.95;
    let alpha = 1.0 - confidence;
    let p_value_z = 1.0 - (alpha / 2.0);
    let normal = Normal::new(0.0, 1.0).unwrap();
    let z_score = normal.inverse_cdf(p_value_z);

    let calculate_interval = move |features: DVector<f64>, predicted_y: f64| -> (f64, f64) {
        if sigma_squared <= 1e-9 {
            return (predicted_y, predicted_y);
        }
        let leverage = (features.transpose() * &cov_matrix * &features)[(0, 0)];
        let pred_variance = sigma_squared * (1.0 + leverage);
        let pred_se = pred_variance.sqrt();
        let margin = z_score * pred_se;
        (predicted_y - margin, predicted_y + margin)
    };

    // Process Specific Adjustments
    let mut results = Vec::new();

    // Mapping Original Index -> Matrix Row
    let group_col = df
        .column(&req.decomposition_params.group_variable)
        .map_err(|e| e.to_string())?;
    let groups_iter = group_col.str().map_err(|e| e.to_string())?.into_iter();

    let mut map_orig_to_matrix: HashMap<usize, (usize, bool)> = HashMap::new(); // Orig -> (MatrixRow, IsGroupA)
    let mut idx_a = 0;
    let mut idx_b = 0;

    for (idx, val_opt) in groups_iter.enumerate() {
        if let Some(val) = val_opt {
            if val == req.decomposition_params.reference_group {
                map_orig_to_matrix.insert(idx, (idx_a, true));
                idx_a += 1;
            } else {
                map_orig_to_matrix.insert(idx, (idx_b, false));
                idx_b += 1;
            }
        }
    }

    let wage_series = df
        .column(&req.decomposition_params.outcome_variable)
        .map_err(|e| e.to_string())?;
    let wage_array = wage_series.f64().map_err(|e| e.to_string())?;

    let feature_names_ref = &feature_names;

    for adj in req.adjustments {
        if let Some((matrix_idx, is_group_a)) = map_orig_to_matrix.get(&adj.index) {
            let matrix_idx = *matrix_idx;
            let is_group_a = *is_group_a;

            // Get features (updated with overrides)
            let features = if is_group_a {
                x_a.row(matrix_idx).transpose()
            } else {
                x_b.row(matrix_idx).transpose()
            };

            // Calculate Fair Wage (Point Estimate)
            let fair_wage = (&features.transpose() * &beta_fair)[(0, 0)];

            let (lower, upper) = calculate_interval(features, fair_wage);

            let current_wage = wage_array.get(adj.index).unwrap_or(0.0);

            // New Wage = Current (from CSV) + Adjustment (Delta)
            // Note: If Predictor Overrides changed the CSV data, current_wage might be weird?
            // No, wage column was NOT modified by overrides (only predictors).
            // But if user meant "wage override" via adjustment, we add it.
            let new_wage = current_wage + adj.value;

            // Defensibility Logic
            let is_defensible = new_wage >= (lower - 1.0);

            let msg = if is_defensible {
                Some("Wage is within or above the calculated fair range.".to_string())
            } else {
                Some(format!(
                    "Wage is {:.2} below the defensible lower bound ({:.2}).",
                    lower - new_wage,
                    lower
                ))
            };

            // Reconstruct contributions
            let mut contribs = Vec::new();
            let matrix = if is_group_a { &x_a } else { &x_b };
            for (j, name) in feature_names_ref.iter().enumerate() {
                if j < matrix.ncols() && j < beta_fair.len() {
                    let val = matrix[(matrix_idx, j)];
                    let coef = beta_fair[j];
                    contribs.push(Contribution {
                        name: name.clone(),
                        value: val * coef,
                    });
                }
            }

            results.push(Adjustment {
                index: adj.index,
                adjustment: adj.value,
                current_wage,
                new_wage,
                fair_wage,
                fair_wage_lower_bound: Some(lower),
                fair_wage_upper_bound: Some(upper),
                contributions: contribs, // Empty for now, simplified
                is_defensible: Some(is_defensible),
                defensibility_message: msg,
            });
        }
    }

    let mut total_need = 0.0;
    for (idx, (matrix_idx, is_group_a)) in &map_orig_to_matrix {
        if !*is_group_a {
            let actual = wage_array.get(*idx).unwrap_or(0.0);
            let features = x_b.row(*matrix_idx).transpose();
            let fair = (&features.transpose() * &beta_fair)[(0, 0)];
            if fair > actual {
                total_need += fair - actual;
            }
        }
    }

    // Calculate metrics
    let mut total_cost = 0.0;
    for adj in &results {
        total_cost += adj.adjustment;
    }

    let mut sum_a = 0.0;
    let mut count_a = 0.0;
    let mut sum_b = 0.0;
    let mut count_b = 0.0;

    let mut new_sum_a = 0.0;
    let mut new_sum_b = 0.0;

    for (idx, val_opt) in wage_array.iter().enumerate() {
        if let Some(v) = val_opt {
            if let Some(&(_matrix_idx, is_group_a)) = map_orig_to_matrix.get(&idx) {
                let mut adjusted_val = v;
                for adj in &results {
                    if adj.index == idx {
                        adjusted_val = adj.new_wage;
                        break;
                    }
                }

                if is_group_a {
                    sum_a += v;
                    new_sum_a += adjusted_val;
                    count_a += 1.0;
                } else {
                    sum_b += v;
                    new_sum_b += adjusted_val;
                    count_b += 1.0;
                }
            }
        }
    }

    let mean_a = if count_a > 0.0 { sum_a / count_a } else { 0.0 };
    let mean_b = if count_b > 0.0 { sum_b / count_b } else { 0.0 };
    let original_gap = mean_a - mean_b;

    let new_mean_a = if count_a > 0.0 {
        new_sum_a / count_a
    } else {
        0.0
    };
    let new_mean_b = if count_b > 0.0 {
        new_sum_b / count_b
    } else {
        0.0
    };
    let new_gap = new_mean_a - new_mean_b;

    // original_unexplained_gap and new_unexplained_gap
    // Calculate using beta_fair and actual wages for Group B
    let mut unexplained_sum_orig = 0.0;
    let mut unexplained_sum_new = 0.0;

    for (idx, (matrix_idx, is_group_a)) in &map_orig_to_matrix {
        if !*is_group_a {
            let actual = wage_array.get(*idx).unwrap_or(0.0);
            let features = x_b.row(*matrix_idx).transpose();
            let fair = (&features.transpose() * &beta_fair)[(0, 0)];

            let mut new_wage = actual;
            for adj in &results {
                if adj.index == *idx {
                    new_wage = adj.new_wage;
                    break;
                }
            }

            unexplained_sum_orig += fair - actual;
            unexplained_sum_new += fair - new_wage;
        }
    }

    let original_unexplained_gap = if count_b > 0.0 {
        unexplained_sum_orig / count_b
    } else {
        0.0
    };
    let new_unexplained_gap = if count_b > 0.0 {
        unexplained_sum_new / count_b
    } else {
        0.0
    };

    // Prepare Coefficients
    let mut model_coefficients = Vec::new();
    for (i, name) in feature_names.iter().enumerate() {
        if i < beta_fair.len() {
            model_coefficients.push(Contribution {
                name: name.clone(),
                value: beta_fair[i],
            });
        }
    }

    Ok(OptimizationResult {
        adjustments: results,
        total_cost,
        original_gap,
        new_gap,
        original_unexplained_gap,
        new_unexplained_gap,
        required_budget: total_need,
        model_coefficients,
    })
}
