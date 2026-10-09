//! 0120-MERIDIAN S9 / T17: `--weights` always travels with `--weights-kind`.
//!
//! The CLI used to reject an integer weights column read from a CSV (`prepare_data` asked polars
//! for Float64), which is exactly what a headcount column is. The integer column below is parsed
//! by polars as Int64.

#![allow(deprecated)] // assert_cmd's `Command::cargo_bin`, as in cli_test.rs

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

/// 16 rows; `hc` is an integer headcount (parsed as Int64), `fte` a fractional weight. The
/// outcome and the predictor are written as floats: the library asks for Float64 there.
const CSV: &str = "\
wage,educ,group,hc,fte
10.0,1.0,A,1,1.0
12.0,2.0,A,1,0.5
14.0,3.0,A,2,1.0
16.0,4.0,A,1,0.5
11.0,1.0,A,3,1.0
13.0,2.0,A,1,0.5
15.0,3.0,A,1,1.0
30.0,4.0,A,2,0.5
20.0,1.0,B,1,1.0
22.0,2.0,B,2,0.5
24.0,3.0,B,1,1.0
26.0,4.0,B,1,0.5
21.0,1.0,B,1,1.0
23.0,2.0,B,3,0.5
25.0,3.0,B,1,1.0
40.0,4.0,B,2,0.5
";

fn csv_path(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("oaxaca_cli_w_{}_{}.csv", std::process::id(), tag));
    std::fs::write(&p, CSV).unwrap();
    p
}

fn cli(tag: &str) -> Command {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg(csv_path(tag))
        .args(["--outcome", "wage", "--group", "group", "--reference", "B"])
        .args(["--predictors", "educ", "--bootstrap-reps", "2"]);
    cmd
}

fn json_of(mut cmd: Command, tag: &str) -> Value {
    let out =
        std::env::temp_dir().join(format!("oaxaca_cli_w_{}_{}.json", std::process::id(), tag));
    let _ = std::fs::remove_file(&out);
    cmd.arg("--output-json").arg(&out).assert().success();
    let v = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let _ = std::fs::remove_file(&out);
    v
}

#[test]
fn weights_without_a_kind_is_refused_by_the_command_line() {
    cli("nokind")
        .args(["--weights", "hc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--weights-kind"));
    cli("kindonly")
        .args(["--weights-kind", "frequency"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--weights"));
}

#[test]
fn an_integer_weights_column_is_read_as_a_frequency() {
    let mut cmd = cli("freq");
    cmd.args(["--weights", "hc", "--weights-kind", "frequency"]);
    let v = json_of(cmd, "freq");
    assert!(v["total_gap"].as_f64().unwrap().is_finite());
}

#[test]
fn a_fractional_weights_column_under_frequency_names_its_row() {
    cli("fractional")
        .args(["--weights", "fte", "--weights-kind", "frequency"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "INVALID_WEIGHT: column=fte, row=1, value=0.5",
        ));
}

#[test]
fn the_same_fractional_column_is_fine_as_relative_weights() {
    let mut cmd = cli("relative");
    cmd.args(["--weights", "fte", "--weights-kind", "relative"]);
    let v = json_of(cmd, "relative");
    assert!(v["total_gap"].as_f64().unwrap().is_finite());
    // Quantile runs take the same pair.
    let mut q = cli("relative-q");
    q.args(["--analysis-type", "quantile", "--quantiles", "0.5"])
        .args(["--weights", "fte", "--weights-kind", "relative"]);
    let v = json_of(q, "relative-q");
    assert!(v["total_gap"].as_f64().unwrap().is_finite());
}
