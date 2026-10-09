//! 0120-MERIDIAN S7 / T14 / V7: prediction intervals, defensibility and the frontier's p-value
//! use Student t on the baseline regression's residual degrees of freedom, at the level the
//! request asks for.
//!
//! Oracle: R `predict.lm(interval = "prediction", level)` for every interval; `pt()` for every
//! p-value; the pooled `lm` for the frontier's group test. The 10 000-row employers fixture has
//! 5105 residual df, where t and z agree to four digits; the 8-row `diag_df5` fixture has 5,
//! where t = 2.5706 and z = 1.9600, so an implementation that kept z cannot pass.

#[path = "../../oaxaca_blinder/tests/support/diag_golden.rs"]
mod diag_golden;

use diag_golden::*;
use pay_equity_engine::analysis::{calculate_efficient_frontier_inner, optimize_inner};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::support::{two_sided_p, DEFENSIBLE_TOLERANCE};
use pay_equity_engine::types::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

fn golden() -> &'static DiagGolden {
    static G: OnceLock<DiagGolden> = OnceLock::new();
    G.get_or_init(|| DiagGolden::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")))
}

fn case(name: &str) -> &'static Value {
    golden().block(&["intervals", name])
}

fn fixture(name: &str) -> &'static str {
    match name {
        "employers" => "employers_trust_fixture.csv",
        "df5" => "diag_df5.csv",
        _ => panic!("{name}"),
    }
}

fn predictors(name: &str) -> Vec<String> {
    case(name)["predictors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn decomposition(name: &str) -> DecompositionRequest {
    let c = case(name);
    DecompositionRequest {
        csv_data: golden().csv(fixture(name)),
        outcome_variable: c["outcome"].as_str().unwrap().to_string(),
        group_variable: c["group"].as_str().unwrap().to_string(),
        reference_group: c["reference_group"].as_str().unwrap().to_string(),
        predictors: predictors(name),
        categorical_predictors: None,
        three_fold: None,
        quantile: None,
        reference_coefficients: Some("GroupB".to_string()),
        bootstrap_reps: Some(0),
    }
}

fn optimisation(name: &str, confidence: Option<f64>) -> OptimizationRequest {
    let d = decomposition(name);
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
        forensic_mode: Some(true),
        adjust_both_groups: None,
        confidence_level: confidence,
        range_target: None,
    }
}

/// statrs' Student t quantile and CDF against R's `qt` / `pt`: measured worst 2.4e-12 relative
/// (4888 and 4889 df); pinned at 1e-10. The intervals built from them are held to 1e-9.
const TOL_DISTRIBUTION: f64 = 1e-10;

const LEVELS: [(&str, f64); 3] = [("0.90", 0.90), ("0.95", 0.95), ("0.99", 0.99)];

fn by_index(res: &OptimizationResult) -> BTreeMap<usize, &Adjustment> {
    res.adjustments.iter().map(|a| (a.index, a)).collect()
}

fn check_rows(label: &str, rows: &Value, got: &BTreeMap<usize, &Adjustment>, tol: f64) -> f64 {
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
                tol,
            );
            worst = worst.max(rel(engine, f(row, k)));
        }
    }
    worst
}

#[test]
fn v7_prediction_intervals_equal_predict_lm_at_three_levels() {
    for name in ["employers", "df5"] {
        let c = case(name);
        let mut worst = 0.0_f64;
        for (key, level) in LEVELS {
            let res = optimize_inner(optimisation(name, Some(level))).unwrap();
            let want = &c["levels"][key];
            assert_eq!(
                res.interval.degrees_of_freedom as i64,
                c["residual_df"].as_i64().unwrap()
            );
            assert!((res.interval.confidence_level - level).abs() < 1e-15);
            assert_close(
                &format!("{name} {key} critical value"),
                res.interval.critical_value,
                f(want, "critical"),
                TOL_DISTRIBUTION,
            );
            let rows = by_index(&res);
            worst = worst.max(check_rows(
                &format!("{name} {key} target"),
                &want["target"],
                &rows,
                1e-9,
            ));
            worst = worst.max(check_rows(
                &format!("{name} {key} reference"),
                &want["reference"],
                &rows,
                1e-9,
            ));
        }
        println!("{name}: engine vs predict.lm, worst relative difference {worst:e}");
    }
}

#[test]
fn v7_the_five_df_fixture_is_where_t_and_z_part() {
    // The gate must be able to fail: at 5 residual df the 95% multiplier is 2.5706, not 1.9600.
    let want = f(&case("df5")["levels"]["0.95"], "critical");
    assert!((want - 2.570581835636314).abs() < 1e-12);
    let res = optimize_inner(optimisation("df5", None)).unwrap();
    assert!((res.interval.critical_value - 1.959963984540054).abs() > 0.5);
    // A z-based bound for the highest-leverage target row would be off by far more than the
    // test tolerance: the half-width shrinks by 1.96/2.5706.
    let target = &case("df5")["levels"]["0.95"]["target"].as_array().unwrap();
    let widest = target
        .iter()
        .map(|r| (f(r, "upr") - f(r, "lwr")) / 2.0)
        .fold(0.0_f64, f64::max);
    let z_half = widest * 1.959963984540054 / want;
    assert!(
        widest - z_half > 100.0,
        "t half-width {widest} vs z {z_half}"
    );
}

#[test]
fn v7_the_level_is_clamped_and_defaults_to_95() {
    let default = optimize_inner(optimisation("df5", None)).unwrap();
    assert_eq!(default.interval.confidence_level, 0.95);
    let low = optimize_inner(optimisation("df5", Some(0.2))).unwrap();
    assert_eq!(low.interval.confidence_level, 0.50);
    let high = optimize_inner(optimisation("df5", Some(2.0))).unwrap();
    assert_eq!(high.interval.confidence_level, 0.999);
}

/// E-REV F1 (contract): under `range_target` LowerBound / UpperBound the payment IS the interval
/// bound, so the t-based bound moves remedy dollars. The oracle is `predict.lm`'s own `lwr` /
/// `upr` for each compared row: the payment is `max(0, bound - wage)` and the new wage is
/// `max(wage, bound)`. (Midpoint payments never touched the interval.)
#[test]
fn v7_range_target_payments_equal_the_predict_lm_bound_minus_the_wage() {
    for name in ["df5", "employers"] {
        let c = case(name);
        for (target, key) in [
            (RangeTarget::LowerBound, "lwr"),
            (RangeTarget::UpperBound, "upr"),
        ] {
            let mut req = optimisation(name, Some(0.95));
            req.range_target = Some(target.clone());
            req.strategy = Some(AllocationStrategy::Greedy);
            req.budget = 1.0e12;
            let res = optimize_inner(req).unwrap();
            let got = by_index(&res);
            let mut paid = 0;
            let mut worst = 0.0_f64;
            for row in c["levels"]["0.95"]["target"].as_array().unwrap() {
                let ordinal = row["ordinal"].as_u64().unwrap() as usize;
                let a = got[&ordinal];
                let (bound, wage) = (f(row, key), f(row, "wage"));
                let want = (bound - wage).max(0.0);
                let err = (a.adjustment - want).abs();
                assert!(
                    err <= 1e-9 * wage.abs(),
                    "{name} {key} row {ordinal}: paid {} against predict.lm bound - wage {want}",
                    a.adjustment
                );
                worst = worst.max(err / wage.abs());
                assert!(
                    (a.new_wage - wage.max(bound)).abs() <= 1e-9 * wage.abs(),
                    "{name} {key} row {ordinal}: new wage {} vs max(wage, bound) {}",
                    a.new_wage,
                    wage.max(bound)
                );
                if want > 0.0 {
                    paid += 1;
                }
            }
            assert!(
                paid > 0,
                "{name} {key}: no compared row is paid to its bound; the case has no teeth"
            );
            println!("{name} {key}: payment vs predict.lm, worst relative difference {worst:e}");
        }
    }
}

#[test]
fn v7_the_range_target_oracle_can_fail_a_normal_theory_bound_is_far_off_at_five_df() {
    // On the 5-df fixture the z half-width is 76% of the t half-width, so a z-based LowerBound
    // payment differs from R's by hundreds of dollars on some compared row.
    let c = case("df5");
    let t = f(&c["levels"]["0.95"], "critical");
    let z = 1.959963984540054;
    let worst = c["levels"]["0.95"]["target"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let (fair, lwr, wage) = (f(row, "fair"), f(row, "lwr"), f(row, "wage"));
            let z_bound = fair - (fair - lwr) * z / t;
            ((lwr - wage).max(0.0) - (z_bound - wage).max(0.0)).abs()
        })
        .fold(0.0_f64, f64::max);
    assert!(
        worst > 100.0,
        "a z bound would move a payment by only {worst}"
    );
}

fn defensibility_request(
    name: &str,
    confidence: Option<f64>,
    adjustments: &[(usize, f64)],
) -> VerificationRequest {
    VerificationRequest {
        decomposition_params: decomposition(name),
        adjustments: adjustments
            .iter()
            .map(|&(index, value)| ProposedAdjustment {
                index,
                row_key: None,
                value,
                predictor_overrides: None,
            })
            .collect(),
        confidence_level: confidence,
    }
}

#[test]
fn v7_defensibility_floor_is_one_cent_below_the_r_lower_bound() {
    // df5 target rows; R's lower bound at 95% and at 99%.
    let c = case("df5");
    let rows95 = c["levels"]["0.95"]["target"].as_array().unwrap();
    let rows99 = c["levels"]["0.99"]["target"].as_array().unwrap();
    let pick = |rows: &Vec<Value>, i: usize| {
        (
            rows[i]["ordinal"].as_u64().unwrap() as usize,
            f(&rows[i], "lwr"),
            f(&rows[i], "wage"),
        )
    };
    let (i0, lwr0, wage0) = pick(rows95, 0);
    let (i1, lwr1, wage1) = pick(rows95, 1);
    let (i2, lwr2, wage2) = pick(rows95, 2);
    // new_wage = lower - 0.005 (inside the cent), lower - 0.02 (outside it), lower + 5.
    let adjustments = [
        (i0, lwr0 - 0.005 - wage0),
        (i1, lwr1 - 0.02 - wage1),
        (i2, lwr2 + 5.0 - wage2),
    ];
    let res = check_defensibility_inner(defensibility_request("df5", None, &adjustments)).unwrap();
    let verdict = |i: usize| {
        res.adjustments
            .iter()
            .find(|a| a.index == i)
            .unwrap()
            .is_defensible
            .unwrap()
    };
    assert_eq!(DEFENSIBLE_TOLERANCE, 0.01);
    assert!(
        verdict(i0),
        "half a cent below the floor is inside the one-cent slack"
    );
    assert!(
        !verdict(i1),
        "two cents below the floor is outside it (the old $1 slack passed it)"
    );
    assert!(verdict(i2));
    let msg = res
        .adjustments
        .iter()
        .find(|a| a.index == i1)
        .unwrap()
        .defensibility_message
        .clone()
        .unwrap();
    assert!(
        msg.starts_with("Wage is 0.02 below the defensible lower bound ("),
        "{msg}"
    );

    // The level comes from the request: at 99% the floor of row 1 drops by far more than two
    // cents, so the same adjustment is now inside the range.
    let (_, lwr1_99, _) = pick(rows99, 1);
    assert!(lwr1 - lwr1_99 > 1.0);
    let wide =
        check_defensibility_inner(defensibility_request("df5", Some(0.99), &adjustments)).unwrap();
    assert!(wide
        .adjustments
        .iter()
        .find(|a| a.index == i1)
        .unwrap()
        .is_defensible
        .unwrap());
    assert_eq!(wide.interval.confidence_level, 0.99);
    // And at 99% the engine's bound is R's.
    let rows = by_index(&wide);
    for r in rows99 {
        let i = r["ordinal"].as_u64().unwrap() as usize;
        if let Some(a) = rows.get(&i) {
            assert_close(
                "defensibility lwr at 99%",
                a.fair_wage_lower_bound.unwrap(),
                f(r, "lwr"),
                1e-9,
            );
        }
    }
}

#[test]
fn v7_the_p_value_function_equals_pt() {
    for p in golden().block(&["t_grid"]).as_array().unwrap() {
        assert_close(
            &format!("2*pt(-{}, {})", f(p, "t"), f(p, "df")),
            two_sided_p(f(p, "t"), f(p, "df")),
            f(p, "p"),
            TOL_DISTRIBUTION,
        );
    }
}

fn frontier(name: &str, confidence: Option<f64>) -> Vec<FrontierPoint> {
    calculate_efficient_frontier_inner(EfficientFrontierRequest {
        decomposition_params: decomposition(name),
        steps: Some(4),
        max_budget: Some(5000.0),
        confidence_level: confidence,
    })
    .unwrap()
}

#[test]
fn v7_frontier_zero_budget_point_is_the_pooled_group_test() {
    for name in ["employers", "df5"] {
        let want = &case(name)["pooled_group_test"];
        let first = &frontier(name, None)[0];
        assert_eq!(first.budget, 0.0);
        assert_eq!(
            first.degrees_of_freedom as i64,
            want["df"].as_i64().unwrap(),
            "{name}"
        );
        assert_close(
            &format!("{name} group coefficient"),
            first.group_coefficient,
            f(want, "coefficient"),
            1e-9,
        );
        assert_close(&format!("{name} t"), first.t_statistic, f(want, "t"), 1e-9);
        assert_close(&format!("{name} p (pt)"), first.p_value, f(want, "p"), 1e-9);
    }
}

#[test]
fn v7_every_frontier_point_carries_a_student_t_p_and_the_requested_threshold() {
    let points = frontier("df5", None);
    assert!(points.len() >= 2);
    for pt in &points {
        assert_close(
            "p = 2 pt(-|t|, df)",
            pt.p_value,
            two_sided_p(pt.t_statistic, pt.degrees_of_freedom as f64),
            1e-14,
        );
        assert_eq!(pt.is_significant, pt.p_value < 0.05);
    }
    // df5 at zero budget: p = 0.0055. Significant at 95%, not at 99.9%: the level is honoured.
    assert!(points[0].is_significant);
    let strict = frontier("df5", Some(0.999));
    assert!(!strict[0].is_significant);
    assert_eq!(strict[0].p_value, points[0].p_value);
    // Normal theory would have said p = 0.0004 here; t on 10 df says 0.0055.
    assert!(points[0].p_value > 0.004);
}
