//! Public-API promises that a 0.2.2 caller and a 0.3.0 caller both rely on (0121-MERIDIAN P2).
//!
//! * Every builder method that takes column names accepts a slice, an array, a `Vec<String>`,
//!   a `&Vec<String>` and an iterator of `&str`, including the `&["education"]` form that
//!   compiled in 0.2.2.
//! * The defaults are documented numbers: 100 bootstrap replicates and the reference group's
//!   coefficients.
//! * The enums that grow carry `#[non_exhaustive]`, so a new variant is not a breaking change.
//!
//! The tests run on default features and on `--all-features`; the experimental methods are
//! covered at the bottom behind their feature.
#![deny(unreachable_patterns)]
// `&["education"]` is the 0.2.2 call shape and is what this file exists to keep compiling.
#![allow(clippy::needless_borrows_for_generic_args)]

use oaxaca_blinder::{
    ExclusionReason, NormalizationConvention, OaxacaBuilder, OaxacaError, ReferenceCoefficients,
    WeightsKind, DEFAULT_BOOTSTRAP_REPS,
};
use polars::prelude::*;

/// Sixty rows, two groups, one numeric driver, one categorical driver, deterministic noise.
fn frame() -> DataFrame {
    let n = 60usize;
    let mut wage = Vec::with_capacity(n);
    let mut education = Vec::with_capacity(n);
    let mut experience = Vec::with_capacity(n);
    let mut dept = Vec::with_capacity(n);
    let mut gender = Vec::with_capacity(n);
    let mut selected = Vec::with_capacity(n);
    let mut instrument = Vec::with_capacity(n);
    for i in 0..n {
        let is_f = i % 2 == 0;
        let edu = 10.0 + (i % 9) as f64;
        let exp = 1.0 + ((i * 7) % 13) as f64;
        let d = ["ops", "sales", "tech"][i % 3];
        let noise = ((i as f64) * 1.7).sin() * 1.5;
        let pay = 20.0
            + 2.0 * edu
            + 0.8 * exp
            + if d == "tech" { 6.0 } else { 0.0 }
            + if is_f { -3.0 } else { 0.0 }
            + noise;
        wage.push(pay);
        education.push(edu);
        experience.push(exp);
        dept.push(d);
        gender.push(if is_f { "F" } else { "M" });
        selected.push(if i % 5 == 0 { 0.0 } else { 1.0 });
        instrument.push(((i * 3) % 7) as f64);
    }
    df!(
        "wage" => wage,
        "education" => education,
        "experience" => experience,
        "dept" => dept,
        "gender" => gender,
        "selected" => selected,
        "instrument" => instrument
    )
    .unwrap()
}

fn builder() -> OaxacaBuilder {
    OaxacaBuilder::new(frame(), "wage", "gender", "M")
}

// ---- F1: names are accepted in every common shape ------------------------------------------

#[test]
fn predictors_accept_every_argument_shape() {
    // The form that compiled in 0.2.2 and stopped compiling when the bound was `Into<String>`.
    builder().predictors(&["education"]).run().unwrap();
    // The rest of the shapes, each one run end to end.
    builder().predictors(["education"]).run().unwrap();
    builder().predictors(vec!["education"]).run().unwrap();
    builder()
        .predictors(vec![String::from("education")])
        .run()
        .unwrap();
    let owned: Vec<String> = vec!["education".to_string(), "experience".to_string()];
    builder().predictors(&owned).run().unwrap();
    let borrowed: &[&str] = &["education", "experience"];
    builder()
        .predictors(borrowed.iter().copied())
        .run()
        .unwrap();
    builder()
        .predictors(owned.iter().map(String::as_str))
        .run()
        .unwrap();
}

#[test]
fn categorical_predictors_accept_every_argument_shape() {
    let owned: Vec<String> = vec!["dept".to_string()];
    let mut b = builder();
    b.predictors(&["education"]);
    b.categorical_predictors(&["dept"]);
    b.run().unwrap();
    b.categorical_predictors(["dept"]);
    b.run().unwrap();
    b.categorical_predictors(vec![String::from("dept")]);
    b.run().unwrap();
    b.categorical_predictors(&owned);
    b.run().unwrap();
    b.categorical_predictors(owned.iter().map(String::as_str));
    b.run().unwrap();
}

#[test]
fn normalize_accepts_every_argument_shape() {
    let owned: Vec<String> = vec!["dept".to_string()];
    let mut b = builder();
    b.predictors(&["education"])
        .categorical_predictors(&["dept"]);
    b.normalize(&["dept"]);
    b.run().unwrap();
    b.normalize(["dept"]);
    b.run().unwrap();
    b.normalize(vec![String::from("dept")]);
    b.run().unwrap();
    b.normalize(&owned);
    b.run().unwrap();
    b.normalize(owned.iter().map(String::as_str));
    b.run().unwrap();
}

#[test]
fn heckman_selection_accepts_every_argument_shape() {
    let owned: Vec<String> = vec!["instrument".to_string()];
    let mut b = builder();
    b.predictors(&["education"]);
    b.heckman_selection("selected", &["instrument"]);
    b.heckman_selection("selected", ["instrument"]);
    b.heckman_selection("selected", vec![String::from("instrument")]);
    b.heckman_selection("selected", &owned);
    b.heckman_selection("selected", owned.iter().map(String::as_str));
}

#[test]
#[allow(deprecated)]
fn machado_mata_builder_accepts_every_argument_shape() {
    use oaxaca_blinder::QuantileDecompositionBuilder;
    let owned: Vec<String> = vec!["education".to_string()];
    let mut b = QuantileDecompositionBuilder::new(frame(), "wage", "gender", "M");
    b.predictors(&["education"]);
    b.predictors(["education"]);
    b.predictors(vec![String::from("education")]);
    b.predictors(&owned);
    b.predictors(owned.iter().map(String::as_str));
    b.categorical_predictors(&["dept"]);
    b.categorical_predictors(["dept"]);
    b.categorical_predictors(vec![String::from("dept")]);
    b.categorical_predictors(&owned);
    b.categorical_predictors(owned.iter().map(String::as_str));
}

// ---- F2: one documented default for the bootstrap -------------------------------------------

#[test]
fn default_bootstrap_is_one_hundred_replicates_with_a_finite_interval() {
    // No `bootstrap_reps` call anywhere.
    let results = builder().predictors(&["education"]).run().unwrap();
    let md = &results.run_metadata;
    assert_eq!(md.bootstrap_reps_requested, 100);
    assert_eq!(
        md.bootstrap_reps_succeeded + md.bootstrap_reps_discarded,
        100
    );
    assert!(
        md.bootstrap_reps_succeeded >= 41,
        "a percentile interval needs 41 replicates, got {}",
        md.bootstrap_reps_succeeded
    );
    let unexplained = results.unexplained().unwrap();
    assert!(unexplained.ci_lower.is_finite() && unexplained.ci_upper.is_finite());
    assert!(unexplained.ci_lower <= unexplained.estimate);
    assert!(unexplained.estimate <= unexplained.ci_upper);
    assert!(unexplained.std_err.is_finite() && unexplained.std_err > 0.0);
}

#[test]
fn default_bootstrap_constant_and_formula_constructor_agree() {
    assert_eq!(DEFAULT_BOOTSTRAP_REPS, 100);
    let results = OaxacaBuilder::from_formula(frame(), "wage ~ education", "gender", "M")
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(results.run_metadata.bootstrap_reps_requested, 100);
}

#[test]
fn an_explicit_replicate_count_still_wins() {
    let results = builder()
        .predictors(&["education"])
        .bootstrap_reps(45)
        .run()
        .unwrap();
    assert_eq!(results.run_metadata.bootstrap_reps_requested, 45);
}

// ---- F3: the default reference is the reference group's own coefficients -------------------

#[test]
fn default_reference_coefficients_are_group_b() {
    assert_eq!(
        ReferenceCoefficients::default(),
        ReferenceCoefficients::GroupB
    );

    let run = |reference: Option<ReferenceCoefficients>| {
        let mut b = builder();
        b.predictors(&["education", "experience"]).bootstrap_reps(5);
        if let Some(r) = reference {
            b.reference_coefficients(r);
        }
        b.run().unwrap().unexplained().unwrap().estimate
    };
    let by_default = run(None);
    let group_b = run(Some(ReferenceCoefficients::GroupB));
    let group_a = run(Some(ReferenceCoefficients::GroupA));
    assert_eq!(by_default, group_b, "the default must be GroupB");
    assert_ne!(
        by_default, group_a,
        "0.2.2's default (GroupA) must give a different split on this data"
    );
}

// ---- F6: enums that grow are non_exhaustive -------------------------------------------------
// Each match lists every variant and then a wildcard. Under `#![deny(unreachable_patterns)]` the
// wildcard is an error unless the enum is `#[non_exhaustive]`, so removing the attribute from an
// enum fails this file's compilation, not just a test.

#[test]
#[allow(deprecated)]
fn reference_coefficients_is_non_exhaustive() {
    let name = |r: ReferenceCoefficients| match r {
        ReferenceCoefficients::GroupA => "a",
        ReferenceCoefficients::GroupB => "b",
        ReferenceCoefficients::Pooled => "pooled",
        ReferenceCoefficients::PooledNoIndicator => "pooled-no-indicator",
        ReferenceCoefficients::Weighted => "weighted",
        ReferenceCoefficients::Cotton => "cotton",
        ReferenceCoefficients::Neumark => "neumark",
        _ => "future",
    };
    assert_eq!(name(ReferenceCoefficients::GroupB), "b");
}

#[test]
fn weights_kind_is_non_exhaustive() {
    let name = |k: WeightsKind| match k {
        WeightsKind::Frequency => "frequency",
        WeightsKind::Relative => "relative",
        _ => "future",
    };
    assert_eq!(name(WeightsKind::Relative), "relative");
}

#[test]
fn normalization_convention_is_non_exhaustive() {
    let name = |c: NormalizationConvention| match c {
        NormalizationConvention::PopulationShare => "population-share",
        NormalizationConvention::EqualShare => "equal-share",
        _ => "future",
    };
    assert_eq!(name(NormalizationConvention::default()), "population-share");
}

#[test]
fn exclusion_reason_is_non_exhaustive() {
    let name = |r: ExclusionReason| match r {
        ExclusionReason::Outcome => "outcome",
        ExclusionReason::NumericPredictor => "numeric",
        ExclusionReason::CategoricalPredictor => "categorical",
        ExclusionReason::Weights => "weights",
        ExclusionReason::SelectionOutcome => "selection-outcome",
        ExclusionReason::SelectionPredictor => "selection-predictor",
        ExclusionReason::GroupValue => "group",
        _ => "future",
    };
    assert_eq!(name(ExclusionReason::Weights), "weights");
}

#[test]
fn oaxaca_error_is_non_exhaustive() {
    let kind = |e: &OaxacaError| match e {
        OaxacaError::PolarsError(_) => "polars",
        OaxacaError::ColumnNotFound(_) => "column",
        OaxacaError::InvalidGroupVariable(_) => "group",
        OaxacaError::NalgebraError(_) => "nalgebra",
        OaxacaError::DiagnosticError(_) => "diagnostic",
        OaxacaError::InsufficientData(_) => "data",
        OaxacaError::EmptyLevelInGroup { .. } => "empty-level",
        OaxacaError::TooManyGroupValues { .. } => "too-many",
        OaxacaError::ReferenceGroupAbsent { .. } => "reference-absent",
        OaxacaError::UnknownReferenceCoefficients { .. } => "unknown-reference",
        OaxacaError::NormalizationError(_) => "normalization",
        OaxacaError::WeightsKindRequired { .. } => "weights-kind",
        OaxacaError::InvalidWeight { .. } => "weight",
        _ => "future",
    };
    assert_eq!(kind(&OaxacaError::ColumnNotFound("x".into())), "column");
}

// ---- L6: the experimental feature is off unless asked for ------------------------------------

#[test]
fn experimental_is_not_a_default_feature() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let default_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("default"))
        .expect("a `default = [...]` feature line");
    assert!(
        !default_line.contains("experimental"),
        "`experimental` must stay off by default: {default_line}"
    );
    assert!(
        manifest.lines().any(|l| l.trim() == "experimental = []"),
        "the `experimental` feature must exist"
    );
}

// ---- L6: the experimental methods take names the same way ----------------------------------

#[cfg(feature = "experimental")]
mod experimental {
    use super::frame;
    use oaxaca_blinder::{AkmBuilder, MatchingEngine};

    #[test]
    fn akm_controls_and_matching_covariates_accept_every_argument_shape() {
        let owned: Vec<String> = vec!["education".to_string()];
        let mut akm = AkmBuilder::new(frame(), "wage", "dept", "gender");
        akm.controls(&["education"]);
        akm.controls(["education"]);
        akm.controls(vec![String::from("education")]);
        akm.controls(&owned);
        akm.controls(owned.iter().map(String::as_str));

        let _ = MatchingEngine::new(frame(), "selected", "wage", &["education"]);
        let _ = MatchingEngine::new(frame(), "selected", "wage", ["education"]);
        let _ = MatchingEngine::new(frame(), "selected", "wage", vec![String::from("education")]);
        let _ = MatchingEngine::new(frame(), "selected", "wage", &owned);
        let _ = MatchingEngine::new(
            frame(),
            "selected",
            "wage",
            owned.iter().map(String::as_str),
        );
    }
}
