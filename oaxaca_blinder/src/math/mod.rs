pub mod diagnostics;
pub mod kde;
pub mod logit;
pub mod normalization;
pub mod ols;
pub mod probit;
pub mod quantile_regression;
pub mod rif;

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{DMatrix, DVector};
    use ndarray::array;
    use polars::prelude::*;
    use std::collections::HashMap;

    #[test]
    fn test_ols_module_export_and_execution() {
        let x = DMatrix::from_vec(5, 2, vec![1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
        let y = DVector::from_vec(vec![1.0, 3.0, 5.0, 7.0, 9.0]);

        let result = ols::ols(&y, &x, None).expect("OLS failed");
        assert_eq!(result.coefficients.len(), 2);
        assert!((result.coefficients[0] - 1.0).abs() < 1e-9);
        assert!((result.coefficients[1] - 2.0).abs() < 1e-9);

        // Insufficient data test
        let x_small = DMatrix::from_vec(2, 3, vec![1.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        let y_small = DVector::from_vec(vec![1.0, 2.0]);
        assert!(ols::ols(&y_small, &x_small, None).is_err());
    }

    #[test]
    fn test_logit_module_export_and_execution() {
        let x = DMatrix::from_vec(
            6,
            2,
            vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0],
        );
        let y = DVector::from_vec(vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);

        let res = logit::logit(&y, &x, 100, 1e-6).expect("Logit failed");
        assert_eq!(res.coefficients.len(), 2);
        assert!(res.iterations > 0);
    }

    #[test]
    fn test_probit_module_export_and_execution() {
        let y = DVector::from_vec(vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
        let x = DMatrix::from_row_slice(
            6,
            2,
            &[1.0, -1.5, 1.0, -0.5, 1.0, 0.0, 1.0, 0.5, 1.0, 1.0, 1.0, 1.5],
        );

        let res = probit::probit(&y, &x, 50, 1e-5).expect("Probit failed");
        assert_eq!(res.coefficients.len(), 2);
        assert_eq!(res.vcov.nrows(), 2);
    }

    #[test]
    fn test_kde_module_export_and_execution() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let bw = kde::silverman_bandwidth(&data);
        assert!(bw > 0.0);

        let grid = vec![3.0];
        let density = kde::kde(&data, None, &grid, bw);
        assert_eq!(density.len(), 1);
        assert!(density[0] > 0.0);
    }

    #[test]
    fn test_diagnostics_module_export_and_execution() {
        let df = df!(
            "x1" => &[1.0, 2.0, 3.0, 4.0, 5.0],
            "x2" => &[2.0, 3.0, 1.0, 5.0, 4.0],
            "x3" => &[1.0, 5.0, 2.0, 4.0, 3.0],
        )
        .unwrap();

        let predictor_names = vec!["x1".to_string(), "x2".to_string(), "x3".to_string()];
        let vif_results = diagnostics::calculate_vif(&df, &predictor_names).unwrap();
        assert_eq!(vif_results.len(), 3);

        // Test error handling for too few predictors
        let single_predictor = vec!["x1".to_string()];
        assert!(diagnostics::calculate_vif(&df, &single_predictor).is_err());
    }

    #[test]
    fn test_normalization_module_export_and_execution() {
        let coeffs = DVector::from_vec(vec![10.0, 2.0, 4.0]);
        let vcov = DMatrix::zeros(3, 3);
        let residuals = DVector::zeros(0);
        let mut ols_res = ols::OlsResult {
            coefficients: coeffs,
            vcov,
            residuals,
        };

        let predictor_names = vec![
            "__ob_intercept__".to_string(),
            "cat_B".to_string(),
            "cat_C".to_string(),
        ];
        let categorical_vars = vec!["cat".to_string()];
        let x_mean = DVector::from_vec(vec![1.0, 0.3, 0.5]);
        let mut category_counts = HashMap::new();
        category_counts.insert("cat".to_string(), 3);

        let base_coeffs = normalization::normalize_categorical_coefficients(
            &mut ols_res,
            &predictor_names,
            &categorical_vars,
            &x_mean,
            &category_counts,
        );

        assert!(base_coeffs.contains_key("cat"));
        assert!((ols_res.coefficients[0] - 12.0).abs() < 1e-9);
    }

    #[test]
    fn test_quantile_regression_module_export_and_execution() {
        let y = array![1.0, 2.0, 3.0, 4.0, 5.0];
        let x = array![[1.0, 1.0], [1.0, 2.0], [1.0, 3.0], [1.0, 4.0], [1.0, 5.0]];

        let result = quantile_regression::solve_qr(&x, &y, 0.5).unwrap();
        assert_eq!(result.len(), 2);
        assert!((result[1] - 1.0).abs() < 1e-3);

        // Invalid tau error
        assert!(quantile_regression::solve_qr(&x, &y, 1.5).is_err());
    }

    #[test]
    fn test_rif_module_export_and_execution() {
        let s = Series::new("y".into(), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let rif_res = rif::calculate_rif(&s, 0.5).unwrap();
        assert_eq!(rif_res.len(), 5);

        let weights = vec![1.0; 5];
        let rif_w_res = rif::calculate_rif_weighted(&s, 0.5, Some(&weights)).unwrap();
        assert_eq!(rif_w_res.len(), 5);

        // Too few observations error
        let s_small = Series::new("y".into(), vec![1.0]);
        assert!(rif::calculate_rif(&s_small, 0.5).is_err());
    }
}
