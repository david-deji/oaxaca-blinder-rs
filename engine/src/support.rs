//! Support, small-sample and percentile diagnostics, and the prediction interval
//! (0120-MERIDIAN S6 / S7 / S8; T12, T13, T14, T16).
//!
//! Everything here is a number plus the threshold it was held against. Wording is the app's.
//!
//! "Baseline group" is the group whose pay line the fair wage extends: the reference group. In
//! the engine's matrices it is the one called `reference`; the compared group is `target`.

use crate::types::*;
use nalgebra::{DMatrix, DVector};
use statrs::distribution::{ContinuousCDF, StudentsT};

/// A defensibility verdict allows one cent of slack below the interval floor, not the old $1.
pub const DEFENSIBLE_TOLERANCE: f64 = 0.01;

/// The interval level when a request gives none.
pub const DEFAULT_CONFIDENCE: f64 = 0.95;

/// Lowest accepted `confidence_level`.
pub const MIN_CONFIDENCE: f64 = 0.50;
/// Highest accepted `confidence_level`.
pub const MAX_CONFIDENCE: f64 = 0.999;

/// The level a request asks for: absent is 0.95; a value that is not finite or lies outside
/// [0.50, 0.999] is refused by name (`INVALID_CONFIDENCE_LEVEL`). A level given as a percentage
/// (95) used to clamp silently to 0.999, a far wider claim than the one asked for.
pub fn resolve_confidence(requested: Option<f64>) -> Result<f64, String> {
    match requested {
        None => Ok(DEFAULT_CONFIDENCE),
        Some(c) if c.is_finite() && (MIN_CONFIDENCE..=MAX_CONFIDENCE).contains(&c) => Ok(c),
        Some(c) => Err(format!(
            "INVALID_CONFIDENCE_LEVEL: confidence_level={c}; give a fraction between \
             {MIN_CONFIDENCE} and {MAX_CONFIDENCE} (0.95 for 95%), or leave it out for 0.95"
        )),
    }
}

/// The named refusal for a fitted group with no residual degrees of freedom (T13). A fair range
/// built from a regression with no residual information is false precision: the old code
/// returned a zero-width interval, which over-adjusts under `LowerBound`.
pub fn insufficient_df_error(group: &str, rows: usize, columns: usize) -> String {
    format!(
        "INSUFFICIENT_RESIDUAL_DF: group={group}, rows={rows}, model_columns={columns}, \
         residual_df={}; the regression needs more analysed rows than model columns",
        rows as i64 - columns as i64
    )
}

/// Two-sided p-value of a t statistic on `dof` degrees of freedom, `2 * pt(-|t|, dof)`
/// (0120-MERIDIAN T14). Computed from the lower tail so a large `t` keeps its precision.
pub fn two_sided_p(t: f64, dof: f64) -> f64 {
    match StudentsT::new(0.0, 1.0, dof) {
        Ok(dist) => 2.0 * dist.cdf(-t.abs()),
        Err(_) => f64::NAN,
    }
}

/// R `quantile(type = 7)` of an ascending-sorted sample. Exact on ties: when the two bracketing
/// values are equal the result is that value, bit for bit.
pub fn type7(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    let index = (n as f64 - 1.0) * p;
    let lo = index.floor();
    let hi = index.ceil();
    let lo_v = sorted[lo as usize];
    let hi_v = sorted[hi as usize];
    if lo_v == hi_v {
        return lo_v;
    }
    let h = index - lo;
    (1.0 - h) * lo_v + h * hi_v
}

fn sorted_copy(values: &[f64]) -> Vec<f64> {
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v
}

fn sample_variance(values: &[f64]) -> Option<f64> {
    let n = values.len();
    if n < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    Some(values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0))
}

/// Leverage `x' (X'X)^-1 x` against the baseline group's design.
pub struct Leverage {
    cov: DMatrix<f64>,
    /// The largest leverage among the baseline group's own rows.
    pub h_max: f64,
}

impl Leverage {
    /// `None` when `X'X` is singular (the callers that need the inverse report that themselves).
    pub fn from_design(x: &DMatrix<f64>) -> Option<Leverage> {
        let cov = (x.transpose() * x).try_inverse()?;
        Some(Leverage::with_cov(x, cov))
    }

    /// From an inverse the caller already holds.
    pub fn with_cov(x: &DMatrix<f64>, cov: DMatrix<f64>) -> Leverage {
        let xc = x * &cov;
        let mut h_max = 0.0_f64;
        for i in 0..x.nrows() {
            let h: f64 = (0..x.ncols()).map(|j| xc[(i, j)] * x[(i, j)]).sum();
            h_max = h_max.max(h);
        }
        Leverage { cov, h_max }
    }

    pub fn at(&self, features: &DVector<f64>) -> f64 {
        (features.transpose() * &self.cov * features)[(0, 0)]
    }

    /// True when `h` is larger than any baseline row's leverage, beyond rounding.
    pub fn exceeds_baseline(&self, h: f64) -> bool {
        h > self.h_max * (1.0 + 1e-9) + 1e-12
    }

    pub fn is_extrapolated(&self, features: &DVector<f64>) -> bool {
        self.exceeds_baseline(self.at(features))
    }
}

/// Student-t prediction intervals of the baseline regression (T14): `predict.lm(interval =
/// "prediction")`. The standard error is `sqrt(sigma^2 (1 + x' (X'X)^-1 x))` and the multiplier
/// the t quantile on `n - k` degrees of freedom.
pub struct IntervalModel {
    sigma_squared: f64,
    leverage: Leverage,
    critical: f64,
    pub basis: IntervalBasis,
}

impl IntervalModel {
    /// Refuses (named error) when the baseline regression has no residual degrees of freedom.
    pub fn new(
        x: &DMatrix<f64>,
        y: &DVector<f64>,
        beta: &DVector<f64>,
        confidence: f64,
    ) -> Result<IntervalModel, String> {
        let n = y.len();
        let k = x.ncols();
        if n <= k {
            return Err(insufficient_df_error("reference", n, k));
        }
        let residuals = y - x * beta;
        let rss = residuals.dot(&residuals);
        let dof = n - k;
        let sigma_squared = rss / dof as f64;
        let cov = (x.transpose() * x)
            .try_inverse()
            .ok_or("Covariance matrix is singular, likely due to perfect multicollinearity.")?;
        let leverage = Leverage::with_cov(x, cov);
        let alpha = 1.0 - confidence;
        let critical = StudentsT::new(0.0, 1.0, dof as f64)
            .map_err(|e| format!("Student t quantile: {e}"))?
            .inverse_cdf(1.0 - alpha / 2.0);
        Ok(IntervalModel {
            sigma_squared,
            leverage,
            critical,
            basis: IntervalBasis {
                confidence_level: confidence,
                degrees_of_freedom: dof,
                critical_value: critical,
            },
        })
    }

    /// `(lower, upper)` prediction bounds around `predicted` for one feature vector. A baseline
    /// fit with no residual variance at all (a perfect line) has a zero-width interval.
    pub fn interval(&self, features: &DVector<f64>, predicted: f64) -> (f64, f64) {
        if self.sigma_squared <= 1e-9 {
            return (predicted, predicted);
        }
        let h = self.leverage.at(features);
        let se = (self.sigma_squared * (1.0 + h)).sqrt();
        let margin = self.critical * se;
        (predicted - margin, predicted + margin)
    }

    pub fn is_extrapolated(&self, features: &DVector<f64>) -> bool {
        self.leverage.is_extrapolated(features)
    }

    pub fn leverage(&self) -> &Leverage {
        &self.leverage
    }
}

/// Which groups the result's regression fits. Only fitted groups are judged on residual df.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fitted {
    /// The baseline group only (optimise, defensibility).
    Reference,
    /// Both groups (decompose, verify).
    Both,
}

/// Support diagnostics for one result, plus the warnings they raise.
///
/// Refuses with `INSUFFICIENT_RESIDUAL_DF` when a fitted group has no residual degrees of
/// freedom. `leverage` is the baseline design's, when the caller already built it.
pub fn support_diagnostics(
    x_reference: &DMatrix<f64>,
    x_target: &DMatrix<f64>,
    feature_names: &[String],
    continuous: &[String],
    fitted: Fitted,
    leverage: Option<&Leverage>,
) -> Result<(SupportDiagnostics, Vec<DiagnosticWarning>), String> {
    let n_ref = x_reference.nrows();
    let n_tgt = x_target.nrows();
    let k = x_reference.ncols();
    let ref_df = n_ref as i64 - k as i64;
    let tgt_df = n_tgt as i64 - k as i64;
    if ref_df <= 0 {
        return Err(insufficient_df_error("reference", n_ref, k));
    }
    if fitted == Fitted::Both && tgt_df <= 0 {
        return Err(insufficient_df_error("target", n_tgt, k));
    }

    let mut warnings = Vec::new();
    let mut predictors = Vec::new();
    for name in continuous {
        let Some(col) = feature_names.iter().position(|f| f == name) else {
            continue;
        };
        let a: Vec<f64> = (0..n_ref).map(|i| x_reference[(i, col)]).collect();
        let b: Vec<f64> = (0..n_tgt).map(|i| x_target[(i, col)]).collect();
        let sorted_a = sorted_copy(&a);
        let (a_min, a_max) = (sorted_a[0], sorted_a[sorted_a.len() - 1]);
        let (p01, p99) = (type7(&sorted_a, 0.01), type7(&sorted_a, 0.99));
        let b_min = b.iter().copied().fold(f64::INFINITY, f64::min);
        let b_max = b.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let share = |pred: &dyn Fn(f64) -> bool| -> f64 {
            if b.is_empty() {
                0.0
            } else {
                b.iter().filter(|v| pred(**v)).count() as f64 / b.len() as f64
            }
        };
        let outside_range = share(&|v| v < a_min || v > a_max);
        let outside_p = share(&|v| v < p01 || v > p99);

        let normalised_difference = match (sample_variance(&a), sample_variance(&b)) {
            (Some(va), Some(vb)) => {
                let s = ((va + vb) / 2.0).sqrt();
                if s > 0.0 {
                    let mean_a = a.iter().sum::<f64>() / a.len() as f64;
                    let mean_b = b.iter().sum::<f64>() / b.len() as f64;
                    Some((mean_b - mean_a) / s)
                } else {
                    None
                }
            }
            _ => None,
        };

        if outside_range > SUPPORT_OUTSIDE_RANGE_SHARE {
            warnings.push(DiagnosticWarning {
                code: WarningCode::OutsideRange,
                subject: Some(name.clone()),
                value: outside_range,
                threshold: SUPPORT_OUTSIDE_RANGE_SHARE,
            });
        }
        if let Some(d) = normalised_difference {
            if d.abs() > SUPPORT_NORMALISED_DIFFERENCE {
                warnings.push(DiagnosticWarning {
                    code: WarningCode::NormalisedDifference,
                    subject: Some(name.clone()),
                    value: d,
                    threshold: SUPPORT_NORMALISED_DIFFERENCE,
                });
            }
        }
        predictors.push(PredictorSupport {
            name: name.clone(),
            reference_min: a_min,
            reference_max: a_max,
            reference_p01: p01,
            reference_p99: p99,
            target_min: b_min,
            target_max: b_max,
            target_outside_range_share: outside_range,
            target_outside_p01_p99_share: outside_p,
            normalised_difference,
        });
    }

    for (group, df, fits) in [
        ("reference", ref_df, true),
        ("target", tgt_df, fitted == Fitted::Both),
    ] {
        if fits && df < SUPPORT_MIN_RESIDUAL_DF {
            warnings.push(DiagnosticWarning {
                code: WarningCode::FewResidualDf,
                subject: Some(group.to_string()),
                value: df as f64,
                threshold: SUPPORT_MIN_RESIDUAL_DF as f64,
            });
        }
    }

    let owned;
    let lev = match leverage {
        Some(l) => Some(l),
        None => {
            owned = Leverage::from_design(x_reference);
            owned.as_ref()
        }
    };
    let extrapolated_target_count = match lev {
        Some(l) => (0..n_tgt)
            .filter(|&i| l.is_extrapolated(&x_target.row(i).transpose()))
            .count(),
        None => 0,
    };

    Ok((
        SupportDiagnostics {
            reference_count: n_ref,
            target_count: n_tgt,
            model_columns: k,
            reference_residual_df: ref_df,
            target_residual_df: tgt_df,
            predictors,
            extrapolated_target_count,
        },
        warnings,
    ))
}

fn group_report(
    values: &[f64],
    tau: f64,
    label: &str,
) -> (QuantileGroupReport, Vec<DiagnosticWarning>) {
    let sorted = sorted_copy(values);
    let n = sorted.len() as f64;
    let q = type7(&sorted, tau);
    let at_or_below = sorted.iter().filter(|v| **v <= q).count() as f64;
    let tied = sorted.iter().filter(|v| **v == q).count() as f64;
    let ecdf = if n > 0.0 { at_or_below / n } else { f64::NAN };
    let tie_share = if n > 0.0 { tied / n } else { 0.0 };
    let offset = ecdf - tau;
    let mut warnings = Vec::new();
    if tie_share > QUANTILE_TIE_SHARE {
        warnings.push(DiagnosticWarning {
            code: WarningCode::TieShare,
            subject: Some(label.to_string()),
            value: tie_share,
            threshold: QUANTILE_TIE_SHARE,
        });
    }
    // A type-7 percentile sits between two order statistics, so even with no tied value
    // F_n(q_tau) - tau lies in (-tau/n, (1 - tau)/n]: up to 1/n by discreteness alone. A group of
    // 23 distinct salaries is off by 0.03 at the median and is not a step grid. The line is the
    // larger of the 0.01 of T16 and 1/n; `tie_share` is the step-grid signal.
    let discreteness = if n > 0.0 { 1.0 / n } else { 0.0 };
    let offset_line = QUANTILE_ECDF_OFFSET.max(discreteness);
    if offset.abs() > offset_line {
        warnings.push(DiagnosticWarning {
            code: WarningCode::EcdfOffset,
            subject: Some(label.to_string()),
            value: offset,
            threshold: offset_line,
        });
    }
    (
        QuantileGroupReport {
            count: sorted.len(),
            quantile_value: q,
            ecdf_at_quantile: ecdf,
            ecdf_offset: offset,
            tie_share,
        },
        warnings,
    )
}

/// The percentile report of a RIF run (T16): the type-7 percentile gap per group, beside the
/// RIF total the model decomposes, with tie and ECDF diagnostics for each group.
pub fn quantile_report(
    tau: f64,
    y_reference: &[f64],
    y_target: &[f64],
    rif_total: f64,
) -> (QuantileReport, Vec<DiagnosticWarning>) {
    let (reference, mut warnings) = group_report(y_reference, tau, "reference");
    let (target, target_warnings) = group_report(y_target, tau, "target");
    warnings.extend(target_warnings);
    (
        QuantileReport {
            tau,
            quantile_gap: target.quantile_value - reference.quantile_value,
            rif_total,
            reference,
            target,
        },
        warnings,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type7_matches_the_textbook_definition() {
        let x = [1.0, 2.0, 4.0, 8.0, 16.0];
        assert_eq!(type7(&x, 0.0), 1.0);
        assert_eq!(type7(&x, 0.5), 4.0);
        assert_eq!(type7(&x, 1.0), 16.0);
        // index 1 + 4 * 0.25 = 2 -> exactly x[1]
        assert_eq!(type7(&x, 0.25), 2.0);
        // index 4 * 0.1 = 0.4 -> 1 + 0.4 * (2 - 1)
        assert!((type7(&x, 0.1) - 1.4).abs() < 1e-15);
    }

    #[test]
    fn type7_is_exact_between_equal_neighbours() {
        let x = [0.1, 0.1, 0.1, 0.1, 0.7];
        assert_eq!(type7(&x, 0.3).to_bits(), 0.1_f64.to_bits());
    }

    #[test]
    fn confidence_is_refused_when_out_of_range_and_defaults_to_95() {
        assert_eq!(resolve_confidence(None), Ok(0.95));
        assert_eq!(resolve_confidence(Some(0.99)), Ok(0.99));
        assert_eq!(resolve_confidence(Some(0.50)), Ok(0.50));
        assert_eq!(resolve_confidence(Some(0.999)), Ok(0.999));
        for bad in [95.0, f64::NAN, f64::INFINITY, 0.4, 1.0, -0.5] {
            let e = resolve_confidence(Some(bad)).unwrap_err();
            assert!(e.starts_with("INVALID_CONFIDENCE_LEVEL"), "{bad}: {e}");
        }
    }

    #[test]
    fn a_perfect_line_has_a_zero_width_interval_and_a_named_refusal_below_it() {
        let x = DMatrix::from_row_slice(4, 2, &[1.0, 0.0, 1.0, 1.0, 1.0, 2.0, 1.0, 3.0]);
        let y = DVector::from_vec(vec![1.0, 3.0, 5.0, 7.0]);
        let beta = DVector::from_vec(vec![1.0, 2.0]);
        let m = IntervalModel::new(&x, &y, &beta, 0.95).unwrap();
        let (lo, hi) = m.interval(&DVector::from_vec(vec![1.0, 1.5]), 4.0);
        assert_eq!((lo, hi), (4.0, 4.0));
        let x2 = DMatrix::from_row_slice(2, 2, &[1.0, 0.0, 1.0, 1.0]);
        let y2 = DVector::from_vec(vec![1.0, 3.0]);
        let err = IntervalModel::new(&x2, &y2, &beta, 0.95).err().unwrap();
        assert!(
            err.starts_with("INSUFFICIENT_RESIDUAL_DF: group=reference, rows=2"),
            "{err}"
        );
    }
}
