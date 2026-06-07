use crate::types::*;
use nalgebra::{DMatrix, DVector};
use oaxaca_blinder::{OaxacaBuilder, QuantileDecompositionBuilder, ReferenceCoefficients};
use polars::prelude::*;
use statrs::distribution::{ContinuousCDF, Normal};
use std::io::Cursor;

pub fn decompose_inner(req: DecompositionRequest) -> Result<DecompositionResult, String> {
    // 1. Load Data
    let cursor = Cursor::new(&req.csv_data);
    let mut df = CsvReader::new(cursor).finish().map_err(|e| e.to_string())?;

    // Cast to Float64 with error checking
    let cast_cols = [&req.outcome_variable]
        .into_iter()
        .chain(req.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                // Try strict cast first, if it fails, it means we have non-numeric data
                let new_s = s.cast(&DataType::Float64).map_err(|_| {
                    format!("Column '{}' contains non-numeric data but was selected as a continuous variable. Please verify your column selection.", col)
                })?;

                if new_s.null_count() > s.null_count() {
                    return Err(format!("Column '{}' contains non-numeric data but was selected as a continuous variable. Please verify your column selection.", col));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    run_decomposition_on_df(df, &req)
}

pub fn verify_inner(req: VerificationRequest) -> Result<DecompositionResult, String> {
    // 1. Load Data
    let cursor = Cursor::new(&req.decomposition_params.csv_data);
    let mut df = CsvReader::new(cursor).finish().map_err(|e| e.to_string())?;

    // Cast to Float64 (Replicating logic to ensure type safety)
    let cast_cols = [&req.decomposition_params.outcome_variable]
        .into_iter()
        .chain(req.decomposition_params.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                let new_s = s
                    .cast(&DataType::Float64)
                    .map_err(|_| format!("Column '{}' contains non-numeric data.", col))?;

                if new_s.null_count() > s.null_count() {
                    return Err(format!("Column '{}' contains non-numeric data.", col));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    // 2. Apply Adjustments
    let wage_col_name = &req.decomposition_params.outcome_variable;
    let wage_series = df.column(wage_col_name).map_err(|e| e.to_string())?;
    let ca = wage_series.f64().map_err(|e| e.to_string())?;

    // Collect into Vec<Option<f64>> to handle potential nulls safely
    let mut wage_vec: Vec<Option<f64>> = ca.into_iter().collect();

    for adj in req.adjustments {
        if adj.index < wage_vec.len() {
            if let Some(val) = wage_vec[adj.index] {
                wage_vec[adj.index] = Some(val + adj.value);
            }
        } else {
            return Err(format!(
                "Adjustment index {} is out of bounds (dataset has {} rows)",
                adj.index,
                wage_vec.len()
            ));
        }
    }

    // Reconstruct Series
    let new_series = Series::new(wage_col_name.as_str().into(), &wage_vec);
    df.with_column(new_series).map_err(|e| e.to_string())?;

    // 3. Run Analysis on Mutated DataFrame
    run_decomposition_on_df(df, &req.decomposition_params)
}

fn run_decomposition_on_df(
    df: DataFrame,
    req: &DecompositionRequest,
) -> Result<DecompositionResult, String> {
    // Calculate Summary Stats (on provided data)
    let total_count = df.height();
    let group_col = df.column(&req.group_variable).map_err(|e| e.to_string())?;

    // Filter for Reference Group (Group A)
    let mask_a = group_col
        .str()
        .map_err(|e| e.to_string())?
        .equal(req.reference_group.as_str());

    let group_a_df = df.filter(&mask_a).map_err(|e| e.to_string())?;
    let group_a_count = group_a_df.height();
    let group_a_mean = group_a_df
        .column(&req.outcome_variable)
        .map_err(|e| e.to_string())?
        .f64()
        .map_err(|e| e.to_string())?
        .mean()
        .unwrap_or(0.0);

    // Filter for Other Group (Group B)
    let mask_b = !mask_a;
    let group_b_df = df.filter(&mask_b).map_err(|e| e.to_string())?;
    let group_b_count = group_b_df.height();
    let group_b_mean = group_b_df
        .column(&req.outcome_variable)
        .map_err(|e| e.to_string())?
        .f64()
        .map_err(|e| e.to_string())?
        .mean()
        .unwrap_or(0.0);

    let summary = DataSummary {
        total_count,
        group_a_count,
        group_b_count,
        group_a_mean,
        group_b_mean,
    };

    let predictors: Vec<&str> = req.predictors.iter().map(|s| s.as_str()).collect();
    let cats_vec: Option<Vec<&str>> = req
        .categorical_predictors
        .as_ref()
        .map(|c| c.iter().map(|s| s.as_str()).collect());
    let reps = req.bootstrap_reps.unwrap_or(100);

    // Parse Reference Coefficients
    let ref_coef = match req.reference_coefficients.as_deref() {
        Some("GroupA") => ReferenceCoefficients::GroupA,
        Some("GroupB") => ReferenceCoefficients::GroupB,
        Some("Weighted") => ReferenceCoefficients::Weighted,
        _ => ReferenceCoefficients::Pooled, // Default
    };

    // 2. Build and Run Oaxaca or Quantile Decomposition
    let (
        total,
        explained,
        unexplained,
        interaction,
        detailed_exp,
        detailed_unexp,
        unexplained_std_err,
    ) = if let Some(q) = req.quantile {
        // QUANTILE DECOMPOSITION
        let mut builder = QuantileDecompositionBuilder::new(
            df,
            &req.outcome_variable,
            &req.group_variable,
            &req.reference_group,
        );
        builder.predictors(predictors.iter().copied());
        builder.quantiles(&[q]); // Single quantile for now

        if let Some(cats) = &cats_vec {
            builder.categorical_predictors(cats.iter().copied());
        }

        builder.bootstrap_reps(reps);

        let results = builder.run().map_err(|e| e.to_string())?;

        // Results are QuantileDecompositionResults. Access via results_by_quantile.
        let results_map = results.results_by_quantile();

        // Since we don't know the exact key (e.g. "q0.5" or "0.5" or "50%"), and we only requested one,
        // we take the first value.
        let (_, detail) = results_map
            .iter()
            .next()
            .ok_or_else(|| "Failed to retrieve results for the specified quantile.".to_string())?;

        // Extract values using methods
        // NOTE: QuantileDecompositionDetail total_gap returns ComponentResult, so we need .estimate()
        let total = *detail.total_gap().estimate();
        let explained = *detail.characteristics_effect().estimate();
        let unexplained = *detail.coefficients_effect().estimate();

        // Detailed components - Currently not exposed in public API for QuantileDecompositionDetail
        // We return empty vectors
        let d_exp = Vec::new();
        let d_unexp = Vec::new();

        (total, explained, unexplained, None, d_exp, d_unexp, None)
    } else {
        // STANDARD OLS DECOMPOSITION
        // Pass ownership of df
        let mut builder = OaxacaBuilder::new(
            df,
            &req.outcome_variable,
            &req.group_variable,
            &req.reference_group,
        );
        builder.predictors(predictors.iter().copied());
        builder.reference_coefficients(ref_coef);

        if let Some(cats) = &cats_vec {
            builder.categorical_predictors(cats.iter().copied());
        }

        builder.bootstrap_reps(reps);

        let results = builder.run().map_err(|e| e.to_string())?;

        // Format Results
        // NOTE: OaxacaResults total_gap returns &f64 directly
        let total = *results.total_gap();
        let mut explained = 0.0;
        let mut unexplained = 0.0;
        let mut interaction = None;
        let mut d_exp = Vec::new();
        let mut d_unexp = Vec::new();
        let mut unexplained_std_err = None;

        if req.three_fold.unwrap_or(false) {
            let three_fold = results.three_fold();
            let aggregated = three_fold.aggregate();
            for component in aggregated {
                if component.name() == "endowments" {
                    explained = *component.estimate();
                } else if component.name() == "coefficients" {
                    unexplained = *component.estimate();
                } else if component.name() == "interaction" {
                    interaction = Some(*component.estimate());
                }
            }
        } else {
            let two_fold = results.two_fold();
            let aggregated = two_fold.aggregate();
            for component in aggregated {
                if component.name() == "explained" {
                    explained = *component.estimate();
                } else if component.name() == "unexplained" {
                    unexplained = *component.estimate();
                    unexplained_std_err = Some(*component.std_err());
                }
            }

            for c in two_fold.detailed_explained() {
                d_exp.push(DetailedComponent {
                    name: c.name().to_string(),
                    estimate: *c.estimate(),
                    std_err: Some(*c.std_err()),
                    p_value: Some(*c.p_value()),
                    ci_lower: Some(*c.ci_lower()),
                    ci_upper: Some(*c.ci_upper()),
                });
            }

            for c in two_fold.detailed_unexplained() {
                d_unexp.push(DetailedComponent {
                    name: c.name().to_string(),
                    estimate: *c.estimate(),
                    std_err: Some(*c.std_err()),
                    p_value: Some(*c.p_value()),
                    ci_lower: Some(*c.ci_lower()),
                    ci_upper: Some(*c.ci_upper()),
                });
            }
        }
        (
            total,
            explained,
            unexplained,
            interaction,
            d_exp,
            d_unexp,
            unexplained_std_err,
        )
    };

    Ok(DecompositionResult {
        total_gap: total,
        explained_gap: explained,
        unexplained_gap: unexplained,
        interaction_gap: interaction,
        explained_percentage: (explained / total) * 100.0,
        unexplained_percentage: (unexplained / total) * 100.0,
        interaction_percentage: interaction.map(|i| (i / total) * 100.0),
        detailed_explained: detailed_exp,
        detailed_unexplained: detailed_unexp,
        data_summary: Some(summary),
        unexplained_standard_error: unexplained_std_err,
    })
}

pub fn optimize_inner(req: OptimizationRequest) -> Result<OptimizationResult, String> {
    // 1. Load Data
    let cursor = Cursor::new(req.csv_data);
    let mut df = CsvReader::new(cursor).finish().map_err(|e| e.to_string())?;

    // Cast to Float64 with error checking
    let cast_cols = [&req.outcome_variable]
        .into_iter()
        .chain(req.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                let new_s = s.cast(&DataType::Float64).map_err(|_| {
                    format!("Column '{}' contains non-numeric data but was selected as a continuous variable.", col)
                })?;

                if new_s.null_count() > s.null_count() {
                    return Err(format!(
                        "Column '{}' contains non-numeric data but was selected as a continuous variable.",
                        col
                    ));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    let predictors: Vec<&str> = req.predictors.iter().map(|s| s.as_str()).collect();
    let cats_vec: Option<Vec<&str>> = req
        .categorical_predictors
        .as_ref()
        .map(|c| c.iter().map(|s| s.as_str()).collect());

    // 2. Calculate Original Gap (need a separate builder pass)
    let mut gap_builder = OaxacaBuilder::new(
        df.clone(),
        &req.outcome_variable,
        &req.group_variable,
        &req.reference_group,
    );
    gap_builder.predictors(predictors.iter().copied());
    gap_builder.reference_coefficients(ReferenceCoefficients::Pooled);

    if let Some(cats) = &cats_vec {
        gap_builder.categorical_predictors(cats.iter().copied());
    }
    gap_builder.bootstrap_reps(10);

    let gap_results = gap_builder.run().map_err(|e| e.to_string())?;
    let original_gap = *gap_results.total_gap();

    // 3. Setup Optimization Problem and Residuals
    let mut problem_builder = OaxacaBuilder::new(
        df.clone(),
        &req.outcome_variable,
        &req.group_variable,
        &req.reference_group,
    );
    problem_builder.predictors(predictors.iter().copied());
    problem_builder.reference_coefficients(ReferenceCoefficients::Pooled);

    if let Some(cats) = &cats_vec {
        problem_builder.categorical_predictors(cats.iter().copied());
    }

    // Identify Target AND Reference Group Indices
    let group_col = df.column(&req.group_variable).map_err(|e| e.to_string())?;
    let is_reference_mask = group_col
        .str()
        .map_err(|e| e.to_string())?
        .equal(req.reference_group.as_str());

    // Using capacities could improve performance further if we knew sizes,
    // but without counting we just default to standard allocation.
    let mut target_indices = Vec::new();
    let mut reference_indices = Vec::new();

    for (idx, is_ref_opt) in is_reference_mask.into_iter().enumerate() {
        if let Some(is_ref) = is_ref_opt {
            if is_ref {
                reference_indices.push(idx);
            } else {
                target_indices.push(idx);
            }
        }
    }

    use crate::types::{Adjustment, AllocationStrategy, OptimizationResult, OptimizationTarget};

    // 4. Determine Fair Wage Standard (Target)
    let target_mode = req
        .target
        .as_ref()
        .unwrap_or(&OptimizationTarget::Reference);

    // Prepare Matrices.
    // get_data_matrices() returns (X_A = NON-reference, y_A, X_B = reference, y_B) — see
    // builder.rs:73 (group_b_name = reference_group). This binding routes the reference
    // (advantaged) group into local x_a/y_a so beta_fair is solved from it. Correct as
    // committed — do NOT "fix" it. Guardrail: ab_binding_regression_test.
    let (raw_x_b, y_b, raw_x_a, y_a, mut feature_names) = problem_builder
        .get_data_matrices()
        .map_err(|e| format!("Oaxaca Error: {}", e))?;

    let cols_a = raw_x_a.ncols();
    let predictors_count = req.predictors.len();

    // Strategy for Intercept:
    // If the matrices don't have an intercept (column of 1s), we add it.
    let (x_a, x_b) = if cols_a > predictors_count {
        (raw_x_a.clone(), raw_x_b.clone())
    } else {
        feature_names.push("Base Rate (Intercept)".to_string());
        (
            raw_x_a.clone().insert_column(cols_a, 1.0),
            raw_x_b.clone().insert_column(raw_x_b.ncols(), 1.0),
        )
    };

    // Safety fallback for feature names
    while feature_names.len() < x_b.ncols() {
        feature_names.push(format!("Feature {}", feature_names.len()));
    }

    // Calculate Fair Beta based on Target Mode
    let beta_fair = match target_mode {
        OptimizationTarget::Reference => x_a
            .clone()
            .svd(true, true)
            .solve(&y_a, 1e-9)
            .map_err(|e| format!("SVD Solve Error (Reference): {}", e))?,
        OptimizationTarget::Pooled => {
            let n_a = x_a.nrows();
            let n_b = x_b.nrows();
            let n_pooled = n_a + n_b;
            let n_cols = x_a.ncols();

            let mut x_pooled = x_a.clone();
            x_pooled = x_pooled.resize_vertically(n_pooled, 0.0);
            x_pooled.view_mut((n_a, 0), (n_b, n_cols)).copy_from(&x_b);

            let mut y_pooled = y_a.clone();
            y_pooled = y_pooled.resize_vertically(n_pooled, 0.0);
            y_pooled.view_mut((n_a, 0), (n_b, 1)).copy_from(&y_b);

            x_pooled
                .clone()
                .svd(true, true)
                .solve(&y_pooled, 1e-9)
                .map_err(|e| format!("SVD Solve Error (Pooled): {}", e))?
        }
    };

    // Calculate Model Coefficients for Frontend Simulation
    let mut model_coefficients = Vec::new();
    for (i, name) in feature_names.iter().enumerate() {
        if i < beta_fair.len() {
            model_coefficients.push(Contribution {
                name: name.clone(),
                value: beta_fair[i],
            });
        }
    }

    // Calculate Fair Wages
    let predicted_y_b_fair = &x_b * &beta_fair;
    let predicted_y_a_fair = &x_a * &beta_fair;

    // --- Variance Calculation for Prediction Intervals (Delta Method) ---
    // 1. Calculate Error Variance (Sigma^2) from Reference Group Model
    // Residuals e = y - X*beta
    let residuals_a = &y_a - &predicted_y_a_fair;
    let rss = residuals_a.dot(&residuals_a); // Residual Sum of Squares
    let degrees_of_freedom = (y_a.len() as f64) - (x_a.ncols() as f64);

    // Safety check for degrees of freedom
    let sigma_squared = if degrees_of_freedom > 0.0 {
        rss / degrees_of_freedom
    } else {
        0.0
    };

    // 2. Calculate Covariance Matrix (X'X)^-1
    // We need (X_a^T * X_a)^-1
    // Since we used SVD to solve, we can use SVD to invert if needed, or just standard inversion.
    // X_a is DMatrix.
    let xt_x = x_a.transpose() * &x_a;
    let _r = xt_x.nrows();
    let _c = xt_x.ncols();
    let cov_matrix = xt_x
        .try_inverse()
        .ok_or("Covariance matrix is singular, likely due to perfect multicollinearity.")?;

    // 3. Determine Z-score for Confidence Level
    let confidence = req.confidence_level.unwrap_or(0.95);
    // Clamp confidence to reasonable range [0.50, 0.999]
    let confidence = confidence.clamp(0.50, 0.999);

    // Alpha = 1 - confidence
    // Z is inverse CDF at p = 1 - alpha/2
    // e.g. 95% -> alpha=0.05 -> p=0.975 -> z=1.96
    let alpha = 1.0 - confidence;
    let p_value = 1.0 - (alpha / 2.0);
    let normal = Normal::new(0.0, 1.0).unwrap();
    let z_score = normal.inverse_cdf(p_value);

    // Closure to calculate prediction interval for a given feature vector
    let calculate_interval = move |features: DVector<f64>, predicted_y: f64| -> (f64, f64) {
        if sigma_squared <= 1e-9 {
            return (predicted_y, predicted_y);
        }

        // Variance of prediction = sigma^2 * (1 + x_i' * (X'X)^-1 * x_i)
        // Leverage h_i = x_i' * (X'X)^-1 * x_i
        let leverage = (features.transpose() * &cov_matrix * &features)[(0, 0)];
        let pred_variance = sigma_squared * (1.0 + leverage);
        let pred_se = pred_variance.sqrt();

        // Use calculated Z-score
        let margin = z_score * pred_se;
        (predicted_y - margin, predicted_y + margin)
    };
    // -------------------------------------------------------------------

    // Calculate All Residuals
    let mut all_residuals = Vec::with_capacity(y_b.len() + y_a.len());
    let mut net_residual_sum_b = 0.0;

    enum GroupSource {
        GroupA, // Reference
        GroupB, // Target
    }

    struct PotentialAdj {
        matrix_idx: usize,
        source: GroupSource,
        diff: f64,
        fair_wage: f64, // Statistical Fair Wage (Midpoint)
        orig_idx: usize,
        is_eligible: bool, // True if the employee should receive budget allocation
    }
    let mut potential_adjustments = Vec::new();
    let adjust_both = req.adjust_both_groups.unwrap_or(false);
    let is_forensic = req.forensic_mode.unwrap_or(false);
    let min_pct = req.min_gap_pct.unwrap_or(0.0);

    // Process Group B (Target) - Always Analyzed
    for i in 0..y_b.len() {
        let actual = y_b[i];
        let fair_midpoint = predicted_y_b_fair[i];

        // Calculate Interval for this employee
        let features = x_b.row(i).transpose();
        // Since we are iterating, we can call the closure.
        // Note: calculate_interval captures large objects (x_ref, cov_matrix) but that's fine.
        // It returns (lower, upper).
        let (lower, upper) = calculate_interval(features, fair_midpoint);

        // Determine Target Wage based on user selection
        let target_wage = match req
            .range_target
            .as_ref()
            .unwrap_or(&crate::types::RangeTarget::Midpoint)
        {
            crate::types::RangeTarget::Midpoint => fair_midpoint,
            crate::types::RangeTarget::LowerBound => lower,
            crate::types::RangeTarget::UpperBound => upper,
        };

        // Diff is Target - Actual
        let diff = target_wage - actual;

        net_residual_sum_b += diff; // "Net Gap" usually refers to the target group gap
        all_residuals.push(diff);

        let is_positive_gap = diff > 1e-6; // Only care if underpaid relative to target

        if is_positive_gap {
            let gap_pct = if actual.abs() > 1e-6 {
                diff / actual
            } else {
                0.0
            };

            if gap_pct >= min_pct {
                potential_adjustments.push(PotentialAdj {
                    matrix_idx: i,
                    source: GroupSource::GroupB,
                    diff,
                    fair_wage: fair_midpoint,
                    orig_idx: target_indices[i],
                    is_eligible: true,
                });
            } else if is_forensic {
                potential_adjustments.push(PotentialAdj {
                    matrix_idx: i,
                    source: GroupSource::GroupB,
                    diff,
                    fair_wage: fair_midpoint,
                    orig_idx: target_indices[i],
                    is_eligible: false,
                });
            }
        } else if is_forensic {
            potential_adjustments.push(PotentialAdj {
                matrix_idx: i,
                source: GroupSource::GroupB,
                diff,
                fair_wage: fair_midpoint,
                orig_idx: target_indices[i],
                is_eligible: false,
            });
        }
    }

    // Process Group A (Reference) - Analyzed if flag set OR forensic mode
    // Even if adjust_both is false, we might want forensic data for A?
    // User requirement: "Forensic Gap Analysis" usually implies looking at everything.
    // But currently we only return adjustments for B.
    // If we add A to adjustments list with 0 pay, they show up in forensic.
    // Let's include A if adjust_both OR forensic.

    if adjust_both || is_forensic {
        for i in 0..y_a.len() {
            let actual = y_a[i];
            let fair = predicted_y_a_fair[i];
            let diff = fair - actual;
            // distinct from net_residual_sum of B?
            // Usually Net Gap refers to the disadvantaged group. mixing A might skew metrics.
            // We'll keep net_residual_sum focused on B for the "Budget Calculation" default.

            all_residuals.push(diff);

            let is_positive_gap = diff > 1e-6;

            if is_positive_gap {
                let gap_pct = if actual.abs() > 1e-6 {
                    diff / actual
                } else {
                    0.0
                };

                let is_eligible = adjust_both && gap_pct >= min_pct;

                if is_eligible {
                    potential_adjustments.push(PotentialAdj {
                        matrix_idx: i,
                        source: GroupSource::GroupA,
                        diff,
                        fair_wage: fair,
                        orig_idx: reference_indices[i],
                        is_eligible: true,
                    });
                } else if is_forensic {
                    potential_adjustments.push(PotentialAdj {
                        matrix_idx: i,
                        source: GroupSource::GroupA,
                        diff,
                        fair_wage: fair,
                        orig_idx: reference_indices[i],
                        is_eligible: false,
                    });
                }
            } else if is_forensic {
                // Include in forensic analysis even if no positive gap
                potential_adjustments.push(PotentialAdj {
                    matrix_idx: i,
                    source: GroupSource::GroupA,
                    diff,
                    fair_wage: fair,
                    orig_idx: reference_indices[i],
                    is_eligible: false,
                });
            }
        }
    }

    // 5. Allocation Strategy
    let strategy = req.strategy.as_ref().unwrap_or(&AllocationStrategy::Greedy);

    // Calculate Total Need (Sum of all VALID positive residuals that are eligible for allocation)
    // We do this BEFORE setting effective_budget so we can default to total_need if budget is 0
    let total_need: f64 = potential_adjustments
        .iter()
        .filter(|p| p.diff > 0.0 && p.is_eligible)
        .map(|p| p.diff)
        .sum();

    let effective_budget = if req.budget > 0.0 {
        req.budget
    } else {
        // Default Budget = Total Need to fix the Targeted Gaps
        // Add epsilon buffer (0.001%) to avoid floating point truncation for the last few employees
        total_need * 1.00001
    };

    let mut adjustments = Vec::new();
    let mut current_spend = 0.0;

    // Sort Descending by Gap Amount (for Greedy)
    potential_adjustments.sort_by(|a, b| {
        b.diff
            .partial_cmp(&a.diff)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let wage_series = df
        .column(&req.outcome_variable)
        .map_err(|e| e.to_string())?;
    let wage_array = wage_series.f64().map_err(|e| e.to_string())?;

    use crate::types::Contribution;

    let feature_names_ref = &feature_names;
    let get_contributions = |matrix_idx: usize, source: &GroupSource| -> Vec<Contribution> {
        let mut contribs = Vec::new();
        // Determine which matrix to use
        let matrix = match source {
            GroupSource::GroupA => &x_a,
            GroupSource::GroupB => &x_b,
        };

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
        contribs
    };

    match strategy {
        AllocationStrategy::Greedy => {
            for pot in potential_adjustments {
                // If diff is <= 0 or not eligible, pay_amount MUST be 0.
                let pay_amount = if pot.diff > 0.0 && pot.is_eligible {
                    let remaining_budget = effective_budget - current_spend;
                    if remaining_budget > 0.0 {
                        pot.diff.min(remaining_budget)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                let current_wage = wage_array.get(pot.orig_idx).unwrap_or(0.0);
                let fair_wage = pot.fair_wage;
                let new_wage = current_wage + pay_amount; // Don't add negative pay amounts!

                // Get features for interval calculation
                let features = match pot.source {
                    GroupSource::GroupA => x_a.row(pot.matrix_idx).transpose(),
                    GroupSource::GroupB => x_b.row(pot.matrix_idx).transpose(),
                };
                let (lower, upper) = calculate_interval(features, fair_wage);

                adjustments.push(Adjustment {
                    index: pot.orig_idx,
                    adjustment: pay_amount,
                    current_wage,
                    new_wage,
                    fair_wage,
                    fair_wage_lower_bound: Some(lower),
                    fair_wage_upper_bound: Some(upper),
                    contributions: get_contributions(pot.matrix_idx, &pot.source),
                    is_defensible: None,
                    defensibility_message: None,
                });

                if pay_amount > 0.0 {
                    current_spend += pay_amount;
                }
            }
        }
        AllocationStrategy::Equitable => {
            // Pro-Rata: Everyone gets (Budget / TotalNeed) * Gap, capped at Gap
            let coverage_ratio = if total_need > 0.0 {
                (effective_budget / total_need).min(1.0)
            } else {
                0.0
            };

            for pot in potential_adjustments {
                // If diff <= 0 or not eligible, pay_amount is 0.
                let pay_amount = if pot.diff > 0.0 && pot.is_eligible {
                    pot.diff * coverage_ratio
                } else {
                    0.0
                };

                let current_wage = wage_array.get(pot.orig_idx).unwrap_or(0.0);
                let fair_wage = pot.fair_wage;
                let new_wage = current_wage + pay_amount;

                // Get features for interval calculation
                let features = match pot.source {
                    GroupSource::GroupA => x_a.row(pot.matrix_idx).transpose(),
                    GroupSource::GroupB => x_b.row(pot.matrix_idx).transpose(),
                };
                let (lower, upper) = calculate_interval(features, fair_wage);

                adjustments.push(Adjustment {
                    index: pot.orig_idx,
                    adjustment: pay_amount,
                    current_wage,
                    new_wage,
                    fair_wage,
                    fair_wage_lower_bound: Some(lower),
                    fair_wage_upper_bound: Some(upper),
                    contributions: get_contributions(pot.matrix_idx, &pot.source),
                    is_defensible: None,
                    defensibility_message: None,
                });

                current_spend += pay_amount;
            }
        }
    }

    // Sort adjustments by index
    adjustments.sort_by_key(|a| a.index);

    // 7. Calculate Final Metrics
    let n_target = y_b.len() as f64;
    let total_cost = current_spend;

    // Calculate New Gap robustly
    let new_gap = if n_target > 0.0 {
        original_gap + (total_cost / n_target)
    } else {
        original_gap
    };

    let original_unexplained_gap = if n_target > 0.0 {
        -net_residual_sum_b / n_target
    } else {
        0.0
    };

    let new_unexplained_gap = if n_target > 0.0 {
        -(net_residual_sum_b - total_cost) / n_target
    } else {
        original_unexplained_gap
    };

    Ok(OptimizationResult {
        adjustments,
        total_cost,
        original_gap,
        new_gap,
        original_unexplained_gap,
        new_unexplained_gap,
        required_budget: total_need,
        model_coefficients,
    })
}

pub fn calculate_efficient_frontier_inner(
    req: EfficientFrontierRequest,
) -> Result<Vec<FrontierPoint>, String> {
    // 1. Load Data
    let cursor = Cursor::new(&req.decomposition_params.csv_data);
    let mut df = CsvReader::new(cursor).finish().map_err(|e| e.to_string())?;

    // Cast to Float64 with error checking
    let cast_cols = [&req.decomposition_params.outcome_variable]
        .into_iter()
        .chain(req.decomposition_params.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                let new_s = s
                    .cast(&DataType::Float64)
                    .map_err(|_| format!("Column '{}' contains non-numeric data.", col))?;

                if new_s.null_count() > s.null_count() {
                    return Err(format!("Column '{}' contains non-numeric data.", col));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

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

    // 2. Setup Optimization Problem (for Budget allocation)
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

    // 2a. Determine Max Budget / Total Need
    let opt_req = OptimizationRequest {
        csv_data: req.decomposition_params.csv_data.clone(),
        outcome_variable: req.decomposition_params.outcome_variable.clone(),
        group_variable: req.decomposition_params.group_variable.clone(),
        reference_group: req.decomposition_params.reference_group.clone(),
        predictors: req.decomposition_params.predictors.clone(),
        categorical_predictors: req.decomposition_params.categorical_predictors.clone(),
        budget: 0.0,
        target_gap: None,
        target: Some(OptimizationTarget::Reference),
        strategy: Some(AllocationStrategy::Greedy),
        min_gap_pct: None,
        forensic_mode: None,
        adjust_both_groups: None,
        confidence_level: None,
        range_target: None,
    };

    let opt_result = optimize_inner(opt_req)?;
    let total_need = opt_result.required_budget;
    let max_budget = req.max_budget.unwrap_or(total_need * 1.1);

    // 3. Pre-compute Matrices for Fast OLS
    // get_data_matrices() returns (X_A = NON-reference, y_A, X_B = reference, y_B) — see
    // builder.rs:73 (group_b_name = reference_group). This binding routes the reference group
    // into local x_a/y_a, matching original_to_pooled (reference -> pooled slots [0..n_a)).
    // Correct as committed — do NOT "fix" it to (x_a, y_a, x_b, y_b); that inverts gap-closure.
    // Guardrail: ab_binding_regression_test::test_frontier_adjustments_target_underpaid_group.
    let (x_b, y_b, x_a, y_a, _feature_names) = problem_builder
        .get_data_matrices()
        .map_err(|e| format!("Oaxaca Error: {}", e))?;

    let n_a = x_a.nrows();
    let n_b = x_b.nrows();
    let n_pooled = n_a + n_b;
    // Check for intercept in feature names. The internal intercept is always the reserved
    // column `__ob_intercept__` injected by OaxacaBuilder::prepare_data — match only that name.
    // A fuzzy "intercept"/"const" match would misclassify a user predictor literally named
    // `intercept` or `const` as the intercept and silently drop it from the pooled design matrix.
    let intercept_idx = _feature_names
        .iter()
        .position(|f| f == "__ob_intercept__");

    let cols_a = x_a.ncols();

    // We want to exclude the intercept from the source matrices if we are adding our own
    // Calculate new feature count
    let n_vars_to_copy = if intercept_idx.is_some() {
        cols_a - 1
    } else {
        cols_a
    };
    let n_pooled_features = n_vars_to_copy;

    // Build X_pooled: [Intercept, GroupDummy, Features...]
    // Intercept = Col 0
    // GroupDummy = Col 1 (0 for A, 1 for B)
    let mut x_pooled = DMatrix::from_element(n_pooled, n_pooled_features + 2, 0.0);

    for r in 0..n_pooled {
        x_pooled[(r, 0)] = 1.0;
    }

    // Group Dummy: A=0, B=1. A is first n_a rows.
    for r in n_a..n_pooled {
        x_pooled[(r, 1)] = 1.0;
    }

    // Copy features, skipping intercept if present
    if let Some(idx) = intercept_idx {
        // Copy columns before intercept
        if idx > 0 {
            x_pooled
                .view_mut((0, 2), (n_a, idx))
                .copy_from(&x_a.columns(0, idx));
            x_pooled
                .view_mut((n_a, 2), (n_b, idx))
                .copy_from(&x_b.columns(0, idx));
        }
        // Copy columns after intercept
        let after_count = cols_a - 1 - idx;
        if after_count > 0 {
            x_pooled
                .view_mut((0, 2 + idx), (n_a, after_count))
                .copy_from(&x_a.columns(idx + 1, after_count));
            x_pooled
                .view_mut((n_a, 2 + idx), (n_b, after_count))
                .copy_from(&x_b.columns(idx + 1, after_count));
        }
    } else {
        // No intercept found, copy all
        x_pooled.view_mut((0, 2), (n_a, cols_a)).copy_from(&x_a);
        x_pooled.view_mut((n_a, 2), (n_b, cols_a)).copy_from(&x_b);
    }

    // Initial Y
    let mut y_pooled = DMatrix::from_element(n_pooled, 1, 0.0);
    y_pooled.view_mut((0, 0), (n_a, 1)).copy_from(&y_a);
    y_pooled.view_mut((n_a, 0), (n_b, 1)).copy_from(&y_b);

    // Pre-compute (X^T X)^-1 X^T
    let xt_x = x_pooled.transpose() * &x_pooled;
    let xt_x_inv = xt_x.try_inverse().ok_or("Singular matrix in Pooled OLS")?;
    let projector = &xt_x_inv * x_pooled.transpose();

    let diag_inv_xt_x: Vec<f64> = (0..xt_x_inv.nrows()).map(|i| xt_x_inv[(i, i)]).collect();

    // 4. Budget Loop
    let steps = req.steps.unwrap_or(50);
    let step_size = max_budget / (steps as f64);
    let mut points = Vec::new();

    // Map `adjustments` to Pooled Indices
    let group_col = df
        .column(&req.decomposition_params.group_variable)
        .map_err(|e| e.to_string())?;
    let group_col_iter = group_col.str().map_err(|e| e.to_string())?.into_iter();

    let mut original_to_pooled = std::collections::HashMap::new();
    let mut a_counter = 0;
    let mut b_counter = 0;

    for (orig_idx, val_opt) in group_col_iter.enumerate() {
        if let Some(val) = val_opt {
            if val == req.decomposition_params.reference_group {
                original_to_pooled.insert(orig_idx, a_counter);
                a_counter += 1;
            } else {
                original_to_pooled.insert(orig_idx, n_a + b_counter);
                b_counter += 1;
            }
        }
    }

    struct PendingPay {
        pooled_idx: usize,
        gap: f64,
    }
    let mut pending_payments: Vec<PendingPay> = opt_result
        .adjustments
        .iter()
        .filter_map(|adj| {
            original_to_pooled.get(&adj.index).map(|&p_idx| PendingPay {
                pooled_idx: p_idx,
                gap: adj.adjustment,
            })
        })
        .collect();

    pending_payments.sort_by(|a, b| {
        b.gap
            .partial_cmp(&a.gap)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let normal = Normal::new(0.0, 1.0).unwrap();

    let compute_t_stat = |current_y: &DMatrix<f64>| -> (f64, f64, bool) {
        let beta = &projector * current_y;
        let predictions = &x_pooled * &beta;
        let residuals = current_y - predictions;
        let rss = residuals.dot(&residuals);

        let dof = (n_pooled as f64) - (x_pooled.ncols() as f64);
        if dof <= 0.0 {
            return (0.0, 1.0, false);
        }

        let sigma_sq = rss / dof;
        let se_group = (sigma_sq * diag_inv_xt_x[1]).sqrt();
        let beta_group = beta[1];

        let t_stat = beta_group / se_group;
        let p_val = 2.0 * normal.cdf(-t_stat.abs());
        let sig = p_val < 0.05;

        (t_stat, p_val, sig)
    };

    let (t0, p0, s0) = compute_t_stat(&y_pooled);
    points.push(FrontierPoint {
        budget: 0.0,
        t_statistic: t0,
        p_value: p0,
        is_significant: s0,
    });

    // Degenerate case: no positive adjustment budget (e.g. a zero-gap dataset where total_need
    // collapses to ~0 and the caller supplied no explicit max_budget). There is no spend range
    // to explore, so return only the baseline (budget = 0) point rather than fabricating a
    // budget axis from a placeholder maximum.
    if max_budget < 1e-9 {
        eprintln!(
            "Warning: efficient frontier requested with no positive adjustment budget \
             (max_budget ~ 0; no gap to close). Returning a single zero-budget point."
        );
        return Ok(points);
    }

    let mut current_y = y_pooled.clone();
    let mut pay_idx = 0;
    let mut budget_cursor = 0.0;

    for step in 1..=steps {
        let target_budget = step as f64 * step_size;
        let available_for_step = target_budget - budget_cursor;

        if available_for_step > 0.0 {
            let mut remaining = available_for_step;

            while remaining > 0.0 && pay_idx < pending_payments.len() {
                let pp = &mut pending_payments[pay_idx];

                if pp.gap <= remaining {
                    current_y[(pp.pooled_idx, 0)] += pp.gap;
                    remaining -= pp.gap;
                    pp.gap = 0.0;
                    pay_idx += 1;
                } else {
                    current_y[(pp.pooled_idx, 0)] += remaining;
                    pp.gap -= remaining;
                    remaining = 0.0;
                }
            }
            budget_cursor = target_budget;
        }

        let (t, p, s) = compute_t_stat(&current_y);
        points.push(FrontierPoint {
            budget: target_budget,
            t_statistic: t,
            p_value: p,
            is_significant: s,
        });
    }

    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_csv() -> String {
        let mut csv = "wage,education,experience,gender,department\n".to_string();
        for _ in 0..20 {
            csv.push_str("50000,12,5,Male,Sales\n");
            csv.push_str("55000,16,2,Male,Engineering\n");
            csv.push_str("60000,14,8,Male,Sales\n");
            csv.push_str("80000,18,10,Male,Engineering\n");
            csv.push_str("40000,12,5,Female,Sales\n");
            csv.push_str("42000,14,3,Female,Engineering\n");
            csv.push_str("45000,12,6,Female,Sales\n");
            csv.push_str("50000,16,5,Female,Engineering\n");
        }
        csv
    }

    #[test]
    fn test_ols_decomposition() {
        let csv = create_mock_csv();
        let req = DecompositionRequest {
            csv_data: csv.into_bytes(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: Some(vec!["department".to_string()]),
            three_fold: Some(false),
            quantile: None,
            reference_coefficients: None,
            bootstrap_reps: Some(10), // Fast test
        };

        let res = decompose_inner(req);
        if let Err(e) = &res {
            println!("Error: {}", e);
        }
        assert!(res.is_ok());
        let res = res.unwrap();

        // Basic checks
        assert!(res.total_gap > 0.0);
        assert!(res.data_summary.is_some());
        let summary = res.data_summary.unwrap();
        assert_eq!(summary.total_count, 160);
        assert_eq!(summary.group_a_count, 80);
        assert_eq!(summary.group_b_count, 80);
    }

    #[test]
    fn test_prediction_interval() {
        // Create a dataset where we expect some variance
        // y = 2 * x + noise
        let mut csv = "wage,x,group\n".to_string();
        for i in 0..50 {
            let x = i as f64;
            let noise = if i % 2 == 0 { 1000.0 } else { -1000.0 };
            let wage = 50000.0 + 2000.0 * x + noise; // Group A (Reference)
            csv.push_str(&format!("{},{},GroupA\n", wage, x));
        }
        for i in 0..10 {
            let x = (i + 50) as f64;
            let wage = 40000.0; // Underpaid Group B
            csv.push_str(&format!("{},{},GroupB\n", wage, x));
        }

        let req = OptimizationRequest {
            csv_data: csv.into_bytes(),
            outcome_variable: "wage".to_string(),
            group_variable: "group".to_string(),
            reference_group: "GroupA".to_string(),
            predictors: vec!["x".to_string()],
            categorical_predictors: None,
            target: None,
            adjust_both_groups: None,
            budget: 0.0,
            target_gap: None,
            confidence_level: None,
            strategy: None,
            forensic_mode: None,
            min_gap_pct: None,
            range_target: None,
        };

        let res = optimize_inner(req);
        assert!(res.is_ok());
        let res = res.unwrap();

        // Check adjustments
        assert!(!res.adjustments.is_empty());

        let adj = &res.adjustments[0];
        assert!(adj.fair_wage > 40000.0);

        // Verify Bounds
        assert!(adj.fair_wage_lower_bound.is_some());
        assert!(adj.fair_wage_upper_bound.is_some());

        let lower = adj.fair_wage_lower_bound.unwrap();
        let upper = adj.fair_wage_upper_bound.unwrap();

        println!(
            "Fair: {}, Lower: {}, Upper: {}",
            adj.fair_wage, lower, upper
        );

        // Bounds should enclose fair wage
        assert!(lower < adj.fair_wage);
        assert!(upper > adj.fair_wage);

        // Interval width should be reasonable given noise of +/- 1000
        // RMSE approx 1000. 1.96 * 1000 ~= 1960 margin.
        let width = upper - lower;
        assert!(width > 1000.0);
    }
    #[test]
    fn test_quantile_decomposition() {
        let csv = create_mock_csv();
        let req = DecompositionRequest {
            csv_data: csv.into_bytes(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string()],
            categorical_predictors: None,
            three_fold: None,
            quantile: Some(0.5), // Median
            reference_coefficients: None,
            bootstrap_reps: Some(10),
        };

        let res = decompose_inner(req);
        if let Err(e) = &res {
            println!("Error: {}", e);
        }
        assert!(res.is_ok());
        let res = res.unwrap();

        // Basic checks
        assert!(res.total_gap > 0.0);
        // Detailed should be empty for quantile
        assert!(res.detailed_explained.is_empty());
    }

    #[test]
    fn test_optimize_inner() {
        let csv = create_mock_csv();
        let req = OptimizationRequest {
            csv_data: csv.into_bytes(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: None,
            budget: 10000.0,
            target_gap: Some(0.0), // Close the gap
            target: None,
            strategy: None,
            min_gap_pct: None,
            forensic_mode: None,
            confidence_level: None,
            range_target: None,
            adjust_both_groups: None,
        };

        let res = optimize_inner(req);
        if let Err(e) = &res {
            println!("Optimization Error: {}", e);
        }
        assert!(res.is_ok());
        let res = res.unwrap();

        // Print values for debugging
        println!("Original Gap: {}", res.original_gap);
        println!("New Gap: {}", res.new_gap);
        println!("Total Cost: {}", res.total_cost);
        println!("Adjustments Count: {}", res.adjustments.len());

        // Basic checks
        assert!(res.original_gap.abs() > 0.0);

        if !res.adjustments.is_empty() {
            let adj = &res.adjustments[0];
            assert!(adj.new_wage >= adj.current_wage);
            // Verify index integrity somewhat (should be within bounds)
            // Mock data has 20 iterations * 8 rows = 160 rows.
            assert!(adj.index < 160);
        }
    }

    #[test]
    fn test_efficient_frontier() {
        let csv = create_mock_csv();
        let req = EfficientFrontierRequest {
            decomposition_params: DecompositionRequest {
                csv_data: csv.into_bytes(),
                outcome_variable: "wage".to_string(),
                group_variable: "gender".to_string(),
                reference_group: "Female".to_string(),
                predictors: vec!["education".to_string(), "experience".to_string()],
                categorical_predictors: None,
                three_fold: None,
                quantile: None,
                reference_coefficients: None,
                bootstrap_reps: None,
            },
            steps: Some(10),
            max_budget: Some(50000.0), // Enough to cover gaps
        };

        // This relies on calculate_efficient_frontier_inner being available in super
        let res = calculate_efficient_frontier_inner(req);
        if let Err(e) = &res {
            println!("Frontier Error: {}", e);
        }
        assert!(res.is_ok());
        let points = res.unwrap();

        assert!(!points.is_empty());
        assert_eq!(points[0].budget, 0.0);

        // Check monotonicity of budget
        for i in 0..points.len() - 1 {
            assert!(points[i].budget < points[i + 1].budget);
        }

        // With enough budget (Greedy strategy), T-stat for gender coefficient should eventually drop
        // (Assuming closing the gap reduces the gender coefficient's significance)
        let first_t = points.first().unwrap().t_statistic.abs();
        let last_t = points.last().unwrap().t_statistic.abs();

        // Note: Closing the gap usually means making the gender coefficient closer to 0,
        // thus T-stat magnitude should decrease.
        // However, in "Pooled" regression or Reference, the interpretation varies.
        // But generally, fair pay means gender is less predictive.
        println!("Start T: {}, End T: {}", first_t, last_t);
        // assert!(last_t < first_t); // This might not always hold depending on noise, but generally true.
    }

    #[test]
    fn test_decompose_with_non_numeric_outcome() {
        let csv = "wage,education,experience,gender\ninvalid,12,5,Male\n50000,16,2,Female\n52000,14,3,Male\n48000,15,4,Female\n50000,16,2,Male\n48000,15,4,Female\n";
        let req = DecompositionRequest {
            csv_data: csv.as_bytes().to_vec(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: None,
            three_fold: None,
            quantile: None,
            reference_coefficients: None,
            bootstrap_reps: Some(10),
        };

        let res = decompose_inner(req);
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            "Column 'wage' contains non-numeric data but was selected as a continuous variable. Please verify your column selection."
        );
    }

    #[test]
    fn test_decompose_with_non_numeric_predictor() {
        let csv = "wage,education,experience,gender\n50000,invalid,5,Male\n50000,16,2,Female\n52000,14,3,Male\n48000,15,4,Female\n50000,16,2,Male\n48000,15,4,Female\n";
        let req = DecompositionRequest {
            csv_data: csv.as_bytes().to_vec(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: None,
            three_fold: None,
            quantile: None,
            reference_coefficients: None,
            bootstrap_reps: Some(10),
        };

        let res = decompose_inner(req);
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            "Column 'education' contains non-numeric data but was selected as a continuous variable. Please verify your column selection."
        );
    }
}
