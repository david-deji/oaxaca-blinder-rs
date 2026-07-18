//! Golden-value parity test for the two-fold Oaxaca-Blinder decomposition.
//!
//! Asserts that the engine's closed-form OLS decomposition matches statsmodels'
//! `OaxacaBlinder` reference values to within 1e-6. The committed golden values
//! (`tests/fixtures/parity_golden.json`) were produced by `verification/gen_parity_golden.py`.
//!
//! This test is OFFLINE — no network, no Python, no statsmodels at test time. It reads
//! only the two committed fixtures (`parity_fixture.csv`, `parity_golden.json`).
//!
//! Regeneration: run `python verification/gen_parity_golden.py` (statsmodels>=0.14) and
//! re-commit both fixtures. Conventions that MUST stay in sync with the generator:
//!   - reference scheme GroupB  == statsmodels two_fold(type='self_submitted', weight=0.0, swap=True)
//!   - reference scheme Pooled  == statsmodels two_fold(type='pooled', swap=True)
//!   - engine A = non-reference ("M"), B = reference ("F"); total_gap = mean(A) - mean(B)
//!   - statsmodels "const" intercept maps to engine "__ob_intercept__"
//!   - only point estimates (`.estimate`) are frozen; bootstrap SE/t/p/CI are RNG-dependent.

use oaxaca_blinder::{ComponentResult, OaxacaBuilder, OaxacaResults, ReferenceCoefficients};
use polars::prelude::*;
use serde_json::Value;

const FIXTURE_PATH: &str = "tests/fixtures/parity_fixture.csv";
const GOLDEN_PATH: &str = "tests/fixtures/parity_golden.json";
const TOLERANCE: f64 = 1e-6; // engine vs statsmodels golden
const INTERNAL_TOL: f64 = 1e-9; // engine self-consistency

fn load_fixture() -> DataFrame {
    LazyCsvReader::new(FIXTURE_PATH)
        .with_has_header(true)
        .finish()
        .expect("parity_fixture.csv must be readable")
        .collect()
        .expect("parity_fixture.csv must parse")
}

fn load_golden() -> Value {
    let text =
        std::fs::read_to_string(GOLDEN_PATH).expect("parity_golden.json must be committed and readable");
    serde_json::from_str(&text).expect("parity_golden.json must parse")
}

fn assert_close(label: &str, engine: f64, golden: f64, tol: f64) {
    let diff = (engine - golden).abs();
    assert!(
        diff < tol,
        "{label}: engine={engine:.12}, golden={golden:.12}, diff={diff:.2e} >= tol {tol:.0e}"
    );
}

fn estimate_by_name(comps: &[ComponentResult], name: &str) -> f64 {
    comps
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("engine result missing component '{name}'"))
        .estimate
}

fn run(scheme: ReferenceCoefficients) -> OaxacaResults {
    let df = load_fixture();
    let mut b = OaxacaBuilder::new(df, "log_wage", "gender", "F");
    // reps>=1: point estimates are deterministic (single full-data pass); reps=1 avoids the
    // empty-slice bootstrap path while keeping the run fast. No bootstrap field is asserted.
    b.predictors(vec!["education", "experience", "tenure"])
        .reference_coefficients(scheme)
        .bootstrap_reps(1);
    b.run().expect("oaxaca run failed")
}

#[test]
fn test_parity_two_fold_decomposition() {
    let raw = std::fs::read_to_string(GOLDEN_PATH).expect("golden readable");
    assert!(
        !raw.contains("FILL_AT_GENERATION_TIME"),
        "golden JSON still contains unfilled placeholders"
    );
    let golden = load_golden();

    // ---- Primary scheme: GroupB (== statsmodels self_submitted weight=0.0, swap=True) ----
    let r = run(ReferenceCoefficients::GroupB);

    assert_eq!(r.n_a, golden["n_a"].as_u64().unwrap() as usize, "n_a mismatch");
    assert_eq!(r.n_b, golden["n_b"].as_u64().unwrap() as usize, "n_b mismatch");
    assert_close(
        "total_gap",
        r.total_gap,
        golden["total_gap"].as_f64().unwrap(),
        TOLERANCE,
    );

    let explained = r.explained().expect("explained component").estimate;
    let unexplained = r.unexplained().expect("unexplained component").estimate;
    assert_close(
        "explained",
        explained,
        golden["two_fold"]["explained"].as_f64().unwrap(),
        TOLERANCE,
    );
    assert_close(
        "unexplained",
        unexplained,
        golden["two_fold"]["unexplained"].as_f64().unwrap(),
        TOLERANCE,
    );

    // Per-variable detailed contributions (oracle: statsmodels' independent group OLS fits).
    let gexp = golden["detailed_explained"].as_object().unwrap();
    assert_eq!(
        gexp.len(),
        r.two_fold.detailed_explained.len(),
        "detailed_explained component count mismatch"
    );
    for (key, gval) in gexp {
        let eng = estimate_by_name(&r.two_fold.detailed_explained, key);
        assert_close(
            &format!("detailed_explained[{key}]"),
            eng,
            gval.as_f64().unwrap(),
            TOLERANCE,
        );
    }
    let gunexp = golden["detailed_unexplained"].as_object().unwrap();
    assert_eq!(
        gunexp.len(),
        r.two_fold.detailed_unexplained.len(),
        "detailed_unexplained component count mismatch"
    );
    for (key, gval) in gunexp {
        let eng = estimate_by_name(&r.two_fold.detailed_unexplained, key);
        assert_close(
            &format!("detailed_unexplained[{key}]"),
            eng,
            gval.as_f64().unwrap(),
            TOLERANCE,
        );
    }

    // Engine internal consistency (belt-and-braces, 1e-9).
    assert_close(
        "internal explained+unexplained==total_gap",
        explained + unexplained,
        r.total_gap,
        INTERNAL_TOL,
    );
    let sum_exp: f64 = r.two_fold.detailed_explained.iter().map(|c| c.estimate).sum();
    let sum_unexp: f64 = r
        .two_fold
        .detailed_unexplained
        .iter()
        .map(|c| c.estimate)
        .sum();
    assert_close("internal sum(detailed_explained)==explained", sum_exp, explained, INTERNAL_TOL);
    assert_close(
        "internal sum(detailed_unexplained)==unexplained",
        sum_unexp,
        unexplained,
        INTERNAL_TOL,
    );

    // ---- Cross-check scheme: Pooled (== statsmodels two_fold_type='pooled') ----
    // A second, independently-parameterized statsmodels match for extra confidence.
    let rp = run(ReferenceCoefficients::Pooled);
    let cc = &golden["cross_check_pooled"];
    let p_exp = rp.explained().unwrap().estimate;
    let p_unexp = rp.unexplained().unwrap().estimate;
    assert_close("pooled.explained", p_exp, cc["explained"].as_f64().unwrap(), TOLERANCE);
    assert_close("pooled.unexplained", p_unexp, cc["unexplained"].as_f64().unwrap(), TOLERANCE);
    assert_close("pooled internal", p_exp + p_unexp, rp.total_gap, INTERNAL_TOL);
}

// ---- AC-9: mean-path point estimates byte-identical to the pre-refactor baseline ----
// The deterministic-RNG refactor (0014-MERIDIAN) does not touch mean-path point math. This
// test freezes that guarantee against `tests/fixtures/parity_meanpath_baseline.json`, captured
// pre-refactor by `examples/capture_meanpath_baseline.rs`. Serializer mirrored from that example.

const MEANPATH_BASELINE_PATH: &str = "tests/fixtures/parity_meanpath_baseline.json";

fn meanpath_point_estimates(r: &OaxacaResults) -> Value {
    let est = |comps: &[ComponentResult], name: &str| -> f64 {
        comps
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("missing component '{name}'"))
            .estimate
    };
    let two_fold = serde_json::json!({
        "explained": est(&r.two_fold.aggregate, "explained"),
        "unexplained": est(&r.two_fold.aggregate, "unexplained"),
    });
    let three_fold = serde_json::json!({
        "endowments": est(&r.three_fold.aggregate, "endowments"),
        "coefficients": est(&r.three_fold.aggregate, "coefficients"),
        "interaction": est(&r.three_fold.aggregate, "interaction"),
    });
    let mut de = serde_json::Map::new();
    for c in &r.two_fold.detailed_explained {
        de.insert(c.name.clone(), serde_json::json!(c.estimate));
    }
    let mut du = serde_json::Map::new();
    for c in &r.two_fold.detailed_unexplained {
        du.insert(c.name.clone(), serde_json::json!(c.estimate));
    }
    serde_json::json!({
        "total_gap": r.total_gap,
        "two_fold": two_fold,
        "three_fold": three_fold,
        "detailed_explained": Value::Object(de),
        "detailed_unexplained": Value::Object(du),
    })
}

#[test]
fn meanpath_point_estimates_byte_unchanged() {
    let current = serde_json::json!({
        "groupb": meanpath_point_estimates(&run(ReferenceCoefficients::GroupB)),
        "pooled": meanpath_point_estimates(&run(ReferenceCoefficients::Pooled)),
    });
    let committed_text = std::fs::read_to_string(MEANPATH_BASELINE_PATH)
        .expect("parity_meanpath_baseline.json must be committed and readable");
    let committed: Value =
        serde_json::from_str(&committed_text).expect("parity_meanpath_baseline.json must parse");
    assert_eq!(
        current, committed,
        "AC-9: mean-path point estimates changed vs the pre-refactor baseline — the RNG refactor must not touch point math"
    );
}
