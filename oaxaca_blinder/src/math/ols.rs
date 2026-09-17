use crate::OaxacaError;
use nalgebra::{DMatrix, DVector};

/// Represents the results of an OLS regression.
#[derive(Debug)]
#[allow(dead_code)]
pub struct OlsResult {
    pub coefficients: DVector<f64>,
    pub vcov: DMatrix<f64>,
    pub residuals: DVector<f64>,
}

/// Performs an Ordinary Least Squares (OLS) regression.
///
/// The function calculates the coefficient vector `β` using the formula:
/// `β = (X'X)⁻¹ * X'y`
///
/// # Arguments
///
/// * `y` - A `DVector` representing the outcome variable.
/// * `x` - A `DMatrix` representing the predictor variables. It is crucial that this
///   matrix includes a column of ones if an intercept is desired in the model.
///
/// # Returns
///
/// A `Result` containing the `OlsResult` on success, or an `OaxacaError` if the
/// `X'X` matrix is singular and cannot be inverted.
/// Performs an Ordinary Least Squares (OLS) or Weighted Least Squares (WLS) regression.
///
/// The function calculates the coefficient vector `β` using the formula:
/// `β = (X'WX)⁻¹ * X'Wy` (where W is the weight matrix, Identity if unweighted)
///
/// # Arguments
///
/// * `y` - A `DVector` representing the outcome variable.
/// * `x` - A `DMatrix` representing the predictor variables. It is crucial that this
///   matrix includes a column of ones if an intercept is desired in the model.
/// * `weights` - An optional `DVector` of sample weights.
///
/// # Returns
///
/// A `Result` containing the `OlsResult` on success, or an `OaxacaError` if the
/// `X'WX` matrix is singular and cannot be inverted.
pub fn ols(
    y: &DVector<f64>,
    x: &DMatrix<f64>,
    weights: Option<&DVector<f64>>,
) -> Result<OlsResult, OaxacaError> {
    let (xtx, xty, n_obs) = if let Some(w) = weights {
        // Weighted Least Squares
        // We want to minimize (y - Xβ)'W(y - Xβ)
        // Normal equations: (X'WX)β = X'Wy

        // Efficiently compute X'WX and X'Wy without creating the full diagonal matrix W
        // X'WX = \sum w_i * x_i * x_i'
        // X'Wy = \sum w_i * x_i * y_i

        // Alternative: Transform data X* = \sqrt{W}X, y* = \sqrt{W}y
        // Then run OLS on X*, y*
        for weight in w.iter() {
            if *weight < 0.0 {
                return Err(OaxacaError::InvalidGroupVariable(
                    "Weights cannot be negative".to_string(),
                ));
            }
        }

        let n_rows = x.nrows();
        let k_cols = x.ncols();

        let w_slice = w.as_slice();
        let y_slice = y.as_slice();

        let x_slice = x.as_slice();
        let mut xtx = DMatrix::zeros(k_cols, k_cols);
        let mut xty = DVector::zeros(k_cols);

        for j in 0..k_cols {
            let col_j = &x_slice[j * n_rows..(j + 1) * n_rows];

            // Compute xty[j] = sum_i col_j[i] * w[i] * y[i]
            let mut sum_y = 0.0;
            for i in 0..n_rows {
                sum_y += col_j[i] * w_slice[i] * y_slice[i];
            }
            xty[j] = sum_y;

            // Compute xtx[(j, m)] = sum_i col_j[i] * col_m[i] * w[i] for m >= j
            for m in j..k_cols {
                let col_m = &x_slice[m * n_rows..(m + 1) * n_rows];
                let mut sum_x = 0.0;
                for i in 0..n_rows {
                    sum_x += col_j[i] * col_m[i] * w_slice[i];
                }
                xtx[(j, m)] = sum_x;
                xtx[(m, j)] = sum_x;
            }
        }

        let n = n_rows as f64;

        (xtx, xty, n)
    } else {
        // Ordinary Least Squares
        let xtx = x.transpose() * x;
        let xty = x.transpose() * y;
        let n = x.nrows() as f64;
        (xtx, xty, n)
    };

    // Attempt Cholesky decomposition on X'X (or X'WX).
    // This is more numerically stable than explicit inversion and acts as a check for positive-definiteness.
    // X'X should be positive definite if there is no perfect multicollinearity.

    let k = x.ncols() as f64;
    let n_obs_f = n_obs;
    if n_obs_f <= k {
        return Err(OaxacaError::InsufficientData(format!(
            "Insufficient data for OLS calculation: n_obs ({}) must be strictly greater than k ({})",
            n_obs, k
        )));
    }

    let cholesky = xtx.cholesky().ok_or_else(|| {
        OaxacaError::NalgebraError(
            "Failed to perform Cholesky decomposition. Matrix may be singular or not positive definite due to multicollinearity.".to_string(),
        )
    })?;

    // Calculate coefficients: β = (X'X)⁻¹ * X'y
    // We solve the linear system (X'X) * β = X'y using the Cholesky factor.
    let coefficients = cholesky.solve(&xty);

    // Calculate residuals (Raw residuals: y - Xβ)
    let y_hat = x * &coefficients;
    let residuals = y - y_hat;

    // Calculate residual variance
    // For WLS: e'We / (n - k)
    let k = x.ncols() as f64;
    let sse = if let Some(w) = weights {
        // Weighted Sum of Squared Errors
        let weighted_residuals = residuals.component_mul(w); // e * w
        residuals.dot(&weighted_residuals) // e' * (w * e) = \sum w_i * e_i^2
    } else {
        residuals.norm_squared()
    };

    let sigma_squared = sse / (n_obs - k);

    // Calculate variance-covariance matrix: (X'X)⁻¹ * σ²
    // We can get the inverse from the Cholesky decomposition efficiently.
    let xtx_inv = cholesky.inverse();
    let vcov = xtx_inv * sigma_squared;

    Ok(OlsResult {
        coefficients,
        vcov,
        residuals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{DMatrix, DVector};

    #[test]
    fn test_ols_simple_regression() {
        let x = DMatrix::from_vec(5, 2, vec![1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
        let y = DVector::from_vec(vec![1.0, 3.0, 5.0, 7.0, 9.0]);

        let result = ols(&y, &x, None).expect("OLS calculation failed on valid data");
        let coeffs = result.coefficients;

        assert_eq!(coeffs.len(), 2);
        assert!((coeffs[0] - 1.0).abs() < 1e-9, "Intercept is incorrect");
        assert!((coeffs[1] - 2.0).abs() < 1e-9, "Slope is incorrect");
    }

    #[test]
    fn test_ols_weighted_regression() {
        let x = DMatrix::from_vec(5, 2, vec![1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 2.0, 3.0, 4.0]);
        let y = DVector::from_vec(vec![1.0, 3.0, 5.0, 7.0, 9.0]);
        let weights = DVector::from_vec(vec![1.0, 2.0, 1.5, 0.5, 2.0]);

        let result = ols(&y, &x, Some(&weights)).expect("Weighted OLS calculation failed on valid data");
        let coeffs = result.coefficients;

        assert_eq!(coeffs.len(), 2);
        assert!((coeffs[0] - 1.0).abs() < 1e-9, "Intercept is incorrect");
        assert!((coeffs[1] - 2.0).abs() < 1e-9, "Slope is incorrect");
    }

    #[test]
    fn test_ols_handles_singular_matrix() {
        let x = DMatrix::from_vec(3, 2, vec![1.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
        let y = DVector::from_vec(vec![1.0, 2.0, 3.0]);

        let result = ols(&y, &x, None);

        assert!(result.is_err());
        match result {
            Err(OaxacaError::NalgebraError(msg)) => {
                assert!(msg.contains("Failed to perform Cholesky decomposition"));
            }
            Err(_) => {
                panic!("Expected a NalgebraError for a singular matrix, but got something else.")
            }
            Ok(_) => panic!("Expected an error, but got Ok"),
        }
    }

    #[test]
    fn test_ols_insufficient_data() {
        // Test OLS with 2 observations and 5 predictors
        let x = DMatrix::from_vec(
            2,
            5,
            vec![
                1.0, 1.0, // Column 1
                2.0, 3.0, // Column 2
                4.0, 5.0, // Column 3
                6.0, 7.0, // Column 4
                8.0, 9.0, // Column 5
            ],
        );
        let y = DVector::from_vec(vec![1.0, 2.0]);

        let result = ols(&y, &x, None);

        assert!(result.is_err());
        match result {
            Err(OaxacaError::InsufficientData(msg)) => {
                assert!(msg.contains("Insufficient data for OLS calculation"));
            }
            Err(_) => panic!("Expected InsufficientData error, but got something else."),
            Ok(_) => panic!("Expected an error, but got Ok"),
        }
    }
}
