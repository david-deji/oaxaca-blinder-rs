#![allow(deprecated)] // QuantileDecompositionBuilder (MM sim) is deprecated but kept as a
                      // self-consistency guard per 0014-MERIDIAN follow-up (Item B).
use oaxaca_blinder::{OaxacaBuilder, QuantileDecompositionBuilder, ReferenceCoefficients};
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
    let df = df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0, 10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "gender" => &["F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M"],
        "sector" => &["A", "A", "A", "A", "A", "A", "A", "A", "A", "B", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A"] // "B" is a rare category
    ).unwrap();

    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    let results = builder
        .predictors(vec!["education"])
        .categorical_predictors(vec!["sector"])
        .bootstrap_reps(5)
        .run()
        .expect("Oaxaca run failed");

    // This is the crucial part. We check if the components are present and if their CIs are valid.
    // The bug would cause a panic here when trying to calculate stats for a component that
    // disappeared in some bootstrap samples, or would produce nonsensical results (e.g. NaN).
    let detailed_unexplained = results.two_fold().detailed_unexplained();

    let intercept = detailed_unexplained
        .iter()
        .find(|c| c.name() == "intercept")
        .unwrap();
    assert!(intercept.ci_lower().is_finite());
    assert!(intercept.ci_upper().is_finite());

    let education = detailed_unexplained
        .iter()
        .find(|c| c.name() == "education")
        .unwrap();
    assert!(education.ci_lower().is_finite());
    assert!(education.ci_upper().is_finite());

    // With the bug, the "sector_B" component might have issues if it's not present in all bootstrap samples.
    // We expect it to be present in the final results, and its stats should be valid numbers.
    let sector_b = detailed_unexplained.iter().find(|c| c.name() == "sector_B");
    assert!(
        sector_b.is_some(),
        "Detailed component for rare category 'sector_B' should be present"
    );
    assert!(sector_b.unwrap().ci_lower().is_finite());
    assert!(sector_b.unwrap().ci_upper().is_finite());

    results.summary();
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
