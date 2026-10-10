//! 0122-MERIDIAN: the remedy's reported figures against an independent R oracle.
//!
//! `verification/gen_remedy_goldens.R` re-implements the remedy in base R (`lm`, `predict.lm`,
//! `uniroot`, `summary.lm`) and writes `fixtures/remedy_goldens_r.json`. NO EXPECTED VALUE HERE
//! COMES FROM ENGINE OUTPUT. In particular the R side REFITS the pay line on the schedule's wages
//! and ROOT-FINDS the budget for a target gap, where the engine uses exact linear weights and a
//! closed form or a walk along the pay order, so a wrong weight, a wrong sign or a wrong order is a
//! disagreement, not an echo.
//!
//! Gates carried (re-ground § 5): V1 target-gap rule (Reference, Pooled, both strategies, already
//! met, unreachable, capped), V2 best reachable gap under thresholds and range targets, V3/V4 the
//! post-schedule gap under reference raises and a Pooled line, V5 closure, V6 unfunded counts,
//! V7 both entry points on one schedule, V8 positions, V9 the group test.
//!
//! The golden is refused when the recorded sha256 of the generator or of any fixture no longer
//! matches the file on disk.

use pay_equity_engine::analysis::optimize_inner;
use pay_equity_engine::defensibility::check_defensibility_on;
use pay_equity_engine::types::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Absolute tolerance on dollars, and the relative floor for large totals.
const DOLLARS: f64 = 1e-6;
const RELATIVE: f64 = 1e-10;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
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
        let path = root().join("engine/tests/fixtures/remedy_goldens_r.json");
        let text = std::fs::read_to_string(&path).expect("remedy_goldens_r.json committed");
        let json: Value = serde_json::from_str(&text).expect("golden parses");
        assert_eq!(
            json["_meta"]["expected_values_from_engine_output"].as_bool(),
            Some(false),
            "the golden must declare that no expected value comes from engine output"
        );
        assert_eq!(
            json["_meta"]["generator_sha256"].as_str().unwrap(),
            sha256_file(&root().join("verification/gen_remedy_goldens.R")),
            "stale golden: verification/gen_remedy_goldens.R changed since the golden was \
             generated. Re-run it and commit the result."
        );
        for (name, want) in json["_meta"]["fixture_sha256"].as_object().unwrap() {
            assert_eq!(
                want.as_str().unwrap(),
                sha256_file(&root().join(name)),
                "stale golden: fixture {name} changed since the golden was generated."
            );
        }
        json
    })
}

fn names(kind: &str) -> Vec<String> {
    golden()["cases"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, c)| c["config"]["kind"].as_str() == Some(kind))
        .map(|(n, _)| n.clone())
        .collect()
}

fn strings(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().map(|s| s.as_str().unwrap().to_string()).collect(),
        // jsonlite writes a length-one character vector as a bare string.
        Value::String(s) => vec![s.clone()],
        _ => vec![],
    }
}

fn target_of(c: &Value) -> OptimizationTarget {
    match c["target"].as_str().unwrap() {
        "Reference" => OptimizationTarget::Reference,
        "Pooled" => OptimizationTarget::Pooled,
        other => panic!("{other}"),
    }
}

fn decomposition(c: &Value) -> DecompositionRequest {
    let cats = strings(&c["cats"]);
    DecompositionRequest {
        csv_data: std::fs::read(root().join(c["fixture"].as_str().unwrap())).unwrap(),
        outcome_variable: c["outcome"].as_str().unwrap().to_string(),
        group_variable: c["group"].as_str().unwrap().to_string(),
        reference_group: c["ref"].as_str().unwrap().to_string(),
        predictors: strings(&c["cont"]),
        categorical_predictors: if cats.is_empty() { None } else { Some(cats) },
        three_fold: None,
        quantile: None,
        reference_coefficients: Some("Pooled".to_string()),
        bootstrap_reps: Some(2),
    }
}

/// The engine request for a golden remedy case.
fn optimization(c: &Value) -> OptimizationRequest {
    let d = decomposition(c);
    OptimizationRequest {
        csv_data: d.csv_data,
        outcome_variable: d.outcome_variable,
        group_variable: d.group_variable,
        reference_group: d.reference_group,
        predictors: d.predictors,
        categorical_predictors: d.categorical_predictors,
        budget: c["budget"].as_f64().unwrap(),
        target_gap: c["target_gap"].as_f64(),
        target: Some(target_of(c)),
        strategy: Some(match c["strategy"].as_str().unwrap() {
            "Greedy" => AllocationStrategy::Greedy,
            _ => AllocationStrategy::Equitable,
        }),
        min_gap_pct: Some(c["min_pct"].as_f64().unwrap()),
        forensic_mode: Some(false),
        adjust_both_groups: Some(c["adjust_both"].as_bool().unwrap()),
        confidence_level: Some(c["level"].as_f64().unwrap()),
        range_target: Some(match c["range"].as_str().unwrap() {
            "Midpoint" => RangeTarget::Midpoint,
            "LowerBound" => RangeTarget::LowerBound,
            _ => RangeTarget::UpperBound,
        }),
    }
}

fn close(got: f64, want: f64) -> bool {
    (got - want).abs() <= DOLLARS.max(RELATIVE * want.abs())
}

fn check(case: &str, field: &str, got: f64, want: &Value) {
    let want = want
        .as_f64()
        .unwrap_or_else(|| panic!("{case}.{field}: golden is not a number: {want}"));
    if std::env::var_os("MERIDIAN_MEASURE").is_some() {
        eprintln!("MEASURED {case} {field} {:e}", (got - want).abs());
    }
    assert!(
        close(got, want),
        "{case}.{field}: engine {got:.9} vs R {want:.9} (diff {:.3e})",
        got - want
    );
}

fn check_opt(case: &str, field: &str, got: Option<f64>, want: &Value) {
    match (got, want.is_null()) {
        (None, true) => {}
        (Some(g), false) => check(case, field, g, want),
        (g, _) => panic!("{case}.{field}: engine {g:?} vs R {want}"),
    }
}

fn position_name(p: RangePosition) -> &'static str {
    match p {
        RangePosition::Below => "Below",
        RangePosition::Inside => "Inside",
        RangePosition::Above => "Above",
    }
}

fn source_name(s: RowSource) -> &'static str {
    match s {
        RowSource::Compared => "Compared",
        RowSource::Reference => "Reference",
    }
}

/// Every remedy figure the golden carries, checked against one engine run. Returns the result.
pub fn check_remedy_case(name: &str) -> OptimizationResult {
    let case = &golden()["cases"][name];
    let cfg = &case["config"];
    let want = &case["expected"];
    let res = optimize_inner(optimization(cfg)).unwrap_or_else(|e| panic!("{name}: {e}"));

    for (field, got) in [
        ("original_gap", res.original_gap),
        ("new_gap", res.new_gap),
        ("original_unexplained_gap", res.original_unexplained_gap),
        ("new_unexplained_gap", res.new_unexplained_gap),
        ("required_budget", res.required_budget),
        ("need_target", res.need_target),
        ("total_cost", res.total_cost),
        ("cost_target", res.cost_target),
        ("cost_reference", res.cost_reference),
    ] {
        check(name, field, got, &want[field]);
    }
    check(
        name,
        "need_reference",
        res.need_reference.unwrap(),
        &want["need_reference"],
    );
    check(
        name,
        "best_reachable_gap",
        res.best_reachable_gap.unwrap(),
        &want["best_reachable_gap"],
    );
    check(
        name,
        "overshoot_mean",
        res.overshoot_mean.unwrap(),
        &want["overshoot_mean"],
    );
    check_opt(name, "closure", res.closure, &want["closure"]);
    check(
        name,
        "unfunded_amount",
        res.unfunded_amount.unwrap(),
        &want["unfunded_amount"],
    );
    assert_eq!(
        res.unfunded_count.unwrap() as u64,
        want["unfunded_count"].as_u64().unwrap(),
        "{name}.unfunded_count"
    );
    assert_eq!(
        res.threshold_excluded_count.unwrap() as u64,
        want["threshold_excluded_count"].as_u64().unwrap(),
        "{name}.threshold_excluded_count"
    );
    assert_eq!(
        res.budget_binding.unwrap(),
        want["budget_binding"].as_bool().unwrap(),
        "{name}.budget_binding"
    );
    // Target-gap block.
    match want["target_gap_reachable"].as_bool() {
        None => {
            assert!(res.target_gap_reachable.is_none(), "{name}");
            assert!(res.target_budget.is_none(), "{name}");
            assert!(res.shortfall_to_target.is_none(), "{name}");
        }
        Some(reachable) => {
            assert_eq!(res.target_gap_reachable, Some(reachable), "{name}");
            check_opt(
                name,
                "target_budget",
                res.target_budget,
                &want["target_budget"],
            );
            check_opt(
                name,
                "shortfall_to_target",
                res.shortfall_to_target,
                &want["shortfall_to_target"],
            );
        }
    }

    // Per-row figures on the rows the engine lists.
    if let Some(rows) = case["rows"].as_array() {
        assert_eq!(
            res.adjustments.len(),
            rows.len(),
            "{name}: the engine lists {} rows, R lists {}",
            res.adjustments.len(),
            rows.len()
        );
        let by_index: HashMap<usize, &Adjustment> =
            res.adjustments.iter().map(|a| (a.index, a)).collect();
        for r in rows {
            let idx = r["index"].as_u64().unwrap() as usize;
            let a = by_index
                .get(&idx)
                .unwrap_or_else(|| panic!("{name}: engine lists no row {idx}"));
            let at = format!("{name}[{idx}]");
            check(&at, "adjustment", a.adjustment, &r["adjustment"]);
            check(&at, "new_wage", a.new_wage, &r["new_wage"]);
            check(&at, "fair_wage", a.fair_wage, &r["fair_wage"]);
            check(&at, "lower", a.fair_wage_lower_bound.unwrap(), &r["lower"]);
            check(&at, "upper", a.fair_wage_upper_bound.unwrap(), &r["upper"]);
            assert_eq!(source_name(a.source), r["source"].as_str().unwrap(), "{at}");
            assert_eq!(
                position_name(a.range_position),
                r["range_position"].as_str().unwrap(),
                "{at} range_position"
            );
            assert_eq!(
                position_name(a.range_position_before),
                r["range_position_before"].as_str().unwrap(),
                "{at} range_position_before"
            );
        }
    }
    res
}

#[test]
fn every_remedy_figure_equals_the_r_oracle() {
    let all = names("remedy");
    assert!(all.len() >= 40, "the golden lost cases: {}", all.len());
    for name in all {
        check_remedy_case(&name);
    }
}

/// The schedule the golden generated, as the engine's request.
fn verification(c: &Value) -> VerificationRequest {
    let mut overrides: HashMap<usize, HashMap<String, String>> = HashMap::new();
    for o in c["overrides"].as_array().unwrap() {
        overrides
            .entry(o["index"].as_u64().unwrap() as usize)
            .or_default()
            .insert(
                o["column"].as_str().unwrap().to_string(),
                format!("{}", o["value"].as_f64().unwrap()),
            );
    }
    let mut adjustments: Vec<ProposedAdjustment> = Vec::new();
    for a in c["adjustments"].as_array().unwrap() {
        let index = a["index"].as_u64().unwrap() as usize;
        adjustments.push(ProposedAdjustment {
            index,
            row_key: None,
            value: a["value"].as_f64().unwrap(),
            predictor_overrides: overrides.remove(&index),
        });
    }
    // An override on a row the schedule does not pay still has to reach the engine.
    for (index, map) in overrides {
        adjustments.push(ProposedAdjustment {
            index,
            row_key: None,
            value: 0.0,
            predictor_overrides: Some(map),
        });
    }
    VerificationRequest {
        decomposition_params: decomposition(c),
        adjustments,
        confidence_level: Some(c["level"].as_f64().unwrap()),
    }
}

pub fn check_schedule_case(name: &str) -> OptimizationResult {
    let case = &golden()["cases"][name];
    let cfg = &case["config"];
    let want = &case["expected"];
    let res = check_defensibility_on(verification(cfg), &target_of(cfg))
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    for (field, got) in [
        ("original_gap", res.original_gap),
        ("new_gap", res.new_gap),
        ("original_unexplained_gap", res.original_unexplained_gap),
        ("new_unexplained_gap", res.new_unexplained_gap),
        ("required_budget", res.required_budget),
        ("need_target", res.need_target),
        ("total_cost", res.total_cost),
        ("cost_target", res.cost_target),
        ("cost_reference", res.cost_reference),
    ] {
        check(name, field, got, &want[field]);
    }
    let counts = res.position_counts.expect("defensibility carries counts");
    for (field, got) in [
        ("below", counts.below),
        ("inside", counts.inside),
        ("above", counts.above),
        ("below_before", counts.below_before),
        ("inside_before", counts.inside_before),
        ("above_before", counts.above_before),
        ("newly_above", counts.newly_above),
    ] {
        assert_eq!(
            got as u64,
            want["position_counts"][field].as_u64().unwrap(),
            "{name}.position_counts.{field}"
        );
    }
    let test = res.group_test.clone().expect("a group test");
    let g = &want["group_test"];
    check(
        name,
        "group_coefficient",
        test.group_coefficient,
        &g["group_coefficient"],
    );
    assert!(
        (test.t_statistic - g["t_statistic"].as_f64().unwrap()).abs()
            <= 1e-8 * g["t_statistic"].as_f64().unwrap().abs().max(1.0),
        "{name}: t {} vs R {}",
        test.t_statistic,
        g["t_statistic"]
    );
    assert!(
        (test.p_value - g["p_value"].as_f64().unwrap()).abs() <= 1e-9,
        "{name}: p {} vs R {}",
        test.p_value,
        g["p_value"]
    );
    assert_eq!(
        test.degrees_of_freedom as u64,
        g["degrees_of_freedom"].as_u64().unwrap(),
        "{name}"
    );
    assert_eq!(
        test.is_significant,
        g["is_significant"].as_bool().unwrap(),
        "{name}"
    );

    if let Some(rows) = case["rows"].as_array() {
        assert_eq!(res.adjustments.len(), rows.len(), "{name}");
        let by_index: HashMap<usize, &Adjustment> =
            res.adjustments.iter().map(|a| (a.index, a)).collect();
        for r in rows {
            let idx = r["index"].as_u64().unwrap() as usize;
            let a = by_index[&idx];
            let at = format!("{name}[{idx}]");
            check(&at, "new_wage", a.new_wage, &r["new_wage"]);
            check(&at, "fair_wage", a.fair_wage, &r["fair_wage"]);
            check(&at, "lower", a.fair_wage_lower_bound.unwrap(), &r["lower"]);
            check(&at, "upper", a.fair_wage_upper_bound.unwrap(), &r["upper"]);
            assert_eq!(source_name(a.source), r["source"].as_str().unwrap(), "{at}");
            assert_eq!(
                position_name(a.range_position),
                r["range_position"].as_str().unwrap(),
                "{at}"
            );
            assert_eq!(
                position_name(a.range_position_before),
                r["range_position_before"].as_str().unwrap(),
                "{at}"
            );
            // `Below` is exactly "not defensible" (F-09).
            assert_eq!(
                a.range_position == RangePosition::Below,
                a.is_defensible == Some(false),
                "{at}"
            );
        }
    }
    res
}

#[test]
fn every_schedule_figure_equals_the_r_oracle() {
    let all = names("schedule");
    assert!(all.len() >= 10, "the golden lost schedules: {}", all.len());
    for name in all {
        check_schedule_case(&name);
    }
}
