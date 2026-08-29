use oaxaca_blinder::OaxacaBuilder;
use polars::prelude::*;

#[test]
fn test_weighted_decomposition() -> Result<(), Box<dyn std::error::Error>> {
    // Create data
    // Group A:
    // 1. Outcome 10, x=1, w=1
    // 2. Outcome 10, x=1, w=1
    // 3. Outcome 2,  x=0, w=10 (Heavy weight on low outcome)
    // Unweighted Mean A = (10+10+2)/3 = 7.333
    // Weighted Mean A = (10*1 + 10*1 + 2*10)/12 = 40/12 = 3.333

    // Group B:
    // 1. Outcome 5, x=0, w=1
    // 2. Outcome 7, x=1, w=1
    // Mean B = 6.0 (Unweighted and Weighted same since weights are 1)

    let df = df!(
        "outcome" => &[10.0, 10.0, 2.0,  5.0, 7.0, 8.0],
        "group" =>   &["A",  "A",  "A",  "B", "B", "B"],
        "weight" =>  &[1.0,  1.0,  10.0, 1.0, 1.0, 1.0],
        "x" =>       &[1.0,  1.0,  0.0,  0.0, 1.0, 1.0]
    )?;

    // Unweighted Gap = 7.333 - 6.0 = 1.333
    // Weighted Gap = 3.333 - 6.0 = -2.666

    // Run unweighted
    let res_unweighted = OaxacaBuilder::new(df.clone(), "outcome", "group", "B")
        .predictors(vec!["x"])
        .bootstrap_reps(0)
        .run()?;

    println!("Unweighted Gap: {}", res_unweighted.total_gap());
    assert!((res_unweighted.total_gap() - 0.666).abs() < 0.01);

    // Run weighted
    let res_weighted = OaxacaBuilder::new(df, "outcome", "group", "B")
        .predictors(vec!["x"])
        .weights("weight")
        .bootstrap_reps(0)
        .run()?;

    println!("Weighted Gap: {}", res_weighted.total_gap());
    assert!((res_weighted.total_gap() - (-3.333)).abs() < 0.01);

    Ok(())
}

// -------------------------------------------------------------------------------------------
// 0097 — the crossing tests. Before this issue `weights_test.rs` held exactly one test, which
// never called `decompose_quantile`, while the two tests that DO call it (`rif_test.rs`,
// `rng_determinism.rs`) never set `weights_col`. The two axes had never met, so the RIF transform
// could ignore weights entirely with every suite green.
// -------------------------------------------------------------------------------------------

fn weighted_frame() -> DataFrame {
    df![
        "wage"   => [10.0f64, 12.0, 14.0, 16.0, 11.0, 13.0, 15.0, 30.0,
                     20.0, 22.0, 24.0, 26.0, 21.0, 23.0, 25.0, 40.0],
        "educ"   => [1.0f64, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0,
                     1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0],
        "group"  => ["A", "A", "A", "A", "A", "A", "A", "A",
                     "B", "B", "B", "B", "B", "B", "B", "B"],
        // The weight pattern must be ASYMMETRIC across groups. A first draft put the mass on the
        // top earner of BOTH groups: each weighted median moved by the same amount, the gap stayed
        // put, and the test would have "passed" only by accident of the fixture. Here the mass sits
        // on group A's top earner and group B's bottom earner, so the two medians move in opposite
        // directions and the gap has to move if the weights are read at all.
        "hc"     => [1.0f64, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 40.0,
                     40.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    ]
    .unwrap()
}

fn gap_at_median(weighted: bool) -> f64 {
    let df = weighted_frame();
    let mut b = OaxacaBuilder::new(df, "wage", "group", "B");
    b.predictors(vec!["educ"]).bootstrap_reps(0);
    if weighted {
        b.weights("hc");
    }
    let r = b.decompose_quantile(0.5).expect("decompose_quantile");
    r.total_gap
}

/// End-to-end: a weighted quantile decomposition must not return the unweighted answer.
///
/// This does NOT isolate the RIF transform, and an earlier version of this comment claimed it did.
/// `weights_col` also weights the OLS, so `total_gap` moves even with the RIF wire deliberately
/// cut — the sabotage check proved this test passes in that state. The guard that actually pins
/// the RIF wire is `builder::tests::rif_replace_outcome_honours_weights_col`, which calls
/// `rif_replace_outcome` directly. Kept here as the end-to-end companion, named for what it
/// really checks.
#[test]
fn a_weighted_quantile_run_differs_from_the_unweighted_one() {
    let bare = gap_at_median(false);
    let weighted = gap_at_median(true);
    assert!(bare.is_finite() && weighted.is_finite());
    assert!(
        (bare - weighted).abs() > 1e-9,
        "weighted and unweighted quantile decomposition returned the same gap ({bare} vs {weighted}) \
         — the weights are being dropped inside the RIF transform again"
    );
}
