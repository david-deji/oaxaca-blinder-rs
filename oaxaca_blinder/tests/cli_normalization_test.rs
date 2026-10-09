//! 0120-MERIDIAN T1 / F4: the CLI normalises categorical predictors on every run, exactly as the
//! engine does (CLI/WASM parity is itself a tested claim, so the two surfaces follow one rule).
//! Expected values are the R oracle's (`norm_goldens_r.json`); `--normalization none` is the
//! documented way back to raw treatment coding.

#![allow(deprecated)] // assert_cmd's `Command::cargo_bin`, as in cli_test.rs

#[path = "support/norm_golden.rs"]
mod norm_golden;

use assert_cmd::prelude::*;
use norm_golden::*;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

fn golden() -> &'static Golden {
    static G: OnceLock<Golden> = OnceLock::new();
    G.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        Golden::load(
            &dir,
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../verification/gen_norm_goldens.R"),
        )
    })
}

fn run_cli(extra: &[&str], tag: &str) -> Value {
    let out = std::env::temp_dir().join(format!(
        "oaxaca_cli_norm_{}_{}.json",
        std::process::id(),
        tag
    ));
    let _ = std::fs::remove_file(&out);
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg(golden().fixture("norm_skewed_fixture.csv"))
        .args([
            "--outcome",
            "log_salary",
            "--group",
            "Gender",
            "--reference",
            "Female",
        ])
        .args([
            "--predictors",
            "Age,Experience_Years",
            "--categorical",
            "Department,Location",
        ])
        .args(["--bootstrap-reps", "2"])
        .arg("--output-json")
        .arg(&out)
        .args(extra);
    cmd.assert().success();
    let text = std::fs::read_to_string(&out).expect("CLI wrote its JSON");
    let _ = std::fs::remove_file(&out);
    serde_json::from_str(&text).expect("CLI JSON parses")
}

fn names(v: &Value, field: &str) -> Vec<(String, f64)> {
    v["two_fold"][field]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap().to_string(),
                c["estimate"].as_f64().unwrap(),
            )
        })
        .collect()
}

fn check(label: &str, v: &Value, g: &Value) {
    compare_vector(
        &format!("{label} unexplained"),
        &names(v, "detailed_unexplained"),
        &g["detailed_unexplained"],
        TOL_REFIT,
    );
    compare_vector(
        &format!("{label} explained"),
        &names(v, "detailed_explained"),
        &g["detailed_explained"],
        TOL_REFIT,
    );
}

#[test]
fn cli_run_mean_normalises_by_default_and_matches_the_oracle_for_every_scheme() {
    let case = golden().case("skewed_popshare");
    for (flag, scheme) in [
        ("group-a", "GroupA"),
        ("group-b", "GroupB"),
        ("pooled", "Pooled"),
        ("pooled-no-indicator", "PooledNoIndicator"),
        ("weighted", "Weighted"),
    ] {
        let v = run_cli(&["--ref-coeffs", flag], flag);
        check(&format!("cli {scheme}"), &v, &case["schemes"][scheme]);
        let rec = &v["run_metadata"]["normalization"];
        assert_eq!(rec["convention"], "population-share");
        assert_eq!(rec["applied"], true);
        // the library never stamps the engine-layer provenance keys
        assert!(v["run_metadata"]
            .get("reference_coefficients_used")
            .is_none());
    }
}

#[test]
fn cli_normalization_flag_selects_the_convention_and_none_is_raw() {
    let v = run_cli(
        &["--ref-coeffs", "group-b", "--normalization", "equal-share"],
        "equal",
    );
    check(
        "cli equal-share",
        &v,
        &golden().case("skewed_equal")["schemes"]["GroupB"],
    );
    assert_eq!(
        v["run_metadata"]["normalization"]["convention"],
        "equal-share"
    );

    let raw = run_cli(
        &["--ref-coeffs", "group-b", "--normalization", "none"],
        "none",
    );
    assert!(
        raw["run_metadata"].get("normalization").is_none(),
        "a raw run records no normalization"
    );
    let raw_names: Vec<String> = names(&raw, "detailed_unexplained")
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(
        !raw_names.contains(&"Department_Admin".to_string()),
        "raw coding drops the base level's row: {raw_names:?}"
    );
    let norm = run_cli(&["--ref-coeffs", "group-b"], "default");
    let norm_names: Vec<String> = names(&norm, "detailed_unexplained")
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(
        norm_names.contains(&"Department_Admin".to_string()),
        "normalised output emits all k levels: {norm_names:?}"
    );
}

#[test]
fn cli_quantile_normalises_on_the_exported_rif() {
    let v = run_cli(
        &[
            "--analysis-type",
            "quantile",
            "--quantiles",
            "0.5",
            "--ref-coeffs",
            "pooled",
        ],
        "q50",
    );
    check(
        "cli q50 Pooled",
        &v,
        &golden().rif_case("q50_popshare")["schemes"]["Pooled"],
    );
    assert_eq!(v["run_metadata"]["normalization"]["applied"], true);
}

#[test]
fn cli_report_normalises_too() {
    let out = std::env::temp_dir().join(format!(
        "oaxaca_cli_norm_report_{}.html",
        std::process::id()
    ));
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    // Pre-existing clap quirk (found by this test, not fixed here): the top-level run flags are
    // flattened next to the subcommands and stay required, so `report` is only reachable when
    // they are given as well. Logged in issue 0120's Log.
    cmd.arg("--data")
        .arg(golden().fixture("norm_skewed_fixture.csv"))
        .args([
            "--outcome",
            "log_salary",
            "--group",
            "Gender",
            "--reference",
            "Female",
        ])
        .arg("report")
        .arg("--data")
        .arg(golden().fixture("norm_skewed_fixture.csv"))
        .args([
            "--outcome",
            "log_salary",
            "--group",
            "Gender",
            "--reference",
            "Female",
        ])
        .args([
            "--predictors",
            "Age,Experience_Years",
            "--categorical",
            "Department,Location",
        ])
        .arg("--output")
        .arg(&out);
    cmd.assert().success();
    let html = std::fs::read_to_string(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    assert!(
        html.contains("Department_Admin"),
        "the report must list the base level (all k levels)"
    );
}
