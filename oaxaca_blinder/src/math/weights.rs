//! Sample-weight semantics (0120-MERIDIAN S9 / T17).
//!
//! No single convention makes both of these true: "uniform fractional weights are a no-op" and
//! "a weight of 2 equals the row twice". A consultant has to say which they mean, so a weights
//! column always travels with a [`WeightsKind`]:
//!
//! * [`WeightsKind::Frequency`]: every weight is a non-negative integer count of identical
//!   employees (a headcount). The estimators treat `w = 2` as the row twice, and so does the
//!   bootstrap: a replicate draws `sum(w)` employees (multinomial, probability `w_i / sum(w)`) and
//!   the draw counts become the replicate's weights, so standard errors, intervals and p-values
//!   match the repeated rows. A fractional weight is refused, naming the row.
//!   [`WeightsKind::Relative`] weights are carried along with their row in a row-level bootstrap.
//! * [`WeightsKind::Relative`]: weights say how much one row counts against another (FTE, survey
//!   design weights). They are rescaled so the rows that carry weight sum to their own count, and
//!   the weighted quantile is a port of `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)`.
//!   Uniform weights, whatever their size, change nothing.

use crate::error::OaxacaError;

/// Which meaning a weights column carries. See the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeightsKind {
    /// Integer replication counts.
    Frequency,
    /// Relative importance, rescaled to sum to the row count.
    Relative,
}

impl WeightsKind {
    /// The accepted wire names, for error messages.
    pub const ACCEPTED_NAMES: [&'static str; 2] = ["frequency", "relative"];

    /// The wire name of this kind.
    pub fn canonical_name(self) -> &'static str {
        match self {
            WeightsKind::Frequency => "frequency",
            WeightsKind::Relative => "relative",
        }
    }

    /// Strict parse of a wire name: exactly `"frequency"` or `"relative"`, nothing else.
    pub fn parse_name(name: &str) -> Result<WeightsKind, OaxacaError> {
        match name {
            "frequency" => Ok(WeightsKind::Frequency),
            "relative" => Ok(WeightsKind::Relative),
            other => Err(OaxacaError::NormalizationError(format!(
                "UNKNOWN_WEIGHTS_KIND: got {:?}; weights_kind must be exactly one of: {}",
                other,
                WeightsKind::ACCEPTED_NAMES.join(", ")
            ))),
        }
    }
}

/// Checks one weight against the kind. `row` is the 0-based original data-row ordinal.
pub(crate) fn check_weight(
    column: &str,
    row: usize,
    value: f64,
    kind: WeightsKind,
) -> Result<(), OaxacaError> {
    let reason = if !value.is_finite() {
        Some("not finite")
    } else if value < 0.0 {
        Some("negative")
    } else if kind == WeightsKind::Frequency && value.fract() != 0.0 {
        Some("frequency weights must be whole numbers; use weights_kind = relative for fractional weights")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(OaxacaError::InvalidWeight {
            column: column.to_string(),
            row,
            value,
            reason: reason.to_string(),
        }),
        None => Ok(()),
    }
}

/// Rescales `weights` so they sum to their own count (`w * n / sum(w)`). A vector that sums to
/// zero is returned unchanged (the caller refuses it).
pub(crate) fn rescale_to_count(weights: &[f64]) -> Vec<f64> {
    let total: f64 = weights.iter().sum();
    if total > 0.0 && total.is_finite() {
        let n = weights.len() as f64;
        weights.iter().map(|w| w * n / total).collect()
    } else {
        weights.to_vec()
    }
}

/// `Hmisc::wtd.quantile(x, weights, probs = tau, type = "quantile")` on weights that are ALREADY
/// normalised (sorted ascending by `sorted_y`, zero-weight rows removed).
///
/// Hmisc collapses ties, takes `n = sum(w)` (fractional), `order = 1 + (n - 1) * tau`,
/// `low = max(floor(order), 1)`, `high = min(low + 1, n)`, and reads both ranks off the
/// cumulative weights with `approx(cumsum(w), x, method = "constant", f = 1, rule = 2)`: the
/// first value whose cumulative weight reaches the rank, clamped at both ends. The result is
/// `(1 - frac) * q(low) + frac * q(high)` with `frac = order %% 1`.
pub(crate) fn hmisc_quantile(sorted_y: &[f64], sorted_w: &[f64], tau: f64) -> f64 {
    let n_rows = sorted_y.len();
    debug_assert_eq!(n_rows, sorted_w.len());
    if n_rows == 0 {
        return f64::NAN;
    }
    let mut cum = Vec::with_capacity(n_rows);
    let mut acc = 0.0;
    for &w in sorted_w {
        acc += w;
        cum.push(acc);
    }
    let n = acc;
    let order = 1.0 + (n - 1.0) * tau;
    let low = order.floor().max(1.0);
    let high = (low + 1.0).min(n);
    let frac = order - order.floor();

    // approx(method = "constant", f = 1, rule = 2): the first j with cum[j] >= xout; below the
    // first cumulative weight and above the last, the end values.
    let at = |xout: f64| -> f64 {
        if xout <= cum[0] {
            return sorted_y[0];
        }
        if xout >= cum[n_rows - 1] {
            return sorted_y[n_rows - 1];
        }
        let j = cum.partition_point(|&c| c < xout);
        sorted_y[j]
    };
    (1.0 - frac) * at(low) + frac * at(high)
}

/// A weighted quantile under the stated semantics, sorted internally.
///
/// * `Frequency`: type 7 on the frequency-expanded sample (`w = 2` is the row twice). With every
///   weight 1 this is R `quantile(type = 7)`.
/// * `Relative`: `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)`.
///
/// Errors on a length mismatch, a non-finite or negative weight, a fractional weight under
/// `Frequency`, or weights that sum to zero.
pub fn weighted_quantile(
    values: &[f64],
    weights: &[f64],
    tau: f64,
    kind: WeightsKind,
) -> Result<f64, OaxacaError> {
    if values.len() != weights.len() {
        return Err(OaxacaError::InsufficientData(format!(
            "weighted_quantile: {} values but {} weights",
            values.len(),
            weights.len()
        )));
    }
    for (i, &w) in weights.iter().enumerate() {
        check_weight("weights", i, w, kind)?;
    }
    if !(0.0..=1.0).contains(&tau) {
        return Err(OaxacaError::InsufficientData(format!(
            "weighted_quantile: tau {tau} is outside [0, 1]"
        )));
    }
    let mut pairs: Vec<(f64, f64)> = values
        .iter()
        .copied()
        .zip(weights.iter().copied())
        .filter(|(_, w)| *w > 0.0)
        .collect();
    if pairs.is_empty() {
        return Err(OaxacaError::InsufficientData(
            "weighted_quantile: no row carries positive weight".to_string(),
        ));
    }
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let sy: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    let sw: Vec<f64> = pairs.iter().map(|p| p.1).collect();
    Ok(match kind {
        WeightsKind::Frequency => crate::math::rif::weighted_quantile_frequency(&sy, &sw, tau),
        WeightsKind::Relative => hmisc_quantile(&sy, &rescale_to_count(&sw), tau),
    })
}
