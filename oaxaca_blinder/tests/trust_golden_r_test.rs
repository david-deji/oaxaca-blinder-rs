//! Statistical-Trust-Layer golden — R `oaxaca`/`lm` oracle (0014-MERIDIAN, AC-3).
//!
//! A SECOND independent oracle beside the statsmodels `parity_test.rs`. The engine's
//! closed-form two-fold + three-fold OB decomposition (GroupB scheme, β*=β_B — the
//! `builder.rs` default) is asserted against reference values produced by R's `lm()`
//! group fits on the PII-stripped Employers fixture (10k rows, real 3-level categoricals).
//!
//! OFFLINE — no R, no network at test time. Reads only the two committed fixtures
//! (`employers_trust_fixture.csv`, `trust_goldens_r.json`). Regenerate with
//! `Rscript verification/gen_trust_goldens.R` (requires R + oaxaca 0.1.5).
//!
//! Design-matrix alignment (silent-failure guard): the engine drops the alphabetically
//! first level of each categorical as the base (`builder.rs:462-470` ascending sort) and
//! names dummies `{col}_{level}` — identical to R's `contr.treatment` default. AC-3 asserts
//! `_meta.design_columns == engine columns` as a SET *before* comparing any value, so a
//! design-matrix disagreement fails structurally rather than as a mystery tolerance miss.

use oaxaca_blinder::{OaxacaBuilder, OaxacaResults, ReferenceCoefficients};
use polars::prelude::*;
use serde_json::Value;
use std::collections::BTreeSet;

const FIXTURE: &str = "tests/fixtures/employers_trust_fixture.csv";
const GOLDEN: &str = "tests/fixtures/trust_goldens_r.json";
const REL_TOL: f64 = 1e-6; // engine vs R lm() oracle
const ABS_FLOOR: f64 = 1e-8; // near-zero channels (small gap dataset)
const INTERNAL_TOL: f64 = 1e-9; // engine self-consistency

fn load_fixture() -> DataFrame {
    LazyCsvReader::new(FIXTURE)
        .with_has_header(true)
        .finish()
        .expect("employers_trust_fixture.csv readable")
        .collect()
        .expect("employers_trust_fixture.csv parses")
}

fn load_golden() -> Value {
    let text = std::fs::read_to_string(GOLDEN).expect("trust_goldens_r.json committed + readable");
    serde_json::from_str(&text).expect("trust_goldens_r.json parses")
}

/// pass iff |a-b| < max(ABS_FLOOR, REL_TOL*|golden|) — rel tol with a near-zero abs floor.
fn assert_close(label: &str, engine: f64, golden: f64, rel: f64, floor: f64) {
    let tol = floor.max(rel * golden.abs());
    let diff = (engine - golden).abs();
    assert!(
        diff <= tol,
        "{label}: engine={engine:.12}, golden={golden:.12}, diff={diff:.3e} > tol {tol:.3e}"
    );
}

fn run_groupb() -> OaxacaResults {
    let df = load_fixture();
    // outcome=log_salary, group=Gender, reference=Female (alpha-first == _meta.group_reference).
    let mut b = OaxacaBuilder::new(df, "log_salary", "Gender", "Female");
    b.predictors(vec!["Age", "Experience_Years"])
        .categorical_predictors(vec!["Education_Level", "Department", "Location"])
        // Explicit GroupB (beta* = beta_B) — matches the R generator's `bstar <- bB`. Set explicitly
        // (not relying on the constructor default) so the golden fails loudly if the default flips.
        .reference_coefficients(ReferenceCoefficients::GroupB)
        // reps=1 → deterministic single-pass point estimates (no bootstrap field asserted here).
        .bootstrap_reps(1);
    b.run().expect("trust-golden oaxaca run")
}

fn names_of(comps: &[oaxaca_blinder::ComponentResult]) -> BTreeSet<String> {
    comps.iter().map(|c| c.name.clone()).collect()
}
fn est(comps: &[oaxaca_blinder::ComponentResult], name: &str) -> f64 {
    comps
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("engine missing component '{name}'"))
        .estimate
}

#[test]
fn ac3_trust_golden_r_two_fold() {
    let raw = std::fs::read_to_string(GOLDEN).expect("golden readable");
    assert!(
        !raw.contains("FILL_AT_GENERATION_TIME"),
        "golden still has unfilled placeholders"
    );
    let golden = load_golden();
    let meta = &golden["_meta"];
    assert_eq!(
        meta["oaxaca_version"].as_str(),
        Some("0.1.5"),
        "golden must be generated against oaxaca 0.1.5"
    );
    assert_eq!(
        meta["packages"]["ddecompose"].as_bool(),
        Some(true),
        "golden must be regenerated on an R machine with ddecompose (no partial pass)"
    );

    let r = run_groupb();
    let gb = &golden["groupb"];

    // ---- AC-3 structural guard: design columns match as a SET, before any value ----
    let golden_cols: BTreeSet<String> = meta["design_columns"]
        .as_array()
        .expect("design_columns array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let engine_cols = names_of(&r.two_fold.detailed_explained);
    assert_eq!(
        engine_cols, golden_cols,
        "design-matrix mismatch: engine columns != golden design_columns (base-category or dummy-naming divergence)"
    );

    // ---- n, gap, aggregate ----
    assert_eq!(r.n_a, gb["n_a"].as_u64().unwrap() as usize, "n_a");
    assert_eq!(r.n_b, gb["n_b"].as_u64().unwrap() as usize, "n_b");
    assert_close(
        "total_gap",
        r.total_gap,
        gb["total_gap"].as_f64().unwrap(),
        REL_TOL,
        ABS_FLOOR,
    );

    let explained = r.explained().expect("explained").estimate;
    let unexplained = r.unexplained().expect("unexplained").estimate;
    assert_close(
        "explained",
        explained,
        gb["aggregate"]["explained"].as_f64().unwrap(),
        REL_TOL,
        ABS_FLOOR,
    );
    assert_close(
        "unexplained",
        unexplained,
        gb["aggregate"]["unexplained"].as_f64().unwrap(),
        REL_TOL,
        ABS_FLOOR,
    );

    // ---- per-variable detail (by name) ----
    for (key, gval) in gb["detailed_explained"].as_object().unwrap() {
        assert_close(
            &format!("detailed_explained[{key}]"),
            est(&r.two_fold.detailed_explained, key),
            gval.as_f64().unwrap(),
            REL_TOL,
            ABS_FLOOR,
        );
    }
    for (key, gval) in gb["detailed_unexplained"].as_object().unwrap() {
        assert_close(
            &format!("detailed_unexplained[{key}]"),
            est(&r.two_fold.detailed_unexplained, key),
            gval.as_f64().unwrap(),
            REL_TOL,
            ABS_FLOOR,
        );
    }

    // ---- three-fold aggregate ----
    let tf = &gb["three_fold"]["aggregate"];
    assert_close(
        "endowments",
        est(&r.three_fold.aggregate, "endowments"),
        tf["endowments"].as_f64().unwrap(),
        REL_TOL,
        ABS_FLOOR,
    );
    assert_close(
        "coefficients",
        est(&r.three_fold.aggregate, "coefficients"),
        tf["coefficients"].as_f64().unwrap(),
        REL_TOL,
        ABS_FLOOR,
    );
    assert_close(
        "interaction",
        est(&r.three_fold.aggregate, "interaction"),
        tf["interaction"].as_f64().unwrap(),
        REL_TOL,
        ABS_FLOOR,
    );

    // ---- engine internal consistency (belt-and-braces) ----
    assert_close(
        "internal explained+unexplained==gap",
        explained + unexplained,
        r.total_gap,
        INTERNAL_TOL,
        INTERNAL_TOL,
    );
    let sum_exp: f64 = r
        .two_fold
        .detailed_explained
        .iter()
        .map(|c| c.estimate)
        .sum();
    assert_close(
        "internal sum(detailed_explained)==explained",
        sum_exp,
        explained,
        INTERNAL_TOL,
        INTERNAL_TOL,
    );
}
