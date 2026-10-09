//! 0120-MERIDIAN S6 / T12 / T13 / V6: support and small-sample diagnostics.
//!
//! Every expected number comes from `verification/gen_diag_goldens.R` (base R, `ddecompose`),
//! never from engine output. The golden covers the five committed fixtures and the four
//! no-overlap / tiny files the re-ground built to separate the benign cases from the broken ones.
//!
//! The thresholds are restated here as literals (5% outside the range, |normalised difference|
//! 0.25, 10 residual df), so moving one in the engine turns a named test red.

#[path = "../../oaxaca_blinder/tests/support/diag_golden.rs"]
mod diag_golden;

use diag_golden::*;
use pay_equity_engine::analysis::{
    calculate_efficient_frontier_inner, decompose_inner, optimize_inner, verify_inner,
};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::types::*;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;

fn golden() -> &'static DiagGolden {
    static G: OnceLock<DiagGolden> = OnceLock::new();
    G.get_or_init(|| DiagGolden::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")))
}

/// (golden case, fixture file)
const CASES: [(&str, &str); 10] = [
    ("employers", "employers_trust_fixture.csv"),
    ("parity", "parity_fixture.csv"),
    ("fixture_f", "0118-fixture-f.csv"),
    ("mem_profile_50k", "mem_profile_50k.csv"),
    ("wage5", "wage.csv"),
    ("nooverlap", "diag_nooverlap.csv"),
    ("linear_nooverlap", "diag_linear_nooverlap.csv"),
    ("kink_overlap", "diag_kink_overlap.csv"),
    ("tiny", "diag_tiny.csv"),
    ("df5", "diag_df5.csv"),
];

fn fixture_of(case: &str) -> &'static str {
    CASES.iter().find(|(c, _)| *c == case).unwrap().1
}

fn strings(v: &Value) -> Vec<String> {
    // jsonlite writes a one-element vector as a bare string.
    if let Some(one) = v.as_str() {
        return vec![one.to_string()];
    }
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn decomposition(case: &str) -> DecompositionRequest {
    let g = golden();
    let c = g.block(&["support", case]);
    DecompositionRequest {
        csv_data: g.csv(fixture_of(case)),
        outcome_variable: c["outcome"].as_str().unwrap().to_string(),
        group_variable: c["group"].as_str().unwrap().to_string(),
        reference_group: c["reference_group"].as_str().unwrap().to_string(),
        predictors: strings(&c["predictors"]),
        categorical_predictors: None,
        three_fold: None,
        quantile: None,
        reference_coefficients: Some("GroupB".to_string()),
        bootstrap_reps: Some(0),
    }
}

fn optimisation(case: &str, forensic: bool) -> OptimizationRequest {
    let d = decomposition(case);
    OptimizationRequest {
        csv_data: d.csv_data,
        outcome_variable: d.outcome_variable,
        group_variable: d.group_variable,
        reference_group: d.reference_group,
        predictors: d.predictors,
        categorical_predictors: None,
        budget: 0.0,
        target_gap: None,
        target: None,
        strategy: None,
        min_gap_pct: None,
        forensic_mode: Some(forensic),
        adjust_both_groups: None,
        confidence_level: None,
        range_target: None,
    }
}

fn codes(warnings: &[DiagnosticWarning]) -> Vec<(WarningCode, Option<String>)> {
    let mut v: Vec<_> = warnings
        .iter()
        .map(|w| (w.code, w.subject.clone()))
        .collect();
    v.sort_by_key(|(c, s)| (format!("{c:?}"), s.clone()));
    v
}

/// What the thresholds say should fire, derived from the R numbers alone.
fn expected_warnings(case: &str, fitted_both: bool) -> Vec<(WarningCode, Option<String>)> {
    let c = golden().block(&["support", case]);
    let mut out = Vec::new();
    for (name, p) in c["per_predictor"].as_object().unwrap() {
        if f(p, "target_outside_range_share") > 0.05 {
            out.push((WarningCode::OutsideRange, Some(name.clone())));
        }
        if let Some(nd) = p["normalised_difference"].as_f64() {
            if nd.abs() > 0.25 {
                out.push((WarningCode::NormalisedDifference, Some(name.clone())));
            }
        }
    }
    if c["reference_residual_df"].as_i64().unwrap() < 10 {
        out.push((WarningCode::FewResidualDf, Some("reference".to_string())));
    }
    if fitted_both && c["target_residual_df"].as_i64().unwrap() < 10 {
        out.push((WarningCode::FewResidualDf, Some("target".to_string())));
    }
    out.sort_by_key(|(c, s)| (format!("{c:?}"), s.clone()));
    out
}

#[test]
fn v6_support_block_equals_r_on_every_fixture() {
    let mut worst = 0.0_f64;
    for (case, _) in CASES {
        let want = golden().block(&["support", case]);
        let res = decompose_inner(decomposition(case))
            .unwrap_or_else(|e| panic!("{case}: decompose failed: {e}"));
        let s = &res.support;
        assert_eq!(
            s.reference_count as i64,
            want["reference_count"].as_i64().unwrap(),
            "{case}"
        );
        assert_eq!(
            s.target_count as i64,
            want["target_count"].as_i64().unwrap(),
            "{case}"
        );
        assert_eq!(
            s.model_columns as i64,
            want["model_columns"].as_i64().unwrap(),
            "{case}"
        );
        assert_eq!(
            s.reference_residual_df,
            want["reference_residual_df"].as_i64().unwrap(),
            "{case}"
        );
        assert_eq!(
            s.target_residual_df,
            want["target_residual_df"].as_i64().unwrap(),
            "{case}"
        );
        assert_eq!(
            s.extrapolated_target_count as i64,
            want["extrapolated_target_count"].as_i64().unwrap(),
            "{case}: target rows beyond the baseline's largest leverage"
        );
        let preds = want["per_predictor"].as_object().unwrap();
        assert_eq!(s.predictors.len(), preds.len(), "{case}");
        for p in &s.predictors {
            let w = &preds[&p.name];
            let at = |k: &str| format!("{case}.{}.{k}", p.name);
            for (k, got) in [
                ("reference_min", p.reference_min),
                ("reference_max", p.reference_max),
                ("reference_p01", p.reference_p01),
                ("reference_p99", p.reference_p99),
                ("target_min", p.target_min),
                ("target_max", p.target_max),
                ("target_outside_range_share", p.target_outside_range_share),
                (
                    "target_outside_p01_p99_share",
                    p.target_outside_p01_p99_share,
                ),
            ] {
                assert_close(&at(k), got, f(w, k), 1e-12);
                worst = worst.max(rel(got, f(w, k)));
            }
            match (p.normalised_difference, w["normalised_difference"].as_f64()) {
                (Some(got), Some(want)) => {
                    assert_close(&at("normalised_difference"), got, want, 1e-12);
                    // The documented relation to ddecompose: Imbens-Rubin = ddecompose * sqrt(2).
                    assert_close(
                        &at("normalised_difference vs ddecompose"),
                        got,
                        f(w, "ddecompose_normalized_difference") * 2.0_f64.sqrt(),
                        1e-12,
                    );
                }
                (None, None) => {}
                (got, want) => panic!("{}: {got:?} vs {want:?}", at("normalised_difference")),
            }
        }
    }
    println!("support block vs R: worst relative difference {worst:e}");
}

#[test]
fn v6_warnings_fire_exactly_where_the_thresholds_say() {
    for (case, _) in CASES {
        let res = decompose_inner(decomposition(case)).unwrap();
        assert_eq!(
            codes(&res.warnings),
            expected_warnings(case, true),
            "{case}: decompose warnings"
        );
        // optimise and defensibility fit the baseline group only
        let opt = optimize_inner(optimisation(case, false)).unwrap();
        assert_eq!(
            codes(&opt.warnings),
            expected_warnings(case, false),
            "{case}: optimise warnings"
        );
        assert_eq!(
            opt.support, res.support,
            "{case}: optimise carries the same support block as decompose"
        );
    }
}

#[test]
fn v6_ordinary_fixtures_are_silent_on_range_extrapolation_and_degrees_of_freedom() {
    for case in ["employers", "fixture_f", "mem_profile_50k", "parity"] {
        let res = decompose_inner(decomposition(case)).unwrap();
        assert_eq!(res.support.extrapolated_target_count, 0, "{case}");
        for w in &res.warnings {
            assert!(
                w.code == WarningCode::NormalisedDifference,
                "{case}: an ordinary fixture raised {w:?}"
            );
        }
        if case != "parity" {
            assert!(res.warnings.is_empty(), "{case}: {:?}", res.warnings);
        }
    }
    // The parity fixture's education and experience differ by 0.45 and 0.39 pooled standard
    // deviations between the groups (R). The Imbens-Rubin rule of thumb is 0.25, so they carry
    // the normalised-difference warning while the range and leverage checks stay silent.
    let parity = decompose_inner(decomposition("parity")).unwrap();
    let subjects: Vec<_> = parity
        .warnings
        .iter()
        .map(|w| w.subject.clone().unwrap())
        .collect();
    assert_eq!(subjects, ["education", "experience"]);
}

#[test]
fn v6_no_overlap_and_tiny_files_fire() {
    for case in ["nooverlap", "linear_nooverlap"] {
        let res = decompose_inner(decomposition(case)).unwrap();
        assert_eq!(res.support.extrapolated_target_count, 60, "{case}");
        let range = res
            .warnings
            .iter()
            .find(|w| w.code == WarningCode::OutsideRange)
            .unwrap();
        assert_eq!(range.subject.as_deref(), Some("edu"));
        assert!((range.value - 1.0).abs() < 1e-12 && range.threshold == 0.05);
        let nd = res
            .warnings
            .iter()
            .find(|w| w.code == WarningCode::NormalisedDifference)
            .unwrap();
        assert!(nd.value.abs() > 6.0, "{case}: {nd:?}");
    }
    let tiny = decompose_inner(decomposition("tiny")).unwrap();
    assert!(tiny
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::FewResidualDf
            && w.subject.as_deref() == Some("reference")
            && w.value == 3.0));
    assert!(tiny
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::FewResidualDf && w.subject.as_deref() == Some("target")));
    assert!(tiny
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::OutsideRange));
    assert_eq!(tiny.support.extrapolated_target_count, 4);
    // Full overlap with a nonlinear truth: the range and leverage checks are silent, only the
    // two groups' very different education levels (a normalised difference of -1.1) is reported.
    let kink = decompose_inner(decomposition("kink_overlap")).unwrap();
    assert_eq!(kink.support.extrapolated_target_count, 0);
    assert!(kink
        .warnings
        .iter()
        .all(|w| w.code == WarningCode::NormalisedDifference));
}

#[test]
fn v6_the_remedy_rows_carry_the_extrapolated_flag() {
    // Every compared employee on the no-overlap file sits beyond the baseline's leverage.
    let res = optimize_inner(optimisation("nooverlap", true)).unwrap();
    let target = golden().block(&["support", "nooverlap"]);
    let n_target = target["target_count"].as_u64().unwrap() as usize;
    let flagged = res.adjustments.iter().filter(|a| a.extrapolated).count();
    assert_eq!(flagged, n_target, "all compared rows are extrapolated");
    assert_eq!(
        res.adjustments.len(),
        2 * n_target,
        "forensic mode lists both groups"
    );
    // ...and no baseline row is: they define the maximum (the flagged count above is exact).
    assert_eq!(res.support.extrapolated_target_count, n_target);

    // The ordinary roster: nothing flagged.
    let employers = optimize_inner(optimisation("employers", false)).unwrap();
    assert!(!employers.adjustments.is_empty());
    assert!(employers.adjustments.iter().all(|a| !a.extrapolated));
}

#[test]
fn v6_defensibility_rows_carry_the_extrapolated_flag_too() {
    let adjustments: Vec<ProposedAdjustment> = (0..3)
        .map(|i| ProposedAdjustment {
            index: 1 + 2 * i,
            row_key: None,
            value: 100.0,
            predictor_overrides: None,
        })
        .collect();
    let mk = |case: &str| VerificationRequest {
        decomposition_params: decomposition(case),
        adjustments: adjustments
            .iter()
            .map(|a| ProposedAdjustment {
                index: a.index,
                row_key: None,
                value: a.value,
                predictor_overrides: None,
            })
            .collect(),
        confidence_level: None,
    };
    let d = check_defensibility_inner(mk("nooverlap")).unwrap();
    // nooverlap alternates M, F, M...; odd ordinals are the compared (F) rows when they exist.
    assert!(d.adjustments.iter().any(|a| a.extrapolated));
    assert_eq!(d.support.extrapolated_target_count, 60);
    let e = check_defensibility_inner(mk("employers")).unwrap();
    assert!(e.adjustments.iter().all(|a| !a.extrapolated));
}

fn tiny_csv(reference_rows: usize, target_rows: usize) -> Vec<u8> {
    let mut s = String::from("wage,grp,a,b,c\n");
    for i in 0..reference_rows {
        s.push_str(&format!(
            "{},M,{},{},{}\n",
            50000 + 1300 * i + (i * i) % 7 * 90,
            10 + i,
            (i * 7 + 3) % 11,
            (i * i * 5 + i) % 13
        ));
    }
    for i in 0..target_rows {
        s.push_str(&format!(
            "{},F,{},{},{}\n",
            48000 + 1100 * i + (i * i) % 5 * 70,
            11 + i,
            (i * 5 + 1) % 11,
            (i * i * 3 + 2 * i) % 13
        ));
    }
    s.into_bytes()
}

fn tiny_decomposition(reference_rows: usize, target_rows: usize) -> DecompositionRequest {
    DecompositionRequest {
        csv_data: tiny_csv(reference_rows, target_rows),
        outcome_variable: "wage".into(),
        group_variable: "grp".into(),
        reference_group: "M".into(),
        predictors: vec!["a".into(), "b".into(), "c".into()],
        categorical_predictors: None,
        three_fold: None,
        quantile: None,
        reference_coefficients: Some("GroupB".into()),
        bootstrap_reps: Some(0),
    }
}

#[test]
fn t13_a_baseline_with_no_residual_df_is_refused_by_name_on_every_entry() {
    // 4 baseline rows against 4 model columns (intercept + a + b + c): zero residual df.
    let expect =
        "INSUFFICIENT_RESIDUAL_DF: group=reference, rows=4, model_columns=4, residual_df=0";
    let d = decompose_inner(tiny_decomposition(4, 9)).err().unwrap();
    assert!(d.starts_with(expect), "{d}");
    let v = verify_inner(VerificationRequest {
        decomposition_params: tiny_decomposition(4, 9),
        adjustments: vec![],
        confidence_level: None,
    })
    .err()
    .unwrap();
    assert!(v.starts_with(expect), "{v}");
    let opt = |reference_rows| {
        let r = tiny_decomposition(reference_rows, 9);
        OptimizationRequest {
            csv_data: r.csv_data,
            outcome_variable: r.outcome_variable,
            group_variable: r.group_variable,
            reference_group: r.reference_group,
            predictors: r.predictors,
            categorical_predictors: None,
            budget: 0.0,
            target_gap: None,
            target: None,
            strategy: None,
            min_gap_pct: None,
            forensic_mode: None,
            adjust_both_groups: None,
            confidence_level: None,
            range_target: None,
        }
    };
    let o = optimize_inner(opt(4)).err().unwrap();
    assert!(o.starts_with(expect), "{o}");
    let def = check_defensibility_inner(VerificationRequest {
        decomposition_params: tiny_decomposition(4, 9),
        adjustments: vec![],
        confidence_level: None,
    })
    .err()
    .unwrap();
    assert!(def.starts_with(expect), "{def}");
    let fr = calculate_efficient_frontier_inner(EfficientFrontierRequest {
        decomposition_params: tiny_decomposition(4, 9),
        steps: Some(3),
        max_budget: Some(1000.0),
        confidence_level: None,
    })
    .err()
    .unwrap();
    assert!(fr.starts_with(expect), "{fr}");

    // One row more is estimable: 1 residual df, and the warning says so.
    let ok = decompose_inner(tiny_decomposition(5, 9)).unwrap();
    assert_eq!(ok.support.reference_residual_df, 1);
    assert!(ok
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::FewResidualDf
            && w.subject.as_deref() == Some("reference")
            && w.value == 1.0
            && w.threshold == 10.0));
    let five = optimize_inner(opt(5));
    assert!(five.is_ok(), "{:?}", five.err());
}

#[test]
fn t13_a_target_group_too_small_to_fit_stops_decompose_but_not_the_remedy() {
    // Decompose fits BOTH groups; the remedy fits only the baseline.
    let e = decompose_inner(tiny_decomposition(12, 4)).err().unwrap();
    assert!(
        e.starts_with("INSUFFICIENT_RESIDUAL_DF: group=target, rows=4, model_columns=4"),
        "{e}"
    );
    let r = tiny_decomposition(12, 4);
    let res = optimize_inner(OptimizationRequest {
        csv_data: r.csv_data,
        outcome_variable: r.outcome_variable,
        group_variable: r.group_variable,
        reference_group: r.reference_group,
        predictors: r.predictors,
        categorical_predictors: None,
        budget: 0.0,
        target_gap: None,
        target: None,
        strategy: None,
        min_gap_pct: None,
        forensic_mode: None,
        adjust_both_groups: None,
        confidence_level: None,
        range_target: None,
    })
    .unwrap();
    assert_eq!(res.support.target_residual_df, 0);
    assert!(
        res.warnings
            .iter()
            .all(|w| w.subject.as_deref() != Some("target")),
        "the target group is not fitted by the remedy: {:?}",
        res.warnings
    );
}

#[test]
fn the_new_blocks_are_in_the_json_a_consumer_reads() {
    let res = decompose_inner(decomposition("employers")).unwrap();
    let v = serde_json::to_value(&res).unwrap();
    assert!(v["support"]["predictors"].is_array());
    assert!(v["warnings"].is_array());
    assert!(
        v.get("quantile_report").is_none(),
        "mean mode carries no percentile report"
    );
    let w = serde_json::to_value(&decompose_inner(decomposition("parity")).unwrap().warnings[0])
        .unwrap();
    assert_eq!(w["code"], "normalised_difference");
    assert_eq!(w["subject"], "education");
}
