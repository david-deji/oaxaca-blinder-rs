//! Categorical-coefficient normalisation (Gardeazabal-Ugidos 2004 / Yun 2005, with the
//! population-share restriction of Kennedy 1986 and Haisken-DeNew & Schmidt 1997).
//!
//! A dummy-coded categorical drops one level, so every other level's coefficient (and its
//! "detailed unexplained" contribution) is measured against whichever level sorts first. The
//! transform here re-expresses the coefficients under a restriction `sum_k s_k * beta_k = 0`
//! over ALL k levels (the dropped level's coefficient is 0 before the transform): every level,
//! the dropped one included, becomes a deviation from a weighted average of the levels, and the
//! intercept absorbs the average. Predictions and the aggregate decomposition do not change.
//!
//! `s` is a [`FactorShares`]. Two conventions exist:
//!
//! * [`NormalizationConvention::PopulationShare`] (0120-MERIDIAN D1, the default): `s_k` is the
//!   level's share of the pooled analysed rows A union B (sum of observation weights when a
//!   weights column is set, row counts otherwise), base level included. One vector is applied
//!   to every coefficient vector of a run (`beta_A`, `beta_B`, the pooled fit, the weighted
//!   mix), which keeps adding-up exact. Adding a small department changes only that
//!   department's row.
//! * [`NormalizationConvention::EqualShare`]: `s_k = 1/m`. This is what Stata `categorical()`,
//!   R `oaxaca` (third formula part) and `ddecompose` (`normalize_factors = TRUE`) print; kept
//!   so the engine can be checked against those packages to 1e-10.
//!
//! Shares are keyed by (variable, level NAME), never by column position.

use crate::error::OaxacaError;
use crate::math::ols::OlsResult;
use crate::INTERCEPT_NAME;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// Which weights the levels of a factor carry in the restriction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum NormalizationConvention {
    /// Level share of the pooled analysed rows (weights if set). 0120-MERIDIAN D1.
    #[default]
    #[serde(rename = "population-share")]
    PopulationShare,
    /// Simple average of the levels (`1/m` each). The Stata / R `oaxaca` / `ddecompose` convention.
    #[serde(rename = "equal-share")]
    EqualShare,
}

impl NormalizationConvention {
    /// The name echoed in `run_metadata.normalization.convention`.
    pub fn name(self) -> &'static str {
        match self {
            Self::PopulationShare => "population-share",
            Self::EqualShare => "equal-share",
        }
    }
}

/// One level's weight in the restriction.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LevelShare {
    pub level: String,
    pub share: f64,
}

/// The restriction weights of one categorical variable: every level, base included, ascending
/// by level name, summing to 1.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FactorShares {
    pub variable: String,
    /// The level that has no dummy column in the design (the alphabetically first one).
    pub base_level: String,
    pub levels: Vec<LevelShare>,
}

impl FactorShares {
    /// Share of `level`, or `None` when the level is not in the vector.
    pub fn share_of(&self, level: &str) -> Option<f64> {
        self.levels
            .iter()
            .find(|l| l.level == level)
            .map(|l| l.share)
    }
}

/// Restriction weights for every normalised variable, keyed by variable name.
pub type ShareMap = BTreeMap<String, FactorShares>;

/// Build the restriction weights of one variable.
///
/// `levels[i]` is row `i`'s level and `weights[i]` its observation weight (`None` = every row
/// counts 1). `dummy_names` are the design's dummy columns; the base level is the single
/// observed level whose `{variable}_{level}` column is NOT among them.
pub fn factor_shares(
    variable: &str,
    levels: &[&str],
    weights: Option<&[f64]>,
    dummy_names: &[String],
    convention: NormalizationConvention,
) -> Result<FactorShares, OaxacaError> {
    if let Some(w) = weights {
        if w.len() != levels.len() {
            return Err(OaxacaError::NormalizationError(format!(
                "weights ({}) and levels ({}) differ in length for '{}'",
                w.len(),
                levels.len(),
                variable
            )));
        }
    }
    let mut mass: BTreeMap<&str, f64> = BTreeMap::new();
    for (i, level) in levels.iter().enumerate() {
        let w = weights.map_or(1.0, |w| w[i]);
        *mass.entry(*level).or_insert(0.0) += w;
    }
    let total: f64 = mass.values().sum();
    if mass.is_empty() || !total.is_finite() || total <= 0.0 {
        return Err(OaxacaError::NormalizationError(format!(
            "no positive weight to compute level shares for '{}'",
            variable
        )));
    }

    let bases: Vec<&str> = mass
        .keys()
        .copied()
        .filter(|l| {
            !dummy_names
                .iter()
                .any(|d| d == &format!("{}_{}", variable, l))
        })
        .collect();
    let base_level = match bases.as_slice() {
        [one] => (*one).to_string(),
        _ => {
            return Err(OaxacaError::NormalizationError(format!(
                "expected exactly one level of '{}' without a dummy column, found {:?}",
                variable, bases
            )))
        }
    };

    let m = mass.len() as f64;
    let levels = mass
        .into_iter()
        .map(|(level, w)| LevelShare {
            level: level.to_string(),
            share: match convention {
                NormalizationConvention::PopulationShare => w / total,
                NormalizationConvention::EqualShare => 1.0 / m,
            },
        })
        .collect();
    Ok(FactorShares {
        variable: variable.to_string(),
        base_level,
        levels,
    })
}

/// Re-express the dummy coefficients of every variable in `shares` under
/// `sum_k s_k * beta_k = 0` (base level's raw coefficient is 0). Mutates `ols_results` in place
/// (intercept absorbs the share-weighted average, each dummy is shifted by it) and returns each
/// variable's NEW base-level coefficient `-sum_j s_j beta_j`, keyed by variable name.
///
/// A dummy the shares name but the design lacks is an error: a silent skip would leave that
/// variable's level coefficients on the old restriction.
pub fn normalize_categorical_coefficients(
    ols_results: &mut OlsResult,
    predictor_names: &[String],
    shares: &ShareMap,
) -> Result<HashMap<String, f64>, OaxacaError> {
    let intercept_idx = predictor_names
        .iter()
        .position(|n| n == INTERCEPT_NAME)
        .ok_or_else(|| {
            OaxacaError::NormalizationError(format!(
                "design has no '{}' column to absorb the average",
                INTERCEPT_NAME
            ))
        })?;

    let mut base_coeffs = HashMap::new();
    for (var, factor) in shares {
        let mut dummy_indices: Vec<(usize, f64)> = Vec::with_capacity(factor.levels.len());
        for level in &factor.levels {
            if level.level == factor.base_level {
                continue;
            }
            let name = format!("{}_{}", var, level.level);
            let idx = predictor_names
                .iter()
                .position(|n| *n == name)
                .ok_or_else(|| {
                    OaxacaError::NormalizationError(format!(
                        "design has no dummy column '{}' for level '{}' of '{}'",
                        name, level.level, var
                    ))
                })?;
            dummy_indices.push((idx, level.share));
        }

        // The base level's raw coefficient is 0, so the restricted average is over dummies only.
        let weighted_mean: f64 = dummy_indices
            .iter()
            .map(|&(i, s)| s * ols_results.coefficients[i])
            .sum();

        base_coeffs.insert(var.clone(), -weighted_mean);
        ols_results.coefficients[intercept_idx] += weighted_mean;
        for &(i, _) in &dummy_indices {
            ols_results.coefficients[i] -= weighted_mean;
        }
    }
    Ok(base_coeffs)
}

/// What `run_metadata.normalization` carries: the convention, whether it was applied, and the
/// restriction weights of the point-estimate sample (bootstrap replicates recompute their own
/// from each replicate's pooled resample).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NormalizationRecord {
    pub convention: &'static str,
    /// What the shares count: `"observation-weights"` when a weights column is set,
    /// `"row-counts"` otherwise.
    pub share_basis: &'static str,
    pub applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<&'static str>,
    pub variables: Vec<FactorShares>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{DMatrix, DVector};

    fn ols_of(coeffs: Vec<f64>) -> OlsResult {
        let k = coeffs.len();
        OlsResult {
            coefficients: DVector::from_vec(coeffs),
            vcov: DMatrix::zeros(k, k),
            residuals: DVector::zeros(0),
        }
    }

    fn names() -> Vec<String> {
        vec![
            INTERCEPT_NAME.to_string(),
            "cat_B".to_string(),
            "cat_C".to_string(),
        ]
    }

    fn dummies() -> Vec<String> {
        vec!["cat_B".to_string(), "cat_C".to_string()]
    }

    #[test]
    fn equal_shares_reproduce_the_classic_transform() {
        // Intercept 10, A (base) = 0, B = 2, C = 4. Equal shares: mean = 2.
        let mut ols = ols_of(vec![10.0, 2.0, 4.0]);
        let sh = factor_shares(
            "cat",
            &["A", "B", "C"],
            None,
            &dummies(),
            NormalizationConvention::EqualShare,
        )
        .unwrap();
        let mut map = ShareMap::new();
        map.insert("cat".to_string(), sh);
        let base = normalize_categorical_coefficients(&mut ols, &names(), &map).unwrap();
        assert!((ols.coefficients[0] - 12.0).abs() < 1e-12);
        assert!((ols.coefficients[1] - 0.0).abs() < 1e-12);
        assert!((ols.coefficients[2] - 2.0).abs() < 1e-12);
        assert!((base["cat"] - (-2.0)).abs() < 1e-12);
    }

    #[test]
    fn population_shares_are_keyed_by_level_name_not_position() {
        // 6 A, 3 B, 1 C -> shares .6 .3 .1 (in that level order). Raw: A=0, B=2, C=4.
        // c = .3*2 + .1*4 = 1.0
        let levels = ["C", "A", "B", "A", "A", "B", "A", "A", "B", "A"];
        let sh = factor_shares(
            "cat",
            &levels,
            None,
            &dummies(),
            NormalizationConvention::PopulationShare,
        )
        .unwrap();
        assert_eq!(sh.base_level, "A");
        assert!((sh.share_of("A").unwrap() - 0.6).abs() < 1e-15);
        assert!((sh.share_of("B").unwrap() - 0.3).abs() < 1e-15);
        assert!((sh.share_of("C").unwrap() - 0.1).abs() < 1e-15);
        let mut ols = ols_of(vec![10.0, 2.0, 4.0]);
        let mut map = ShareMap::new();
        map.insert("cat".to_string(), sh);
        let base = normalize_categorical_coefficients(&mut ols, &names(), &map).unwrap();
        assert!((ols.coefficients[0] - 11.0).abs() < 1e-12);
        assert!((ols.coefficients[1] - 1.0).abs() < 1e-12);
        assert!((ols.coefficients[2] - 3.0).abs() < 1e-12);
        assert!((base["cat"] + 1.0).abs() < 1e-12);
        // Restriction holds over ALL levels: .6*(-1) + .3*1 + .1*3 = 0
        let restricted = 0.6 * base["cat"] + 0.3 * ols.coefficients[1] + 0.1 * ols.coefficients[2];
        assert!(restricted.abs() < 1e-12);
    }

    #[test]
    fn weights_replace_row_counts() {
        // Same levels, weights make C carry 50%.
        let levels = ["A", "B", "C"];
        let w = [1.0, 1.0, 2.0];
        let sh = factor_shares(
            "cat",
            &levels,
            Some(&w),
            &dummies(),
            NormalizationConvention::PopulationShare,
        )
        .unwrap();
        assert!((sh.share_of("C").unwrap() - 0.5).abs() < 1e-15);
        assert!((sh.share_of("A").unwrap() - 0.25).abs() < 1e-15);
    }

    #[test]
    fn a_missing_dummy_column_is_an_error_not_a_skip() {
        let sh = factor_shares(
            "cat",
            &["A", "B", "C"],
            None,
            &dummies(),
            NormalizationConvention::EqualShare,
        )
        .unwrap();
        let mut map = ShareMap::new();
        map.insert("cat".to_string(), sh);
        let mut ols = ols_of(vec![10.0, 2.0]);
        let short = vec![INTERCEPT_NAME.to_string(), "cat_B".to_string()];
        assert!(normalize_categorical_coefficients(&mut ols, &short, &map).is_err());
    }
}
