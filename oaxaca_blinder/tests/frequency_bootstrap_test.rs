//! 0120-MERIDIAN E-REV-1: under `WeightsKind::Frequency` the bootstrap resamples the EXPANDED
//! sample, so a standard error on `w = 2` is the standard error on the row written twice.
//!
//! The repetition identity already held for point estimates (`weights_kind_test.rs`, with
//! `bootstrap_reps(0)`). The bootstrap used to draw one index per ROW and carry the weight along,
//! which resamples `rows` units instead of `sum(w)`: on `norm_skewed_fixture.csv` with `w = 2` the
//! standard errors came out 1.32x (explained) and 1.45x (unexplained) those of the duplicated
//! rows, and Department_Admin's unexplained p-value was 0.084 against 0.000. The same seed cannot
//! reproduce the same draws on two different frames, so the comparison is a tolerance on the SE
//! ratio, wide enough for Monte-Carlo noise (about 4% at 400 replicates) and far inside the 1.3 to
//! 1.45 the row-level draw produced.

use oaxaca_blinder::{OaxacaBuilder, OaxacaResults, WeightsKind};
use polars::prelude::*;
use std::path::PathBuf;

const REPS: usize = 400;

fn fixture() -> DataFrame {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/norm_skewed_fixture.csv");
    LazyCsvReader::new(path)
        .with_has_header(true)
        .finish()
        .expect("fixture readable")
        .collect()
        .expect("fixture parses")
        .drop("w")
        .expect("w column")
}

/// The fixture with `weights` as the frequency column, and the same rows written out `w` times
/// with `w = 1` (the expanded sample).
fn weighted_and_expanded(weights: &[f64]) -> (DataFrame, DataFrame) {
    let base = fixture();
    assert_eq!(base.height(), weights.len());
    let mut weighted = base.clone();
    weighted
        .with_column(Column::new("w".into(), weights.to_vec()))
        .unwrap();
    let mut idx: Vec<u32> = Vec::new();
    for (i, w) in weights.iter().enumerate() {
        for _ in 0..(*w as usize) {
            idx.push(i as u32);
        }
    }
    let mut expanded = base.take(&IdxCa::from_vec("i".into(), idx)).unwrap();
    expanded
        .with_column(Column::new("w".into(), vec![1.0f64; expanded.height()]))
        .unwrap();
    (weighted, expanded)
}

fn builder(df: DataFrame) -> OaxacaBuilder {
    let mut b = OaxacaBuilder::new(df, "log_salary", "Gender", "Female");
    b.predictors(["Age", "Experience_Years"])
        .categorical_predictors(["Department"])
        .weights("w")
        .weights_kind(WeightsKind::Frequency)
        .bootstrap_reps(REPS);
    b
}

fn se_pairs(a: &OaxacaResults, b: &OaxacaResults) -> Vec<(String, f64, f64)> {
    let mut out = Vec::new();
    for (x, y) in [
        (a.explained().unwrap(), b.explained().unwrap()),
        (a.unexplained().unwrap(), b.unexplained().unwrap()),
    ] {
        out.push((x.name.clone(), x.std_err, y.std_err));
    }
    for (x, y) in a
        .two_fold
        .detailed_unexplained()
        .iter()
        .zip(b.two_fold.detailed_unexplained())
    {
        assert_eq!(x.name, y.name);
        out.push((format!("unexplained:{}", x.name), x.std_err, y.std_err));
    }
    for (x, y) in a
        .two_fold
        .detailed_explained()
        .iter()
        .zip(b.two_fold.detailed_explained())
    {
        out.push((format!("explained:{}", x.name), x.std_err, y.std_err));
    }
    out
}

fn assert_se_agree(label: &str, a: &OaxacaResults, b: &OaxacaResults) {
    let pairs = se_pairs(a, b);
    // The two headline totals are the quantities a consultant reads first.
    for (name, sa, sb) in pairs.iter().take(2) {
        let ratio = sa / sb;
        assert!(
            (ratio - 1.0).abs() < 0.1,
            "{label}: SE of {name} on the weighted frame is {ratio:.3}x the expanded frame ({sa} vs {sb}); \
             a row-level draw gives 1.3 to 1.45"
        );
    }
    // Per-row noise is larger, so the whole table is held to its mean ratio.
    // (a term that is exactly zero on both frames, such as an omitted base level, has no ratio)
    let ratios: Vec<f64> = pairs
        .iter()
        .filter(|(_, sa, sb)| *sa != 0.0 || *sb != 0.0)
        .map(|(_, sa, sb)| sa / sb)
        .collect();
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    assert!(
        (mean - 1.0).abs() < 0.05,
        "{label}: mean SE ratio over {} terms is {mean:.3}",
        ratios.len()
    );
    let worst = ratios.iter().map(|r| (r - 1.0).abs()).fold(0.0, f64::max);
    assert!(
        worst < 0.3,
        "{label}: worst single-term SE ratio off by {worst:.3}"
    );
    // The bootstrap loses a replicate to a vanished level at about the same rate on both frames.
    println!("{label}: mean SE ratio {mean:.4}, worst term {worst:.4}");
}

#[test]
fn frequency_two_matches_the_row_twice_in_standard_errors() {
    let (weighted, expanded) = weighted_and_expanded(&vec![2.0; fixture().height()]);
    let w = builder(weighted).run().unwrap();
    let e = builder(expanded).run().unwrap();
    // point estimates were already identical
    assert!((w.total_gap - e.total_gap).abs() < 1e-9);
    assert!((w.unexplained().unwrap().estimate - e.unexplained().unwrap().estimate).abs() < 1e-9);
    assert_se_agree("w = 2 everywhere", &w, &e);
    // The replicate draws unit positions in the expanded sample from the same stream, so on a
    // frame whose rows are repeated in order the two runs are not merely alike in distribution:
    // they draw the same expanded sample, and every standard error agrees to rounding.
    for (name, sa, sb) in se_pairs(&w, &e) {
        assert!(
            (sa - sb).abs() <= 1e-9 * sb.abs().max(1e-12),
            "{name}: {sa} vs {sb}"
        );
    }
}

#[test]
fn frequency_weights_of_one_to_three_match_the_expanded_rows_in_standard_errors() {
    let n = fixture().height();
    let weights: Vec<f64> = (0..n).map(|i| 1.0 + (i % 3) as f64).collect();
    let (weighted, expanded) = weighted_and_expanded(&weights);
    let w = builder(weighted).run().unwrap();
    let e = builder(expanded).run().unwrap();
    assert!((w.unexplained().unwrap().estimate - e.unexplained().unwrap().estimate).abs() < 1e-9);
    assert_se_agree("w in {1,2,3}", &w, &e);
}

#[test]
fn the_quantile_bootstrap_draws_the_expanded_sample_too() {
    let (weighted, expanded) = weighted_and_expanded(&vec![2.0; fixture().height()]);
    let mut bw = builder(weighted);
    bw.bootstrap_reps(300);
    let mut be = builder(expanded);
    be.bootstrap_reps(300);
    let w = bw.decompose_quantile(0.5).unwrap();
    let e = be.decompose_quantile(0.5).unwrap();
    assert!((w.total_gap - e.total_gap).abs() < 1e-8);
    assert_se_agree("quantile 0.5, w = 2", &w, &e);
}

#[test]
fn a_frequency_run_is_reproducible_and_a_relative_run_still_draws_rows() {
    // Same seed, same frame: bit-identical standard errors.
    let (weighted, _) = weighted_and_expanded(&vec![2.0; fixture().height()]);
    let a = builder(weighted.clone()).bootstrap_reps(60).run().unwrap();
    let b = builder(weighted.clone()).bootstrap_reps(60).run().unwrap();
    assert_eq!(
        a.unexplained().unwrap().std_err.to_bits(),
        b.unexplained().unwrap().std_err.to_bits()
    );
    // Relative weights keep the row-level draw: rescaling them changes nothing, so the standard
    // errors under w = 2 and w = 1 coincide exactly (the weights are scaled to the row count).
    let mut rel2 = builder(weighted.clone());
    rel2.weights_kind(WeightsKind::Relative).bootstrap_reps(60);
    let mut ones = weighted;
    ones.with_column(Column::new("w".into(), vec![1.0f64; ones.height()]))
        .unwrap();
    let mut rel1 = builder(ones);
    rel1.weights_kind(WeightsKind::Relative).bootstrap_reps(60);
    let (r2, r1) = (rel2.run().unwrap(), rel1.run().unwrap());
    assert!(
        (r2.unexplained().unwrap().std_err - r1.unexplained().unwrap().std_err).abs() < 1e-12,
        "relative weights are scale-free in the bootstrap as well"
    );
}
