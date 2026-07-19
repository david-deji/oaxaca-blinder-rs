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
    let y_vec: Vec<f64> = series.f64()?.into_no_null_iter().collect();
    let n = y_vec.len() as f64;

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

    // 1. Calculate Sample Quantile (Q_tau) using R Type 7 interpolation
    let mut sorted_y = y_vec.clone();
    sorted_y.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let h = (n - 1.0) * quantile;
    let h_floor = h.floor();
    let h_ceil = h.ceil();
    let frac = h - h_floor;
    let q_tau = if h_floor == h_ceil {
        sorted_y[h_floor as usize]
    } else {
        let y0 = sorted_y[h_floor as usize];
        let y1 = sorted_y[h_ceil as usize];
        y0 + frac * (y1 - y0)
    };

    // 2. Estimate Density at Q_tau using Gaussian Kernel
    // Bandwidth selection (Silverman's Rule of Thumb)
    let mean = y_vec.iter().sum::<f64>() / n;
    let variance = y_vec.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std_dev = variance.sqrt();

    let q75_idx = (0.75 * n).ceil() as usize;
    let q75_idx = if q75_idx == 0 { 0 } else { q75_idx - 1 };

    let q25_idx = (0.25 * n).ceil() as usize;
    let q25_idx = if q25_idx == 0 { 0 } else { q25_idx - 1 };

    let iqr = sorted_y[q75_idx.min(sorted_y.len() - 1)] - sorted_y[q25_idx.min(sorted_y.len() - 1)];

    let min_spread = if iqr > 1e-8 {
        std_dev.min(iqr / 1.34)
    } else {
        std_dev
    };
    // Fallback if spread is zero (all values same)
    let min_spread = if min_spread < 1e-8 { 1.0 } else { min_spread };

    let h = 0.9 * min_spread * n.powf(-0.2);

    // Gaussian Kernel Density Estimation
    // f(x) = (1 / (n * h)) * sum(K((x - Xi) / h))
    // K(u) = (1 / sqrt(2 * pi)) * exp(-0.5 * u^2)

    let density: f64 = y_vec
        .iter()
        .map(|&yi| {
            let u = (q_tau - yi) / h;
            (1.0 / (2.0 * PI).sqrt()) * (-0.5 * u.powi(2)).exp()
        })
        .sum::<f64>()
        / (n * h);

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
