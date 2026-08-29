use polars::prelude::*;
use std::f64::consts::PI;

/// Calculates the Recentered Influence Function (RIF) for a given quantile.
///
/// # Arguments
///
/// * `series` - The outcome variable series.
/// * `quantile` - The target quantile (between 0.0 and 1.0).
///
/// # Returns
///
/// A `Result` containing the RIF series or a `PolarsError`.
pub fn calculate_rif(series: &Series, quantile: f64) -> Result<Series, PolarsError> {
    calculate_rif_weighted(series, quantile, None)
}

/// Weighted quantile, defined as R type 7 on the frequency-expanded sample (0097).
///
/// Conceptually: repeat each value `w_i` times, then take the ordinary type-7 quantile of that
/// expanded array — without ever materialising it. `h = (W - 1) * tau` indexes into the expansion,
/// and `value_at_rank` maps an expanded rank back to its source value through the cumulative
/// weights.
///
/// Three properties, all of which the tests pin:
///   - **Reduces to type 7 exactly** under unit weights, so the unweighted path cannot move.
///   - **Exact for frequency weights**: `w = 2` on one row gives the same answer as that row
///     appearing twice, which is what a headcount or FTE weight means.
///   - **Monotone**: moving mass onto larger values raises the quantile. An earlier draft here
///     normalised by `W - w_last`, which silently cancelled the largest observation's weight
///     entirely — `upweighting_the_high_tail_raises_the_median` is the test that caught it.
fn weighted_quantile(sorted_y: &[f64], sorted_w: &[f64], quantile: f64) -> f64 {
    let n = sorted_y.len();
    if n == 1 {
        return sorted_y[0];
    }
    let total: f64 = sorted_w.iter().sum();
    if total <= 1.0 {
        return sorted_y[0];
    }
    // Cumulative weight through each sorted value; `cum[j]` is the exclusive upper expanded rank.
    let mut cum = Vec::with_capacity(n);
    let mut acc = 0.0;
    for &w in sorted_w {
        acc += w;
        cum.push(acc);
    }
    let value_at_rank = |r: f64| -> f64 {
        for (j, &c) in cum.iter().enumerate() {
            if r < c {
                return sorted_y[j];
            }
        }
        sorted_y[n - 1]
    };

    let h = (total - 1.0) * quantile.clamp(0.0, 1.0);
    let lo = value_at_rank(h.floor());
    let hi = value_at_rank(h.ceil());
    let frac = h - h.floor();
    lo + frac * (hi - lo)
}

/// RIF with optional sample weights.
///
/// 0097 — `calculate_rif` took no weights, while `OaxacaBuilder` honours `weights_col` in
/// `clean_dataframe`, `weighted_levels_present` and `ols()`. A weighted `decompose_quantile`
/// therefore computed an UNWEIGHTED RIF transform and then ran a WEIGHTED regression on it: finite,
/// plausible, silent, and wrong. Pay-equity data is weighted data (headcount, FTE, stratified
/// samples), so this was the normal case, not an exotic one.
///
/// Every weight-sensitive ingredient is threaded: the sample quantile, the Silverman bandwidth's
/// mean/variance and IQR, the kernel sum, and the sample size in `n^-0.2` (Kish's effective N,
/// `(Σw)² / Σw²`, which is exactly `n` under unit weights). Passing `None` reproduces the previous
/// arithmetic bit for bit — the unweighted path is unchanged, and that is asserted, not assumed.
pub fn calculate_rif_weighted(
    series: &Series,
    quantile: f64,
    weights: Option<&[f64]>,
) -> Result<Series, PolarsError> {
    let y_vec: Vec<f64> = series.f64()?.into_no_null_iter().collect();
    let n = y_vec.len() as f64;

    if let Some(w) = weights {
        if w.len() != y_vec.len() {
            return Err(PolarsError::ComputeError(
                format!(
                    "RIF weights length {} does not match outcome length {}",
                    w.len(),
                    y_vec.len()
                )
                .into(),
            ));
        }
        if w.iter().any(|&x| !(x.is_finite()) || x < 0.0) {
            return Err(PolarsError::ComputeError(
                "RIF weights must be finite and non-negative".into(),
            ));
        }
        if w.iter().sum::<f64>() <= 0.0 {
            return Err(PolarsError::ComputeError(
                "RIF weights must sum to a positive total".into(),
            ));
        }
    }
    let w_vec: Vec<f64> = match weights {
        Some(w) => w.to_vec(),
        None => vec![1.0; y_vec.len()],
    };

    if n < 2.0 {
        // REACHABLE via the public API: `OaxacaBuilder::split_groups` (builder.rs) only
        // requires >=2 DISTINCT group values in the full dataset -- it enforces no per-group
        // minimum row count, so a group with exactly one row surviving `clean_dataframe`'s
        // null-drop reaches this branch through `decompose_quantile` -> `rif_replace_outcome`
        // (builder.rs), both for the point estimate and for every bootstrap replicate.
        // Sample variance and the KDE bandwidth are both undefined for n<2, so silently
        // returning the caller's own series as its "RIF" would be a silent wrong result
        // (agentic failure mode #6) rather than a safe no-op. Fail loudly instead.
        return Err(PolarsError::ComputeError(
            format!(
                "RIF density estimation requires at least 2 observations per group, got {}",
                n as usize
            )
            .into(),
        ));
    }

    // Sort the outcome and carry each observation's weight with it — the weighted quantile and the
    // weighted IQR both need the pairing preserved.
    let mut pairs: Vec<(f64, f64)> = y_vec.iter().copied().zip(w_vec.iter().copied()).collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let sorted_y: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    let sorted_w: Vec<f64> = pairs.iter().map(|p| p.1).collect();

    let w_total: f64 = w_vec.iter().sum();

    // 1. Sample quantile (Q_tau). Weighted type-7 positions; identical to the previous
    //    `(n-1)*tau` interpolation when every weight is 1.
    let q_tau = weighted_quantile(&sorted_y, &sorted_w, quantile);

    // 2. Density at Q_tau, Gaussian kernel, Silverman bandwidth — every ingredient weighted.
    let mean = y_vec
        .iter()
        .zip(w_vec.iter())
        .map(|(x, w)| x * w)
        .sum::<f64>()
        / w_total;
    // Frequency-weight variance: divides by (W - 1), which is (n - 1) under unit weights.
    let var_denom = if w_total > 1.0 { w_total - 1.0 } else { 1.0 };
    let variance = y_vec
        .iter()
        .zip(w_vec.iter())
        .map(|(x, w)| w * (x - mean).powi(2))
        .sum::<f64>()
        / var_denom;
    let std_dev = variance.sqrt();

    let iqr = weighted_quantile(&sorted_y, &sorted_w, 0.75)
        - weighted_quantile(&sorted_y, &sorted_w, 0.25);

    let min_spread = if iqr > 1e-8 {
        std_dev.min(iqr / 1.34)
    } else {
        std_dev
    };
    // Fallback if spread is zero (all values same)
    let min_spread = if min_spread < 1e-8 { 1.0 } else { min_spread };

    // Kish's effective sample size — (Sum w)^2 / Sum w^2 — which is exactly `n` under unit weights,
    // so the bandwidth is unchanged on the unweighted path.
    let sum_w_sq: f64 = w_vec.iter().map(|w| w * w).sum();
    let n_eff = if sum_w_sq > 0.0 {
        (w_total * w_total) / sum_w_sq
    } else {
        n
    };

    let h = 0.9 * min_spread * n_eff.powf(-0.2);

    // Weighted Gaussian KDE:  f(x) = (1 / (W * h)) * sum( w_i * K((x - Xi) / h) )
    let density: f64 = y_vec
        .iter()
        .zip(w_vec.iter())
        .map(|(&yi, &wi)| {
            let u = (q_tau - yi) / h;
            wi * (1.0 / (2.0 * PI).sqrt()) * (-0.5 * u.powi(2)).exp()
        })
        .sum::<f64>()
        / (w_total * h);

    // Avoid division by zero or extremely small density
    let density = if density < 1e-8 { 1e-8 } else { density };

    // 3. Calculate RIF for each observation
    // RIF(y; Q_tau) = Q_tau + (tau - I(y <= Q_tau)) / f(Q_tau)
    let rif_values: Vec<f64> = y_vec
        .iter()
        .map(|&yi| {
            let indicator = if yi <= q_tau { 1.0 } else { 0.0 };
            q_tau + (quantile - indicator) / density
        })
        .collect();

    Ok(Series::new(series.name().clone(), rif_values))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[f64]) -> Series {
        Series::new("y".into(), v.to_vec())
    }
    fn vals(r: &Series) -> Vec<f64> {
        r.f64().unwrap().into_no_null_iter().collect()
    }

    #[test]
    fn weighted_quantile_matches_type_7_when_weights_are_equal() {
        // The backward-compatibility anchor: unit weights must reproduce R type 7 exactly, so the
        // unweighted path cannot move when the weighted one is introduced.
        let y = vec![1.0, 2.0, 4.0, 8.0, 16.0];
        let w = vec![1.0; 5];
        for &tau in &[0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let h = (y.len() as f64 - 1.0) * tau;
            let (lo, hi) = (h.floor() as usize, h.ceil() as usize);
            let type7 = if lo == hi {
                y[lo]
            } else {
                y[lo] + (h - h.floor()) * (y[hi] - y[lo])
            };
            let got = weighted_quantile(&y, &w, tau);
            assert!(
                (got - type7).abs() < 1e-12,
                "tau={tau}: {got} != type7 {type7}"
            );
        }
    }

    #[test]
    fn unit_weights_reproduce_the_unweighted_rif_bit_for_bit() {
        let y = s(&[3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0]);
        let bare = vals(&calculate_rif(&y, 0.5).unwrap());
        let ones = vals(&calculate_rif_weighted(&y, 0.5, Some(&[1.0; 8])).unwrap());
        assert_eq!(
            bare, ones,
            "unit weights must not perturb the unweighted arithmetic"
        );
    }

    #[test]
    fn weights_actually_change_the_transform() {
        // The defect: before 0097 these two were identical, because the weights never reached here.
        let y = s(&[1.0, 2.0, 3.0, 10.0, 20.0, 30.0]);
        let bare = vals(&calculate_rif(&y, 0.5).unwrap());
        let heavy_tail =
            vals(&calculate_rif_weighted(&y, 0.5, Some(&[1.0, 1.0, 1.0, 9.0, 9.0, 9.0])).unwrap());
        assert_ne!(
            bare, heavy_tail,
            "weights must move the RIF; identical output means they were dropped"
        );
    }

    #[test]
    fn upweighting_the_high_tail_raises_the_median() {
        // Direction check on the weighted quantile itself, independent of the KDE.
        let y = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let flat = weighted_quantile(&y, &[1.0; 5], 0.5);
        let top_heavy = weighted_quantile(&y, &[1.0, 1.0, 1.0, 1.0, 20.0], 0.5);
        assert!(
            top_heavy > flat,
            "mass on the top value must pull the median up: {top_heavy} !> {flat}"
        );
    }

    #[test]
    fn an_integer_weight_equals_repeating_the_row() {
        // What a headcount or FTE weight *means*. Exact by construction under the
        // frequency-expansion definition, and the strongest single check on it.
        let y = vec![1.0, 2.0, 3.0, 7.0];
        let w = vec![1.0, 3.0, 2.0, 1.0];
        let expanded = vec![1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 7.0];
        let ones = vec![1.0; expanded.len()];
        for &tau in &[0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let a = weighted_quantile(&y, &w, tau);
            let b = weighted_quantile(&expanded, &ones, tau);
            assert!(
                (a - b).abs() < 1e-12,
                "tau={tau}: weighted {a} != expanded {b}"
            );
        }
    }

    #[test]
    fn malformed_weights_are_refused_rather_than_silently_normalised() {
        let y = s(&[1.0, 2.0, 3.0]);
        assert!(
            calculate_rif_weighted(&y, 0.5, Some(&[1.0, 1.0])).is_err(),
            "length mismatch"
        );
        assert!(
            calculate_rif_weighted(&y, 0.5, Some(&[1.0, -1.0, 1.0])).is_err(),
            "negative weight"
        );
        assert!(
            calculate_rif_weighted(&y, 0.5, Some(&[0.0, 0.0, 0.0])).is_err(),
            "zero total"
        );
        assert!(
            calculate_rif_weighted(&y, 0.5, Some(&[1.0, f64::NAN, 1.0])).is_err(),
            "non-finite"
        );
    }
}
