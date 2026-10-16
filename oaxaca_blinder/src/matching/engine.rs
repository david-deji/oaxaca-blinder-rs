use crate::matching::distance::MahalanobisDistance;
use crate::matching::logistic::LogisticRegression;
use crate::OaxacaError;
use kdtree::distance::squared_euclidean;
use kdtree::KdTree;
use nalgebra::{DMatrix, DVector};
use polars::prelude::*;

/// Struct for performing matching.
pub struct MatchingEngine {
    dataframe: DataFrame,
    treatment_col: String,
    outcome_col: String,
    covariates: Vec<String>,
}

impl MatchingEngine {
    pub fn new<I, S>(
        dataframe: DataFrame,
        treatment_col: &str,
        outcome_col: &str,
        covariates: I,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            dataframe,
            treatment_col: treatment_col.to_string(),
            outcome_col: outcome_col.to_string(),
            covariates: covariates
                .into_iter()
                .map(|s| s.as_ref().to_string())
                .collect(),
        }
    }

    /// Prepares the data matrices for treated and control groups.
    #[allow(clippy::type_complexity)]
    fn prepare_data(
        &self,
    ) -> Result<(DMatrix<f64>, DMatrix<f64>, DVector<f64>, DVector<f64>), OaxacaError> {
        let df = &self.dataframe;

        // Filter treated and control
        let treated_mask = df
            .column(&self.treatment_col)?
            .as_materialized_series()
            .equal(1)?;
        let control_mask = df
            .column(&self.treatment_col)?
            .as_materialized_series()
            .equal(0)?;

        let treated_df = df.filter(&treated_mask)?;
        let control_df = df.filter(&control_mask)?;

        if treated_df.height() == 0 || control_df.height() == 0 {
            return Err(OaxacaError::InvalidGroupVariable(
                "One group is empty".to_string(),
            ));
        }

        // Extract covariates
        let get_matrix = |d: &DataFrame| -> Result<DMatrix<f64>, OaxacaError> {
            let selected = d.select(&self.covariates)?;
            let mat = selected.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
            let vec: Vec<f64> = mat.iter().copied().collect();
            Ok(DMatrix::from_row_slice(
                selected.height(),
                selected.width(),
                &vec,
            ))
        };

        let x_treated = get_matrix(&treated_df)?;
        let x_control = get_matrix(&control_df)?;

        // Extract outcomes
        let get_vec = |d: &DataFrame| -> Result<DVector<f64>, OaxacaError> {
            let s = d.column(&self.outcome_col)?.f64()?;
            let v: Vec<f64> = s
                .into_iter()
                .map(|opt| {
                    opt.ok_or_else(|| {
                        OaxacaError::InvalidGroupVariable("Null values in outcomes".to_string())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DVector::from_vec(v))
        };

        let y_treated = get_vec(&treated_df)?;
        let y_control = get_vec(&control_df)?;

        Ok((x_treated, x_control, y_treated, y_control))
    }

    /// Performs Nearest Neighbor Matching.
    ///
    /// # Arguments
    ///
    /// * `k` - Number of neighbors to match.
    /// * `metric` - Distance metric to use.
    pub fn match_nearest_neighbor(
        &self,
        k: usize,
        use_mahalanobis: bool,
    ) -> Result<DVector<f64>, OaxacaError> {
        let weights = self.run_matching(k, use_mahalanobis)?;
        Ok(DVector::from_vec(weights))
    }

    /// Helper to get weights with index mapping
    pub fn run_matching(&self, k: usize, use_mahalanobis: bool) -> Result<Vec<f64>, OaxacaError> {
        // 1. Add index column to track original rows
        let mut df = self.dataframe.clone();
        let indices: Vec<u32> = (0..df.height() as u32).collect();
        let _ = df.with_column(Series::new("orig_index".into(), indices))?;

        // 2. Split
        let treated_mask = df
            .column(&self.treatment_col)?
            .as_materialized_series()
            .equal(1)?;
        let control_mask = df
            .column(&self.treatment_col)?
            .as_materialized_series()
            .equal(0)?;

        let treated_df = df.filter(&treated_mask)?;
        let control_df = df.filter(&control_mask)?;

        // 3. Prepare Data for Matching
        let get_matrix = |d: &DataFrame| -> Result<DMatrix<f64>, OaxacaError> {
            let selected = d.select(&self.covariates)?;
            let mat = selected.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
            let vec: Vec<f64> = mat.iter().copied().collect();
            Ok(DMatrix::from_row_slice(
                selected.height(),
                selected.width(),
                &vec,
            ))
        };

        let mut x_treated = get_matrix(&treated_df)?;
        let mut x_control = get_matrix(&control_df)?;

        // 4. Transform if Mahalanobis
        if use_mahalanobis {
            // Calculate covariance on CONTROL group (usually) or Pooled?
            // Usually on Control group for ATT.
            let metric =
                MahalanobisDistance::new(&x_control).map_err(OaxacaError::DiagnosticError)?;
            // We need to transform x_treated and x_control.
            // X_new = X * L^-T ? No.
            // Mahalanobis d(x,y) = sqrt((x-y)^T S^-1 (x-y))
            // Let S^-1 = W^T W (Cholesky of inverse covariance)
            // Then d(x,y) = sqrt((x-y)^T W^T W (x-y)) = sqrt( (W(x-y))^T (W(x-y)) ) = || Wx - Wy ||
            // So we need to multiply X by W.
            // S = Cov. S^-1 = InvCov.
            // Cholesky of S^-1: L such that L L^T = S^-1.
            // Then transformed X = X * L.

            let inv_cov = metric.inv_covariance;
            let cholesky = inv_cov.cholesky().ok_or(OaxacaError::DiagnosticError(
                "Cholesky decomposition failed".to_string(),
            ))?;
            let l = cholesky.l(); // Lower triangular

            // Transform
            // x_vec (row) * L
            // DMatrix stores row-major? No, we constructed it.
            // X is N x K. L is K x K.
            // X * L
            x_treated *= &l;
            x_control *= &l;
        }

        // 5. Build Tree on Control
        let n_features = x_control.ncols();
        let mut kdtree = KdTree::new(n_features);

        for i in 0..x_control.nrows() {
            let row = x_control.row(i);
            let point: Vec<f64> = row.iter().copied().collect();
            kdtree
                .add(point, i)
                .map_err(|e| OaxacaError::DiagnosticError(format!("K-D Tree error: {}", e)))?;
        }

        // 6. Match
        let mut control_counts = vec![0.0; x_control.nrows()];

        // Preallocate a single buffer to hold the current point being queried
        let mut point = Vec::with_capacity(n_features);

        for i in 0..x_treated.nrows() {
            let row = x_treated.row(i);

            point.clear();
            point.extend(row.iter().copied());

            let nearest = kdtree.nearest(&point, k, &squared_euclidean).map_err(|e| {
                OaxacaError::DiagnosticError(format!("K-D Tree search error: {}", e))
            })?;

            for (_dist, &index) in nearest {
                control_counts[index] += 1.0 / (k as f64);
            }
        }

        // 7. Reassemble Weights
        let mut final_weights = vec![0.0; df.height()];

        // Treated units get weight 1
        let treated_indices = treated_df.column("orig_index")?.u32()?;
        for i in treated_indices.into_iter().flatten() {
            final_weights[i as usize] = 1.0;
        }

        // Control units get calculated weights
        let control_indices = control_df.column("orig_index")?.u32()?;
        for (local_idx, orig_idx) in control_indices.into_iter().enumerate() {
            if let Some(i) = orig_idx {
                final_weights[i as usize] = control_counts[local_idx];
            }
        }

        Ok(final_weights)
    }

    /// Propensity Score Matching
    pub fn match_psm(&self, k: usize) -> Result<Vec<f64>, OaxacaError> {
        // 1. Fit Logistic Regression
        let (_x_treated, _x_control, _, _) = self.prepare_data()?;

        // Combine for training
        // We need the full X and Y (treatment)
        let df = &self.dataframe;
        let get_matrix = |d: &DataFrame| -> Result<DMatrix<f64>, OaxacaError> {
            let selected = d.select(&self.covariates)?;
            let mat = selected.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
            let vec: Vec<f64> = mat.iter().copied().collect();
            Ok(DMatrix::from_row_slice(
                selected.height(),
                selected.width(),
                &vec,
            ))
        };

        let x_full = get_matrix(df)?;
        // Add intercept
        let mut x_full_intercept = x_full.clone();
        x_full_intercept = x_full_intercept.insert_column(0, 1.0);

        let y_treatment = df.column(&self.treatment_col)?.f64()?;
        let y_vec: Vec<f64> = y_treatment
            .into_iter()
            .map(|opt| opt.unwrap_or(0.0))
            .collect();
        let y_full = DVector::from_vec(y_vec);

        let mut logit = LogisticRegression::new();
        logit
            .fit(&x_full_intercept, &y_full, 100, 1e-6)
            .map_err(OaxacaError::DiagnosticError)?;

        // 2. Predict Propensity Scores
        let scores = logit.predict_proba(&x_full_intercept);

        // 3. Match on Scores (1D)
        // We can reuse the run_matching logic but with "score" as the only covariate.
        let mut df_with_score = df.clone();
        let score_series = Series::new("propensity_score".into(), scores.as_slice());
        df_with_score.with_column(score_series)?;

        let engine = MatchingEngine::new(
            df_with_score,
            &self.treatment_col,
            &self.outcome_col,
            vec!["propensity_score"],
        );
        engine.run_matching(k, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::matching::distance::EuclideanDistance;

    // Helper to create a dummy dataframe
    fn create_dummy_df() -> DataFrame {
        // 2 features: age, education
        // Treatment: 0 (control), 1 (treated)
        // Outcome: wage

        let s0 = Series::new("treatment".into(), &[1, 1, 0, 0, 0]);
        let s1 = Series::new("outcome".into(), &[10.0, 12.0, 8.0, 9.0, 8.5]);
        let s2 = Series::new("age".into(), &[30.0, 40.0, 31.0, 41.0, 35.0]);
        let s3 = Series::new("education".into(), &[12.0, 16.0, 12.0, 16.0, 14.0]);

        DataFrame::new(vec![
            Column::Series(s0),
            Column::Series(s1),
            Column::Series(s2),
            Column::Series(s3),
        ])
        .unwrap()
    }

    #[test]
    fn test_match_nearest_neighbor_euclidean() {
        let df = create_dummy_df();
        let engine = MatchingEngine::new(df, "treatment", "outcome", vec!["age", "education"]);

        let weights = engine.match_nearest_neighbor(1, false).unwrap();

        assert_eq!(weights.len(), 5);

        // Check weights logic
        // Treated units (indices 0, 1) should have weight 1.0
        assert_eq!(weights[0], 1.0);
        assert_eq!(weights[1], 1.0);
    }
}
