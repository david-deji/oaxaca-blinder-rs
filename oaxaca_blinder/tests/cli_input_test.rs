//! The command line accepts what a first-time user types: a whole-number outcome, a formula without `--outcome`,
//! and it says so on standard error when the bootstrap is too small for an interval. A runtime error names the
//! cause and does not print the help text.

#![allow(deprecated)] // assert_cmd's `Command::cargo_bin`, as in cli_test.rs

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::path::PathBuf;
use std::process::Command;

fn sample() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/src/sample/wage.csv")
}

/// The sample file with `wage` written as whole numbers (`45600`, not `45600.0`).
fn integer_wage_copy(tag: &str) -> PathBuf {
    let text = std::fs::read_to_string(sample()).expect("sample file");
    let mut lines = text.lines();
    let header = lines.next().expect("header");
    let mut out = String::from(header);
    out.push('\n');
    for line in lines {
        let (wage, rest) = line.split_once(',').expect("comma");
        let whole = wage.trim_end_matches(".0");
        assert!(
            !whole.contains('.'),
            "sample wage {wage} is not a whole number"
        );
        out.push_str(&format!("{whole},{rest}\n"));
    }
    let path =
        std::env::temp_dir().join(format!("oaxaca_cli_int_{}_{tag}.csv", std::process::id()));
    std::fs::write(&path, out).expect("write copy");
    path
}

fn base(data: &PathBuf) -> Command {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg(data)
        .args(["--group", "gender", "--reference", "F"]);
    cmd
}

#[test]
fn a_whole_number_outcome_gives_the_same_table_as_a_decimal_one() {
    let ints = integer_wage_copy("same");
    let run = |data: &PathBuf| {
        let mut cmd = base(data);
        cmd.args(["--outcome", "wage", "--predictors", "education,experience"])
            .args(["--ref-coeffs", "pooled"]);
        let out = cmd.assert().success().get_output().stdout.clone();
        String::from_utf8(out).unwrap()
    };
    let decimal = run(&sample());
    let whole = run(&ints);
    let _ = std::fs::remove_file(&ints);
    assert_eq!(decimal, whole);
    assert!(whole.contains("Total Gap: 7975.0000"));
}

#[test]
fn a_formula_names_the_outcome_so_outcome_is_not_required() {
    base(&sample())
        .args(["--formula", "wage ~ education + experience"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Total Gap: 7975.0000"));
}

#[test]
fn without_a_formula_the_outcome_is_still_required() {
    base(&sample())
        .args(["--predictors", "education"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--outcome"));
}

#[test]
fn too_few_bootstrap_replicates_warn_on_standard_error() {
    base(&sample())
        .args(["--outcome", "wage", "--predictors", "education"])
        .args(["--bootstrap-reps", "30"])
        .assert()
        .success()
        .stderr(predicate::str::contains("30 bootstrap replicates"))
        .stderr(predicate::str::contains("at least 41"));
}

#[test]
fn enough_bootstrap_replicates_print_no_warning() {
    base(&sample())
        .args(["--outcome", "wage", "--predictors", "education"])
        .args(["--bootstrap-reps", "41"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Warning").not());
}

#[test]
fn a_runtime_error_names_the_cause_and_prints_no_help() {
    let missing = std::env::temp_dir().join("oaxaca_cli_no_such_file.csv");
    base(&missing)
        .args(["--outcome", "wage", "--predictors", "education"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"))
        .stdout(predicate::str::contains("Usage").not())
        .stderr(predicate::str::contains("Usage").not());
}
