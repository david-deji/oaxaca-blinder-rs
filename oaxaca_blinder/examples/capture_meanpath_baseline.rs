//! One-off baseline capture for AC-9 (mean-path byte-unchanged).
//!
//! Runs the mean-path (non-bootstrap point-estimate) decomposition on the parity
//! fixture and writes the point estimates to
//! `tests/fixtures/parity_meanpath_baseline.json`.
//!
//! The deterministic-RNG refactor (0014-MERIDIAN) must reproduce this file
//! byte-identically — the mean-path point math is untouched by the RNG change.
//!
//! Regenerate: `cargo run -p oaxaca_blinder --example capture_meanpath_baseline`
//! The canonical serializer here is mirrored verbatim in
//! `tests/parity_test.rs::meanpath_point_estimates` so the test can byte-compare.

use oaxaca_blinder::{ComponentResult, OaxacaBuilder, OaxacaResults, ReferenceCoefficients};
use polars::prelude::*;
use serde_json::{json, Map, Value};

fn fixture_path() -> String {
    format!(
        "{}/tests/fixtures/parity_fixture.csv",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn baseline_path() -> String {
    format!(
        "{}/tests/fixtures/parity_meanpath_baseline.json",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn load_fixture() -> DataFrame {
    LazyCsvReader::new(fixture_path())
        .with_has_header(true)
        .finish()
        .expect("parity_fixture.csv must be readable")
        .collect()
        .expect("parity_fixture.csv must parse")
}

fn run(scheme: ReferenceCoefficients) -> OaxacaResults {
    let df = load_fixture();
    let mut b = OaxacaBuilder::new(df, "log_wage", "gender", "F");
    b.predictors(vec!["education", "experience", "tenure"])
        .reference_coefficients(scheme)
        .bootstrap_reps(1);
    b.run().expect("oaxaca run failed")
}

/// Canonical mean-path point-estimate serializer. Keys are sorted (serde_json Map
/// is a BTreeMap without the `preserve_order` feature), floats use ryu shortest
/// round-trip — so identical f64 bits produce byte-identical output.
fn meanpath_point_estimates(r: &OaxacaResults) -> Value {
    let est = |comps: &[ComponentResult], name: &str| -> f64 {
        comps
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("missing component '{name}'"))
            .estimate
    };
    let two_fold = json!({
        "explained": est(&r.two_fold.aggregate, "explained"),
        "unexplained": est(&r.two_fold.aggregate, "unexplained"),
    });
    let three_fold = json!({
        "endowments": est(&r.three_fold.aggregate, "endowments"),
        "coefficients": est(&r.three_fold.aggregate, "coefficients"),
        "interaction": est(&r.three_fold.aggregate, "interaction"),
    });
    let mut de = Map::new();
    for c in &r.two_fold.detailed_explained {
        de.insert(c.name.clone(), json!(c.estimate));
    }
    let mut du = Map::new();
    for c in &r.two_fold.detailed_unexplained {
        du.insert(c.name.clone(), json!(c.estimate));
    }
    json!({
        "total_gap": r.total_gap,
        "two_fold": two_fold,
        "three_fold": three_fold,
        "detailed_explained": Value::Object(de),
        "detailed_unexplained": Value::Object(du),
    })
}

fn main() {
    let value = json!({
        "groupb": meanpath_point_estimates(&run(ReferenceCoefficients::GroupB)),
        "pooled": meanpath_point_estimates(&run(ReferenceCoefficients::Pooled)),
    });
    let text = serde_json::to_string_pretty(&value).expect("serialize baseline");
    std::fs::write(baseline_path(), &text).expect("write baseline file");
    println!("wrote {} ({} bytes)", baseline_path(), text.len());
}
