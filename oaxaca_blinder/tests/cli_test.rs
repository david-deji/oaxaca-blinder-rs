#![allow(deprecated)]
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn test_mean_decomposition() {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg("tests/data/wage.csv")
        .arg("--outcome")
        .arg("wage")
        .arg("--group")
        .arg("gender")
        .arg("--reference")
        .arg("F")
        .arg("--predictors")
        .arg("education")
        .arg("--bootstrap-reps")
        .arg("2");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Oaxaca-Blinder Decomposition Results",
        ))
        .stdout(predicate::str::contains("Two-Fold Decomposition"))
        .stdout(predicate::str::contains(
            "Detailed Decomposition (Explained)",
        ))
        .stdout(predicate::str::contains(
            "Detailed Decomposition (Unexplained)",
        ));
}

#[test]
fn test_mean_decomposition_with_categorical() {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg("tests/data/wage.csv")
        .arg("--outcome")
        .arg("wage")
        .arg("--group")
        .arg("gender")
        .arg("--reference")
        .arg("F")
        .arg("--predictors")
        .arg("education")
        .arg("--categorical")
        .arg("sector")
        .arg("--bootstrap-reps")
        .arg("2");

    cmd.assert().success().stdout(predicate::str::contains(
        "Oaxaca-Blinder Decomposition Results",
    ));
}

#[test]
fn test_quantile_decomposition() {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg("tests/data/wage.csv")
        .arg("--outcome")
        .arg("wage")
        .arg("--group")
        .arg("gender")
        .arg("--reference")
        .arg("F")
        .arg("--predictors")
        .arg("education")
        .arg("--analysis-type")
        .arg("quantile")
        .arg("--bootstrap-reps")
        .arg("2")
        .arg("--simulations")
        .arg("10");

    // Stage-3 rewired the CLI quantile path from Machado-Mata simulation to the SAME RIF-regression
    // method the WASM/MCP surfaces use (0014-MERIDIAN ruling a-1, main.rs:235). `--simulations` is
    // now inert (kept for CLI back-compat); the header reflects RIF, and each τ prints a two-fold
    // decomposition summary.
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("(RIF-regression)"))
        .stdout(predicate::str::contains("Two-Fold Decomposition"));
}

#[test]
fn test_invalid_argument() {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg("tests/data/non_existent_file.csv")
        .arg("--outcome")
        .arg("wage")
        .arg("--group")
        .arg("gender")
        .arg("--reference")
        .arg("F")
        .arg("--predictors")
        .arg("education");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}

/// 0121-MERIDIAN F2: the command line and the library share one bootstrap default, 100.
#[test]
fn test_default_bootstrap_reps_is_100_and_matches_the_library() {
    let out = std::env::temp_dir().join(format!(
        "oaxaca_cli_default_reps_{}.json",
        std::process::id()
    ));
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.args(["--data", "tests/data/wage.csv"])
        .args(["--outcome", "wage", "--group", "gender", "--reference", "F"])
        .args(["--predictors", "education"])
        .arg("--output-json")
        .arg(&out);
    cmd.assert().success();

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let _ = std::fs::remove_file(&out);
    let requested = json["run_metadata"]["bootstrap_reps_requested"]
        .as_u64()
        .expect("run_metadata.bootstrap_reps_requested");
    assert_eq!(requested, 100, "CLI default must be 100");
    assert_eq!(requested as usize, oaxaca_blinder::DEFAULT_BOOTSTRAP_REPS);
}

#[test]
fn test_help_states_the_bootstrap_default() {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.args(["run", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[default: 100]"));
}

/// AKM and matching are not on the default command line: they are not offered in the help, and asking for one
/// is refused with the way to get it.
#[cfg(not(feature = "experimental"))]
#[test]
fn test_experimental_analysis_types_are_absent_by_default() {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.args(["--data", "tests/data/wage.csv"])
        .args(["--outcome", "wage", "--group", "gender", "--reference", "F"])
        .args(["--analysis-type", "akm"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--features experimental"))
        .stderr(predicate::str::contains("Usage").not());

    let mut help = Command::cargo_bin("oaxaca-cli").unwrap();
    help.arg("--help");
    help.assert().success().stdout(predicate::str::contains(
        "[possible values: mean, quantile]",
    ));
}

#[cfg(feature = "experimental")]
#[test]
fn test_experimental_analysis_types_are_listed_with_the_feature() {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.args(["run", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("akm"))
        .stdout(predicate::str::contains("--matching-method"));
}
