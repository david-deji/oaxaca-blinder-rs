use crate::math::kde::{kde, silverman_bandwidth};
use crate::math::logit::logit;
use crate::OaxacaError;
use nalgebra::{DMatrix, DVector};
use polars::prelude::*;
use serde::Serialize;

/// Holds the results of a DiNardo-Fortin-Lemieux (DFL) reweighting analysis.
#[derive(Debug, Serialize)]
pub struct DflResult {
    /// The grid points where the density was evaluated.
    pub grid: Vec<f64>,
    /// The density of the outcome for Group A (Advantaged).
    pub density_a: Vec<f64>,
    /// The density of the outcome for Group B (Disadvantaged/Reference).
    pub density_b: Vec<f64>,
    /// The counterfactual density of Group B if they had the characteristics of Group A.
    pub density_b_counterfactual: Vec<f64>,
}

/// Performs DFL Reweighting to estimate the counterfactual density of Group B.
///
/// # Arguments
///
/// * `df` - The DataFrame containing the data.
/// * `outcome` - The name of the outcome variable.
/// * `group` - The name of the group variable.
/// * `reference_group` - The value of the reference group (Group B).
/// * `predictors` - The list of predictor variables to use for the propensity score.
///
/// # Returns
///
/// A `Result` containing the `DflResult`.
pub fn run_dfl(
    df: &DataFrame,
    outcome: &str,
    group: &str,
    reference_group: &str,
    predictors: &[String],
) -> Result<DflResult, OaxacaError> {
    // 1. Prepare Data for Logit
    // Target: 1 if Group A, 0 if Group B
    let unique_groups = df.column(group)?.unique()?.sort(SortOptions {
        descending: false,
        nulls_last: false,
        ..Default::default()
    })?;
    let group_b_name = reference_group;
    let group_a_name_temp = unique_groups.str()?.get(0).unwrap_or(reference_group);
    let group_a_name = if group_a_name_temp == group_b_name {
        unique_groups.str()?.get(1).unwrap_or("")
    } else {
        group_a_name_temp
    };

    let group_series = df.column(group)?;
    let target_vec: Vec<f64> = group_series
        .str()?
        .into_iter()
        .map(|opt_s| {
            if opt_s.unwrap_or("") == group_a_name {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let y = DVector::from_vec(target_vec);

    // Prepare X matrix (predictors + intercept)
    // Note: We need to handle categorical variables here too, but for simplicity let's assume numeric or pre-processed for now.
    // Ideally we reuse the `prepare_data` logic from OaxacaBuilder, but it's private.
    // For this implementation, let's assume numeric predictors for the MVP.

    let mut x_cols = Vec::new();
    let intercept = Series::new(crate::INTERCEPT_NAME.into(), vec![1.0; df.height()]);
    x_cols.push(Column::Series(intercept));

    for pred in predictors {
        let col = df.column(pred)?;
        let dtype = col.dtype();
        if dtype == &DataType::String || dtype.to_string().starts_with("Categorical") {
            let unique_vals = col.unique()?.sort(SortOptions {
                descending: false,
                nulls_last: false,
                ..Default::default()
            })?;
            for val in unique_vals.str()?.into_iter().skip(1).flatten() {
                let dummy_name = format!("{}_{}", col.name(), val);
                let ca = col
                    .as_materialized_series()
                    .equal(val)
                    .map_err(OaxacaError::from)?;
                let mut dummy_series = ca.into_series();
                dummy_series = dummy_series.cast(&DataType::Float64)?;
                dummy_series.rename(dummy_name.as_str().into());
                x_cols.push(Column::Series(dummy_series));
            }
        } else {
            let numeric_col = col.cast(&DataType::Float64)?;
            x_cols.push(numeric_col);
        }
    }

    let x_df = DataFrame::new(x_cols).map_err(OaxacaError::from)?;
    let x_matrix = x_df.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
    let x_vec: Vec<f64> = x_matrix.iter().copied().collect();
    let x = DMatrix::from_row_slice(x_df.height(), x_df.width(), &x_vec);

    // 2. Estimate Propensity Score P(A|X) using Logit
    let logit_res = logit(&y, &x, 100, 1e-6)?;
    let probs = logit_res.predicted_probs; // P(A|X)

    // 3. Calculate Weights
    // Weight for Group B to look like Group A:
    // psi(x) = (P(A|X) / (1 - P(A|X))) * (P(B) / P(A))

    let n = df.height() as f64;
    let n_a = df
        .filter(
            &df.column(group)?
                .as_materialized_series()
                .equal(group_a_name)?,
        )?
        .height() as f64;
    let n_b = df
        .filter(
            &df.column(group)?
                .as_materialized_series()
                .equal(group_b_name)?,
        )?
        .height() as f64;

    let p_a_marginal = n_a / n;
    let p_b_marginal = n_b / n;
    let ratio_marginal = p_b_marginal / p_a_marginal;

    let mut weights_counterfactual = Vec::new();
    let outcome_series = df.column(outcome)?.f64()?;
    let mut outcome_b = Vec::new();
    let mut outcome_a = Vec::new();

    for i in 0..df.height() {
        let is_group_b = y[i] == 0.0;
        let val = outcome_series.get(i).ok_or_else(|| {
            OaxacaError::InvalidGroupVariable("Null outcome encountered in DFL".to_string())
        })?;

        if is_group_b {
            let p_x = probs[i];
            // Avoid division by zero
            let p_x = p_x.clamp(0.0001, 0.9999);

            let weight = (p_x / (1.0 - p_x)) * ratio_marginal;
            weights_counterfactual.push(weight);
            outcome_b.push(val);
        } else {
            outcome_a.push(val);
        }
    }

    // 4. Compute KDEs
    // Define Grid
    let all_outcomes: Vec<f64> = outcome_series.into_no_null_iter().collect();
    let min_val = all_outcomes.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max_val = all_outcomes
        .iter()
        .fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let range = max_val - min_val;
    let grid_size = 100;
    let step = range / (grid_size as f64);
    let grid: Vec<f64> = (0..grid_size).map(|i| min_val + i as f64 * step).collect();

    let bandwidth_a = silverman_bandwidth(&outcome_a);
    let bandwidth_b = silverman_bandwidth(&outcome_b);
    // Use average bandwidth for counterfactual to be comparable? Or B's bandwidth?
    // Usually keep bandwidth consistent or re-estimate. Let's re-estimate or use B's.
    // Using B's bandwidth is safer for the counterfactual of B.

    let density_a = kde(&outcome_a, None, &grid, bandwidth_a);
    let density_b = kde(&outcome_b, None, &grid, bandwidth_b);
    let density_b_counterfactual = kde(
        &outcome_b,
        Some(&weights_counterfactual),
        &grid,
        bandwidth_b,
    );

    Ok(DflResult {
        grid,
        density_a,
        density_b,
        density_b_counterfactual,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dfl_with_categorical() -> Result<(), OaxacaError> {
        let s_y = Series::new(
            "y".into(),
            vec![
                10.0, 15.0, 20.0, 12.0, 18.0, 22.0, 10.0, 15.0, 20.0, 12.0, 11.0, 14.0,
            ],
        );
        let s_g = Series::new(
            "g".into(),
            vec!["A", "A", "A", "A", "A", "A", "B", "B", "B", "B", "B", "B"],
        );
        let s_x_num = Series::new(
            "x_num".into(),
            vec![1.0, 2.1, 1.5, 3.2, 2.5, 1.9, 1.2, 2.2, 1.7, 3.1, 2.4, 1.8],
        );
        let s_x_cat_str = Series::new(
            "x_cat".into(),
            vec![
                "C1", "C2", "C1", "C2", "C3", "C3", "C1", "C2", "C1", "C2", "C3", "C3",
            ],
        );

        let df = DataFrame::new(vec![
            s_y.into(),
            s_g.into(),
            s_x_num.into(),
            s_x_cat_str.into(),
        ])
        .unwrap();

        let res = run_dfl(
            &df,
            "y",
            "g",
            "B",
            &["x_num".to_string(), "x_cat".to_string()],
        );
        if res.is_err() {
            println!("Error: {:?}", res.as_ref().err());
        }
        assert!(res.is_ok());

        let dfl = res.unwrap();
        assert_eq!(dfl.grid.len(), 100);

        Ok(())
    }
}
