use nalgebra::DVector;
use serde::Serialize;

/// Which coefficient vector `β*` prices the characteristics gap in the two-fold decomposition:
/// `explained = (x̄_A − x̄_B)'β*`, `unexplained = gap − explained`.
///
/// "Group A" is the NON-reference ("compared") group and "Group B" is the group named by
/// `reference_group` (`OaxacaBuilder::split_groups`: `group_b_name = reference_group`). Which
/// group is the advantaged one is a property of the data, not of these names.
///
/// External oracles (R `oaxaca` 0.1.5 `twofold$overall`, rows by `group.weight`): GroupA = 1,
/// GroupB = 0, Weighted = the share of group A, PooledNoIndicator = −1, Pooled = −2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReferenceCoefficients {
    /// `β*` = the compared (non-reference) group's own coefficients.
    GroupA,
    /// `β*` = the reference group's own coefficients: the compared group is priced as if it
    /// were paid under the reference group's pay structure.
    #[default]
    GroupB,
    /// `β*` from one regression on both groups WITH a group-indicator column; the indicator's
    /// coefficient is dropped from `β*` and is exactly the unexplained gap (Jann 2008 `pooled`;
    /// Fortin 2008; Elder, Goddeeris & Haider, IZA DP 4159).
    Pooled,
    /// `β*` from one regression on both groups WITHOUT a group indicator (Neumark 1988; Stata
    /// `oaxaca, omega`; R `oaxaca` weight −1). Differs from [`Pooled`](Self::Pooled): the
    /// indicator's effect is absorbed into the other coefficients.
    PooledNoIndicator,
    /// `β*` = the sample-share-weighted average of the two groups' coefficients (Cotton 1988).
    Weighted,
    /// Alias for [`Weighted`](Self::Weighted) (Cotton's method).
    Cotton,
    /// Deprecated alias of [`Pooled`](Self::Pooled), kept so existing callers compile and keep
    /// their numbers. The name is historically wrong: it computes the pooled regression WITH
    /// the group indicator, which is not Neumark's estimator. Use
    /// [`PooledNoIndicator`](Self::PooledNoIndicator) for Neumark.
    #[deprecated(
        since = "0.3.0",
        note = "computes `Pooled` (with a group indicator), not Neumark's estimator; use `Pooled` or `PooledNoIndicator`"
    )]
    Neumark,
}

impl ReferenceCoefficients {
    /// The exact names the shipped surfaces accept, in documentation order.
    pub const ACCEPTED_NAMES: [&'static str; 5] = [
        "GroupA",
        "GroupB",
        "Pooled",
        "PooledNoIndicator",
        "Weighted",
    ];

    /// Strict parse used at every engine boundary (0120-MERIDIAN S4). An absent value or any
    /// string outside [`ACCEPTED_NAMES`](Self::ACCEPTED_NAMES) (including `"pooled"` and the
    /// old aliases) is an error; there is no fallback scheme.
    pub fn parse_name(name: Option<&str>) -> Result<Self, crate::error::OaxacaError> {
        match name {
            Some("GroupA") => Ok(Self::GroupA),
            Some("GroupB") => Ok(Self::GroupB),
            Some("Pooled") => Ok(Self::Pooled),
            Some("PooledNoIndicator") => Ok(Self::PooledNoIndicator),
            Some("Weighted") => Ok(Self::Weighted),
            other => Err(crate::error::OaxacaError::UnknownReferenceCoefficients {
                given: other.map(str::to_string),
            }),
        }
    }

    /// The canonical name of the scheme as echoed in `run_metadata.reference_coefficients_used`.
    pub fn canonical_name(self) -> &'static str {
        #[allow(deprecated)]
        match self {
            Self::GroupA => "GroupA",
            Self::GroupB => "GroupB",
            Self::Pooled | Self::Neumark => "Pooled",
            Self::PooledNoIndicator => "PooledNoIndicator",
            Self::Weighted | Self::Cotton => "Weighted",
        }
    }
}

/// Holds the results of the three-fold decomposition.
#[derive(Debug, Clone)]
pub struct ThreeFoldDecomposition {
    pub endowments: f64,
    pub coefficients: f64,
    pub interaction: f64,
}

/// Holds the results of the two-fold decomposition.
#[derive(Debug, Clone)]
pub struct TwoFoldDecomposition {
    pub explained: f64,
    pub unexplained: f64,
}

/// Represents the contribution of a single variable to a decomposition component.
#[derive(Debug, PartialEq, Clone)]
pub struct DetailedComponent {
    pub variable_name: String,
    pub contribution: f64,
}

/// Represents a recommended adjustment for an individual to improve pay equity.
#[derive(Debug, Clone, Serialize)]
pub struct BudgetAdjustment {
    /// The index of the individual in the reference group (Group B) data.
    pub index: usize,
    /// The original unexplained residual for this individual (negative means underpaid).
    pub original_residual: f64,
    /// The recommended adjustment amount (raise).
    pub adjustment: f64,
}

/// Computes the two-fold Oaxaca-Blinder decomposition.
pub fn two_fold_decomposition(
    xa_mean: &DVector<f64>,
    xb_mean: &DVector<f64>,
    beta_a: &DVector<f64>,
    beta_b: &DVector<f64>,
    beta_star: &DVector<f64>,
) -> TwoFoldDecomposition {
    let explained = (xa_mean - xb_mean).dot(beta_star);
    let total_gap = xa_mean.dot(beta_a) - xb_mean.dot(beta_b);
    let unexplained = total_gap - explained;
    TwoFoldDecomposition {
        explained,
        unexplained,
    }
}

/// Computes the three-fold Oaxaca-Blinder decomposition.
pub fn three_fold_decomposition(
    xa_mean: &DVector<f64>,
    xb_mean: &DVector<f64>,
    beta_a: &DVector<f64>,
    beta_b: &DVector<f64>,
) -> ThreeFoldDecomposition {
    let diff_x = xa_mean - xb_mean;
    let diff_beta = beta_a - beta_b;
    let endowments = diff_x.dot(beta_b);
    let coefficients = xb_mean.dot(&diff_beta);
    let interaction = diff_x.dot(&diff_beta);
    ThreeFoldDecomposition {
        endowments,
        coefficients,
        interaction,
    }
}

/// Computes the detailed decomposition for both explained and unexplained parts.
pub fn detailed_decomposition(
    xa_mean: &DVector<f64>,
    xb_mean: &DVector<f64>,
    beta_a: &DVector<f64>,
    beta_b: &DVector<f64>,
    beta_star: &DVector<f64>,
    predictor_names: &[String],
) -> (Vec<DetailedComponent>, Vec<DetailedComponent>) {
    let explained: Vec<DetailedComponent> = (0..predictor_names.len())
        .map(|i| {
            let contribution = (xa_mean[i] - xb_mean[i]) * beta_star[i];
            DetailedComponent {
                variable_name: predictor_names[i].clone(),
                contribution,
            }
        })
        .collect();

    let unexplained: Vec<DetailedComponent> = (0..predictor_names.len())
        .map(|i| {
            let contribution =
                xa_mean[i] * (beta_a[i] - beta_star[i]) + xb_mean[i] * (beta_star[i] - beta_b[i]);
            DetailedComponent {
                variable_name: predictor_names[i].clone(),
                contribution,
            }
        })
        .collect();

    (explained, unexplained)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::DVector;

    #[test]
    fn test_three_fold_decomposition() {
        let xa_mean = DVector::from_vec(vec![1.0, 5.0]);
        let xb_mean = DVector::from_vec(vec![1.0, 3.0]);
        let beta_a = DVector::from_vec(vec![2.0, 4.0]);
        let beta_b = DVector::from_vec(vec![1.0, 3.0]);
        let result = three_fold_decomposition(&xa_mean, &xb_mean, &beta_a, &beta_b);
        assert!((result.endowments - 6.0).abs() < 1e-9);
        assert!((result.coefficients - 4.0).abs() < 1e-9);
        assert!((result.interaction - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_detailed_decomposition_sums() {
        let predictor_names = vec![crate::INTERCEPT_NAME.to_string(), "age".to_string()];
        let beta_a = DVector::from_vec(vec![2.0, 4.0]);
        let beta_b = DVector::from_vec(vec![1.0, 3.0]);
        let xa_mean = DVector::from_vec(vec![1.0, 5.0]);
        let xb_mean = DVector::from_vec(vec![1.0, 3.0]);

        // Case 1: beta* = beta_b
        let beta_star_b = beta_b.clone();
        let (explained_detailed, unexplained_detailed) = detailed_decomposition(
            &xa_mean,
            &xb_mean,
            &beta_a,
            &beta_b,
            &beta_star_b,
            &predictor_names,
        );
        let two_fold_b = two_fold_decomposition(&xa_mean, &xb_mean, &beta_a, &beta_b, &beta_star_b);

        let total_explained: f64 = explained_detailed.iter().map(|c| c.contribution).sum();
        let total_unexplained: f64 = unexplained_detailed.iter().map(|c| c.contribution).sum();

        assert!((total_explained - two_fold_b.explained).abs() < 1e-9);
        assert!((total_unexplained - two_fold_b.unexplained).abs() < 1e-9);

        // Case 2: beta* = beta_a
        let beta_star_a = beta_a.clone();
        let (explained_detailed_a, unexplained_detailed_a) = detailed_decomposition(
            &xa_mean,
            &xb_mean,
            &beta_a,
            &beta_b,
            &beta_star_a,
            &predictor_names,
        );
        let two_fold_a = two_fold_decomposition(&xa_mean, &xb_mean, &beta_a, &beta_b, &beta_star_a);

        let total_explained_a: f64 = explained_detailed_a.iter().map(|c| c.contribution).sum();
        let total_unexplained_a: f64 = unexplained_detailed_a.iter().map(|c| c.contribution).sum();

        assert!((total_explained_a - two_fold_a.explained).abs() < 1e-9);
        assert!((total_unexplained_a - two_fold_a.unexplained).abs() < 1e-9);
    }
}
