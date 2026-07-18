//! Stage-3 (engine-parallel-surface) threading ACs for the RIF quantile path (0014-MERIDIAN).
//! AC-6 (In-Scope 12: per-predictor detail is populated, not the old MM empties), AC-8 (adding-up
//! identity), AC-11 (seed propagation + fixed_rif metadata). The ddecompose golden (AC-7/AC-12) is
//! owned by the statistical-trust-layer domain (stage 4), not here.

use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;

fn fixture() -> DataFrame {
    let path = format!(
        "{}/tests/fixtures/parity_fixture.csv",
        env!("CARGO_MANIFEST_DIR")
    );
    LazyCsvReader::new(path)
        .with_has_header(true)
        .finish()
        .expect("fixture readable")
        .collect()
        .expect("fixture parses")
}

fn build(seed: u64) -> OaxacaBuilder {
    let mut b = OaxacaBuilder::new(fixture(), "log_wage", "gender", "F");
    b.predictors(vec!["education", "experience", "tenure"])
        .reference_coefficients(ReferenceCoefficients::GroupB)
        .bootstrap_reps(64)
        .seed(seed);
    b
}

/// AC-6 (In-Scope 12): the WASM/RIF quantile path returns non-empty per-predictor detail — the
/// core fix (the old MM path returned unconditional empties). One component per predictor.
#[test]
fn ac6_quantile_detail_non_empty() {
    let r = build(1).decompose_quantile(0.5).unwrap();
    let tf = r.two_fold();
    assert!(
        !tf.detailed_explained().is_empty(),
        "RIF quantile path must populate detailed_explained (In-Scope 12), not return empties"
    );
    assert!(
        !tf.detailed_unexplained().is_empty(),
        "RIF quantile path must populate detailed_unexplained"
    );
}

/// AC-8: adding-up identity — the sum of the per-predictor detail equals the aggregate (the RIF
/// path is the mean-path OB algebra on RIF-transformed data, so this is inherited from run()).
#[test]
fn ac8_quantile_adding_up() {
    let r = build(1).decompose_quantile(0.5).unwrap();
    let tf = r.two_fold();
    let mut agg_explained = 0.0;
    let mut agg_unexplained = 0.0;
    for c in tf.aggregate() {
        match c.name().as_str() {
            "explained" => agg_explained = *c.estimate(),
            "unexplained" => agg_unexplained = *c.estimate(),
            _ => {}
        }
    }
    let sum_explained: f64 = tf.detailed_explained().iter().map(|c| *c.estimate()).sum();
    let sum_unexplained: f64 = tf
        .detailed_unexplained()
        .iter()
        .map(|c| *c.estimate())
        .sum();
    assert!(
        (sum_explained - agg_explained).abs() < 1e-6,
        "adding-up: Σ detailed explained ({sum_explained}) != aggregate explained ({agg_explained})"
    );
    assert!(
        (sum_unexplained - agg_unexplained).abs() < 1e-6,
        "adding-up: Σ detailed unexplained ({sum_unexplained}) != aggregate unexplained ({agg_unexplained})"
    );
}

/// AC-11 (council CV-1 seam): the RIF quantile path forwards the seed. Same seed reproduces
/// byte-identically; different seeds differ; RunMetadata records the seed and fixed_rif=false
/// (ruling 4 — RIF recomputed per replicate).
#[test]
fn ac11_quantile_seed_propagation() {
    let r1a = build(1).decompose_quantile(0.5).unwrap();
    let r1b = build(1).decompose_quantile(0.5).unwrap();
    let r2 = build(2).decompose_quantile(0.5).unwrap();
    let s1a = serde_json::to_string(&r1a).unwrap();
    let s1b = serde_json::to_string(&r1b).unwrap();
    let s2 = serde_json::to_string(&r2).unwrap();
    assert_eq!(
        s1a, s1b,
        "same seed must reproduce byte-identically on the RIF quantile path"
    );
    assert_ne!(s1a, s2, "different seeds must produce different output");
    assert_eq!(
        r1a.run_metadata().seed,
        1,
        "RunMetadata.seed must record the chosen seed"
    );
    assert_eq!(
        r1a.run_metadata().fixed_rif,
        Some(false),
        "ruling 4: RIF recomputed per replicate => fixed_rif = Some(false)"
    );
}
