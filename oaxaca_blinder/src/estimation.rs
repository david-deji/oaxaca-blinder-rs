use std::collections::HashMap;

use nalgebra::{DMatrix, DVector};
use polars::prelude::*;

use crate::error::OaxacaError;
use crate::heckman::heckman_two_step;
use crate::math::normalization::{normalize_categorical_coefficients, ShareMap};
use crate::math::ols::ols;

pub(crate) struct EstimationResult {
    /// Coefficients the detailed two-fold is computed from: normalised when `ctx.shares` is
    /// non-empty, the raw treatment-coded fit otherwise.
    pub beta_a: DVector<f64>,
    pub beta_b: DVector<f64>,
    /// The treatment-coded (un-normalised) fit. The three-fold aggregate is computed from
    /// these: it is invariant to the coding, the normalised one is not (0120 T3).
    pub beta_a_raw: DVector<f64>,
    pub beta_b_raw: DVector<f64>,
    pub xa_mean: DVector<f64>,
    pub xb_mean: DVector<f64>,
    pub predictor_names: Vec<String>,
    pub residuals_a: DVector<f64>,
    pub residuals_b: DVector<f64>,
    pub base_coeffs_a: HashMap<String, f64>,
    pub base_coeffs_b: HashMap<String, f64>,
    pub selection_coeffs_a: Option<DVector<f64>>,
    pub selection_coeffs_b: Option<DVector<f64>>,
    pub selection_means_a: Option<DVector<f64>>,
    pub selection_means_b: Option<DVector<f64>>,
    pub selection_names: Option<Vec<String>>,
    pub imr_delta_a: Option<f64>,
    pub imr_delta_b: Option<f64>,
}

pub(crate) struct EstimationContext<'a> {
    pub df_a: &'a DataFrame,
    pub df_b: &'a DataFrame,
    pub x_a: &'a DMatrix<f64>,
    pub y_a: &'a DVector<f64>,
    pub w_a: &'a Option<DVector<f64>>,
    pub x_b: &'a DMatrix<f64>,
    pub y_b: &'a DVector<f64>,
    pub w_b: &'a Option<DVector<f64>>,
    pub predictor_names: &'a [String],
    /// Restriction weights of the variables to normalise, built once per pass from the pooled
    /// analysed rows. Empty = no normalisation (raw library mode, or Heckman).
    pub shares: &'a ShareMap,
}

pub(crate) trait Estimator {
    fn estimate(&self, ctx: &EstimationContext) -> Result<EstimationResult, OaxacaError>;
}

pub(crate) struct OlsEstimator;

impl Estimator for OlsEstimator {
    fn estimate(&self, ctx: &EstimationContext) -> Result<EstimationResult, OaxacaError> {
        let mut ols_a = ols(ctx.y_a, ctx.x_a, ctx.w_a.as_ref())?;
        let mut ols_b = ols(ctx.y_b, ctx.x_b, ctx.w_b.as_ref())?;

        let calculate_mean = |x: &DMatrix<f64>, w: &Option<DVector<f64>>| -> DVector<f64> {
            if let Some(weights) = w {
                let total_weight = weights.sum();
                let mut means = DVector::zeros(x.ncols());
                for j in 0..x.ncols() {
                    let col = x.column(j);
                    means[j] = col.dot(weights) / total_weight;
                }
                means
            } else {
                x.row_mean().transpose()
            }
        };

        let xa_mean = calculate_mean(ctx.x_a, ctx.w_a);
        let xb_mean = calculate_mean(ctx.x_b, ctx.w_b);

        let beta_a_raw = ols_a.coefficients.clone();
        let beta_b_raw = ols_b.coefficients.clone();

        let mut base_coeffs_a = HashMap::new();
        let mut base_coeffs_b = HashMap::new();

        // One share vector for both groups (and, in the builder, for the pooled fit and the
        // weighted mix): the same restriction on every coefficient vector is what keeps
        // explained + unexplained == gap exact for any constant.
        if !ctx.shares.is_empty() {
            base_coeffs_a =
                normalize_categorical_coefficients(&mut ols_a, ctx.predictor_names, ctx.shares)?;
            base_coeffs_b =
                normalize_categorical_coefficients(&mut ols_b, ctx.predictor_names, ctx.shares)?;
        }

        Ok(EstimationResult {
            beta_a: ols_a.coefficients,
            beta_b: ols_b.coefficients,
            beta_a_raw,
            beta_b_raw,
            xa_mean,
            xb_mean,
            predictor_names: ctx.predictor_names.to_vec(),
            residuals_a: ols_a.residuals,
            residuals_b: ols_b.residuals,
            base_coeffs_a,
            base_coeffs_b,
            selection_coeffs_a: None,
            selection_coeffs_b: None,
            selection_means_a: None,
            selection_means_b: None,
            selection_names: None,
            imr_delta_a: None,
            imr_delta_b: None,
        })
    }
}

pub(crate) struct HeckmanEstimator {
    pub selection_outcome: String,
    pub selection_predictors: Vec<String>,
}

impl Estimator for HeckmanEstimator {
    fn estimate(&self, ctx: &EstimationContext) -> Result<EstimationResult, OaxacaError> {
        let (x_sel_a, y_sel_a, x_sel_sub_a) = self.prepare_selection_data(ctx.df_a)?;
        let (x_sel_b, y_sel_b, x_sel_sub_b) = self.prepare_selection_data(ctx.df_b)?;

        let (x_a_filt, y_a_filt) = self.filter_outcome_rows(ctx.x_a, ctx.y_a, ctx.df_a)?;
        let (x_b_filt, y_b_filt) = self.filter_outcome_rows(ctx.x_b, ctx.y_b, ctx.df_b)?;

        let res_a = heckman_two_step(&y_sel_a, &x_sel_a, &y_a_filt, &x_a_filt, &x_sel_sub_a)?;
        let res_b = heckman_two_step(&y_sel_b, &x_sel_b, &y_b_filt, &x_b_filt, &x_sel_sub_b)?;

        let mut beta_a = res_a.outcome_coeffs;
        let mut beta_b = res_b.outcome_coeffs;

        let k = beta_a.len();
        beta_a = beta_a.insert_row(k, res_a.imr_coeff);
        beta_b = beta_b.insert_row(k, res_b.imr_coeff);

        let mut xa_mean = x_a_filt.row_mean().transpose();
        let mut xb_mean = x_b_filt.row_mean().transpose();

        let imr_mean_a = res_a.imr.mean();
        let imr_mean_b = res_b.imr.mean();

        let k_x = xa_mean.len();
        xa_mean = xa_mean.insert_row(k_x, imr_mean_a);
        xb_mean = xb_mean.insert_row(k_x, imr_mean_b);

        let mut final_names = ctx.predictor_names.to_vec();
        final_names.push("IMR".to_string());

        let residuals_a = DVector::zeros(y_a_filt.len());
        let residuals_b = DVector::zeros(y_b_filt.len());

        Ok(EstimationResult {
            beta_a_raw: beta_a.clone(),
            beta_b_raw: beta_b.clone(),
            beta_a,
            beta_b,
            xa_mean,
            xb_mean,
            predictor_names: final_names,
            residuals_a,
            residuals_b,
            base_coeffs_a: HashMap::new(),
            base_coeffs_b: HashMap::new(),
            selection_coeffs_a: Some(res_a.selection_coeffs),
            selection_coeffs_b: Some(res_b.selection_coeffs),
            selection_means_a: Some(x_sel_a.row_mean().transpose().clone()),
            selection_means_b: Some(x_sel_b.row_mean().transpose().clone()),
            selection_names: Some(ctx.predictor_names.to_vec()),
            imr_delta_a: Some(res_a.imr_delta),
            imr_delta_b: Some(res_b.imr_delta),
        })
    }
}

impl HeckmanEstimator {
    #[allow(clippy::type_complexity)]
    fn prepare_selection_data(
        &self,
        df_group: &DataFrame,
    ) -> Result<(DMatrix<f64>, DVector<f64>, DMatrix<f64>), OaxacaError> {
        let y_sel_series = df_group.column(&self.selection_outcome)?.f64()?;
        let y_sel_vec: Result<Vec<f64>, OaxacaError> = y_sel_series
            .into_iter()
            .map(|opt| {
                opt.ok_or_else(|| {
                    OaxacaError::InvalidGroupVariable(
                        "Selection outcome contains nulls".to_string(),
                    )
                })
            })
            .collect();
        let y_sel = DVector::from_vec(y_sel_vec?);

        let mut x_sel_df = df_group.select(&self.selection_predictors)?;
        let intercept = Series::new(crate::INTERCEPT_NAME.into(), vec![1.0; df_group.height()]);
        x_sel_df.with_column(intercept)?;
        let mut cols = vec![crate::INTERCEPT_NAME.to_string()];
        cols.extend(self.selection_predictors.clone());
        let x_sel_df = x_sel_df.select(&cols)?;

        let x_sel_mat = x_sel_df.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
        let x_sel_vec: Vec<f64> = x_sel_mat.iter().copied().collect();
        let x_sel = DMatrix::from_row_slice(x_sel_df.height(), x_sel_df.width(), &x_sel_vec);

        let mask = df_group
            .column(&self.selection_outcome)?
            .as_materialized_series()
            .equal(1)?;
        let df_subset = df_group.filter(&mask)?;

        let mut x_sel_sub_df = df_subset.select(&self.selection_predictors)?;
        x_sel_sub_df.with_column(Series::new(
            crate::INTERCEPT_NAME.into(),
            vec![1.0; df_subset.height()],
        ))?;
        let x_sel_sub_df = x_sel_sub_df.select(&cols)?;

        let x_sel_sub_mat = x_sel_sub_df.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
        let x_sel_sub_vec: Vec<f64> = x_sel_sub_mat.iter().copied().collect();
        let x_sel_sub =
            DMatrix::from_row_slice(x_sel_sub_df.height(), x_sel_sub_df.width(), &x_sel_sub_vec);

        Ok((x_sel, y_sel, x_sel_sub))
    }

    fn filter_outcome_rows(
        &self,
        x: &DMatrix<f64>,
        y: &DVector<f64>,
        df: &DataFrame,
    ) -> Result<(DMatrix<f64>, DVector<f64>), OaxacaError> {
        let mask = df
            .column(&self.selection_outcome)?
            .as_materialized_series()
            .equal(1)?;

        let mut true_indices = Vec::with_capacity(df.height());
        for i in 0..df.height() {
            if mask.get(i) == Some(true) {
                true_indices.push(i);
            }
        }

        if true_indices.is_empty() {
            return Err(OaxacaError::InvalidGroupVariable(
                "No observed outcomes in group".to_string(),
            ));
        }

        let num_true = true_indices.len();
        let ncols = x.ncols();

        let mut x_vals = Vec::with_capacity(num_true * ncols);
        for j in 0..ncols {
            let col = x.column(j);
            for &i in &true_indices {
                x_vals.push(col[i]);
            }
        }
        let x_filtered = DMatrix::from_vec(num_true, ncols, x_vals);

        let mut y_vals = Vec::with_capacity(num_true);
        for &i in &true_indices {
            y_vals.push(y[i]);
        }
        let y_filtered = DVector::from_vec(y_vals);

        Ok((x_filtered, y_filtered))
    }
}
