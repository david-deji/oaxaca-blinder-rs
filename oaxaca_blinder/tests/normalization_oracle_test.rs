//! 0120-MERIDIAN S1/S2/S4 oracle tests: V1, V1b, V1c, V1d, V2 and the library half of V4.
//!
//! WHAT THESE TESTS COMPARE. Engine output against `tests/fixtures/norm_goldens_r.json`, which
//! `verification/gen_norm_goldens.R` writes from base-R `lm()` refits under weighted effect
//! coding, `ddecompose(normalize_factors = TRUE)` and R `oaxaca()` (third formula part). NO
//! expected value comes from engine output (see `support/norm_golden.rs`, which also refuses a
//! golden whose generator or fixtures changed).
//!
//! WHAT EACH ORACLE CAN AND CANNOT SEE (so a green row is read for what it proves):
//!  * ddecompose / R oaxaca implement the EQUAL-share restriction only, and R oaxaca can
//!    normalise a single categorical only. They anchor `EqualShare` to 1e-10 (the engine's
//!    OLS and decomposition arithmetic, the base-level row, the all-k-levels output).
//!  * No package implements the population-share restriction. It is checked against an
//!    independent refit (weighted effect coding), whose OB arithmetic the generator has already
//!    anchored to the packages at 1e-9 before writing anything.
//!  * On the BALANCED fixture (every factor exactly balanced in the pooled sample, unbalanced
//!    within each group) population-share and equal-share coincide, so ddecompose is a direct
//!    oracle for the population-share code there, and a share vector taken from one group, the
//!    wrong rows, or row counts instead of weights breaks the identity.
//!
//! Tolerances: 1e-10 against packages, 1e-9 against refits, 1e-12 for identities. No normalised
//! check here uses a tolerance above 1e-6.

#[path = "support/norm_golden.rs"]
mod norm_golden;

use norm_golden::*;
use oaxaca_blinder::{
    ComponentResult, NormalizationConvention, OaxacaBuilder, OaxacaResults, ReferenceCoefficients,
};
use polars::prelude::*;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
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

fn read(name: &str) -> DataFrame {
    let mut df = LazyCsvReader::new(fixtures_dir().join(name))
        .with_has_header(true)
        .finish()
        .unwrap_or_else(|e| panic!("{name} readable: {e}"))
        .collect()
        .unwrap_or_else(|e| panic!("{name} parses: {e}"));
    if df.get_column_names().iter().any(|c| c.as_str() == "w") {
        let w = df.column("w").unwrap().cast(&DataType::Float64).unwrap();
        df.with_column(w).unwrap();
    }
    df
}

#[derive(Clone)]
struct Model {
    outcome: &'static str,
    nums: Vec<&'static str>,
    cats: Vec<&'static str>,
    weights: Option<&'static str>,
}

impl Model {
    fn employers(cats: &[&'static str]) -> Model {
        Model {
            outcome: "log_salary",
            nums: vec!["Age", "Experience_Years"],
            cats: cats.to_vec(),
            weights: None,
        }
    }
    fn skewed() -> Model {
        Model {
            outcome: "log_salary",
            nums: vec!["Age", "Experience_Years"],
            cats: vec!["Department", "Location"],
            weights: None,
        }
    }
}

fn scheme_of(name: &str) -> ReferenceCoefficients {
    match name {
        "GroupA" => ReferenceCoefficients::GroupA,
        "GroupB" => ReferenceCoefficients::GroupB,
        "Pooled" => ReferenceCoefficients::Pooled,
        "PooledNoIndicator" => ReferenceCoefficients::PooledNoIndicator,
        "Weighted" => ReferenceCoefficients::Weighted,
        other => panic!("unknown scheme {other}"),
    }
}

fn builder(
    df: DataFrame,
    m: &Model,
    scheme: ReferenceCoefficients,
    conv: Option<NormalizationConvention>,
) -> OaxacaBuilder {
    let mut b = OaxacaBuilder::new(df, m.outcome, "Gender", "Female");
    b.predictors(m.nums.clone())
        .categorical_predictors(m.cats.clone())
        .reference_coefficients(scheme)
        .bootstrap_reps(1);
    if let Some(w) = m.weights {
        b.weights(w);
    }
    if let Some(c) = conv {
        b.normalization_convention(c).normalize_all_categoricals();
    }
    b
}

fn run(
    df: &DataFrame,
    m: &Model,
    scheme: ReferenceCoefficients,
    conv: Option<NormalizationConvention>,
) -> OaxacaResults {
    builder(df.clone(), m, scheme, conv)
        .run()
        .expect("decomposition runs")
}

fn vec_of(comps: &[ComponentResult]) -> Vec<(String, f64)> {
    comps.iter().map(|c| (c.name.clone(), c.estimate)).collect()
}

fn est(r: &OaxacaResults, which: &str) -> f64 {
    match which {
        "explained" => r.explained().unwrap().estimate,
        _ => r.unexplained().unwrap().estimate,
    }
}

/// Engine result vs one golden scheme block. Returns the worst absolute difference.
fn check_scheme(label: &str, r: &OaxacaResults, golden_scheme: &Value, tol: f64) -> f64 {
    let a = compare_vector(
        &format!("{label} detailed_explained"),
        &vec_of(r.two_fold.detailed_explained()),
        &golden_scheme["detailed_explained"],
        tol,
    );
    let b = compare_vector(
        &format!("{label} detailed_unexplained"),
        &vec_of(r.two_fold.detailed_unexplained()),
        &golden_scheme["detailed_unexplained"],
        tol,
    );
    let c = assert_close(
        &format!("{label} explained"),
        est(r, "explained"),
        golden_scheme["explained"].as_f64().unwrap(),
        tol,
    );
    let d = assert_close(
        &format!("{label} unexplained"),
        est(r, "unexplained"),
        golden_scheme["unexplained"].as_f64().unwrap(),
        tol,
    );
    // adding-up, the engine's own identity, at identity tolerance
    assert_close(
        &format!("{label} explained+unexplained==gap"),
        est(r, "explained") + est(r, "unexplained"),
        r.total_gap,
        TOL_REFIT,
    );
    a.max(b).max(c).max(d)
}

/// All five schemes of one golden case against the engine.
fn check_case(
    case: &str,
    df: &DataFrame,
    m: &Model,
    conv: NormalizationConvention,
    tol: f64,
) -> f64 {
    let g = golden().case(case);
    let mut worst = 0.0_f64;
    for sc in SCHEMES {
        let r = run(df, m, scheme_of(sc), Some(conv));
        assert_eq!(r.n_a as u64, g["n_a"].as_u64().unwrap(), "{case} n_a");
        assert_eq!(r.n_b as u64, g["n_b"].as_u64().unwrap(), "{case} n_b");
        assert_close(
            &format!("{case} total_gap"),
            r.total_gap,
            g["total_gap"].as_f64().unwrap(),
            TOL_REFIT,
        );
        worst = worst.max(check_scheme(
            &format!("{case}/{sc}"),
            &r,
            &g["schemes"][sc],
            tol,
        ));
        // Elder, Goddeeris & Haider: under Pooled the unexplained gap IS the indicator coefficient.
        if sc == "Pooled" {
            assert_close(
                &format!("{case} Pooled unexplained == indicator coefficient"),
                est(&r, "unexplained"),
                g["schemes"]["Pooled"]["indicator_coefficient"]
                    .as_f64()
                    .unwrap(),
                TOL_REFIT,
            );
        }
    }
    println!("{case}: worst |engine - oracle| = {worst:.3e}");
    worst
}

// ---------------------------------------------------------------------------------------------
// V1: packages (equal-share)
// ---------------------------------------------------------------------------------------------

#[test]
fn v1_equal_share_matches_ddecompose_on_three_categoricals() {
    let df = read("employers_trust_fixture.csv");
    let m = Model::employers(&["Education_Level", "Department", "Location"]);
    let pkg = golden().package("employers_ddecompose_equal");
    for sc in ["GroupB", "GroupA"] {
        let r = run(
            &df,
            &m,
            scheme_of(sc),
            Some(NormalizationConvention::EqualShare),
        );
        let w = check_scheme(
            &format!("employers ddecompose/{sc}"),
            &r,
            &pkg[sc],
            TOL_PACKAGE_10K,
        );
        println!("employers ddecompose/{sc}: worst |engine - ddecompose| = {w:.3e}");
    }
}

#[test]
fn v1_equal_share_matches_oaxaca_on_a_single_categorical() {
    // R oaxaca() can normalise a single categorical only; rows are pinned by group.weight value:
    // 0 = GroupB, 1 = GroupA, 5108/10000 = the engine's Weighted (share of group A).
    let df = read("employers_trust_fixture.csv");
    let m = Model::employers(&["Department"]);
    let pkg = golden().package("employers_dept_oaxaca_equal");
    for sc in ["GroupB", "GroupA", "Weighted"] {
        let r = run(
            &df,
            &m,
            scheme_of(sc),
            Some(NormalizationConvention::EqualShare),
        );
        let w = check_scheme(&format!("oaxaca/{sc}"), &r, &pkg[sc], TOL_PACKAGE);
        println!("oaxaca/{sc}: worst |engine - oaxaca| = {w:.3e}");
    }
    // The pooled schemes: R oaxaca does not re-estimate beta*, so only the aggregate is an oracle.
    for sc in ["Pooled", "PooledNoIndicator"] {
        let r = run(
            &df,
            &m,
            scheme_of(sc),
            Some(NormalizationConvention::EqualShare),
        );
        let agg = &pkg[format!("aggregate_{sc}")];
        assert_close(
            &format!("oaxaca aggregate {sc} unexplained"),
            est(&r, "unexplained"),
            agg["unexplained"].as_f64().unwrap(),
            TOL_REFIT,
        );
        assert_close(
            &format!("oaxaca aggregate {sc} explained"),
            est(&r, "explained"),
            agg["explained"].as_f64().unwrap(),
            TOL_REFIT,
        );
    }
}

// ---------------------------------------------------------------------------------------------
// V1b / V1c: refit oracle, every scheme, every level
// ---------------------------------------------------------------------------------------------

#[test]
fn v1_refit_equal_share_all_schemes_employers() {
    let df = read("employers_trust_fixture.csv");
    let m = Model::employers(&["Education_Level", "Department", "Location"]);
    check_case(
        "employers_equal",
        &df,
        &m,
        NormalizationConvention::EqualShare,
        TOL_PACKAGE_10K,
    );
}

#[test]
fn v1c_population_share_all_schemes_employers() {
    let df = read("employers_trust_fixture.csv");
    let m = Model::employers(&["Education_Level", "Department", "Location"]);
    check_case(
        "employers_popshare",
        &df,
        &m,
        NormalizationConvention::PopulationShare,
        TOL_REFIT,
    );
}

#[test]
fn v1c_population_share_all_schemes_skewed_fixture() {
    // Levels 60/30/9/1 percent, mix differing by >20 points between the groups, a 8-person
    // department: pooled, group-A-only, group-B-only and equal shares all give different tables.
    let df = read("norm_skewed_fixture.csv");
    check_case(
        "skewed_popshare",
        &df,
        &Model::skewed(),
        NormalizationConvention::PopulationShare,
        TOL_REFIT,
    );
    check_case(
        "skewed_equal",
        &df,
        &Model::skewed(),
        NormalizationConvention::EqualShare,
        TOL_REFIT,
    );
}

#[test]
fn v1c_weights_replace_row_counts_in_shares_and_fits() {
    // Integer weights: the oracle ALSO checked, inside the generator, that the weighted table
    // equals the table on the rows expanded by their weights.
    let df = read("norm_skewed_fixture.csv");
    let mut m = Model::skewed();
    m.weights = Some("w");
    check_case(
        "skewed_weighted_popshare",
        &df,
        &m,
        NormalizationConvention::PopulationShare,
        TOL_REFIT,
    );
    check_case(
        "skewed_weighted_equal",
        &df,
        &m,
        NormalizationConvention::EqualShare,
        TOL_REFIT,
    );
}

#[test]
fn v1c_shares_come_from_the_rows_that_survive_cleaning() {
    // Six rows are blank in Tenure, a predictor that is not the outcome: they leave the model
    // frame, so they must leave the share counts too.
    let df = read("norm_skewed_fixture.csv");
    let mut m = Model::skewed();
    m.nums = vec!["Age", "Experience_Years", "Tenure"];
    check_case(
        "skewed_dropped_popshare",
        &df,
        &m,
        NormalizationConvention::PopulationShare,
        TOL_REFIT,
    );
}

// ---------------------------------------------------------------------------------------------
// V1c: the identity and its teeth
// ---------------------------------------------------------------------------------------------

#[test]
fn v1c_balanced_pooled_design_makes_population_share_equal_ddecompose() {
    let df = read("norm_balanced_fixture.csv");
    let m = Model {
        outcome: "log_salary",
        nums: vec!["Age", "Experience_Years"],
        cats: vec!["Department", "Location", "Union"],
        weights: None,
    };
    // refit oracle, all schemes
    check_case(
        "balanced_popshare",
        &df,
        &m,
        NormalizationConvention::PopulationShare,
        TOL_REFIT,
    );
    // direct package oracle for the population-share code path
    let pkg = golden().package("balanced_ddecompose_equal");
    for sc in ["GroupB", "GroupA"] {
        let r = run(
            &df,
            &m,
            scheme_of(sc),
            Some(NormalizationConvention::PopulationShare),
        );
        let w = check_scheme(
            &format!("balanced popshare vs ddecompose/{sc}"),
            &r,
            &pkg[sc],
            TOL_PACKAGE,
        );
        println!("balanced popshare vs ddecompose/{sc}: worst {w:.3e}");
    }
    // the two conventions coincide here, level by level, for every scheme
    for sc in SCHEMES {
        let p = run(
            &df,
            &m,
            scheme_of(sc),
            Some(NormalizationConvention::PopulationShare),
        );
        let e = run(
            &df,
            &m,
            scheme_of(sc),
            Some(NormalizationConvention::EqualShare),
        );
        let (pv, ev) = (
            vec_of(p.two_fold.detailed_unexplained()),
            vec_of(e.two_fold.detailed_unexplained()),
        );
        for (n, x) in &pv {
            let y = ev.iter().find(|(m, _)| m == n).unwrap().1;
            assert!(
                (x - y).abs() <= TOL_IDENTITY * 10.0,
                "{sc}[{n}]: population {x} vs equal {y}"
            );
        }
    }
}

#[test]
fn v1c_the_conventions_differ_where_the_pooled_mix_is_skewed() {
    // The teeth of the identity above: on the skewed fixture population-share and equal-share
    // disagree by far more than any tolerance used here.
    let df = read("norm_skewed_fixture.csv");
    let m = Model::skewed();
    let p = run(
        &df,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::PopulationShare),
    );
    let e = run(
        &df,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::EqualShare),
    );
    let mut worst = 0.0_f64;
    for (n, x) in vec_of(p.two_fold.detailed_unexplained()) {
        let y = vec_of(e.two_fold.detailed_unexplained())
            .into_iter()
            .find(|(m, _)| *m == n)
            .unwrap()
            .1;
        worst = worst.max((x - y).abs());
    }
    assert!(
        worst > 1e-2,
        "conventions agree to {worst:e} on a skewed design: the identity test has no teeth"
    );
}

fn legal_frame(df: &DataFrame) -> DataFrame {
    // The first 20 Engineering rows (file order) become a new "Legal" department.
    let mut out = df.clone();
    let dept = df.column("Department").unwrap().str().unwrap().clone();
    let mut left = 20;
    let new: Vec<String> = dept
        .into_iter()
        .map(|v| {
            let v = v.unwrap();
            if v == "Engineering" && left > 0 {
                left -= 1;
                "Legal".to_string()
            } else {
                v.to_string()
            }
        })
        .collect();
    out.with_column(Series::new("Department".into(), new))
        .unwrap();
    out
}

#[test]
fn v1c_carving_out_a_small_department_moves_only_that_department() {
    let df = read("employers_trust_fixture.csv");
    let legal = legal_frame(&df);
    let m = Model::employers(&["Department"]);

    // value compare against the refit oracle on the carved-out data (every level, intercept, base)
    let r_pop = run(
        &legal,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::PopulationShare),
    );
    check_scheme(
        "legal popshare GroupB",
        &r_pop,
        &golden().case("employers_legal_dept_popshare")["schemes"]["GroupB"],
        TOL_REFIT,
    );
    let r_eq = run(
        &legal,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::EqualShare),
    );
    check_scheme(
        "legal equal GroupB",
        &r_eq,
        &golden().case("employers_legal_dept_equal")["schemes"]["GroupB"],
        TOL_REFIT,
    );

    // stability: levels present before and after, EXCLUDING the intercept (it absorbs the average
    // and moves 5.5e-4), the donor Engineering (it lost 20 rows) and the new Legal level
    let base_pop = run(
        &df,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::PopulationShare),
    );
    let base_eq = run(
        &df,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::EqualShare),
    );
    let moved = |a: &OaxacaResults, b: &OaxacaResults| -> f64 {
        let (va, vb) = (
            vec_of(a.two_fold.detailed_unexplained()),
            vec_of(b.two_fold.detailed_unexplained()),
        );
        va.iter()
            .filter(|(n, _)| {
                n.starts_with("Department_")
                    && n != "Department_Engineering"
                    && n != "Department_Legal"
            })
            .map(|(n, x)| (x - vb.iter().find(|(m, _)| m == n).unwrap().1).abs())
            .fold(0.0, f64::max)
    };
    let pop_move = moved(&base_pop, &r_pop);
    let eq_move = moved(&base_eq, &r_eq);
    println!("carve-out: untouched departments move {pop_move:.2e} (population) vs {eq_move:.2e} (equal)");
    assert!(
        pop_move < 1e-4,
        "population-share moved an untouched department by {pop_move:e}"
    );
    assert!(
        eq_move > 1e-3,
        "the equal-share path should move them by >1e-3 (got {eq_move:e}): the test has no teeth"
    );
}

#[test]
fn v1c_renaming_the_base_level_changes_no_driver_row() {
    let df = read("employers_trust_fixture.csv");
    let m = Model::employers(&["Education_Level", "Department", "Location"]);

    // rename Engineering -> zz_Engineering: it stops being the alphabetical base
    let mut relabelled = df.clone();
    let dept = df.column("Department").unwrap().str().unwrap().clone();
    let new: Vec<String> = dept
        .into_iter()
        .map(|v| {
            if v.unwrap() == "Engineering" {
                "zz_Engineering".into()
            } else {
                v.unwrap().to_string()
            }
        })
        .collect();
    relabelled
        .with_column(Series::new("Department".into(), new))
        .unwrap();

    let rename = |n: &str| {
        if n == "Department_Engineering" {
            "Department_zz_Engineering".to_string()
        } else {
            n.to_string()
        }
    };
    for conv in [
        NormalizationConvention::PopulationShare,
        NormalizationConvention::EqualShare,
    ] {
        for sc in [
            "GroupA",
            "GroupB",
            "Pooled",
            "Weighted",
            "PooledNoIndicator",
        ] {
            let a = run(&df, &m, scheme_of(sc), Some(conv));
            let b = run(&relabelled, &m, scheme_of(sc), Some(conv));
            for (which, va, vb) in [
                (
                    "unexplained",
                    vec_of(a.two_fold.detailed_unexplained()),
                    vec_of(b.two_fold.detailed_unexplained()),
                ),
                (
                    "explained",
                    vec_of(a.two_fold.detailed_explained()),
                    vec_of(b.two_fold.detailed_explained()),
                ),
            ] {
                assert_eq!(va.len(), vb.len());
                for (n, x) in &va {
                    let y = vb
                        .iter()
                        .find(|(m, _)| *m == rename(n))
                        .unwrap_or_else(|| panic!("{n} missing after relabel"))
                        .1;
                    assert!(
                        (x - y).abs() <= TOL_REFIT,
                        "{conv:?}/{sc} {which}[{n}]: {x} vs {y} after renaming the base level"
                    );
                }
            }
        }
    }

    // teeth: without normalisation the same rename moves driver rows by far more
    let raw_a = run(&df, &m, ReferenceCoefficients::GroupB, None);
    let raw_b = run(&relabelled, &m, ReferenceCoefficients::GroupB, None);
    let (va, vb) = (
        vec_of(raw_a.two_fold.detailed_unexplained()),
        vec_of(raw_b.two_fold.detailed_unexplained()),
    );
    let worst = va
        .iter()
        .filter(|(n, _)| n.starts_with("Department_") && n != "Department_Engineering")
        // Raw runs name only the k-1 dummies, so the dropped level differs between the two runs.
        .filter_map(|(n, x)| {
            vb.iter()
                .find(|(m, _)| *m == *n)
                .map(|(_, y)| (x - y).abs())
        })
        .fold(0.0, f64::max);
    assert!(
        worst > 1e-3,
        "a raw run moved by only {worst:e} under the rename: the test has no teeth"
    );
}

// ---------------------------------------------------------------------------------------------
// V2: three-fold from the treatment-coded vectors
// ---------------------------------------------------------------------------------------------

#[test]
fn v2_three_fold_is_computed_from_the_raw_vectors_and_adds_up_under_normalisation() {
    let df = read("norm_skewed_fixture.csv");
    let m = Model::skewed();
    let g = golden().case("skewed_popshare");
    let raw = run(&df, &m, ReferenceCoefficients::GroupB, None);
    for conv in [
        NormalizationConvention::PopulationShare,
        NormalizationConvention::EqualShare,
    ] {
        let n = run(&df, &m, ReferenceCoefficients::GroupB, Some(conv));
        let agg = |r: &OaxacaResults, name: &str| {
            r.three_fold
                .aggregate()
                .iter()
                .find(|c| c.name == name)
                .unwrap()
                .estimate
        };
        let mut sum = 0.0;
        for k in ["endowments", "coefficients", "interaction"] {
            assert_close(
                &format!("three-fold {k} ({conv:?}) vs oracle"),
                agg(&n, k),
                g["three_fold_raw"][k].as_f64().unwrap(),
                TOL_REFIT,
            );
            assert_eq!(
                agg(&n, k),
                agg(&raw, k),
                "normalisation must not touch the three-fold aggregate ({k})"
            );
            sum += agg(&n, k);
        }
        assert_close("E + C + I == gap", sum, n.total_gap, TOL_REFIT);
    }
}

#[test]
fn v2_three_fold_matches_r_oaxaca_threefold_overall() {
    let df = read("employers_trust_fixture.csv");
    let m = Model::employers(&["Department"]);
    let want = &golden().package("employers_dept_oaxaca_equal")["threefold_overall"];
    let r = run(
        &df,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::EqualShare),
    );
    for k in ["endowments", "coefficients", "interaction"] {
        let got = r
            .three_fold
            .aggregate()
            .iter()
            .find(|c| c.name == k)
            .unwrap()
            .estimate;
        assert_close(
            &format!("R oaxaca threefold {k}"),
            got,
            want[k].as_f64().unwrap(),
            TOL_PACKAGE,
        );
    }
}

// ---------------------------------------------------------------------------------------------
// V1d: the RIF / quantile path, on the engine's own exported RIF column
// ---------------------------------------------------------------------------------------------

#[test]
fn v1d_the_committed_rif_columns_are_the_ones_the_engine_computes_today() {
    // The R oracle treated norm_skewed_rif.csv's rif_* columns as ordinary outcomes. If the RIF
    // transform changes, this fails until the goldens are regenerated; it cannot pass stale.
    let df = read("norm_skewed_fixture.csv");
    let m = Model::skewed();
    let committed = read("norm_skewed_rif.csv");
    for (col, tau) in [("rif_q10", 0.1), ("rif_q50", 0.5), ("rif_q90", 0.9)] {
        let frame = builder(df.clone(), &m, ReferenceCoefficients::GroupB, None)
            .rif_outcome_frame(tau)
            .expect("rif frame");
        let now: Vec<f64> = frame
            .column("log_salary")
            .unwrap()
            .f64()
            .unwrap()
            .into_no_null_iter()
            .collect();
        let was: Vec<f64> = committed
            .column(col)
            .unwrap()
            .f64()
            .unwrap()
            .into_no_null_iter()
            .collect();
        assert_eq!(now.len(), was.len());
        let worst = now
            .iter()
            .zip(&was)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(
            worst < 1e-12,
            "{col}: engine RIF differs from the committed column by {worst:e}"
        );
    }
}

#[test]
fn v1d_normalised_quantile_decomposition_matches_packages_on_the_exported_rif() {
    // Header: 5e-4 and 2e-2 bounds belong to the UNNORMALISED ddecompose comparison in
    // quantile_detail_golden_test.rs (ddecompose estimates its own density). This test needs
    // none: RIF-OLS on a given y is plain OLS, so the normalisation is checked at 1e-10.
    let df = read("norm_skewed_fixture.csv");
    let m = Model::skewed();
    for (tag, tau) in [("q10", 0.1), ("q50", 0.5), ("q90", 0.9)] {
        for (suffix, conv) in [
            ("equal", NormalizationConvention::EqualShare),
            ("popshare", NormalizationConvention::PopulationShare),
        ] {
            let case = golden().rif_case(&format!("{tag}_{suffix}"));
            for sc in SCHEMES {
                let r = builder(df.clone(), &m, scheme_of(sc), Some(conv))
                    .decompose_quantile(tau)
                    .expect("quantile decomposition");
                let w = check_scheme(
                    &format!("rif {tag} {suffix}/{sc}"),
                    &r,
                    &case["schemes"][sc],
                    TOL_PACKAGE,
                );
                assert!(w < 1e-6);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// run_metadata.normalization, and replicate bookkeeping
// ---------------------------------------------------------------------------------------------

fn echoed_shares(r: &OaxacaResults) -> Vec<(String, Vec<(String, f64)>)> {
    r.run_metadata
        .normalization
        .as_ref()
        .expect("normalization recorded")
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

#[test]
fn run_metadata_echoes_the_shares_the_oracle_computed() {
    let df = read("norm_skewed_fixture.csv");
    let mut m = Model::skewed();

    let r = run(
        &df,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::PopulationShare),
    );
    let rec = r.run_metadata.normalization.as_ref().unwrap();
    assert_eq!(rec.convention, "population-share");
    assert_eq!(rec.share_basis, "row-counts");
    assert!(rec.applied && rec.skipped_reason.is_none());
    compare_shares(
        "skewed",
        &echoed_shares(&r),
        &golden().case("skewed_popshare")["shares"],
    );
    for f in &rec.variables {
        assert_eq!(
            f.base_level, f.levels[0].level,
            "the base level is the alphabetically first"
        );
    }

    m.weights = Some("w");
    let r = run(
        &df,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::PopulationShare),
    );
    assert_eq!(
        r.run_metadata.normalization.as_ref().unwrap().share_basis,
        "observation-weights"
    );
    compare_shares(
        "skewed weighted",
        &echoed_shares(&r),
        &golden().case("skewed_weighted_popshare")["shares"],
    );

    let e = run(
        &df,
        &Model::skewed(),
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::EqualShare),
    );
    assert_eq!(
        e.run_metadata.normalization.as_ref().unwrap().convention,
        "equal-share"
    );
    compare_shares(
        "skewed equal",
        &echoed_shares(&e),
        &golden().case("skewed_equal")["shares"],
    );

    // a raw run records nothing, so its serialized bytes are what they always were
    let raw = run(&df, &Model::skewed(), ReferenceCoefficients::GroupB, None);
    assert!(raw.run_metadata.normalization.is_none());
    let json = serde_json::to_value(&raw.run_metadata).unwrap();
    for key in [
        "normalization",
        "reference_coefficients_used",
        "engine_version",
        "method",
        "bootstrap_discard_levels",
    ] {
        assert!(
            json.get(key).is_none(),
            "raw run_metadata must not carry {key}"
        );
    }
}

#[test]
fn a_replicate_that_loses_a_level_is_discarded_and_the_level_is_named() {
    // Zeta has 3 reference-group rows: a 260-row resample misses all of them about 5% of the time.
    let df = read("norm_skewed_fixture.csv");
    let m = Model::skewed();
    let mut b = builder(
        df,
        &m,
        ReferenceCoefficients::GroupB,
        Some(NormalizationConvention::PopulationShare),
    );
    b.bootstrap_reps(300).seed(7);
    let r = b.run().unwrap();
    let md = &r.run_metadata;
    assert!(
        md.bootstrap_reps_discarded > 0,
        "expected some replicates to lose Zeta"
    );
    assert_eq!(md.bootstrap_reps_requested, 300);
    assert_eq!(
        md.bootstrap_reps_succeeded + md.bootstrap_reps_discarded,
        300
    );
    assert!(
        md.bootstrap_discard_levels
            .contains(&"Department=Zeta".to_string()),
        "levels named: {:?}",
        md.bootstrap_discard_levels
    );
    assert!(
        md.bootstrap_discard_levels
            .iter()
            .all(|l| l == "Department=Zeta" || l == "Department=Ops"),
        "only rare levels can cost replicates: {:?}",
        md.bootstrap_discard_levels
    );
}

// ---------------------------------------------------------------------------------------------
// S4 (library half)
// ---------------------------------------------------------------------------------------------

#[test]
fn s4_scheme_names_are_exact() {
    for ok in ReferenceCoefficients::ACCEPTED_NAMES {
        assert!(ReferenceCoefficients::parse_name(Some(ok)).is_ok(), "{ok}");
    }
    for bad in [
        None,
        Some("pooled"),
        Some("POOLED"),
        Some("groupb"),
        Some("Neumark"),
        Some("Cotton"),
        Some(""),
        Some(" Pooled"),
        Some("Pooled "),
    ] {
        let err = ReferenceCoefficients::parse_name(bad)
            .unwrap_err()
            .to_string();
        assert!(
            err.starts_with("UNKNOWN_REFERENCE_COEFFICIENTS"),
            "{bad:?}: {err}"
        );
        assert!(
            err.contains("PooledNoIndicator"),
            "the error must name the valid set: {err}"
        );
    }
}

#[test]
fn s4_pooled_no_indicator_is_not_pooled() {
    let df = read("norm_skewed_fixture.csv");
    let m = Model::skewed();
    let a = run(
        &df,
        &m,
        ReferenceCoefficients::Pooled,
        Some(NormalizationConvention::PopulationShare),
    );
    let b = run(
        &df,
        &m,
        ReferenceCoefficients::PooledNoIndicator,
        Some(NormalizationConvention::PopulationShare),
    );
    assert!((est(&a, "unexplained") - est(&b, "unexplained")).abs() > 1e-4);
    #[allow(deprecated)]
    let alias = ReferenceCoefficients::Neumark;
    let c = run(
        &df,
        &m,
        alias,
        Some(NormalizationConvention::PopulationShare),
    );
    assert_eq!(
        est(&a, "unexplained"),
        est(&c, "unexplained"),
        "the Neumark alias keeps computing Pooled"
    );
}
