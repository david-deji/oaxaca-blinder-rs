//! 0120-MERIDIAN T8: the optimiser's `Pooled` target is the decomposition's `Pooled` line.
//!
//! Before T8 the Pooled arm stacked both groups with no group column, which is Neumark's
//! `PooledNoIndicator`. The remedy was priced against one line while the decomposition headline
//! it sits beside was measured against another, so the two "unexplained" dollars could not be
//! reconciled. The Pooled target now fits the pooled regression WITH a target-group indicator,
//! drops the indicator, and reads every fair wage at indicator 0; the interval comes from that
//! same fit (its sigma^2, (X'X)^-1 and residual df), so each bound is `predict.lm` on the pooled
//! fit exactly.
//!
//! Oracle: R `lm` / `predict.lm(interval = "prediction")` / `hatvalues` / `qt`, plus the R
//! `oaxaca` package's pooled-with-indicator weight, by `verification/gen_pooled_target_goldens.R`
//! into `oaxaca_blinder/tests/fixtures/pooled_target_goldens_r.json`. NO EXPECTED VALUE HERE
//! COMES FROM ENGINE OUTPUT. The one engine-against-engine comparison (`optimize` against
//! `decompose` on the same file) is a second statement of the same identity, and R's `gamma`
//! stands behind both.
//!
//! The golden is refused when the recorded sha256 of the generator or of any fixture no longer
//! matches the file on disk: a stale golden is an error, not a pass.

use pay_equity_engine::analysis::{decompose_inner, optimize_inner};
use pay_equity_engine::defensibility::{check_defensibility_inner, check_defensibility_on};
use pay_equity_engine::types::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const TOL: f64 = 1e-9;
/// statrs' Student t quantile against R's `qt` (measured worst 2.4e-12 relative).
const TOL_CRITICAL: f64 = 1e-10;
const LEVELS: [(&str, f64); 3] = [("0.90", 0.90), ("0.95", 0.95), ("0.99", 0.99)];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn fixtures_dir() -> PathBuf {
    root().join("oaxaca_blinder/tests/fixtures")
}

fn sha256_file(path: &Path) -> String {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn golden() -> &'static Value {
    static G: OnceLock<Value> = OnceLock::new();
    G.get_or_init(|| {
        let text = std::fs::read_to_string(fixtures_dir().join("pooled_target_goldens_r.json"))
            .expect("pooled_target_goldens_r.json committed and readable");
        let json: Value = serde_json::from_str(&text).expect("golden parses");
        assert_eq!(
            json["_meta"]["expected_values_from_engine_output"].as_bool(),
            Some(false),
            "the golden must declare that no expected value comes from engine output"
        );
        assert_eq!(
            json["_meta"]["generator_sha256"].as_str().unwrap(),
            sha256_file(&root().join("verification/gen_pooled_target_goldens.R")),
            "stale golden: verification/gen_pooled_target_goldens.R changed since the golden was \
             generated. Re-run it and commit the result."
        );
        for (name, want) in json["_meta"]["fixture_sha256"].as_object().unwrap() {
            assert_eq!(
                want.as_str().unwrap(),
                sha256_file(&fixtures_dir().join(name)),
                "stale golden: fixture {name} changed since the golden was generated. \
                 Re-run verification/gen_pooled_target_goldens.R and commit the result."
            );
        }
        json
    })
}

fn case(name: &str) -> &'static Value {
    let c = &golden()["cases"][name];
    assert!(!c.is_null(), "golden has no case {name}");
    c
}

fn fixture_file(name: &str) -> &'static str {
    match name {
        "employers" | "employers_cat" => "employers_trust_fixture.csv",
        "df5" => "diag_df5.csv",
        "nooverlap" => "diag_nooverlap.csv",
        "tiny" => "diag_tiny.csv",
        "skewed" => "norm_skewed_fixture.csv",
        "balanced" => "norm_balanced_fixture.csv",
        _ => panic!("{name}"),
    }
}

const CASES: [&str; 7] = [
    "employers",
    "employers_cat",
    "df5",
    "nooverlap",
    "tiny",
    "skewed",
    "balanced",
];

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn csv(name: &str) -> Vec<u8> {
    std::fs::read(fixtures_dir().join(fixture_file(name))).unwrap()
}

fn f(v: &Value, key: &str) -> f64 {
    v[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{key} is not a number in {v}"))
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / 1.0_f64.max(a.abs()).max(b.abs())
}

fn assert_close(label: &str, got: f64, want: f64, tol: f64) {
    assert!(
        rel(got, want) <= tol,
        "{label}: engine={got:.17e} oracle={want:.17e} rel diff {:.3e} > {tol:.1e}",
        rel(got, want)
    );
}

/// The Pooled-target request for a case: every row listed (forensic), the default midpoint.
fn request(name: &str, target: OptimizationTarget, confidence: Option<f64>) -> OptimizationRequest {
    let c = case(name);
    let cats = strings(&c["categorical"]);
    OptimizationRequest {
        csv_data: csv(name),
        outcome_variable: c["outcome"].as_str().unwrap().to_string(),
        group_variable: c["group"].as_str().unwrap().to_string(),
        reference_group: c["reference_group"].as_str().unwrap().to_string(),
        predictors: strings(&c["predictors"]),
        categorical_predictors: if cats.is_empty() { None } else { Some(cats) },
        budget: 0.0,
        target_gap: None,
        target: Some(target),
        strategy: None,
        min_gap_pct: None,
        forensic_mode: Some(true),
        adjust_both_groups: None,
        confidence_level: confidence,
        range_target: None,
    }
}

fn pooled(name: &str, confidence: Option<f64>) -> OptimizationResult {
    optimize_inner(request(name, OptimizationTarget::Pooled, confidence)).unwrap()
}

fn decomposition(name: &str) -> DecompositionRequest {
    let c = case(name);
    let cats = strings(&c["categorical"]);
    DecompositionRequest {
        csv_data: csv(name),
        outcome_variable: c["outcome"].as_str().unwrap().to_string(),
        group_variable: c["group"].as_str().unwrap().to_string(),
        reference_group: c["reference_group"].as_str().unwrap().to_string(),
        predictors: strings(&c["predictors"]),
        categorical_predictors: if cats.is_empty() { None } else { Some(cats) },
        three_fold: None,
        quantile: None,
        reference_coefficients: Some("Pooled".to_string()),
        bootstrap_reps: Some(0),
    }
}

fn by_index(res: &OptimizationResult) -> std::collections::BTreeMap<usize, &Adjustment> {
    res.adjustments.iter().map(|a| (a.index, a)).collect()
}

#[test]
fn t8_original_unexplained_gap_is_the_pooled_indicator_coefficient() {
    // Midpoint: the target group's mean shortfall to the pooled line is gamma, the coefficient
    // of the group indicator in `lm(y ~ x + group)`. R's `lm` and `oaxaca` weight -2 say so.
    let mut worst = 0.0_f64;
    for name in CASES {
        let res = pooled(name, None);
        let gamma = f(case(name), "gamma");
        worst = worst.max(rel(res.original_unexplained_gap, gamma));
        assert_close(
            &format!("{name}: original_unexplained_gap vs R lm group coefficient"),
            res.original_unexplained_gap,
            gamma,
            TOL,
        );
    }
    println!("original_unexplained_gap vs R gamma, worst relative difference {worst:e}");
}

#[test]
fn t8_optimiser_gap_equals_the_decompositions_pooled_unexplained() {
    // The same file through the decomposition under `Pooled`: one line, one number (1e-9).
    let mut worst = 0.0_f64;
    for name in ["employers", "employers_cat", "df5", "skewed", "balanced"] {
        let opt = pooled(name, None);
        let dec = decompose_inner(decomposition(name)).unwrap();
        worst = worst.max(rel(opt.original_unexplained_gap, dec.unexplained_gap));
        assert_close(
            &format!("{name}: optimise vs decompose Pooled unexplained"),
            opt.original_unexplained_gap,
            dec.unexplained_gap,
            TOL,
        );
    }
    println!("optimise vs decompose Pooled unexplained, worst relative difference {worst:e}");
}

#[test]
fn t8_fair_wage_terms_are_the_pooled_coefficients_without_the_indicator() {
    for name in CASES {
        let c = case(name);
        let res = pooled(name, None);
        let intercept = res
            .model_coefficients
            .iter()
            .find(|t| t.name == oaxaca_blinder::INTERCEPT_NAME)
            .unwrap_or_else(|| panic!("{name}: no intercept term"));
        assert_close(
            &format!("{name}: intercept"),
            intercept.value,
            f(c, "intercept"),
            TOL,
        );
        let mut matched = 0;
        for term in &res.model_coefficients {
            if let Some(want) = c["coefficients"].get(&term.name).and_then(Value::as_f64) {
                assert_close(&format!("{name}: {}", term.name), term.value, want, TOL);
                matched += 1;
            }
        }
        assert_eq!(
            matched,
            strings(&c["predictors"]).len(),
            "{name}: every continuous predictor's coefficient is compared with R's"
        );
        // The group indicator is not a term of the fair wage; one extra column would show here.
        assert_eq!(
            res.model_coefficients.len() as u64,
            c["model_columns"].as_u64().unwrap(),
            "{name}: the indicator's coefficient must not be a model term"
        );
        // A row's contributions add up to its fair wage: the indicator is not hiding in it.
        for a in res.adjustments.iter().take(20) {
            let sum: f64 = a.contributions.iter().map(|t| t.value).sum();
            assert_close(&format!("{name}: contributions sum"), sum, a.fair_wage, TOL);
        }
    }
}

fn check_rows(
    label: &str,
    rows: &Value,
    got: &std::collections::BTreeMap<usize, &Adjustment>,
) -> f64 {
    let mut worst = 0.0_f64;
    for row in rows.as_array().unwrap() {
        let ordinal = row["ordinal"].as_u64().unwrap() as usize;
        let a = got
            .get(&ordinal)
            .unwrap_or_else(|| panic!("{label}: the engine returned no row {ordinal}"));
        for (k, engine) in [
            ("fair", a.fair_wage),
            ("lwr", a.fair_wage_lower_bound.unwrap()),
            ("upr", a.fair_wage_upper_bound.unwrap()),
        ] {
            assert_close(
                &format!("{label} row {ordinal} {k}"),
                engine,
                f(row, k),
                TOL,
            );
            worst = worst.max(rel(engine, f(row, k)));
        }
    }
    worst
}

#[test]
fn t8_bounds_equal_predict_lm_on_the_pooled_fit_at_three_levels() {
    let mut worst = 0.0_f64;
    for name in CASES {
        let c = case(name);
        for (key, level) in LEVELS {
            let res = pooled(name, Some(level));
            let want = &c["levels"][key];
            assert_eq!(
                res.interval.degrees_of_freedom as u64,
                c["residual_df"].as_u64().unwrap(),
                "{name} {key}: interval df is the pooled fit's n - k - 1"
            );
            assert_close(
                &format!("{name} {key} critical value"),
                res.interval.critical_value,
                f(want, "critical"),
                TOL_CRITICAL,
            );
            let rows = by_index(&res);
            worst = worst.max(check_rows(
                &format!("{name} {key} target"),
                &want["target"],
                &rows,
            ));
            worst = worst.max(check_rows(
                &format!("{name} {key} reference"),
                &want["reference"],
                &rows,
            ));
        }
    }
    println!("engine vs predict.lm(pooled fit), worst relative difference {worst:e}");
}

#[test]
fn t8_the_pooled_interval_is_not_the_baseline_only_interval() {
    // The gate must be able to fail: on `df5` the baseline-only fit has 5 residual df and the
    // pooled fit 10, so the critical values differ (2.5706 against 2.2281 at 95%).
    let c = case("df5");
    assert_eq!(c["baseline_only_residual_df"].as_u64(), Some(5));
    assert_eq!(c["residual_df"].as_u64(), Some(10));
    let res = pooled("df5", None);
    assert_eq!(res.interval.degrees_of_freedom, 10);
    assert!((res.interval.critical_value - 2.570581835636314).abs() > 0.3);
    // The Reference target keeps the baseline-only fit.
    let reference = optimize_inner(request("df5", OptimizationTarget::Reference, None)).unwrap();
    assert_eq!(reference.interval.degrees_of_freedom, 5);
    assert!((reference.interval.critical_value - 2.570581835636314).abs() < 1e-9);
}

#[test]
fn t8_extrapolated_rows_are_the_pooled_leverage_set() {
    // A row is extrapolated when (x, 0)' (Z'Z)^-1 (x, 0) exceeds the largest leverage among the
    // reference rows of the pooled design. Compared row by row as an exact set.
    for name in CASES {
        let c = case(name);
        assert_eq!(
            c["rows_near_line"].as_u64(),
            Some(0),
            "{name}: a row sits within 1e-6 of the leverage line; the tolerance would decide it"
        );
        let want: BTreeSet<usize> = c["extrapolated_ordinals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        let res = pooled(name, None);
        let got: BTreeSet<usize> = res
            .adjustments
            .iter()
            .filter(|a| a.extrapolated)
            .map(|a| a.index)
            .collect();
        assert_eq!(got, want, "{name}: extrapolated ordinals");
        assert_eq!(
            res.support.extrapolated_target_count as u64,
            c["extrapolated_target_count"].as_u64().unwrap(),
            "{name}: support.extrapolated_target_count"
        );
    }
    // The set must not be empty everywhere, or the test could not fail.
    assert!(
        case("nooverlap")["extrapolated_target_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(case("df5")["extrapolated_target_count"].as_u64().unwrap() > 0);
}

#[test]
fn t8_few_residual_df_judges_the_pooled_fit_under_subject_pooled() {
    let few: Vec<(String, f64)> = pooled("tiny", None)
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::FewResidualDf)
        .map(|w| (w.subject.clone().unwrap_or_default(), w.value))
        .collect();
    // 14 rows, 4 model columns + the indicator: 9 residual df, below the line of 10. The
    // reference group alone has 3, but it is not the fit that produced the interval.
    assert_eq!(few, vec![("pooled".to_string(), 9.0)], "tiny: {few:?}");
    // df5: the baseline-only fit has 5 residual df (would warn), the pooled fit has 10 (does not).
    let df5 = pooled("df5", None);
    assert!(
        df5.warnings
            .iter()
            .all(|w| w.code != WarningCode::FewResidualDf),
        "df5 pooled fit has 10 residual df: {:?}",
        df5.warnings
    );
    let reference = optimize_inner(request("df5", OptimizationTarget::Reference, None)).unwrap();
    assert!(
        reference
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::FewResidualDf
                && w.subject.as_deref() == Some("reference"))
    );
}

fn tiny_csv(reference_rows: usize, target_rows: usize) -> Vec<u8> {
    let mut s = String::from("pay,grp,x\n");
    for i in 0..reference_rows {
        s.push_str(&format!("{},A,{}\n", 100 + 7 * i + (i * i) % 5, i));
    }
    for i in 0..target_rows {
        s.push_str(&format!("{},B,{}\n", 90 + 7 * i + (i * i) % 3, i));
    }
    s.into_bytes()
}

fn tiny_request(csv: Vec<u8>, target: OptimizationTarget) -> OptimizationRequest {
    OptimizationRequest {
        csv_data: csv,
        outcome_variable: "pay".to_string(),
        group_variable: "grp".to_string(),
        reference_group: "A".to_string(),
        predictors: vec!["x".to_string()],
        categorical_predictors: None,
        budget: 0.0,
        target_gap: None,
        target: Some(target),
        strategy: None,
        min_gap_pct: None,
        forensic_mode: None,
        adjust_both_groups: None,
        confidence_level: None,
        range_target: None,
    }
}

#[test]
fn t8_a_pooled_fit_with_no_residual_df_is_refused_by_name_and_a_small_baseline_is_not() {
    // 2 + 1 rows, 3 columns (intercept, x, indicator): no residual df in the pooled fit.
    let e = optimize_inner(tiny_request(tiny_csv(2, 1), OptimizationTarget::Pooled))
        .err()
        .unwrap();
    assert!(
        e.starts_with("INSUFFICIENT_RESIDUAL_DF: group=pooled, rows=3, model_columns=3"),
        "{e}"
    );
    // A baseline of 2 rows cannot carry its own line (2 rows, 2 columns): refused under
    // Reference, by the baseline's name. The pooled line over 2 + 6 rows has 5 df and is fine.
    let e = optimize_inner(tiny_request(tiny_csv(2, 6), OptimizationTarget::Reference))
        .err()
        .unwrap();
    assert!(
        e.starts_with("INSUFFICIENT_RESIDUAL_DF: group=reference, rows=2"),
        "{e}"
    );
    let ok = optimize_inner(tiny_request(tiny_csv(2, 6), OptimizationTarget::Pooled)).unwrap();
    assert_eq!(ok.interval.degrees_of_freedom, 5);
}

// ---------------------------------------------------------------------------------------------
// Review N8 ("E2-c"): `check_defensibility` judges the amounts on the line the remedy priced.
//
// Before, the remedy under `Pooled` read its bounds and extension marks off the pooled fit while
// the check that scores the proposed amounts always read the Reference line, so the ledger chips
// (the remedy's rows) and the "after adjustment" card (the check's rows) could disagree on one
// screen. The oracle is the same R golden: every bound, fair wage, critical value, df and
// extrapolated ordinal the optimiser is held to is held here too, on the check's own rows.
// ---------------------------------------------------------------------------------------------

fn verification(
    name: &str,
    confidence: Option<f64>,
    rows: &OptimizationResult,
) -> VerificationRequest {
    VerificationRequest {
        decomposition_params: decomposition(name),
        adjustments: rows
            .adjustments
            .iter()
            .map(|a| ProposedAdjustment {
                index: a.index,
                row_key: None,
                value: a.adjustment,
                predictor_overrides: None,
            })
            .collect(),
        confidence_level: confidence,
    }
}

fn defended(
    name: &str,
    target: &OptimizationTarget,
    confidence: Option<f64>,
) -> OptimizationResult {
    // Every row is listed, with the amounts the Pooled optimiser proposes for it.
    let opt = pooled(name, confidence);
    check_defensibility_on(verification(name, confidence, &opt), target).unwrap()
}

#[test]
fn n8_defensibility_on_the_pooled_target_equals_predict_lm_on_the_pooled_fit() {
    let mut worst = 0.0_f64;
    for name in CASES {
        let c = case(name);
        for (key, level) in LEVELS {
            let res = defended(name, &OptimizationTarget::Pooled, Some(level));
            let want = &c["levels"][key];
            assert_eq!(
                res.interval.degrees_of_freedom as u64,
                c["residual_df"].as_u64().unwrap(),
                "{name} {key}: the check's interval df is the pooled fit's n - k - 1"
            );
            assert_close(
                &format!("{name} {key} check critical value"),
                res.interval.critical_value,
                f(want, "critical"),
                TOL_CRITICAL,
            );
            let rows = by_index(&res);
            worst = worst.max(check_rows(
                &format!("{name} {key} check target"),
                &want["target"],
                &rows,
            ));
            worst = worst.max(check_rows(
                &format!("{name} {key} check reference"),
                &want["reference"],
                &rows,
            ));
        }
    }
    println!("check_defensibility(Pooled) vs predict.lm(pooled fit), worst relative difference {worst:e}");
}

#[test]
fn n8_the_checks_extension_marks_are_the_pooled_leverage_set_and_equal_the_remedys() {
    for name in CASES {
        let want: BTreeSet<usize> = case(name)["extrapolated_ordinals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        let checked = defended(name, &OptimizationTarget::Pooled, None);
        let got: BTreeSet<usize> = checked
            .adjustments
            .iter()
            .filter(|a| a.extrapolated)
            .map(|a| a.index)
            .collect();
        assert_eq!(
            got, want,
            "{name}: the check's extrapolated ordinals (R hatvalues)"
        );
        // The remedy and its check mark the same people: one screen, one set.
        let remedy: BTreeSet<usize> = pooled(name, None)
            .adjustments
            .iter()
            .filter(|a| a.extrapolated)
            .map(|a| a.index)
            .collect();
        assert_eq!(
            got, remedy,
            "{name}: remedy and check disagree on extension"
        );
        assert_eq!(
            checked.support.extrapolated_target_count as u64,
            case(name)["extrapolated_target_count"].as_u64().unwrap(),
            "{name}: support.extrapolated_target_count of the check"
        );
    }
}

#[test]
fn n8_the_default_target_is_the_reference_line_and_changes_nothing() {
    // df5: the baseline-only fit has 5 residual df, the pooled fit 10. The default (and an explicit
    // Reference) stays on 5, as every caller before this field saw.
    let opt = pooled("df5", None);
    let by_default = check_defensibility_inner(verification("df5", None, &opt)).unwrap();
    let explicit = check_defensibility_on(
        verification("df5", None, &opt),
        &OptimizationTarget::Reference,
    )
    .unwrap();
    assert_eq!(by_default.interval.degrees_of_freedom, 5);
    assert_eq!(explicit.interval.degrees_of_freedom, 5);
    assert_eq!(
        serde_json::to_string(&by_default).unwrap(),
        serde_json::to_string(&explicit).unwrap(),
        "an explicit Reference is the default, byte for byte"
    );
    let pooled_check =
        check_defensibility_on(verification("df5", None, &opt), &OptimizationTarget::Pooled)
            .unwrap();
    assert_eq!(pooled_check.interval.degrees_of_freedom, 10);
    assert!(
        (pooled_check.interval.critical_value - by_default.interval.critical_value).abs() > 0.3,
        "the gate can fail: 2.2281 against 2.5706"
    );
}

#[test]
fn n8_the_checks_few_df_warning_names_the_pooled_fit_and_a_pooled_fit_with_no_df_is_refused() {
    let tiny = defended("tiny", &OptimizationTarget::Pooled, None);
    let few: Vec<(String, f64)> = tiny
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::FewResidualDf)
        .map(|w| (w.subject.clone().unwrap_or_default(), w.value))
        .collect();
    assert_eq!(few, vec![("pooled".to_string(), 9.0)], "tiny: {few:?}");

    let csv = tiny_csv(2, 1);
    let req = VerificationRequest {
        decomposition_params: DecompositionRequest {
            csv_data: csv,
            outcome_variable: "pay".to_string(),
            group_variable: "grp".to_string(),
            reference_group: "A".to_string(),
            predictors: vec!["x".to_string()],
            categorical_predictors: None,
            three_fold: None,
            quantile: None,
            reference_coefficients: Some("Pooled".to_string()),
            bootstrap_reps: Some(0),
        },
        adjustments: vec![],
        confidence_level: None,
    };
    let e = check_defensibility_on(req, &OptimizationTarget::Pooled)
        .err()
        .unwrap();
    assert!(
        e.starts_with("INSUFFICIENT_RESIDUAL_DF: group=pooled, rows=3, model_columns=3"),
        "{e}"
    );
}

#[test]
fn n8_the_request_carries_the_target_through_two_levels_of_flatten() {
    // The WASM entry deserialises `DefensibilityRequest`, which flattens `VerificationRequest`, which
    // flattens `DecompositionRequest`. A field lost in that nesting would silently run the Reference
    // line, so the wire shape is pinned: the target rides beside the decomposition fields.
    let body = r#"{
        "csv_data": [112, 97, 121, 10],
        "outcome_variable": "pay", "group_variable": "grp", "reference_group": "A",
        "predictors": ["x"], "reference_coefficients": "Pooled",
        "adjustments": [{"index": 3, "value": 10.0, "predictor_overrides": null}],
        "confidence_level": 0.9,
        "target": "Pooled"
    }"#;
    let req: DefensibilityRequest = serde_json::from_str(body).unwrap();
    assert!(matches!(req.target, Some(OptimizationTarget::Pooled)));
    assert_eq!(req.verification.confidence_level, Some(0.9));
    assert_eq!(req.verification.adjustments.len(), 1);
    assert_eq!(req.verification.decomposition_params.reference_group, "A");
    let without = body.replace(r#""target": "Pooled""#, r#""unrelated": 1"#);
    let req: DefensibilityRequest = serde_json::from_str(&without).unwrap();
    assert!(
        req.target.is_none(),
        "absent means the default Reference line"
    );
}
