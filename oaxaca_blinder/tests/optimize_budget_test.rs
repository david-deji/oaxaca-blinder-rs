//! The deprecated library method keeps its tests: callers of it still exist (0122-MERIDIAN T12).
#![allow(deprecated)]

use oaxaca_blinder::OaxacaBuilder;
use polars::prelude::*;

#[test]
fn test_optimize_budget() -> Result<(), Box<dyn std::error::Error>> {
    // Create data
    let df = df!(
        "wage" => &[
            // Group A (Reference A) - Mean 32
            30.0, 32.0, 34.0,
            // Group B (Reference B) - Mean 16
            10.0, // Edu 10. Pred: 15. Res: -5
            15.0, // Edu 10. Pred: 15. Res: 0
            20.0, // Edu 10. Pred: 15. Res: +5
            12.0, // Edu 12. Pred: 17. Res: -5
            17.0, // Edu 12. Pred: 17. Res: 0
            22.0  // Edu 12. Pred: 17. Res: +5
        ],
        "education" => &[
            10.0, 12.0, 14.0,
            10.0, 10.0, 10.0, 12.0, 12.0, 12.0
        ],
        "group" => &[
            "A", "A", "A",
            "B", "B", "B", "B", "B", "B"
        ]
    )?;

    let results = OaxacaBuilder::new(df, "wage", "group", "B")
        .predictors(vec!["education"])
        .run()?;

    // Total Gap = 32 - 16 = 16.
    assert!((results.total_gap() - 16.0).abs() < 1e-9);

    // Case 1: Small budget, large target reduction (impossible to reach target, spend all budget)
    // Target Gap 10. Required reduction 6. Total needed 6 * 6 = 36.
    // Budget 5.
    // Should spend 5.0 on one of the -5 residuals.
    let adjustments = results.optimize_budget(5.0, 10.0);
    assert_eq!(adjustments.len(), 1);
    assert!((adjustments[0].adjustment - 5.0).abs() < 1e-9);
    assert!((adjustments[0].original_residual + 5.0).abs() < 1e-9); // Residual was -5

    // Case 2: Large budget, small target reduction (reach target)
    // Target Gap 15. Required reduction 1. Total needed 1 * 6 = 6.
    // Budget 100.
    // Should spend 6.0 total.
    // Candidates are the two -5 residuals.
    // Should fix one fully (5) and one partially (1).
    let adjustments = results.optimize_budget(100.0, 15.0);
    assert_eq!(adjustments.len(), 2);
    let total_adjustment: f64 = adjustments.iter().map(|a| a.adjustment).sum();
    assert!((total_adjustment - 6.0).abs() < 1e-9);

    // Verify individual adjustments
    // One should be 5.0, one should be 1.0.
    let mut amounts: Vec<f64> = adjustments.iter().map(|a| a.adjustment).collect();
    amounts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    assert!((amounts[0] - 1.0).abs() < 1e-9);
    assert!((amounts[1] - 5.0).abs() < 1e-9);

    // Case 3: Budget enough to fix all negative residuals, but target gap requires less.
    // Same as Case 2.

    // Case 4: Target gap already met.
    let adjustments = results.optimize_budget(100.0, 20.0);
    assert!(adjustments.is_empty());

    // Case 5 (0122-MERIDIAN C-09): a budget or a target that is not a number pays nothing. A NaN
    // budget used to fund the whole need, because `f64::min` returns the other argument.
    for budget in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -5.0] {
        assert!(
            results.optimize_budget(budget, 10.0).is_empty(),
            "budget {budget} paid someone"
        );
    }
    assert!(results.optimize_budget(100.0, f64::NAN).is_empty());

    Ok(())
}
