use oaxaca_blinder::{decompose_changes, run_dfl, OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;

fn create_dummy_data() -> DataFrame {
    df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "experience" => &[5.0, 10.0, 7.0, 12.0, 15.0, 5.0, 10.0, 7.0, 12.0, 15.0],
        "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M"]
    )
    .unwrap()
}

#[test]
fn test_reference_groups() {
    let df = create_dummy_data();

    // Test Cotton
    let mut builder_cotton = OaxacaBuilder::new(df.clone(), "wage", "gender", "F");
    builder_cotton
        .predictors(vec!["education", "experience"])
        .reference_coefficients(ReferenceCoefficients::Cotton);
    let results_cotton = builder_cotton.run().expect("Cotton decomposition failed");

    assert!(results_cotton.total_gap() > &0.0);

    // Neumark: a deprecated alias that still computes `Pooled` (pooled regression WITH a group
    // indicator), so existing callers keep their numbers (0120-MERIDIAN T7).
    #[allow(deprecated)]
    let neumark = ReferenceCoefficients::Neumark;
    let mut builder_neumark = OaxacaBuilder::new(df.clone(), "wage", "gender", "F");
    builder_neumark
        .predictors(vec!["education", "experience"])
        .reference_coefficients(neumark);
    let results_neumark = builder_neumark.run().expect("Neumark decomposition failed");
    assert!(results_neumark.total_gap() > &0.0);

    let mut builder_pooled = OaxacaBuilder::new(df.clone(), "wage", "gender", "F");
    builder_pooled
        .predictors(vec!["education", "experience"])
        .reference_coefficients(ReferenceCoefficients::Pooled);
    let results_pooled = builder_pooled.run().expect("Pooled decomposition failed");
    assert_eq!(
        results_neumark.unexplained().unwrap().estimate,
        results_pooled.unexplained().unwrap().estimate,
        "the Neumark alias must keep computing Pooled"
    );

    // PooledNoIndicator is the estimator Neumark's name promised. On THIS data both groups have
    // identical characteristics, so every scheme returns the whole gap as unexplained and the two
    // pooled schemes coincide; that they DIFFER where they should is asserted on a skewed design in
    // normalization_oracle_test.rs (`s4_pooled_no_indicator_is_not_pooled`). Here: it runs and adds up.
    let mut builder_omega = OaxacaBuilder::new(df, "wage", "gender", "F");
    builder_omega
        .predictors(vec!["education", "experience"])
        .reference_coefficients(ReferenceCoefficients::PooledNoIndicator);
    let results_omega = builder_omega.run().expect("PooledNoIndicator failed");
    assert!(
        (results_omega.explained().unwrap().estimate
            + results_omega.unexplained().unwrap().estimate
            - results_omega.total_gap)
            .abs()
            < 1e-9
    );
}

#[test]
fn test_jmp_decomposition() {
    // Create T1 data
    let df_t1 = df!(
        "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M"]
    )
    .unwrap();

    // Create T2 data (Gap reduced: Women paid more)
    let df_t2 = df!(
        "wage" => &[15.0, 17.0, 16.0, 18.0, 20.0, 20.0, 22.0, 21.0, 23.0, 25.0],
        "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
        "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M"]
    )
    .unwrap();

    let mut builder_t1 = OaxacaBuilder::new(df_t1, "wage", "gender", "F");
    builder_t1.predictors(vec!["education"]);

    let mut builder_t2 = OaxacaBuilder::new(df_t2, "wage", "gender", "F");
    builder_t2.predictors(vec!["education"]);

    let jmp_results = decompose_changes(&builder_t1, &builder_t2).expect("JMP failed");

    jmp_results.summary();

    // Gap T1: Mean(M) - Mean(F) = 22.2 - 12.2 = 10.0
    // Gap T2: Mean(M) - Mean(F) = 22.2 - 17.2 = 5.0
    // Total Change = 5.0 - 10.0 = -5.0

    assert!((jmp_results.total_change - (-5.0)).abs() < 1e-4);
}

#[test]
fn test_dfl_reweighting() {
    let df = create_dummy_data();

    let dfl_results = run_dfl(
        &df,
        "wage",
        "gender",
        "F",
        &["education".to_string(), "experience".to_string()],
    )
    .expect("DFL failed");

    assert_eq!(dfl_results.grid.len(), 100);
    assert_eq!(dfl_results.density_a.len(), 100);
    assert_eq!(dfl_results.density_b.len(), 100);
    assert_eq!(dfl_results.density_b_counterfactual.len(), 100);
}
