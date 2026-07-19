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
