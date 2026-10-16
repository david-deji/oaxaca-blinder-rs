//! The main options (`--data`, `--outcome`, `--group`, `--reference`, ...) go before the subcommand, after it, or in
//! both places. `report` used to need them in both: once before the word `report` and again after it.

#![allow(deprecated)] // assert_cmd's `Command::cargo_bin`, as in cli_test.rs

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::path::PathBuf;
use std::process::Command;

fn sample() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/src/sample/wage.csv")
}

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("oaxaca_cli_sub_{}_{tag}.html", std::process::id()))
}

fn main_options() -> Vec<String> {
    let data = sample().display().to_string();
    [
        "--data",
        &data,
        "--outcome",
        "wage",
        "--group",
        "gender",
        "--reference",
        "F",
        "--predictors",
        "education,experience",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Runs `oaxaca-cli <before...> <sub> <after...>` and returns what it wrote to `out`.
fn report(before: &[String], after: &[String], out: &PathBuf) -> String {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.args(before)
        .arg("report")
        .args(after)
        .arg("--output")
        .arg(out);
    cmd.assert().success();
    let html = std::fs::read_to_string(out).expect("report written");
    let _ = std::fs::remove_file(out);
    html
}

#[test]
fn report_takes_the_main_options_after_the_subcommand_alone() {
    let out = tmp("after");
    let html = report(&[], &main_options(), &out);
    assert!(
        html.contains("experience"),
        "the report lists the predictors"
    );
}

#[test]
fn report_still_takes_the_main_options_before_the_subcommand() {
    let out = tmp("before");
    let html = report(&main_options(), &[], &out);
    assert!(
        html.contains("experience"),
        "the report lists the predictors"
    );
}

#[test]
fn report_gives_the_same_page_in_every_placement() {
    let after = report(&[], &main_options(), &tmp("same_after"));
    let before = report(&main_options(), &[], &tmp("same_before"));
    let both = report(&main_options(), &main_options(), &tmp("same_both"));
    assert_eq!(after, before);
    assert_eq!(after, both);
}

#[test]
fn run_takes_the_main_options_in_either_place() {
    let run = |before: &[String], after: &[String]| {
        let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
        cmd.args(before)
            .arg("run")
            .args(after)
            .args(["--bootstrap-reps", "50"]);
        let out = cmd.assert().success().get_output().stdout.clone();
        String::from_utf8(out).unwrap()
    };
    let none = {
        let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
        cmd.args(main_options()).args(["--bootstrap-reps", "50"]);
        String::from_utf8(cmd.assert().success().get_output().stdout.clone()).unwrap()
    };
    assert!(none.contains("Total Gap: 7975.0000"));
    assert_eq!(none, run(&[], &main_options()));
    assert_eq!(none, run(&main_options(), &[]));
}

#[test]
fn a_main_option_after_the_subcommand_wins_over_one_before_it() {
    // `--group nonsense` before, the real column after: the report runs, so the later value was used.
    let mut before = main_options();
    let at = before.iter().position(|a| a == "gender").unwrap();
    before[at] = "no_such_column".to_string();
    let after = vec!["--group".to_string(), "gender".to_string()];
    let html = report(&before, &after, &tmp("later_wins"));
    assert!(html.contains("experience"));
}

#[test]
fn report_without_the_main_options_names_what_is_missing() {
    let out = tmp("missing");
    Command::cargo_bin("oaxaca-cli")
        .unwrap()
        .args(["report", "--output"])
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--data <DATA>"))
        .stderr(predicate::str::contains("--group <GROUP>"))
        .stderr(predicate::str::contains("--reference <REFERENCE>"));
    assert!(!out.exists(), "nothing is written when options are missing");
}

#[test]
fn report_without_an_outcome_says_so() {
    let mut args = main_options();
    let at = args.iter().position(|a| a == "--outcome").unwrap();
    args.drain(at..at + 2);
    let out = tmp("no_outcome");
    Command::cargo_bin("oaxaca-cli")
        .unwrap()
        .arg("report")
        .args(&args)
        .arg("--output")
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--outcome"));
    assert!(!out.exists());
}
