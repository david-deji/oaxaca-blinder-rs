// oaxaca_blinder/src/builder.rs
use std::collections::HashMap;

use nalgebra::{DMatrix, DVector};
use polars::prelude::*;
use rayon::prelude::*;

use crate::decomposition::{
    detailed_decomposition, three_fold_decomposition, two_fold_decomposition, DetailedComponent,
    ReferenceCoefficients, ThreeFoldDecomposition, TwoFoldDecomposition,
};
use crate::error::OaxacaError;
use crate::estimation::{EstimationContext, Estimator, HeckmanEstimator, OlsEstimator};
use crate::formula::Formula;
use crate::inference::bootstrap_stats;
use crate::math::normalization::normalize_categorical_coefficients;
use crate::math::ols::ols;
use crate::math::rif::calculate_rif;
use crate::rng::{resample_indices, unit_rng, RepOutcome, RngPurpose, RunMetadata, DEFAULT_SEED};
use crate::types::{ComponentResult, DecompositionDetail, OaxacaResults, TwoFoldResults};

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct SinglePassResult {
    three_fold: ThreeFoldDecomposition,
    two_fold: TwoFoldDecomposition,
    detailed_explained: Vec<DetailedComponent>,
    detailed_unexplained: Vec<DetailedComponent>,
    total_gap: f64,
    residuals_a: DVector<f64>,
    residuals_b: DVector<f64>,
    xa_mean: DVector<f64>,
    xb_mean: DVector<f64>,
    beta_star: DVector<f64>,
    detailed_selection: Vec<DetailedComponent>,
}

/// Lightweight per-rep bootstrap estimates — only the scalar decomposition
/// components the SE/CI reduction actually consumes. Extracted from the full
/// [`SinglePassResult`] inside the parallel map so each rep's heavy fields
/// (`residuals_a/b`, `xa_mean`, `xb_mean`, `beta_star` — never read from
/// bootstrap reps; the final result reads those from the point estimate)
/// are freed immediately instead of accumulating R times.
///
/// This is the 0014-MERIDIAN Stage-2 memory fix (`mem-profile-report.md`
/// Finding 2): the retained bootstrap pile — not the thread count — was the
/// dominant memory term (~4 MiB/rep at 50k → ~40 GiB at the R=10 000 cap).
/// Retaining only these scalars (~KB/rep) drops peak into the portable
/// SharedArrayBuffer envelope. Field values are copied verbatim from
/// `SinglePassResult`, so the reduction output is numerically identical
/// (INV-02 within-platform byte-identity and the AC-9 mean-path parity
/// baseline both still hold).
#[derive(Clone)]
pub(crate) struct RepEstimates {
    three_fold: ThreeFoldDecomposition,
    two_fold: TwoFoldDecomposition,
    detailed_explained: Vec<DetailedComponent>,
    detailed_unexplained: Vec<DetailedComponent>,
    detailed_selection: Vec<DetailedComponent>,
}

impl RepEstimates {
    /// Copy only the consumed scalar components out of a full pass result. The
    /// borrowed `SinglePassResult` is dropped by the caller immediately after,
    /// freeing its residual/mean/beta allocations.
    fn from_pass(r: &SinglePassResult) -> Self {
        RepEstimates {
            three_fold: r.three_fold.clone(),
            two_fold: r.two_fold.clone(),
            detailed_explained: r.detailed_explained.clone(),
            detailed_unexplained: r.detailed_unexplained.clone(),
            detailed_selection: r.detailed_selection.clone(),
        }
    }
}

pub struct OaxacaBuilder {
    dataframe: DataFrame,
    outcome: String,
    predictors: Vec<String>,
    categorical_predictors: Vec<String>,
    group: String,
    reference_group: String,
    bootstrap_reps: usize,
    reference_coeffs: ReferenceCoefficients,
    normalization_vars: Vec<String>,
    weights_col: Option<String>,
    selection_outcome: Option<String>,
    selection_predictors: Vec<String>,
    /// Master seed for bootstrap resampling. `None` resolves to `DEFAULT_SEED` at `run()`.
    seed: Option<u64>,
}

pub(crate) struct GroupSplit {
    pub df_a: DataFrame,
    pub df_b: DataFrame,
    pub group_a_name: String,
    #[allow(dead_code)]
    pub group_b_name: String,
}

impl OaxacaBuilder {
    fn split_groups(&self, df: &DataFrame) -> Result<GroupSplit, OaxacaError> {
        let unique_groups = df.column(&self.group)?.unique()?.sort(SortOptions {
            descending: false,
            nulls_last: false,
            ..Default::default()
        })?;
        if unique_groups.len() < 2 {
            return Err(OaxacaError::InvalidGroupVariable(
                "Not enough groups for comparison".to_string(),
            ));
        }

        let group_b_name = self.reference_group.clone();
        let group_a_name_temp = unique_groups
            .str()?
            .get(0)
            .unwrap_or(self.reference_group.as_str())
            .to_string();
        let group_a_name = if group_a_name_temp == group_b_name {
            unique_groups.str()?.get(1).unwrap_or("").to_string()
        } else {
            group_a_name_temp
        };

        let df_a = df.filter(
            &df.column(&self.group)?
                .as_materialized_series()
                .equal(group_a_name.as_str())?,
        )?;
        let df_b = df.filter(
            &df.column(&self.group)?
                .as_materialized_series()
                .equal(group_b_name.as_str())?,
        )?;

        Ok(GroupSplit {
            df_a,
            df_b,
            group_a_name,
            group_b_name,
        })
    }
}

impl OaxacaBuilder {
    /// Creates a new `OaxacaBuilder`.
    ///
    /// # Arguments
    ///
    /// * `dataframe` - A `polars::DataFrame` containing the data for the analysis.
    /// * `outcome` - The name of the column representing the outcome variable (e.g., "wage").
    /// * `group` - The name of the column that divides the data into two groups (e.g., "gender").
    /// * `reference_group` - The value within the `group` column that identifies the reference group (the lower-outcome group, or Group B).
    pub fn new(dataframe: DataFrame, outcome: &str, group: &str, reference_group: &str) -> Self {
        Self {
            dataframe,
            outcome: outcome.to_string(),
            predictors: Vec::new(),
            categorical_predictors: Vec::new(),
            group: group.to_string(),
            reference_group: reference_group.to_string(),
            bootstrap_reps: 20,
            reference_coeffs: ReferenceCoefficients::GroupB,
            normalization_vars: Vec::new(),
            weights_col: None,
            selection_outcome: None,
            selection_predictors: Vec::new(),
            seed: None,
        }
    }

    /// Creates a new `OaxacaBuilder` using an R-style formula.
    ///
    /// # Arguments
    ///
    /// * `dataframe` - The Polars DataFrame containing the data.
    /// * `formula` - A string representing the model formula (e.g., "wage ~ education + experience + C(sector)").
    /// * `group` - The name of the column defining the two groups.
    /// * `reference_group` - The value in the `group` column representing the reference group (Group B).
    pub fn from_formula(
        dataframe: DataFrame,
        formula: &str,
        group: &str,
        reference_group: &str,
    ) -> Result<Self, OaxacaError> {
        let parsed_formula = Formula::parse(formula)?;
        Ok(Self {
            dataframe,
            outcome: parsed_formula.outcome,
            predictors: parsed_formula.predictors,
            categorical_predictors: parsed_formula.categorical_predictors,
            group: group.to_string(),
            reference_group: reference_group.to_string(),
            bootstrap_reps: 20,
            reference_coeffs: ReferenceCoefficients::GroupB,
            normalization_vars: Vec::new(),
            weights_col: None,
            selection_outcome: None,
            selection_predictors: Vec::new(),
            seed: None,
        })
    }

    /// Sets the reference coefficients for the decomposition.
    ///
    /// The default is `ReferenceCoefficients::GroupB`.
    pub fn reference_coefficients(&mut self, reference: ReferenceCoefficients) -> &mut Self {
        self.reference_coeffs = reference;
        self
    }

    /// Sets the predictor variables for the model.
    ///
    /// # Arguments
    ///
    /// * `predictors` - An iterator over strings representing the column names of the predictor variables.
    pub fn predictors<I, S>(&mut self, predictors: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.predictors = predictors.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Sets the categorical predictor variables for the model.
    ///
    /// # Arguments
    ///
    /// * `predictors` - An iterator over strings representing the column names of the categorical predictor variables.
    pub fn categorical_predictors<I, S>(&mut self, predictors: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.categorical_predictors = predictors.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Sets the number of bootstrap replications for standard error calculation.
    ///
    /// # Arguments
    ///
    /// * `reps` - The number of bootstrap samples to generate. Defaults to 100.
    pub fn bootstrap_reps(&mut self, reps: usize) -> &mut Self {
        self.bootstrap_reps = reps;
        self
    }

    /// Sets a fixed master seed for all bootstrap resampling. Reproducible-by-default:
    /// the same seed and input always produce byte-identical output within a platform.
    pub fn seed(&mut self, seed: u64) -> &mut Self {
        self.seed = Some(seed);
        self
    }

    /// Copies an `Option<u64>` master seed verbatim (`None` stays `None`, resolving to
    /// `DEFAULT_SEED` at `run()`). Used to forward a seed to an inner builder — e.g.
    /// `decompose_quantile` — so `.seed(X)` reproduces on the RIF quantile path (CV-1).
    pub fn seed_opt(&mut self, seed: Option<u64>) -> &mut Self {
        self.seed = seed;
        self
    }

    /// Draws one master seed from OS entropy and records it in `RunMetadata`, so the
    /// run stays reproducible after the fact via `.seed(recorded_value)`. Native-only:
    /// `oaxaca_blinder` has no direct `getrandom` dependency, so entropy seeding is a
    /// CLI/native affordance; the wasm path uses `DEFAULT_SEED` or an explicit seed (D1).
    #[cfg(not(target_family = "wasm"))]
    pub fn seed_from_entropy(&mut self) -> &mut Self {
        self.seed = Some(crate::rng::draw_entropy_seed());
        self
    }

    /// Sets the categorical variables for which to apply coefficient normalization.
    ///
    /// # Arguments
    ///
    /// * `vars` - An iterator over strings representing the column names of the categorical variables to normalize.
    pub fn normalize<I, S>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.normalization_vars = vars.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Sets the column name for sample weights.
    ///
    /// # Arguments
    ///
    /// * `weights` - The name of the column containing sample weights.
    pub fn weights(&mut self, weights: &str) -> &mut Self {
        self.weights_col = Some(weights.to_string());
        self
    }

    /// Configures the Heckman selection model.
    ///
    /// # Arguments
    ///
    /// * `outcome` - The binary selection variable (e.g., "employed").
    /// * `predictors` - The predictors for the selection equation (should include exclusion restriction).
    pub fn heckman_selection<I, S>(&mut self, outcome: &str, predictors: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.selection_outcome = Some(outcome.to_string());
        self.selection_predictors = predictors.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Exposes the internal data matrices for advanced usage (e.g., optimization).
    /// This method prepares the data (creating dummies, etc.) and returns the matrices for Group A and Group B.
    ///
    /// Returns `(X_A, y_A, X_B, y_B, predictor_names)`. Per `split_groups`, which sets
    /// `group_b_name = reference_group`, the binding is:
    ///   * **A = the NON-reference group** (every group value that is not `reference_group`),
    ///   * **B = the reference group** (the `reference_group` value passed to `new`/`from_formula`).
    ///
    /// A consumer that solves a fair-wage standard from the advantaged/reference group must
    /// therefore bind the *third* returned matrix (`X_B`), not the first. See the engine crate's
    /// `ab_binding_regression_test` for the guardrail that locks this convention against the
    /// recurring "A = reference" mistake.
    #[allow(clippy::type_complexity)]
    pub fn get_data_matrices(
        &self,
    ) -> Result<
        (
            DMatrix<f64>,
            DVector<f64>,
            DMatrix<f64>,
            DVector<f64>,
            Vec<String>,
        ),
        OaxacaError,
    > {
        let df_dirty = self.dataframe.clone();
        let mut df = self.clean_dataframe(&df_dirty)?;

        let mut all_dummy_names = Vec::new();
        // let mut category_counts = std::collections::HashMap::new();

        if !self.categorical_predictors.is_empty() {
            for cat_pred in &self.categorical_predictors {
                let series = df.column(cat_pred)?;
                let (dummies, _, _) =
                    self.create_dummies_manual(series.as_materialized_series())?;
                // category_counts.insert(cat_pred.clone(), m);
                for s in dummies.get_columns() {
                    all_dummy_names.push(s.name().to_string());
                }
                df = df.hstack(dummies.get_columns())?;
            }
        }

        let groups = self.split_groups(&df)?;
        let df_a = groups.df_a;
        let df_b = groups.df_b;

        let (x_a, y_a, _, predictor_names) = self.prepare_data(&df_a, &all_dummy_names, &[])?;
        let (x_b, y_b, _, _) = self.prepare_data(&df_b, &all_dummy_names, &[])?;

        Ok((x_a, y_a, x_b, y_b, predictor_names))
    }

    #[allow(clippy::type_complexity)]
    fn prepare_data(
        &self,
        df: &DataFrame,
        all_dummy_names: &[String],
        extra_predictors: &[String],
    ) -> Result<
        (
            DMatrix<f64>,
            DVector<f64>,
            Option<DVector<f64>>,
            Vec<String>,
        ),
        OaxacaError,
    > {
        let y_series = df.column(&self.outcome)?.f64()?;
        // Safe to unwrap because we ran clean_dataframe
        let y_vec: Vec<f64> = y_series
            .into_iter()
            .map(|opt| {
                opt.ok_or_else(|| {
                    OaxacaError::PolarsError(PolarsError::ComputeError(
                        "Null values found in outcome after cleaning".into(),
                    ))
                })
            })
            .collect::<Result<Vec<f64>, _>>()?;
        let y = DVector::from_vec(y_vec);

        let mut current_predictors = self.predictors.clone();
        current_predictors.extend_from_slice(extra_predictors);

        let mut final_predictors: Vec<String> = vec!["__ob_intercept__".to_string()];
        final_predictors.extend_from_slice(&current_predictors);
        final_predictors.extend_from_slice(all_dummy_names);

        let mut x_df = df.select(&current_predictors)?;
        let intercept_series = Series::new("__ob_intercept__".into(), vec![1.0; df.height()]);
        x_df.with_column(intercept_series)?;

        for name in all_dummy_names {
            if df
                .get_column_names()
                .iter()
                .any(|s| s.as_str() == name.as_str())
            {
                x_df.with_column(df.column(name)?.clone())?;
            } else {
                let zero_series = Series::new(name.into(), vec![0.0; df.height()]);
                x_df.with_column(zero_series)?;
            }
        }

        let x_df_selected = x_df.select(&final_predictors)?;
        let x_matrix = x_df_selected.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
        let x_vec: Vec<f64> = x_matrix.iter().copied().collect();
        let final_names = x_df_selected
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        let weights = if let Some(w_col) = &self.weights_col {
            let w_series = df.column(w_col)?.f64()?;
            let w_vec: Vec<f64> = w_series
                .into_iter()
                .map(|opt| {
                    opt.ok_or_else(|| {
                        OaxacaError::PolarsError(PolarsError::ComputeError(
                            "Null weights found after cleaning".into(),
                        ))
                    })
                })
                .collect::<Result<Vec<f64>, _>>()?;
            Some(DVector::from_vec(w_vec))
        } else {
            None
        };

        Ok((
            DMatrix::from_row_slice(x_df_selected.height(), x_df_selected.width(), &x_vec),
            y,
            weights,
            final_names,
        ))
    }

    fn create_dummies_manual(
        &self,
        series: &Series,
    ) -> Result<(DataFrame, usize, String), OaxacaError> {
        let unique_vals = series.unique()?.sort(SortOptions {
            descending: false,
            nulls_last: false,
            ..Default::default()
        })?;
        let m = unique_vals.len();
        let mut dummy_vars: Vec<Series> = Vec::new();

        let reference_val = if let Some(s) = unique_vals.str()?.get(0) {
            s
        } else {
            return Err(OaxacaError::InvalidGroupVariable(format!(
                "Could not get reference category for {}",
                series.name()
            )));
        };
        let reference_name = format!("{}_{}", series.name(), reference_val);

        for val in unique_vals.str()?.into_iter().flatten().skip(1) {
            // Skip the first category as the reference
            let dummy_name = format!("{}_{}", series.name(), val);
            let ca = series.equal(val)?;
            let mut dummy_series = ca.into_series();
            dummy_series = dummy_series.cast(&DataType::Float64)?;
            dummy_series.rename(dummy_name.as_str().into());
            dummy_vars.push(dummy_series);
        }

        Ok((
            DataFrame::new(dummy_vars.into_iter().map(Column::Series).collect())
                .map_err(OaxacaError::from)?,
            m,
            reference_name,
        ))
    }

    fn run_single_pass(
        &self,
        df: &DataFrame,
        all_dummy_names: &[String],
        category_counts: &std::collections::HashMap<String, usize>,
        base_categories: &std::collections::HashMap<String, String>,
    ) -> Result<SinglePassResult, OaxacaError> {
        let groups = self.split_groups(df)?;
        let df_a = groups.df_a;
        let df_b = groups.df_b;
        let group_a_name = groups.group_a_name;
        if df_a.height() == 0 || df_b.height() == 0 {
            return Err(OaxacaError::InvalidGroupVariable(
                "One group has no data".to_string(),
            ));
        }

        let (x_a, y_a, w_a, predictor_names) = self.prepare_data(&df_a, all_dummy_names, &[])?;
        let (x_b, y_b, w_b, _) = self.prepare_data(&df_b, all_dummy_names, &[])?;

        let ctx = EstimationContext {
            df_a: &df_a,
            df_b: &df_b,
            x_a: &x_a,
            y_a: &y_a,
            w_a: &w_a,
            x_b: &x_b,
            y_b: &y_b,
            w_b: &w_b,
            predictor_names: &predictor_names,
            category_counts,
        };

        let estimator: Box<dyn Estimator> = if let Some(sel_outcome) = &self.selection_outcome {
            Box::new(HeckmanEstimator {
                selection_outcome: sel_outcome.clone(),
                selection_predictors: self.selection_predictors.clone(),
            })
        } else {
            Box::new(OlsEstimator {
                normalization_vars: self.normalization_vars.clone(),
            })
        };

        let result = estimator.estimate(&ctx)?;

        // ... Calculate beta_star and decompositions ...
        let beta_a = &result.beta_a;
        let beta_b = &result.beta_b;
        let xa_mean = result.xa_mean;
        let xb_mean = result.xb_mean;
        let final_predictor_names = result.predictor_names;
        let residuals_a = result.residuals_a;
        let residuals_b = result.residuals_b;
        let base_coeffs_a = result.base_coeffs_a;
        let base_coeffs_b = result.base_coeffs_b;

        // Detailed Selection Decomposition (if Heckman)
        let mut detailed_selection_components = Vec::new();
        if let (
            Some(gamma_a),
            Some(gamma_b),
            Some(z_mean_a),
            Some(z_mean_b),
            Some(delta_a),
            Some(delta_b),
            Some(_sel_names),
        ) = (
            &result.selection_coeffs_a,
            &result.selection_coeffs_b,
            &result.selection_means_a,
            &result.selection_means_b,
            result.imr_delta_a,
            result.imr_delta_b,
            &result.selection_names,
        ) {
            // Identify theta (IMR coefficient). It's the last element of beta.
            // But which reference group?
            // Explained selection = theta_ref * (lambda_a - lambda_b)
            // Approx = theta_ref * delta_ref * gamma_ref * (Z_A - Z_B)

            let (theta_ref, delta_ref, gamma_ref) = match self.reference_coeffs {
                ReferenceCoefficients::GroupA => (beta_a[beta_a.len() - 1], delta_a, gamma_a),
                ReferenceCoefficients::GroupB => (beta_b[beta_b.len() - 1], delta_b, gamma_b),
                _ => (beta_b[beta_b.len() - 1], delta_b, gamma_b), // Default/Simplified
            };

            // Selection names usually include intercept at 0.
            // gamma and z_mean should align with sel_names.
            // However, heckman_two_step probit includes intercept.
            // Check self.selection_predictors.
            // If Estimator logic added intercept, we need to match indices.
            // HeckmanEstimator::prepare_selection_data adds intercept at col 0.

            // We iterate through selection predictors.
            // We assume gamma, z_mean, and sel_names are aligned including intercept.
            // But we might want to skip intercept for "contribution"? Or include it?
            // Usually we show variables.

            // Reconstruct selection variable names: "intercept" + sel_predictors
            let mut full_sel_names = vec!["__ob_intercept__".to_string()];
            full_sel_names.extend(self.selection_predictors.clone());

            // Check dimensions
            if gamma_ref.len() == full_sel_names.len() && z_mean_a.len() == full_sel_names.len() {
                for (i, name) in full_sel_names.iter().enumerate() {
                    let diff_z = z_mean_a[i] - z_mean_b[i];
                    let contribution = theta_ref * delta_ref * gamma_ref[i] * diff_z;
                    detailed_selection_components.push(DetailedComponent {
                        variable_name: name.clone(),
                        contribution,
                    });
                }
            }
        }

        let mut base_coeffs_star = std::collections::HashMap::new();
        let beta_star_owned: DVector<f64>;
        let beta_star: &DVector<f64> = match self.reference_coeffs {
            ReferenceCoefficients::GroupA => {
                base_coeffs_star = base_coeffs_a.clone();
                beta_a
            }
            ReferenceCoefficients::GroupB => {
                base_coeffs_star = base_coeffs_b.clone();
                beta_b
            }
            ReferenceCoefficients::Pooled | ReferenceCoefficients::Neumark => {
                let mut df_pooled = df_a.vstack(&df_b)?;
                let group_indicator = Series::new(
                    "__ob_group_indicator__".into(),
                    df_pooled
                        .column(&self.group)?
                        .as_materialized_series()
                        .equal(group_a_name.as_str())?
                        .into_series()
                        .cast(&DataType::Float64)?,
                );
                df_pooled.with_column(group_indicator)?;

                let (x_pooled, y_pooled, w_pooled, pooled_predictor_names) = self.prepare_data(
                    &df_pooled,
                    all_dummy_names,
                    &["__ob_group_indicator__".to_string()],
                )?;

                let mut ols_pooled = ols(&y_pooled, &x_pooled, w_pooled.as_ref())?;

                if !self.normalization_vars.is_empty() {
                    let n_a = df_a.height() as f64;
                    let n_b = df_b.height() as f64;
                    let x_pool_mean = (xa_mean.clone() * n_a + xb_mean.clone() * n_b) / (n_a + n_b);
                    base_coeffs_star = normalize_categorical_coefficients(
                        &mut ols_pooled,
                        &pooled_predictor_names,
                        &self.normalization_vars,
                        &x_pool_mean,
                        category_counts,
                    );
                }
                let group_indicator_idx = pooled_predictor_names
                    .iter()
                    .position(|r| r == "__ob_group_indicator__")
                    .ok_or_else(|| {
                        OaxacaError::NalgebraError(
                            "group_indicator not found in pooled model predictors".to_string(),
                        )
                    })?;
                beta_star_owned = ols_pooled.coefficients.remove_row(group_indicator_idx);
                &beta_star_owned
            }
            ReferenceCoefficients::Weighted | ReferenceCoefficients::Cotton => {
                let n_a = if let Some(w) = &w_a {
                    w.sum()
                } else {
                    df_a.height() as f64
                };
                let n_b = if let Some(w) = &w_b {
                    w.sum()
                } else {
                    df_b.height() as f64
                };
                let total_n = n_a + n_b;
                if total_n == 0.0 {
                    return Err(OaxacaError::InvalidGroupVariable(
                        "No data in groups for weighted coefficients.".to_string(),
                    ));
                }
                let weight_a = n_a / total_n;
                let weight_b = 1.0 - weight_a;
                if !self.normalization_vars.is_empty() {
                    for var in &self.normalization_vars {
                        let coeff_a = base_coeffs_a.get(var).unwrap_or(&0.0);
                        let coeff_b = base_coeffs_b.get(var).unwrap_or(&0.0);
                        base_coeffs_star
                            .insert(var.clone(), coeff_a * weight_a + coeff_b * weight_b);
                    }
                }
                beta_star_owned = beta_a * weight_a + beta_b * weight_b;
                &beta_star_owned
            }
        };

        let three_fold = three_fold_decomposition(&xa_mean, &xb_mean, beta_a, beta_b);
        let mut two_fold = two_fold_decomposition(&xa_mean, &xb_mean, beta_a, beta_b, beta_star);
        let (mut detailed_explained, mut detailed_unexplained) = detailed_decomposition(
            &xa_mean,
            &xb_mean,
            beta_a,
            beta_b,
            beta_star,
            &final_predictor_names,
        );

        if !self.normalization_vars.is_empty() && self.selection_outcome.is_none() {
            for var in &self.normalization_vars {
                let base_dummy_name = if let Some(name) = base_categories.get(var) {
                    name
                } else {
                    continue;
                };

                let dummy_indices: Vec<usize> = final_predictor_names
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| name.starts_with(&format!("{}_", var)))
                    .map(|(i, _)| i)
                    .collect();

                let xa_mean_base = 1.0 - dummy_indices.iter().map(|&i| xa_mean[i]).sum::<f64>();
                let xb_mean_base = 1.0 - dummy_indices.iter().map(|&i| xb_mean[i]).sum::<f64>();

                let beta_a_base = base_coeffs_a.get(var).cloned().unwrap_or(0.0);
                let beta_b_base = base_coeffs_b.get(var).cloned().unwrap_or(0.0);
                let beta_star_base = base_coeffs_star.get(var).cloned().unwrap_or(0.0);

                let contribution_unexplained = xa_mean_base * (beta_a_base - beta_star_base)
                    + xb_mean_base * (beta_star_base - beta_b_base);

                let contribution_explained = (xa_mean_base - xb_mean_base) * beta_star_base;

                detailed_unexplained.push(DetailedComponent {
                    variable_name: base_dummy_name.clone(),
                    contribution: contribution_unexplained,
                });

                detailed_explained.push(DetailedComponent {
                    variable_name: base_dummy_name.clone(),
                    contribution: contribution_explained,
                });

                two_fold.explained += contribution_explained;
                two_fold.unexplained += contribution_unexplained;
            }
        }

        let total_gap = if let Some(w) = &w_a {
            y_a.dot(w) / w.sum()
        } else {
            y_a.mean()
        } - if let Some(w) = &w_b {
            y_b.dot(w) / w.sum()
        } else {
            y_b.mean()
        };

        Ok(SinglePassResult {
            three_fold,
            two_fold,
            detailed_explained,
            detailed_unexplained,
            total_gap,
            residuals_a,
            residuals_b,
            xa_mean: xa_mean.clone(),
            xb_mean: xb_mean.clone(),
            beta_star: beta_star.clone(),
            detailed_selection: detailed_selection_components,
        })
    }

    /// Performs a RIF-Regression decomposition for a specific quantile.
    ///
    /// This method transforms the outcome variable into its Recentered Influence Function (RIF)
    /// for each group separately and then performs the standard Oaxaca-Blinder decomposition
    /// on the transformed variable. This allows for decomposing the difference in quantiles
    /// (e.g., the 90th percentile gap) into explained and unexplained components.
    ///
    /// # Arguments
    ///
    /// * `quantile` - The target quantile (e.g., 0.9 for the 90th percentile).
    pub fn decompose_quantile(&self, quantile: f64) -> Result<OaxacaResults, OaxacaError> {
        // RIF-regression quantile decomposition (Firpo-Fortin-Lemieux 2009), 0014-MERIDIAN
        // rulings a-1 / 4.
        //
        // Ruling 4 (`fixed_rif: false`): the RIF transform is recomputed INSIDE each bootstrap
        // replicate — every resample re-estimates its own quantile q_τ and density f_Y(q_τ)
        // before the RIF-OLS — so the bootstrap CIs reflect the full sampling variability of
        // the quantile estimate (the legacy compute-once-on-the-full-sample form understated
        // them). The POINT estimate still uses the full-sample RIF.
        //
        // Structure mirrors `run()` (shared dummy prep → `run_single_pass` → the
        // bounded-parallel chunked bootstrap → `aggregate_results`), with one addition: each
        // resampled group's outcome is replaced by its per-replicate RIF before the pass.
        // Reusing run()'s chunked loop keeps the stage-2 WASM-OOM bound (peak = H_res +
        // chunk·Sc) AND the fixed rep-index reduction order (so the quantile path is
        // byte-identical across thread counts too, INV-02), and reusing the same
        // `unit_rng(master, Bootstrap, rep*2 [+1])` streams keeps determinism.
        let df_dirty = self.dataframe.clone();
        let mut df = self.clean_dataframe(&df_dirty)?;

        // Categorical one-hot dummies — same construction as run() so run_single_pass sees the
        // one-hot columns and the Gardeazabal-Ugidos normalization applies unchanged (L6).
        let mut all_dummy_names = Vec::new();
        let mut category_counts = std::collections::HashMap::new();
        let mut base_categories = std::collections::HashMap::new();
        if !self.categorical_predictors.is_empty() {
            for cat_pred in &self.categorical_predictors {
                let series = df.column(cat_pred)?;
                let (dummies, m, base_name) =
                    self.create_dummies_manual(series.as_materialized_series())?;
                category_counts.insert(cat_pred.clone(), m);
                base_categories.insert(cat_pred.clone(), base_name);
                for s in dummies.get_columns() {
                    all_dummy_names.push(s.name().to_string());
                }
                df = df.hstack(dummies.get_columns())?;
            }
        }

        let groups = self.split_groups(&df)?;
        let df_a_global = groups.df_a;
        let df_b_global = groups.df_b;

        // Point estimate: RIF computed once on the FULL sample of each group.
        let point_df = self
            .rif_replace_outcome(&df_a_global, quantile)?
            .vstack(&self.rif_replace_outcome(&df_b_global, quantile)?)?;
        let point_estimates = self.run_single_pass(
            &point_df,
            &all_dummy_names,
            &category_counts,
            &base_categories,
        )?;

        let master = self.seed.unwrap_or(DEFAULT_SEED);

        // Bounded-parallel chunked bootstrap (stage-2 invariant — NEVER an unbounded
        // into_par_iter().collect(), which reintroduces the WASM-OOM). Each rep: resample the
        // ORIGINAL groups → recompute the per-group RIF on the resample (ruling 4) → RIF-OLS
        // decomposition pass. Same streams + index-ordered chunk consumption as run().
        let chunk = rayon::current_num_threads().max(1);
        let mut bootstrap_results: Vec<RepEstimates> = Vec::with_capacity(self.bootstrap_reps);
        let mut discarded = 0usize;
        let mut start = 0usize;
        while start < self.bootstrap_reps {
            let end = (start + chunk).min(self.bootstrap_reps);
            let chunk_out: Vec<RepOutcome<RepEstimates>> = (start..end)
                .into_par_iter()
                .map(|rep| {
                    let rep = rep as u64;
                    let mut rng_a = unit_rng(master, RngPurpose::Bootstrap, rep * 2);
                    let mut rng_b = unit_rng(master, RngPurpose::Bootstrap, rep * 2 + 1);

                    let result = (|| -> Result<SinglePassResult, OaxacaError> {
                        let sample_a = df_a_global
                            .take(&resample_indices(&mut rng_a, df_a_global.height()))?;
                        let sample_b = df_b_global
                            .take(&resample_indices(&mut rng_b, df_b_global.height()))?;
                        // Ruling 4: recompute the RIF per replicate, per group.
                        let sample_a = self.rif_replace_outcome(&sample_a, quantile)?;
                        let sample_b = self.rif_replace_outcome(&sample_b, quantile)?;
                        let sample_df = sample_a.vstack(&sample_b)?;
                        self.run_single_pass(
                            &sample_df,
                            &all_dummy_names,
                            &category_counts,
                            &base_categories,
                        )
                    })();

                    match result {
                        Ok(r) => RepOutcome::Ok(RepEstimates::from_pass(&r)),
                        Err(_) => RepOutcome::Failed,
                    }
                })
                .collect();

            for o in chunk_out {
                match o {
                    RepOutcome::Ok(r) => bootstrap_results.push(r),
                    RepOutcome::Failed => discarded += 1,
                }
            }
            start = end;
        }

        let successful_bootstraps = bootstrap_results.len();
        if successful_bootstraps < self.bootstrap_reps {
            eprintln!(
                "Warning: {} out of {} bootstrap replications failed and were discarded. The analysis is based on {} successful replications.",
                self.bootstrap_reps - successful_bootstraps, self.bootstrap_reps, successful_bootstraps
            );
        }

        // fixed_rif = Some(false): RIF recomputed per replicate (ruling 4).
        Ok(self.aggregate_results(
            &point_estimates,
            &bootstrap_results,
            master,
            successful_bootstraps,
            discarded,
            df_a_global.height(),
            df_b_global.height(),
            Some(false),
        ))
    }

    /// Replace the outcome column of `g` with its Recentered Influence Function at
    /// `quantile` (RIF-regression transform, FFL 2009). Called once per group for the point
    /// estimate and once per group PER bootstrap replicate (ruling 4). `clone()` is an
    /// Arc/COW refcount bump (stage-2 Finding 1), so it adds no meaningful per-rep memory.
    fn rif_replace_outcome(&self, g: &DataFrame, quantile: f64) -> Result<DataFrame, OaxacaError> {
        let rif = calculate_rif(g.column(&self.outcome)?.as_materialized_series(), quantile)
            .map_err(OaxacaError::PolarsError)?;
        let mut out = g.clone();
        out.with_column(rif)?;
        Ok(out)
    }

    /// Helper to drop rows with missing values in relevant columns.
    fn clean_dataframe(&self, df: &DataFrame) -> Result<DataFrame, OaxacaError> {
        let mut cols = vec![self.outcome.clone(), self.group.clone()];
        cols.extend(self.predictors.clone());
        cols.extend(self.categorical_predictors.clone());

        if let Some(w) = &self.weights_col {
            cols.push(w.to_string());
        }
        if let Some(sel_out) = &self.selection_outcome {
            cols.push(sel_out.to_string());
        }
        cols.extend(self.selection_predictors.clone());

        // Ensure all columns exist before trying to drop nulls on them
        for c in &cols {
            if df.column(c).is_err() {
                return Err(OaxacaError::ColumnNotFound(c.clone()));
            }
        }

        let clean_df = df
            .drop_nulls(Some(&cols))
            .map_err(OaxacaError::PolarsError)?;
        Ok(clean_df)
    }

    /// Executes the Oaxaca-Blinder decomposition.
    pub fn run(&self) -> Result<OaxacaResults, OaxacaError> {
        let df_dirty = self.dataframe.clone();
        let mut df = self.clean_dataframe(&df_dirty)?;

        let mut all_dummy_names = Vec::new();
        let mut category_counts = std::collections::HashMap::new();
        let mut base_categories = std::collections::HashMap::new();
        if !self.categorical_predictors.is_empty() {
            for cat_pred in &self.categorical_predictors {
                let series = df.column(cat_pred)?;
                let (dummies, m, base_name) =
                    self.create_dummies_manual(series.as_materialized_series())?;
                category_counts.insert(cat_pred.clone(), m);
                base_categories.insert(cat_pred.clone(), base_name);
                for s in dummies.get_columns() {
                    all_dummy_names.push(s.name().to_string());
                }
                df = df.hstack(dummies.get_columns())?;
            }
        }

        // Checkpoint A (D1, mem-profile only): resident bytes immediately after
        // the categorical dummy hstack — a sub-component of H_res. Additive,
        // side-effect-only; does not touch the RNG/resampling logic below.
        #[cfg(feature = "mem-profile")]
        crate::mem_profile::checkpoint_a();

        let groups = self.split_groups(&df)?;

        let point_estimates =
            self.run_single_pass(&df, &all_dummy_names, &category_counts, &base_categories)?;

        let df_a_global = groups.df_a;
        let df_b_global = groups.df_b;

        // Resolve the master seed once. `None` -> DEFAULT_SEED (reproducible-by-default).
        let master = self.seed.unwrap_or(DEFAULT_SEED);

        // Checkpoint B (D1, mem-profile only): resident bytes immediately before
        // the bootstrap into_par_iter() loop — the H_res(n) measurement point.
        // Additive, side-effect-only; does not touch the RNG/resampling logic
        // below (unit_rng/resample_indices/take/vstack are untouched).
        #[cfg(feature = "mem-profile")]
        crate::mem_profile::checkpoint_b();

        // Deterministic bootstrap. Each rep's resample and success/failure is a pure
        // function of (master, rep, base frames): group A draws from stream `rep*2`,
        // group B from `rep*2+1`, both via owned index-vector resampling (`take`) — no
        // dependence on execution order. The indexed `into_par_iter().map` collects in
        // order, then a sequential partition records the discard count deterministically
        // (D5, INV-02). `take` borrows the shared base frames; no per-rep clone.
        // Bounded-parallel bootstrap (0014-MERIDIAN memory fix). A single
        // `(0..reps).into_par_iter().map(..).collect()` lets rayon keep many
        // reps' ~Sc-sized `run_single_pass` working sets in flight at once, so
        // peak memory scales with the rep count — harmless natively (the
        // allocator reclaims it) but fatal in wasm, whose linear memory only
        // ever grows. Processing reps in index-ordered chunks of the pool size
        // caps the concurrently-live working sets at `chunk`, so peak =
        // H_res + chunk·Sc (the D2 shared-memory budget model), while each
        // chunk still runs fully in parallel. Chunks and within-chunk results
        // are consumed in strict rep-index order, so the estimates reach the
        // SE/CI reduction in a fixed order independent of thread count →
        // deterministic floating-point reduction (INV-02 byte-identity across
        // thread counts) and the AC-9 parity baseline both hold.
        let chunk = rayon::current_num_threads().max(1);
        let mut bootstrap_results: Vec<RepEstimates> = Vec::with_capacity(self.bootstrap_reps);
        let mut discarded = 0usize; // sequential fold over ordered chunks — deterministic
        let mut start = 0usize;
        while start < self.bootstrap_reps {
            let end = (start + chunk).min(self.bootstrap_reps);
            let chunk_out: Vec<RepOutcome<RepEstimates>> = (start..end)
                .into_par_iter()
                .map(|rep| {
                    let rep = rep as u64;
                    let mut rng_a = unit_rng(master, RngPurpose::Bootstrap, rep * 2);
                    let mut rng_b = unit_rng(master, RngPurpose::Bootstrap, rep * 2 + 1);

                    let result = (|| -> Result<SinglePassResult, OaxacaError> {
                        let sample_a = df_a_global
                            .take(&resample_indices(&mut rng_a, df_a_global.height()))?;
                        let sample_b = df_b_global
                            .take(&resample_indices(&mut rng_b, df_b_global.height()))?;
                        let sample_df = sample_a.vstack(&sample_b)?;
                        self.run_single_pass(
                            &sample_df,
                            &all_dummy_names,
                            &category_counts,
                            &base_categories,
                        )
                    })();

                    match result {
                        // Extract the small scalar estimates, then let the heavy
                        // SinglePassResult (`r`) drop at the end of this arm so
                        // its residual/mean/beta allocations free before the next
                        // chunk — bounding in-flight memory to `chunk` working sets.
                        Ok(r) => RepOutcome::Ok(RepEstimates::from_pass(&r)),
                        Err(_) => RepOutcome::Failed,
                    }
                })
                .collect();

            for o in chunk_out {
                match o {
                    RepOutcome::Ok(r) => bootstrap_results.push(r),
                    RepOutcome::Failed => discarded += 1,
                }
            }
            start = end;
        }

        let successful_bootstraps = bootstrap_results.len();
        if successful_bootstraps < self.bootstrap_reps {
            eprintln!(
                "Warning: {} out of {} bootstrap replications failed and were discarded. The analysis is based on {} successful replications.",
                self.bootstrap_reps - successful_bootstraps, self.bootstrap_reps, successful_bootstraps
            );
        }

        // fixed_rif = None: mean/OLS path (no quantile RIF transform). The aggregation is
        // shared verbatim with decompose_quantile() via aggregate_results() — one path, no
        // divergent copy (RK2). The mean path is byte-identical to before the extraction
        // (AC-9 / AC-6 baselines) and INV-02 deterministic across thread counts.
        Ok(self.aggregate_results(
            &point_estimates,
            &bootstrap_results,
            master,
            successful_bootstraps,
            discarded,
            df_a_global.height(),
            df_b_global.height(),
            None,
        ))
    }

    /// Shared bootstrap-aggregation: turns the point-estimate pass plus the collected per-rep
    /// scalar estimates into the final `OaxacaResults` (per-component SE / percentile-CI /
    /// p-value / t-stat). Used verbatim by `run()` (mean/OLS, `fixed_rif` = None) and
    /// `decompose_quantile()` (RIF, `fixed_rif` = Some(false)) — one path, no divergent copy
    /// (RK2). The computation and iteration order are identical for both, so the mean path
    /// stays byte-identical and INV-02 within-platform determinism holds.
    #[allow(clippy::too_many_arguments)]
    fn aggregate_results(
        &self,
        point_estimates: &SinglePassResult,
        bootstrap_results: &[RepEstimates],
        master: u64,
        successful_bootstraps: usize,
        discarded: usize,
        n_a: usize,
        n_b: usize,
        fixed_rif: Option<bool>,
    ) -> OaxacaResults {
        let process_component = |name: &str, point: f64, estimates: Vec<f64>| {
            let (std_err, p_value, (ci_lower, ci_upper)) = bootstrap_stats(&estimates, point);
            let t_stat = if std_err.abs() > 1e-9 {
                point / std_err
            } else {
                0.0
            };
            ComponentResult {
                name: name.to_string(),
                estimate: point,
                std_err,
                t_stat,
                p_value,
                ci_lower,
                ci_upper,
            }
        };

        let two_fold_agg = vec![
            process_component(
                "explained",
                point_estimates.two_fold.explained,
                bootstrap_results
                    .iter()
                    .map(|r| r.two_fold.explained)
                    .collect(),
            ),
            process_component(
                "unexplained",
                point_estimates.two_fold.unexplained,
                bootstrap_results
                    .iter()
                    .map(|r| r.two_fold.unexplained)
                    .collect(),
            ),
        ];
        let three_fold_agg = vec![
            process_component(
                "endowments",
                point_estimates.three_fold.endowments,
                bootstrap_results
                    .iter()
                    .map(|r| r.three_fold.endowments)
                    .collect(),
            ),
            process_component(
                "coefficients",
                point_estimates.three_fold.coefficients,
                bootstrap_results
                    .iter()
                    .map(|r| r.three_fold.coefficients)
                    .collect(),
            ),
            process_component(
                "interaction",
                point_estimates.three_fold.interaction,
                bootstrap_results
                    .iter()
                    .map(|r| r.three_fold.interaction)
                    .collect(),
            ),
        ];

        let detailed_explained = self.process_detailed_components(
            &point_estimates.detailed_explained,
            bootstrap_results,
            |r| &r.detailed_explained,
            &process_component,
        );
        let detailed_unexplained = self.process_detailed_components(
            &point_estimates.detailed_unexplained,
            bootstrap_results,
            |r| &r.detailed_unexplained,
            &process_component,
        );

        let detailed_selection = self.process_detailed_components(
            &point_estimates.detailed_selection,
            bootstrap_results,
            |r| &r.detailed_selection,
            &process_component,
        );

        let mut run_metadata = RunMetadata::new(
            master,
            self.bootstrap_reps,
            successful_bootstraps,
            discarded,
        );
        if let Some(fr) = fixed_rif {
            run_metadata = run_metadata.with_fixed_rif(fr);
        }

        OaxacaResults {
            total_gap: point_estimates.total_gap,
            two_fold: TwoFoldResults {
                aggregate: two_fold_agg,
                detailed_explained,
                detailed_unexplained,
                detailed_selection,
            },
            three_fold: DecompositionDetail {
                aggregate: three_fold_agg,
                detailed: Vec::new(),
            },
            n_a,
            n_b,
            residuals: point_estimates.residuals_b.iter().copied().collect(),
            xa_mean: point_estimates.xa_mean.clone(),
            xb_mean: point_estimates.xb_mean.clone(),
            beta_star: point_estimates.beta_star.clone(),
            run_metadata,
        }
    }

    fn process_detailed_components<'a, F>(
        &self,
        point_components: &[DetailedComponent],
        bootstrap_results: &'a [RepEstimates],
        extract_fn: F,
        process_component: &dyn Fn(&str, f64, Vec<f64>) -> ComponentResult,
    ) -> Vec<ComponentResult>
    where
        F: Fn(&'a RepEstimates) -> &'a Vec<DetailedComponent> + Sync,
    {
        let num_bootstraps = bootstrap_results.len();
        let mut bootstrap_map: HashMap<String, Vec<f64>> = HashMap::new();
        for r in bootstrap_results.iter() {
            for comp in extract_fn(r) {
                bootstrap_map
                    .entry(comp.variable_name.clone())
                    .or_default()
                    .push(comp.contribution);
            }
        }

        point_components
            .iter()
            .map(|comp| {
                let mut estimates = bootstrap_map
                    .get(&comp.variable_name)
                    .cloned()
                    .unwrap_or_default();

                // If a component was missing from some bootstrap samples, pad with zeros
                if estimates.len() < num_bootstraps {
                    estimates.resize(num_bootstraps, 0.0);
                }

                process_component(&comp.variable_name, comp.contribution, estimates)
            })
            .collect()
    }
}

impl OaxacaResults {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_detailed_components_with_missing_estimates() {
        let df = df!(
            "y" => &[1.0, 2.0],
            "x" => &[1.0, 2.0],
            "group" => &["A", "B"]
        )
        .unwrap();
        let builder = OaxacaBuilder::new(df, "y", "group", "B");

        let point_components = vec![
            DetailedComponent {
                variable_name: "var1".to_string(),
                contribution: 10.0,
            },
            DetailedComponent {
                variable_name: "var2".to_string(),
                contribution: 5.0,
            },
        ];

        // Three bootstrap results, but "var2" is missing from the second one
        let bootstrap_results = vec![
            SinglePassResult {
                three_fold: ThreeFoldDecomposition {
                    endowments: 0.0,
                    coefficients: 0.0,
                    interaction: 0.0,
                },
                two_fold: TwoFoldDecomposition {
                    explained: 0.0,
                    unexplained: 0.0,
                },
                detailed_explained: vec![
                    DetailedComponent {
                        variable_name: "var1".to_string(),
                        contribution: 11.0,
                    },
                    DetailedComponent {
                        variable_name: "var2".to_string(),
                        contribution: 6.0,
                    },
                ],
                detailed_unexplained: Vec::new(),
                total_gap: 0.0,
                residuals_a: DVector::zeros(0),
                residuals_b: DVector::zeros(0),
                xa_mean: DVector::zeros(0),
                xb_mean: DVector::zeros(0),
                beta_star: DVector::zeros(0),
                detailed_selection: Vec::new(),
            },
            SinglePassResult {
                three_fold: ThreeFoldDecomposition {
                    endowments: 0.0,
                    coefficients: 0.0,
                    interaction: 0.0,
                },
                two_fold: TwoFoldDecomposition {
                    explained: 0.0,
                    unexplained: 0.0,
                },
                detailed_explained: vec![DetailedComponent {
                    variable_name: "var1".to_string(),
                    contribution: 9.0,
                }],
                detailed_unexplained: Vec::new(),
                total_gap: 0.0,
                residuals_a: DVector::zeros(0),
                residuals_b: DVector::zeros(0),
                xa_mean: DVector::zeros(0),
                xb_mean: DVector::zeros(0),
                beta_star: DVector::zeros(0),
                detailed_selection: Vec::new(),
            },
            SinglePassResult {
                three_fold: ThreeFoldDecomposition {
                    endowments: 0.0,
                    coefficients: 0.0,
                    interaction: 0.0,
                },
                two_fold: TwoFoldDecomposition {
                    explained: 0.0,
                    unexplained: 0.0,
                },
                detailed_explained: vec![
                    DetailedComponent {
                        variable_name: "var1".to_string(),
                        contribution: 10.0,
                    },
                    DetailedComponent {
                        variable_name: "var2".to_string(),
                        contribution: 4.0,
                    },
                ],
                detailed_unexplained: Vec::new(),
                total_gap: 0.0,
                residuals_a: DVector::zeros(0),
                residuals_b: DVector::zeros(0),
                xa_mean: DVector::zeros(0),
                xb_mean: DVector::zeros(0),
                beta_star: DVector::zeros(0),
                detailed_selection: Vec::new(),
            },
        ];

        let process_fn = |name: &str, point: f64, estimates: Vec<f64>| ComponentResult {
            name: name.to_string(),
            estimate: point,
            std_err: estimates.len() as f64, // Hack to verify number of estimates
            t_stat: 0.0,
            p_value: 0.0,
            ci_lower: 0.0,
            ci_upper: 0.0,
        };

        let results = builder.process_detailed_components(
            &point_components,
            &bootstrap_results,
            |r| &r.detailed_explained,
            &process_fn,
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "var1");
        assert_eq!(results[0].std_err, 3.0); // 3 estimates

        assert_eq!(results[1].name, "var2");
        assert_eq!(results[1].std_err, 3.0); // Should be 3 estimates (padded with 0.0)
    }

    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
