//! This module contains functions for statistical inference, primarily bootstrapping.

/// Smallest replication count at which the two-sided 95% percentile interval has *interior*
/// endpoints. Below it both tails collapse onto the sample extremes and the returned pair is the
/// full range of the draws — a 100% interval wearing a 95% label.
///
/// Derivation, from the index arithmetic below: the lower endpoint is interior when
/// `floor(0.025n) >= 1`, i.e. `n >= 40`; the upper endpoint is interior when
/// `floor(0.975n) <= n - 2`, i.e. `n > 40`. Both hold from 41. (41 is the structural floor, not a
/// recommendation — a percentile bootstrap wants far more draws before its tails mean anything.)
pub const MIN_PERCENTILE_CI_REPS: usize = 41;

/// Calculates the standard error, p-value, and confidence interval from a vector of bootstrap estimates.
///
/// INV-02: summation here stays sequential (`iter().sum`) — do NOT introduce a rayon
/// reduce/fold. Float addition is non-associative; a parallel reduce reorders partial sums
/// and breaks bit-identity across thread counts. The input is the indexed, order-fixed
/// `estimates` vector built by the caller.
///
/// `_point_estimate` is unused and that is correct for this estimator: the **percentile** method
/// reads its interval straight off the ordered bootstrap distribution and never references the
/// full-sample estimate. A bias-corrected interval (BC / BCa) does — it needs the point estimate to
/// compute the bias-correction `z0` — so the parameter is kept rather than dropped, and this note
/// exists so the underscore reads as a deliberate choice rather than a discarded ingredient.
///
/// Returns `NaN` rather than a number in two cases, both deliberate (`rif.rs`'s `n < 2` branch sets
/// the house posture: refuse loudly rather than return a plausible wrong figure):
///   - fewer than 2 estimates: the variance denominator `n - 1` is zero, so there is no standard
///     error to report. Previously `0.0 / 0.0` produced NaN by accident at `n == 1`; it is now
///     produced on purpose, alongside a NaN p-value and interval.
///   - fewer than `MIN_PERCENTILE_CI_REPS`: the interval only, since the standard error and the
///     p-value remain meaningful (merely coarse) at low replication counts.
pub fn bootstrap_stats(estimates: &[f64], _point_estimate: f64) -> (f64, f64, (f64, f64)) {
    // n < 2 has no variance denominator: refuse the whole triple rather than emit 0.0/0.0.
    if estimates.len() < 2 {
        return (f64::NAN, f64::NAN, (f64::NAN, f64::NAN));
    }
    // Standard error is the standard deviation of the bootstrap estimates.
    let n = estimates.len() as f64;
    let mean: f64 = estimates.iter().sum::<f64>() / n;
    let std_err = (estimates
        .iter()
        .map(|&val| (val - mean).powi(2))
        .sum::<f64>()
        / (n - 1.0))
        .sqrt();

    // p-value: Two-tailed test for H0: theta = 0
    // We calculate the proportion of bootstrap estimates that are on the opposite side of zero
    // relative to the majority, multiplied by 2.
    let prop_positive = estimates.iter().filter(|&&val| val >= 0.0).count() as f64 / n;
    let prop_negative = estimates.iter().filter(|&&val| val <= 0.0).count() as f64 / n;
    let p_value = (2.0 * prop_positive.min(prop_negative)).min(1.0);

    // Confidence interval using the percentile method. Refuse below the structural floor: with
    // fewer draws both endpoints are the sample extremes, so the pair would describe the full
    // range of the draws while being read as a 95% interval.
    if estimates.len() < MIN_PERCENTILE_CI_REPS {
        return (std_err, p_value, (f64::NAN, f64::NAN));
    }
    let mut sorted_estimates = estimates.to_vec();
    sorted_estimates.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lower_idx = (0.025 * n).floor() as usize;
    let upper_idx = ((0.975 * n).floor() as usize).min(estimates.len().saturating_sub(1));
    let ci_lower = sorted_estimates.get(lower_idx).copied().unwrap_or(f64::NAN);
    let ci_upper = sorted_estimates.get(upper_idx).copied().unwrap_or(f64::NAN);

    (std_err, p_value, (ci_lower, ci_upper))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_stats_p_value() {
        // Case 1: Estimates are all positive (far from 0). p-value should be 0.
        let estimates = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let (_, p_val, _) = bootstrap_stats(&estimates, 3.0);
        assert_eq!(p_val, 0.0);

        // Case 2: Estimates are centered around 0. p-value should be high (~1.0).
        let estimates = vec![-2.0, -1.0, 0.0, 1.0, 2.0];
        let (_, p_val, _) = bootstrap_stats(&estimates, 0.0);
        assert!((p_val - 1.0).abs() < 1e-9);

        // Case 3: Estimates are mostly positive but some cross 0.
        // 1 negative out of 5 -> prop_neg = 0.2. p-value = 2 * 0.2 = 0.4.
        let estimates = vec![-1.0, 1.0, 2.0, 3.0, 4.0];
        let (_, p_val, _) = bootstrap_stats(&estimates, 2.0);
        assert!((p_val - 0.4).abs() < 1e-9);
    }

    // 0097 — the interval, which nothing verified before this issue.

    #[test]
    fn a_single_estimate_has_no_variance_and_says_so() {
        // Was 0.0 / 0.0 = NaN by accident; now NaN on purpose, across the whole triple.
        let (se, p, (lo, hi)) = bootstrap_stats(&[3.0], 3.0);
        assert!(se.is_nan(), "n == 1 has no variance denominator");
        assert!(p.is_nan());
        assert!(lo.is_nan() && hi.is_nan());
    }

    #[test]
    fn below_the_floor_the_interval_is_refused_not_degraded() {
        // The defect this guard exists to remove: at 10 draws the old code returned
        // (min, max) — the full range of the draws — and the caller read it as 95%.
        let estimates: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let (se, p, (lo, hi)) = bootstrap_stats(&estimates, 4.5);
        assert!(
            lo.is_nan() && hi.is_nan(),
            "a 10-draw percentile interval is not a 95% interval"
        );
        assert!(
            se.is_finite(),
            "the standard error stays meaningful at low n"
        );
        assert!(p.is_finite(), "so does the p-value, merely coarse");
    }

    #[test]
    fn the_floor_is_where_both_endpoints_become_interior() {
        // 40 still puts the upper endpoint on the sample maximum; 41 is the first n where
        // neither endpoint is an extreme. Pins the derivation in MIN_PERCENTILE_CI_REPS.
        let forty: Vec<f64> = (0..40).map(|i| i as f64).collect();
        assert!(
            bootstrap_stats(&forty, 20.0).2 .0.is_nan(),
            "40 draws is still degenerate"
        );

        let forty_one: Vec<f64> = (0..41).map(|i| i as f64).collect();
        let (_, _, (lo, hi)) = bootstrap_stats(&forty_one, 20.0);
        assert!(lo.is_finite() && hi.is_finite());
        assert!(
            lo > 0.0,
            "lower endpoint must be interior, not the sample minimum"
        );
        assert!(
            hi < 40.0,
            "upper endpoint must be interior, not the sample maximum"
        );
    }

    #[test]
    fn the_interval_brackets_the_estimate_and_widens_with_spread() {
        // Containment: nothing asserted this before 0097. A percentile interval on a
        // distribution centred at the estimate must contain it.
        let tight: Vec<f64> = (0..201).map(|i| 10.0 + (i as f64 - 100.0) * 0.01).collect();
        let (_, _, (lo, hi)) = bootstrap_stats(&tight, 10.0);
        assert!(
            lo <= 10.0 && 10.0 <= hi,
            "interval must contain the point estimate"
        );

        let wide: Vec<f64> = (0..201).map(|i| 10.0 + (i as f64 - 100.0) * 0.10).collect();
        let (_, _, (wlo, whi)) = bootstrap_stats(&wide, 10.0);
        assert!(
            whi - wlo > hi - lo,
            "a more dispersed bootstrap must widen the interval"
        );
    }

    #[test]
    fn the_interval_is_ordered_and_inside_the_observed_range() {
        let estimates: Vec<f64> = (0..500).map(|i| (i as f64) * 0.5 - 100.0).collect();
        let (_, _, (lo, hi)) = bootstrap_stats(&estimates, 25.0);
        assert!(lo < hi);
        let min = estimates.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = estimates.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            lo > min && hi < max,
            "a 95% interval must sit strictly inside the draws"
        );
    }
}
