//! 0120-MERIDIAN: the SHIPPED engine entry points (`decompose_inner`, `verify_inner`,
//! `optimize_inner`, `check_defensibility_inner`, `calculate_efficient_frontier_inner`)
//! against the R oracle, plus the intercept contract (V3, engine half) and strict scheme
//! parsing (V4).
//!
//! Same oracle, same fixtures and same comparator as the library test
//! (`oaxaca_blinder/tests/normalization_oracle_test.rs`), read through
//! `../oaxaca_blinder/tests/fixtures`. This is the path the WASM blob, the MCP server and the
//! app run: it normalises every categorical predictor, under the pooled-sample population
//! shares, on every call. NO expected value comes from engine output.

#[path = "../../oaxaca_blinder/tests/support/norm_golden.rs"]
mod norm_golden;

use norm_golden::*;
use pay_equity_engine::analysis::{
    calculate_efficient_frontier_inner, decompose_inner, optimize_inner, verify_inner,
};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::intercept_token;
use pay_equity_engine::types::{
    DecompositionRequest, DecompositionResult, EfficientFrontierRequest, OptimizationRequest,
    ProposedAdjustment, VerificationRequest,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oaxaca_blinder/tests/fixtures")
}

fn golden() -> &'static Golden {
    static G: OnceLock<Golden> = OnceLock::new();
    G.get_or_init(|| {
        Golden::load(
            &fixtures_dir(),
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../verification/gen_norm_goldens.R"),
        )
    })
}

fn csv(name: &str) -> String {
    std::fs::read_to_string(fixtures_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn request(
    csv: &str,
    nums: &[&str],
    cats: &[&str],
    scheme: Option<&str>,
    quantile: Option<f64>,
) -> DecompositionRequest {
    DecompositionRequest {
        csv_data: csv.as_bytes().to_vec(),
        outcome_variable: "log_salary".to_string(),
        group_variable: "Gender".to_string(),
        reference_group: "Female".to_string(),
        predictors: nums.iter().map(|s| s.to_string()).collect(),
        categorical_predictors: if cats.is_empty() {
            None
        } else {
            Some(cats.iter().map(|s| s.to_string()).collect())
        },
        three_fold: None,
        quantile,
        reference_coefficients: scheme.map(|s| s.to_string()),
        bootstrap_reps: Some(1),
    }
}

fn detail(r: &DecompositionResult, which: &str) -> Vec<(String, f64)> {
    let v = if which == "explained" {
        &r.detailed_explained
    } else {
        &r.detailed_unexplained
    };
    v.iter().map(|c| (c.name.clone(), c.estimate)).collect()
}

fn check_result(label: &str, r: &DecompositionResult, g: &Value, tol: f64) -> f64 {
    let a = compare_vector(
        &format!("{label} detailed_explained"),
        &detail(r, "explained"),
        &g["detailed_explained"],
        tol,
    );
    let b = compare_vector(
        &format!("{label} detailed_unexplained"),
        &detail(r, "unexplained"),
        &g["detailed_unexplained"],
        tol,
    );
    let c = assert_close(
        &format!("{label} explained_gap"),
        r.explained_gap,
        g["explained"].as_f64().unwrap(),
        tol,
    );
    let d = assert_close(
        &format!("{label} unexplained_gap"),
        r.unexplained_gap,
        g["unexplained"].as_f64().unwrap(),
        tol,
    );
    a.max(b).max(c).max(d)
}

fn shares_echo(r: &DecompositionResult) -> Vec<(String, Vec<(String, f64)>)> {
    r.run_metadata
        .normalization
        .as_ref()
        .expect("normalization recorded on a categorical run")
        .variables
        .iter()
        .map(|f| {
            (
                f.variable.clone(),
                f.levels
                    .iter()
                    .map(|l| (l.level.clone(), l.share))
                    .collect(),
            )
        })
        .collect()
}

struct ShippedCase {
    case: &'static str,
    file: &'static str,
    nums: &'static [&'static str],
    cats: &'static [&'static str],
    tol: f64,
}

const CASES: [ShippedCase; 4] = [
    ShippedCase {
        case: "employers_popshare",
        file: "employers_trust_fixture.csv",
        nums: &["Age", "Experience_Years"],
        cats: &["Education_Level", "Department", "Location"],
        tol: TOL_PACKAGE_10K,
    },
    ShippedCase {
        case: "skewed_popshare",
        file: "norm_skewed_fixture.csv",
        nums: &["Age", "Experience_Years"],
        cats: &["Department", "Location"],
        tol: TOL_REFIT,
    },
    ShippedCase {
        case: "balanced_popshare",
        file: "norm_balanced_fixture.csv",
        nums: &["Age", "Experience_Years"],
        cats: &["Department", "Location", "Union"],
        tol: TOL_REFIT,
    },
    ShippedCase {
        case: "skewed_dropped_popshare",
        file: "norm_skewed_fixture.csv",
        nums: &["Age", "Experience_Years", "Tenure"],
        cats: &["Department", "Location"],
        tol: TOL_REFIT,
    },
];

#[test]
fn decompose_matches_the_r_oracle_for_every_scheme_on_every_fixture() {
    for c in &CASES {
        let data = csv(c.file);
        let g = golden().case(c.case);
        let mut worst = 0.0_f64;
        for sc in SCHEMES {
            let r =
                decompose_inner(request(&data, c.nums, c.cats, Some(sc), None)).expect("decompose");
            worst = worst.max(check_result(
                &format!("{}/{sc}", c.case),
                &r,
                &g["schemes"][sc],
                c.tol,
            ));
            assert_close(
                &format!("{} total_gap", c.case),
                r.total_gap,
                g["total_gap"].as_f64().unwrap(),
                TOL_REFIT,
            );

            // run_metadata: scheme, version, method, and the shares the oracle computed
            let md = &r.run_metadata;
            assert_eq!(md.reference_coefficients_used.as_deref(), Some(sc));
            assert_eq!(
                md.engine_version.as_deref(),
                Some(env!("CARGO_PKG_VERSION"))
            );
            assert_eq!(md.method.as_deref(), Some("oaxaca-blinder-mean"));
            let rec = md.normalization.as_ref().unwrap();
            assert_eq!(
                (rec.convention, rec.share_basis, rec.applied),
                ("population-share", "row-counts", true)
            );
            compare_shares(c.case, &shares_echo(&r), &g["shares"]);
        }
        println!(
            "{}: shipped decompose worst |engine - oracle| = {worst:.3e}",
            c.case
        );
    }
}

#[test]
fn three_fold_through_the_engine_is_the_raw_three_fold() {
    let data = csv("norm_skewed_fixture.csv");
    let g = golden().case("skewed_popshare");
    let mut req = request(
        &data,
        &["Age", "Experience_Years"],
        &["Department", "Location"],
        Some("GroupB"),
        None,
    );
    req.three_fold = Some(true);
    let r = decompose_inner(req).unwrap();
    let want = &g["three_fold_raw"];
    assert_close(
        "endowments",
        r.explained_gap,
        want["endowments"].as_f64().unwrap(),
        TOL_REFIT,
    );
    assert_close(
        "coefficients",
        r.unexplained_gap,
        want["coefficients"].as_f64().unwrap(),
        TOL_REFIT,
    );
    assert_close(
        "interaction",
        r.interaction_gap.unwrap(),
        want["interaction"].as_f64().unwrap(),
        TOL_REFIT,
    );
    assert_close(
        "E+C+I == gap",
        r.explained_gap + r.unexplained_gap + r.interaction_gap.unwrap(),
        r.total_gap,
        TOL_REFIT,
    );
}

#[test]
fn verify_normalises_exactly_as_decompose_does() {
    let data = csv("norm_skewed_fixture.csv");
    let req = || {
        request(
            &data,
            &["Age", "Experience_Years"],
            &["Department", "Location"],
            Some("Pooled"),
            None,
        )
    };
    let d = decompose_inner(req()).unwrap();
    let v = verify_inner(VerificationRequest {
        decomposition_params: req(),
        adjustments: vec![],
    })
    .unwrap();
    assert_eq!(detail(&d, "unexplained"), detail(&v, "unexplained"));
    assert_eq!(detail(&d, "explained"), detail(&v, "explained"));
    assert!(v.run_metadata.normalization.is_some());
    assert_eq!(
        v.run_metadata.reference_coefficients_used.as_deref(),
        Some("Pooled")
    );
}

#[test]
fn quantile_path_normalises_and_matches_the_oracle_on_the_exported_rif() {
    let data = csv("norm_skewed_fixture.csv");
    for (tag, tau) in [("q10", 0.1), ("q50", 0.5), ("q90", 0.9)] {
        let g = golden().rif_case(&format!("{tag}_popshare"));
        for sc in SCHEMES {
            let r = decompose_inner(request(
                &data,
                &["Age", "Experience_Years"],
                &["Department", "Location"],
                Some(sc),
                Some(tau),
            ))
            .unwrap();
            check_result(&format!("rif {tag}/{sc}"), &r, &g["schemes"][sc], TOL_REFIT);
            assert_eq!(r.run_metadata.method.as_deref(), Some("rif-quantile"));
            assert_eq!(
                r.run_metadata.reference_coefficients_used.as_deref(),
                Some(sc)
            );
            assert!(r.run_metadata.normalization.as_ref().unwrap().applied);
        }
    }
}

fn rewrite_department(data: &str, f: &mut dyn FnMut(&str) -> String) -> String {
    let mut lines = data.lines();
    let header = lines.next().unwrap();
    let col = header.split(',').position(|h| h == "Department").unwrap();
    let mut out = String::from(header);
    out.push('\n');
    for line in lines {
        let mut cells: Vec<String> = line.split(',').map(|s| s.to_string()).collect();
        cells[col] = f(&cells[col]);
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

#[test]
fn carving_a_department_out_moves_only_that_department_through_the_shipped_path() {
    let data = csv("employers_trust_fixture.csv");
    let mut left = 20;
    let legal = rewrite_department(&data, &mut |d| {
        if d == "Engineering" && left > 0 {
            left -= 1;
            "Legal".to_string()
        } else {
            d.to_string()
        }
    });
    let nums = ["Age", "Experience_Years"];
    let before =
        decompose_inner(request(&data, &nums, &["Department"], Some("GroupB"), None)).unwrap();
    let after = decompose_inner(request(
        &legal,
        &nums,
        &["Department"],
        Some("GroupB"),
        None,
    ))
    .unwrap();
    check_result(
        "legal GroupB",
        &after,
        &golden().case("employers_legal_dept_popshare")["schemes"]["GroupB"],
        TOL_REFIT,
    );

    let (vb, va) = (
        detail(&before, "unexplained"),
        detail(&after, "unexplained"),
    );
    let moved = vb
        .iter()
        .filter(|(n, _)| n.starts_with("Department_") && n != "Department_Engineering")
        .map(|(n, x)| (x - va.iter().find(|(m, _)| m == n).unwrap().1).abs())
        .fold(0.0, f64::max);
    println!("shipped carve-out: untouched departments move {moved:.2e}");
    assert!(moved < 1e-4, "an untouched department moved by {moved:e}");
}

#[test]
fn renaming_the_base_level_changes_no_driver_row_through_the_shipped_path() {
    let data = csv("employers_trust_fixture.csv");
    let renamed = rewrite_department(&data, &mut |d| {
        if d == "Engineering" {
            "zz_Engineering".into()
        } else {
            d.to_string()
        }
    });
    let nums = ["Age", "Experience_Years"];
    let cats = ["Education_Level", "Department", "Location"];
    for sc in SCHEMES {
        let a = decompose_inner(request(&data, &nums, &cats, Some(sc), None)).unwrap();
        let b = decompose_inner(request(&renamed, &nums, &cats, Some(sc), None)).unwrap();
        for which in ["explained", "unexplained"] {
            let (va, vb) = (detail(&a, which), detail(&b, which));
            assert_eq!(va.len(), vb.len());
            for (n, x) in &va {
                let m = if n == "Department_Engineering" {
                    "Department_zz_Engineering"
                } else {
                    n.as_str()
                };
                let y = vb
                    .iter()
                    .find(|(k, _)| k == m)
                    .unwrap_or_else(|| panic!("{m} missing"))
                    .1;
                assert!((x - y).abs() <= TOL_REFIT, "{sc} {which}[{n}]: {x} vs {y}");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// V4: strict scheme parsing at every engine boundary that consumes it
// ---------------------------------------------------------------------------------------------

#[test]
fn an_absent_or_unknown_scheme_is_an_error_and_never_a_fallback() {
    let data = csv("norm_skewed_fixture.csv");
    let nums = ["Age", "Experience_Years"];
    for bad in [
        None,
        Some("pooled"),
        Some("POOLED"),
        Some("Neumark"),
        Some("Cotton"),
        Some("groupb"),
        Some(""),
        Some("Standard"),
    ] {
        let e = decompose_inner(request(&data, &nums, &["Department"], bad, None)).unwrap_err();
        assert!(
            e.starts_with("UNKNOWN_REFERENCE_COEFFICIENTS"),
            "decompose {bad:?}: {e}"
        );
        for valid in [
            "GroupA",
            "GroupB",
            "Pooled",
            "PooledNoIndicator",
            "Weighted",
        ] {
            assert!(e.contains(valid), "the error must name {valid}: {e}");
        }
        let e = verify_inner(VerificationRequest {
            decomposition_params: request(&data, &nums, &["Department"], bad, None),
            adjustments: vec![],
        })
        .unwrap_err();
        assert!(
            e.starts_with("UNKNOWN_REFERENCE_COEFFICIENTS"),
            "verify {bad:?}: {e}"
        );
        // refused before any data is read: garbage bytes still give the scheme error
        let mut garbage = request(&data, &nums, &["Department"], bad, None);
        garbage.csv_data = vec![0xff, 0xfe, 0x00];
        assert!(decompose_inner(garbage)
            .unwrap_err()
            .starts_with("UNKNOWN_REFERENCE_COEFFICIENTS"));
    }
    // every accepted name works, quantile branch included
    for ok in [
        "GroupA",
        "GroupB",
        "Pooled",
        "PooledNoIndicator",
        "Weighted",
    ] {
        decompose_inner(request(&data, &nums, &["Department"], Some(ok), None)).unwrap();
        decompose_inner(request(&data, &nums, &["Department"], Some(ok), Some(0.5))).unwrap();
    }
}

#[test]
fn frontier_and_defensibility_do_not_consume_a_scheme() {
    // They fit their own pooled model and ignore `reference_coefficients`. Pinned so that a
    // future change cannot start requiring it (or silently honouring it) unnoticed.
    let data = csv("norm_skewed_fixture.csv");
    let nums = ["Age", "Experience_Years"];
    let f = calculate_efficient_frontier_inner(EfficientFrontierRequest {
        decomposition_params: request(&data, &nums, &["Department", "Location"], None, None),
        steps: Some(3),
        max_budget: Some(50_000.0),
    });
    assert!(f.is_ok(), "{:?}", f.err());
    let d = check_defensibility_inner(VerificationRequest {
        decomposition_params: request(&data, &nums, &["Department", "Location"], None, None),
        adjustments: vec![ProposedAdjustment {
            index: 0,
            row_key: None,
            value: 100.0,
            predictor_overrides: None,
        }],
    });
    assert!(d.is_ok(), "{:?}", d.err());
}

// ---------------------------------------------------------------------------------------------
// V3 (engine half): the intercept contract
// ---------------------------------------------------------------------------------------------

const FORBIDDEN_NAMES: [&str; 3] = ["intercept", "Base Rate (Intercept)", "ob intercept"];

fn assert_token_once(label: &str, names: &[String]) {
    let token = intercept_token();
    let n = names.iter().filter(|n| **n == token).count();
    assert_eq!(
        n, 1,
        "{label}: the intercept token {token:?} must appear exactly once, got {n} in {names:?}"
    );
}

fn assert_no_forbidden(label: &str, names: &[String]) {
    for bad in FORBIDDEN_NAMES {
        assert!(
            !names.iter().any(|n| n == bad),
            "{label}: found forbidden name {bad:?} in {names:?}"
        );
    }
}

fn walk_decomposition(label: &str, r: &DecompositionResult, collision: bool) {
    for which in ["explained", "unexplained"] {
        let names: Vec<String> = detail(r, which).into_iter().map(|(n, _)| n).collect();
        assert_token_once(&format!("{label} {which}"), &names);
        if !collision {
            assert_no_forbidden(&format!("{label} {which}"), &names);
        }
    }
}

fn optimization_request(csv: &str, nums: &[&str], cats: &[&str]) -> OptimizationRequest {
    OptimizationRequest {
        csv_data: csv.as_bytes().to_vec(),
        outcome_variable: "log_salary".to_string(),
        group_variable: "Gender".to_string(),
        reference_group: "Female".to_string(),
        predictors: nums.iter().map(|s| s.to_string()).collect(),
        categorical_predictors: Some(cats.iter().map(|s| s.to_string()).collect()),
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
}

#[test]
fn the_intercept_token_is_the_one_name_of_the_constant_in_every_output_vector() {
    assert_eq!(intercept_token(), oaxaca_blinder::INTERCEPT_NAME);
    let data = csv("norm_skewed_fixture.csv");
    let nums = ["Age", "Experience_Years"];
    let cats = ["Department", "Location"];

    // decompose: OLS and quantile, every scheme
    for sc in SCHEMES {
        walk_decomposition(
            &format!("ols/{sc}"),
            &decompose_inner(request(&data, &nums, &cats, Some(sc), None)).unwrap(),
            false,
        );
        walk_decomposition(
            &format!("rif/{sc}"),
            &decompose_inner(request(&data, &nums, &cats, Some(sc), Some(0.5))).unwrap(),
            false,
        );
    }

    // optimize: model_coefficients and per-employee contributions
    let opt = optimize_inner(optimization_request(&data, &nums, &cats)).unwrap();
    let names: Vec<String> = opt
        .model_coefficients
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_token_once("optimize model_coefficients", &names);
    assert_no_forbidden("optimize model_coefficients", &names);
    assert!(!opt.adjustments.is_empty());
    for a in &opt.adjustments {
        let names: Vec<String> = a.contributions.iter().map(|c| c.name.clone()).collect();
        assert_token_once(
            &format!("optimize contributions of row {}", a.index),
            &names,
        );
        assert_no_forbidden("optimize contributions", &names);
    }

    // defensibility: same two vectors
    let def = check_defensibility_inner(VerificationRequest {
        decomposition_params: request(&data, &nums, &cats, Some("Pooled"), None),
        adjustments: vec![ProposedAdjustment {
            index: 0,
            row_key: None,
            value: 100.0,
            predictor_overrides: None,
        }],
    })
    .unwrap();
    let names: Vec<String> = def
        .model_coefficients
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_token_once("defensibility model_coefficients", &names);
    assert_no_forbidden("defensibility model_coefficients", &names);
    assert_eq!(def.adjustments.len(), 1);
    let names: Vec<String> = def.adjustments[0]
        .contributions
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_token_once("defensibility contributions", &names);
    assert_no_forbidden("defensibility contributions", &names);
}

/// A roster with a real numeric column literally named `intercept` (and the same data with that
/// column renamed). The name must travel through the engine untouched and never be taken for the
/// constant.
fn collision_csv(column: &str) -> String {
    let data = csv("norm_skewed_fixture.csv");
    let mut lines = data.lines();
    let header = lines.next().unwrap().replace("Experience_Years", column);
    let mut out = format!("{header}\n");
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}

#[test]
fn a_column_named_intercept_is_a_predictor_and_never_the_constant() {
    let real = collision_csv("intercept");
    let renamed = collision_csv("tenure_years");
    let cats = ["Department", "Location"];

    let a = decompose_inner(request(
        &real,
        &["Age", "intercept"],
        &cats,
        Some("GroupB"),
        None,
    ))
    .unwrap();
    let b = decompose_inner(request(
        &renamed,
        &["Age", "tenure_years"],
        &cats,
        Some("GroupB"),
        None,
    ))
    .unwrap();
    walk_decomposition("collision", &a, true);
    for which in ["explained", "unexplained"] {
        let names: Vec<String> = detail(&a, which).into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names.iter().filter(|n| *n == "intercept").count(),
            1,
            "the real predictor appears once under its own name"
        );
        // value-for-value the same as the renamed column: the name has no effect on any number
        let (va, vb) = (detail(&a, which), detail(&b, which));
        for ((na, x), (nb, y)) in va.iter().zip(&vb) {
            let expect = if na == "intercept" {
                "tenure_years"
            } else {
                na.as_str()
            };
            assert_eq!(expect, nb);
            assert_eq!(
                x, y,
                "{which}[{na}] differs between 'intercept' and 'tenure_years'"
            );
        }
    }

    // optimize, defensibility and the frontier all ignore the name too
    let oa = optimize_inner(optimization_request(&real, &["Age", "intercept"], &cats)).unwrap();
    let ob = optimize_inner(optimization_request(
        &renamed,
        &["Age", "tenure_years"],
        &cats,
    ))
    .unwrap();
    assert_eq!(oa.total_cost, ob.total_cost);
    assert_eq!(oa.original_gap, ob.original_gap);
    let names: Vec<String> = oa
        .model_coefficients
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_token_once("collision optimize", &names);
    assert_eq!(names.iter().filter(|n| *n == "intercept").count(), 1);

    let fa = calculate_efficient_frontier_inner(EfficientFrontierRequest {
        decomposition_params: request(&real, &["Age", "intercept"], &cats, None, None),
        steps: Some(4),
        max_budget: Some(40_000.0),
    })
    .unwrap();
    let fb = calculate_efficient_frontier_inner(EfficientFrontierRequest {
        decomposition_params: request(&renamed, &["Age", "tenure_years"], &cats, None, None),
        steps: Some(4),
        max_budget: Some(40_000.0),
    })
    .unwrap();
    assert_eq!(fa.len(), fb.len());
    for (x, y) in fa.iter().zip(&fb) {
        assert_eq!(
            x.t_statistic, y.t_statistic,
            "the frontier treated the column named 'intercept' as the constant"
        );
    }
}
