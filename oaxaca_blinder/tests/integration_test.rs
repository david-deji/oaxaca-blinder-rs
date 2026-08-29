#![allow(deprecated)] // QuantileDecompositionBuilder (MM sim) is deprecated but kept as a
                      // self-consistency guard per 0014-MERIDIAN follow-up (Item B).
use oaxaca_blinder::{
    OaxacaBuilder, OaxacaError, QuantileDecompositionBuilder, ReferenceCoefficients,
};
use polars::prelude::*;

fn create_sample_dataframe() -> DataFrame {
    df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0, 10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M", "F", "F", "F", "F", "F", "M", "M", "M", "M", "M"]
    ).unwrap()
}

// Helper function to avoid test duplication
fn run_and_check(builder: OaxacaBuilder, expected_gap: f64) {
    let results = builder.run().expect("Oaxaca run failed");

    // Check that the calculated gap is correct
    assert!((results.total_gap() - expected_gap).abs() < 1e-9);

    // Check that the two-fold decomposition sums to the total gap
    let explained = results
        .two_fold()
        .aggregate()
        .iter()
        .find(|c| c.name() == "explained")
        .unwrap()
        .estimate();
    let unexplained = results
        .two_fold()
        .aggregate()
        .iter()
        .find(|c| c.name() == "unexplained")
        .unwrap()
        .estimate();
    let total_gap = results.total_gap();
    println!(
        "Explained: {}, Unexplained: {}, Sum: {}, Total Gap: {}",
        explained,
        unexplained,
        explained + unexplained,
        total_gap
    );
    assert!(
        (explained + unexplained - results.total_gap()).abs() < 1e-9,
        "Decomposition does not sum to total gap"
    );

    // Check that the number of observations is correct
    assert_eq!(*results.n_a(), 10);
    assert_eq!(*results.n_b(), 10);

    // Call summary to make sure it doesn't panic
    results.summary();
}

#[test]
fn test_detailed_components_with_rare_category() {
    // "sector=B" occurs exactly once, and that row is in the reference group
    // ("F"), so sector "B" is entirely absent from group "M" in the FULL data —
    // not merely rare in a resample. This is the level-confinement case (0014-close
    // round-1, D1/A4), not a per-replicate bootstrap discard: it fails at the point
    // estimate, before any bootstrap rep runs, so `run()` returns the named
    // `EmptyLevelInGroup` refusal rather than reaching Cholesky.
    let df = df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0, 10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "gender" => &["F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M"],
        "sector" => &["A", "A", "A", "A", "A", "A", "A", "A", "A", "B", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A"] // "B" is a rare category
    ).unwrap();

    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    let result = builder
        .predictors(vec!["education"])
        .categorical_predictors(vec!["sector"])
        .bootstrap_reps(5)
        .run();

    match result {
        Err(OaxacaError::EmptyLevelInGroup {
            column,
            level,
            missing_from_group,
        }) => {
            assert_eq!(column, "sector");
            assert_eq!(level, "B");
            assert_eq!(missing_from_group, "M");
        }
        Err(e) => panic!("Expected EmptyLevelInGroup, got a different error: {e}"),
        Ok(_) => panic!("Expected EmptyLevelInGroup refusal for a level confined to one group"),
    }
}

#[test]
fn test_level_confined_to_an_excluded_third_group() {
    // Adversary-A MINOR-1 (0014-close round-1): with a 3-valued group column the
    // comparison is F vs M, so X's rows never enter either group frame — but the
    // dummy columns are encoded from the UNSPLIT frame, so sector "C" still gets a
    // column that is constant-zero inside both design matrices. Scanning only
    // df_a ∪ df_b would never see "C" and would fall through to the opaque
    // Cholesky message. The refusal names it because the scan reads the full frame.
    let df = df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0, 30.0, 31.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 14.0, 15.0],
        "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M", "X", "X"],
        "sector" => &["A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "C", "C"]
    )
    .unwrap();

    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    let result = builder
        .predictors(vec!["education"])
        .categorical_predictors(vec!["sector"])
        .bootstrap_reps(5)
        .run();

    match result {
        Err(OaxacaError::EmptyLevelInGroup {
            column,
            level,
            missing_from_group,
        }) => {
            assert_eq!(column, "sector");
            assert_eq!(level, "C");
            // "C" is missing from BOTH compared groups; group A is named first.
            assert_eq!(missing_from_group, "M");
        }
        Err(e) => panic!("Expected EmptyLevelInGroup, got a different error: {e}"),
        Ok(_) => {
            panic!("Expected EmptyLevelInGroup refusal for a level confined to the excluded group")
        }
    }
}

#[test]
fn test_zero_weight_level_is_absent_for_estimation() {
    // Adversary-A MINOR-2: sector "B" has rows in group M, but every one carries
    // weight 0. `math/ols.rs` scales rows by sqrt(weight) before forming X'WX, so
    // that level contributes an effectively-zero column — the same singularity a
    // row-count-only check would wave through.
    let df = df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M"],
        "sector" => &["A", "A", "A", "B", "B", "A", "A", "A", "B", "B"],
        "w" => &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]
    )
    .unwrap();

    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    let result = builder
        .predictors(vec!["education"])
        .categorical_predictors(vec!["sector"])
        .weights("w")
        .bootstrap_reps(5)
        .run();

    match result {
        Err(OaxacaError::EmptyLevelInGroup {
            column,
            level,
            missing_from_group,
        }) => {
            assert_eq!(column, "sector");
            assert_eq!(level, "B");
            assert_eq!(missing_from_group, "M");
        }
        Err(e) => panic!("Expected EmptyLevelInGroup, got a different error: {e}"),
        Ok(_) => panic!("Expected EmptyLevelInGroup refusal for an all-zero-weight level"),
    }
}

#[test]
fn test_full_run_group_b_ref() {
    let df = create_sample_dataframe();
    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    builder.predictors(vec!["education"]).bootstrap_reps(5); // Default is GroupB
    run_and_check(builder, 10.0);
}

#[test]
fn test_full_run_group_a_ref() {
    let df = create_sample_dataframe();
    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    builder
        .predictors(vec!["education"])
        .bootstrap_reps(5)
        .reference_coefficients(ReferenceCoefficients::GroupA);
    run_and_check(builder, 10.0);
}

#[test]
fn test_full_run_pooled_ref() {
    let df = create_sample_dataframe();
    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    builder
        .predictors(vec!["education"])
        .bootstrap_reps(5)
        .reference_coefficients(ReferenceCoefficients::Pooled);
    run_and_check(builder, 10.0);
}

#[test]
fn test_full_run_weighted_ref() {
    let df = create_sample_dataframe();
    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    builder
        .predictors(vec!["education"])
        .bootstrap_reps(5)
        .reference_coefficients(ReferenceCoefficients::Weighted);
    run_and_check(builder, 10.0);
}

#[test]
fn test_with_categorical_variable() {
    let df = df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0, 10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M", "F", "F", "F", "F", "F", "M", "M", "M", "M", "M"],
        "union" => &["none", "union", "union_plus", "none", "union", "union_plus", "none", "union", "union_plus", "none", "none", "union", "union_plus", "none", "union", "union_plus", "none", "union", "union_plus", "none"]
    ).unwrap();

    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    builder
        .predictors(vec!["education"])
        .categorical_predictors(vec!["union"])
        .normalize(vec!["union"])
        .bootstrap_reps(5);

    run_and_check(builder, 10.0);
}

#[test]
fn test_quantile_decomposition() {
    let df = df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0, 9.0, 18.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 10.0, 20.0],
        "gender" => &["F", "F", "F", "F", "F", "F", "M", "M", "M", "M", "M", "M"]
    )
    .unwrap();

    let quantiles_to_test = &[0.25, 0.5, 0.75];
    let mut builder = QuantileDecompositionBuilder::new(df, "wage", "gender", "F");
    let results = builder
        .predictors(vec!["education"])
        .quantiles(quantiles_to_test)
        .simulations(10) // Low number for fast testing
        .bootstrap_reps(2) // Low number for fast testing
        .run()
        .unwrap();

    assert!(results.results_by_quantile().contains_key("q25"));
    assert!(results.results_by_quantile().contains_key("q50"));
    assert!(results.results_by_quantile().contains_key("q75"));

    for key in &["q25", "q50", "q75"] {
        let detail = results.results_by_quantile().get(*key).unwrap();
        let gap = detail.total_gap().estimate();
        let chars = detail.characteristics_effect().estimate();
        let coeffs = detail.coefficients_effect().estimate();
        assert!((chars + coeffs - gap).abs() < 1e-9);
    }

    results.summary();
}
