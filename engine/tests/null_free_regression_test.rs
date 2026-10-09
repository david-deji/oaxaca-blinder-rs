//! 0118-MERIDIAN V10: files with no blank cell produce the numbers they produced before.
//!
//! The 0118 change reads employee identity from the row ordinals the builder reports instead of
//! recomputing it from the raw group column. On a file with no blank model cell and no third
//! group value those two are the same list, so every number the engine returns must match the
//! pre-0118 engine. This test pins that.
//!
//! HOW THE GOLDEN WAS MADE. `fixtures/0118-null-free-golden.txt` holds one line per case,
//! `name<TAB>canonical-json`: the engine's result (serde_json) with the four fields 0118 added
//! removed (`analysed_reference_count`, `analysed_target_count`, `excluded_rows`,
//! `adjustments_on_excluded_rows`). It was recorded by running THIS FILE against the pre-0118
//! engine source (commit 8e4e552, `main` before the 0118 epic), where those four fields do not
//! exist and the removal is a no-op. It is not regenerated from the current engine: a changed
//! number here is a changed number on a null-free file.
//!
//! TOLERANCE. Strings, bools, nulls, integers, object keys and array lengths must match exactly.
//! Every float must satisfy |a-b| <= 1e-9 * max(1, |a|, |b|). Bit-identity across machines is not
//! available: the linear algebra underneath (nalgebra over matrixmultiply) picks AVX/FMA or
//! scalar kernels at run time from the CPU it finds, and those round the last bit differently,
//! so the same engine gives different low bits on different x86-64 hosts. 1e-9 sits about six
//! orders of magnitude above that noise and far below any real change. The worst relative
//! difference per case is printed with --nocapture and named in the failure message.
//!
//!   MERIDIAN_PRINT_GOLDEN=json cargo test -p pay-equity-engine --test null_free_regression_test -- --nocapture
//!   (prints `JSON<TAB>name<TAB>json` per case; strip the `JSON<TAB>` prefix to build the golden)

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::{
    decomposition_request, frontier_request, optimization_request, verification_request,
};
use pay_equity_engine::analysis::{
    calculate_efficient_frontier_inner, decompose_inner, optimize_inner, verify_inner,
};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::types::{
    AllocationStrategy, OptimizationTarget, ProposedAdjustment, RangeTarget,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use support::FixtureF;

const ADDED_BY_0118: [&str; 4] = [
    "analysed_reference_count",
    "analysed_target_count",
    "excluded_rows",
    "adjustments_on_excluded_rows",
];

fn canonical<T: Serialize>(result: &T) -> String {
    let mut v = serde_json::to_value(result).expect("result serializes");
    if let Value::Object(map) = &mut v {
        for key in ADDED_BY_0118 {
            map.remove(key);
        }
    }
    serde_json::to_string(&v).unwrap()
}

fn adjustments() -> Vec<ProposedAdjustment> {
    let mut overrides = HashMap::new();
    overrides.insert("Experience".to_string(), "9".to_string());
    vec![
        ProposedAdjustment {
            index: 1,
            row_key: None,
            value: 1500.0,
            predictor_overrides: None,
        },
        ProposedAdjustment {
            index: 13,
            row_key: None,
            value: 250.0,
            predictor_overrides: Some(overrides),
        },
        ProposedAdjustment {
            index: 98,
            row_key: None,
            value: 2650.0,
            predictor_overrides: None,
        },
        ProposedAdjustment {
            index: 52,
            row_key: None,
            value: 100.0,
            predictor_overrides: None,
        },
    ]
}

/// Every case: (name, canonical JSON of the result minus the 0118 fields).
fn cases() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |name: &str, json: String| out.push((name.to_string(), json));

    let exact = FixtureF::exact().csv_bytes();
    let noisy = FixtureF::noisy().csv_bytes();
    let dept = FixtureF::noisy().with_dept().csv_bytes();
    let parity = include_bytes!("../../oaxaca_blinder/tests/fixtures/parity_fixture.csv").to_vec();

    // ---- decompose ----
    for (label, csv, categorical) in [
        ("exact", &exact, false),
        ("noisy", &noisy, false),
        ("noisy+dept", &dept, true),
    ] {
        let mut req = decomposition_request(csv.clone(), categorical);
        req.bootstrap_reps = Some(20);
        add(
            &format!("decompose/{label}/pooled"),
            canonical(&decompose_inner(req).unwrap()),
        );
    }
    for coefficients in ["GroupA", "GroupB", "Weighted"] {
        let mut req = decomposition_request(noisy.clone(), false);
        req.bootstrap_reps = Some(20);
        req.reference_coefficients = Some(coefficients.to_string());
        add(
            &format!("decompose/noisy/{coefficients}"),
            canonical(&decompose_inner(req).unwrap()),
        );
    }
    let mut req = decomposition_request(noisy.clone(), false);
    req.bootstrap_reps = Some(20);
    req.three_fold = Some(true);
    add(
        "decompose/noisy/three_fold",
        canonical(&decompose_inner(req).unwrap()),
    );
    for q in [0.25, 0.5] {
        let mut req = decomposition_request(noisy.clone(), false);
        req.bootstrap_reps = Some(20);
        req.quantile = Some(q);
        add(
            &format!("decompose/noisy/quantile{q}"),
            canonical(&decompose_inner(req).unwrap()),
        );
    }
    add(
        "decompose/parity_fixture",
        canonical(
            &decompose_inner(pay_equity_engine::types::DecompositionRequest {
                csv_data: parity,
                outcome_variable: "log_wage".to_string(),
                group_variable: "gender".to_string(),
                reference_group: "F".to_string(),
                predictors: vec![
                    "education".to_string(),
                    "experience".to_string(),
                    "tenure".to_string(),
                ],
                categorical_predictors: None,
                three_fold: Some(true),
                quantile: None,
                reference_coefficients: None,
                bootstrap_reps: Some(20),
            })
            .unwrap(),
        ),
    );

    // ---- optimize ----
    let optimize =
        |csv: &Vec<u8>,
         categorical: bool,
         edit: &dyn Fn(&mut pay_equity_engine::types::OptimizationRequest)| {
            let mut req = optimization_request(csv.clone(), categorical);
            edit(&mut req);
            canonical(&optimize_inner(req).unwrap())
        };
    add("optimize/exact/default", optimize(&exact, false, &|_| {}));
    add("optimize/noisy/default", optimize(&noisy, false, &|_| {}));
    add(
        "optimize/noisy+dept/default",
        optimize(&dept, true, &|_| {}),
    );
    add(
        "optimize/noisy/budget25000",
        optimize(&noisy, false, &|r| r.budget = 25_000.0),
    );
    add(
        "optimize/noisy/equitable25000",
        optimize(&noisy, false, &|r| {
            r.budget = 25_000.0;
            r.strategy = Some(AllocationStrategy::Equitable);
        }),
    );
    add(
        "optimize/noisy/forensic_both",
        optimize(&noisy, false, &|r| {
            r.forensic_mode = Some(true);
            r.adjust_both_groups = Some(true);
        }),
    );
    add(
        "optimize/noisy/lower_bound",
        optimize(&noisy, false, &|r| {
            r.range_target = Some(RangeTarget::LowerBound)
        }),
    );
    add(
        "optimize/noisy/upper_bound",
        optimize(&noisy, false, &|r| {
            r.range_target = Some(RangeTarget::UpperBound)
        }),
    );
    add(
        "optimize/noisy/pooled_target",
        optimize(&noisy, false, &|r| {
            r.target = Some(OptimizationTarget::Pooled)
        }),
    );
    add(
        "optimize/noisy/min_gap_2pct_conf90",
        optimize(&noisy, false, &|r| {
            r.min_gap_pct = Some(0.02);
            r.confidence_level = Some(0.9);
        }),
    );

    // ---- verify_adjustments ----
    for (label, csv) in [("exact", &exact), ("noisy", &noisy)] {
        let mut req = verification_request(csv.clone(), false, adjustments());
        req.decomposition_params.bootstrap_reps = Some(20);
        add(
            &format!("verify/{label}"),
            canonical(&verify_inner(req).unwrap()),
        );
    }

    // ---- check_defensibility ----
    for (label, csv) in [("exact", &exact), ("noisy", &noisy)] {
        add(
            &format!("defensibility/{label}"),
            canonical(
                &check_defensibility_inner(verification_request(csv.clone(), false, adjustments()))
                    .unwrap(),
            ),
        );
    }
    add(
        "defensibility/noisy+dept",
        canonical(
            &check_defensibility_inner(verification_request(dept.clone(), true, adjustments()))
                .unwrap(),
        ),
    );

    // ---- calculate_efficient_frontier ----
    add(
        "frontier/noisy/default",
        canonical(
            &calculate_efficient_frontier_inner(frontier_request(noisy.clone(), 50, None)).unwrap(),
        ),
    );
    add(
        "frontier/exact/steps10",
        canonical(
            &calculate_efficient_frontier_inner(frontier_request(
                exact.clone(),
                10,
                Some(60_000.0),
            ))
            .unwrap(),
        ),
    );

    out
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/0118-null-free-golden.txt")
}

const REL_TOL: f64 = 1e-9;

fn rel_diff(a: f64, b: f64) -> f64 {
    (a - b).abs() / 1.0_f64.max(a.abs()).max(b.abs())
}

/// Structural comparison. Returns the worst relative float difference seen, or the path and
/// reason of the first mismatch.
fn compare(path: &str, want: &Value, got: &Value) -> Result<f64, String> {
    match (want, got) {
        (Value::Null, Value::Null) => Ok(0.0),
        (Value::Bool(a), Value::Bool(b)) if a == b => Ok(0.0),
        (Value::String(a), Value::String(b)) if a == b => Ok(0.0),
        (Value::Number(a), Value::Number(b)) => {
            if (a.is_i64() || a.is_u64()) && (b.is_i64() || b.is_u64()) {
                return if a == b {
                    Ok(0.0)
                } else {
                    Err(format!("{path}: integer {a} != {b}"))
                };
            }
            let (x, y) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            let d = rel_diff(x, y);
            if d <= REL_TOL {
                Ok(d)
            } else {
                Err(format!("{path}: {x:e} vs {y:e} (relative diff {d:e})"))
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: array length {} vs {}", a.len(), b.len()));
            }
            let mut worst = 0.0_f64;
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                worst = worst.max(compare(&format!("{path}[{i}]"), x, y)?);
            }
            Ok(worst)
        }
        (Value::Object(a), Value::Object(b)) => {
            let ka: Vec<&String> = a.keys().collect();
            let kb: Vec<&String> = b.keys().collect();
            if ka != kb {
                return Err(format!("{path}: keys {ka:?} vs {kb:?}"));
            }
            let mut worst = 0.0_f64;
            for (k, x) in a {
                worst = worst.max(compare(&format!("{path}.{k}"), x, &b[k])?);
            }
            Ok(worst)
        }
        _ => Err(format!("{path}: {want} vs {got}")),
    }
}

#[test]
fn null_free_outputs_match_the_pre_0118_engine() {
    let cases = cases();

    if std::env::var("MERIDIAN_PRINT_GOLDEN").as_deref() == Ok("json") {
        for (name, json) in &cases {
            println!("JSON\t{name}\t{json}");
        }
        return;
    }

    let golden = std::fs::read_to_string(golden_path()).expect("golden file present");
    let want: HashMap<&str, &str> = golden
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| l.split_once('\t').expect("name<TAB>json line"))
        .collect();

    assert_eq!(
        want.len(),
        cases.len(),
        "the golden file and the case list must agree on the cases"
    );
    let mut moved = Vec::new();
    for (name, json) in &cases {
        let expected = want
            .get(name.as_str())
            .unwrap_or_else(|| panic!("case {name} has no golden entry"));
        let want_v: Value = serde_json::from_str(expected).expect("golden json parses");
        let got_v: Value = serde_json::from_str(json).expect("result json parses");
        match compare("$", &want_v, &got_v) {
            Ok(worst) => println!("{name}: worst relative difference {worst:e}"),
            Err(why) => moved.push(format!("{name}: {why}")),
        }
    }
    assert!(
        moved.is_empty(),
        "output changed on a null-free file:\n{}\nRe-run with MERIDIAN_PRINT_GOLDEN=json against \
         the pre-0118 engine (8e4e552) to compare.",
        moved.join("\n")
    );
}
