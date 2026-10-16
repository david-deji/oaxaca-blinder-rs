//! Experimental: Juhn-Murphy-Pierce decomposition of changes over time. No external oracle;
//! results may change in any release. Needs the `experimental` feature.

use crate::{OaxacaBuilder, OaxacaError};
use serde::Serialize;

/// Holds the results of a Juhn-Murphy-Pierce (JMP) decomposition of changes over time.
#[derive(Debug, Serialize)]
pub struct JmpDecomposition {
    /// The total change in the gap between the two time periods (Gap_T2 - Gap_T1).
    pub total_change: f64,
    /// The portion of the change due to changes in observable characteristics (Quantity Effect).
    pub quantity_effect: f64,
    /// The portion of the change due to changes in coefficients/prices (Price Effect).
    pub price_effect: f64,
    /// The portion of the change due to changes in unobserved residuals (Gap Effect).
    pub gap_effect: f64,
}

impl JmpDecomposition {
    /// Prints a summary of the JMP decomposition.
    pub fn summary(&self) {
        println!("Juhn-Murphy-Pierce (JMP) Decomposition of Changes");
        println!("==================================================");
        println!("Total Change in Gap: {:.4}", self.total_change);
        println!("  Quantity Effect:   {:.4}", self.quantity_effect);
        println!("  Price Effect:      {:.4}", self.price_effect);
        println!("  Gap Effect:        {:.4}", self.gap_effect);
    }
}

/// Decomposes the change in the pay gap between two time periods using the JMP method.
///
/// This simplified JMP implementation decomposes the change in the mean gap into three components:
/// 1. **Quantity Effect**: Changes in the distribution of observable characteristics (X).
/// 2. **Price Effect**: Changes in the returns to those characteristics (Beta).
/// 3. **Gap Effect**: Changes in the unobserved residual inequality (Interaction/Residuals).
///
/// # Arguments
///
/// * `builder_t1` - An `OaxacaBuilder` configured for the first time period (T1).
/// * `builder_t2` - An `OaxacaBuilder` configured for the second time period (T2).
///
/// # Returns
///
/// A `JmpDecomposition` struct containing the results.
pub fn decompose_changes(
    builder_t1: &OaxacaBuilder,
    builder_t2: &OaxacaBuilder,
) -> Result<JmpDecomposition, OaxacaError> {
    // Run standard Oaxaca for both periods
    let results_t1 = builder_t1.run()?;
    let results_t2 = builder_t2.run()?;

    // Gap = Mean(A) - Mean(B)
    // Gap_T1 = Explained_T1 + Unexplained_T1
    // Gap_T2 = Explained_T2 + Unexplained_T2

    let gap_t1 = results_t1.total_gap;
    let gap_t2 = results_t2.total_gap;
    let total_change = gap_t2 - gap_t1;

    // Explained_T1 = (Xa_1 - Xb_1) * Beta*_1
    // Explained_T2 = (Xa_2 - Xb_2) * Beta*_2

    // Quantity Effect: Change in X, holding prices (Beta) constant at T1 levels.
    // Q_Effect = ((Xa_2 - Xb_2) - (Xa_1 - Xb_1)) * Beta*_1

    let diff_x_t1 = results_t1.xa_mean() - results_t1.xb_mean();
    let diff_x_t2 = results_t2.xa_mean() - results_t2.xb_mean();

    let beta_star_t1 = results_t1.beta_star();
    // let beta_star_t2 = results_t2.beta_star(); // Not strictly needed for Q effect if we use T1 base

    let quantity_effect = (diff_x_t2 - diff_x_t1).dot(beta_star_t1);

    // Price Effect: Change in Beta, holding quantities (X) constant.
    // P_Effect = (Explained_T2 - Explained_T1) - Quantity_Effect
    // This effectively is: Explained_T2 - Predicted_Explained_T2_using_T1_prices
    // Or: (Xa_2 - Xb_2) * (Beta*_2 - Beta*_1) + Interaction term?
    // Actually, Total Explained Change = Quantity Effect + Price Effect (including interaction usually allocated to price in this simple view)

    let get_estimate = |res: &crate::TwoFoldResults, name: &str| -> f64 {
        *res.aggregate()
            .iter()
            .find(|c| c.name() == name)
            .map(|c| c.estimate())
            .unwrap_or(&0.0)
    };

    let explained_change = get_estimate(results_t2.two_fold(), "explained")
        - get_estimate(results_t1.two_fold(), "explained");

    // Price Effect = Total Explained Change - Quantity Effect
    let price_effect = explained_change - quantity_effect;

    // Gap Effect: Change in Unexplained
    // Gap Effect = Unexplained_T2 - Unexplained_T1
    let unexplained_change = get_estimate(results_t2.two_fold(), "unexplained")
        - get_estimate(results_t1.two_fold(), "unexplained");
    let gap_effect = unexplained_change;

    Ok(JmpDecomposition {
        total_change,
        quantity_effect,
        price_effect,
        gap_effect,
    })
}
