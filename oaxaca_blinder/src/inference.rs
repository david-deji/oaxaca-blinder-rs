//! This module contains functions for statistical inference, primarily bootstrapping.

/// Calculates the standard error, p-value, and confidence interval from a vector of bootstrap estimates.
///
/// INV-02: summation here stays sequential (`iter().sum`) — do NOT introduce a rayon
/// reduce/fold. Float addition is non-associative; a parallel reduce reorders partial sums
/// and breaks bit-identity across thread counts. The input is the indexed, order-fixed
/// `estimates` vector built by the caller.
pub fn bootstrap_stats(estimates: &[f64], _point_estimate: f64) -> (f64, f64, (f64, f64)) {
    if estimates.is_empty() {
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

    // Confidence interval using the percentile method.
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
}
