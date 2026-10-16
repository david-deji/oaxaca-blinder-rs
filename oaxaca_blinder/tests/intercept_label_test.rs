//! A person reading a table sees `(intercept)`. Data keeps the reserved token `__ob_intercept__`: the results'
//! row names, the JSON output and `INTERCEPT_NAME`.

#![allow(deprecated)] // assert_cmd's `Command::cargo_bin`, as in cli_test.rs

use assert_cmd::prelude::*;
use oaxaca_blinder::{OaxacaBuilder, INTERCEPT_LABEL, INTERCEPT_NAME};
use polars::prelude::*;
use std::path::PathBuf;
use std::process::Command;

fn sample() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/src/sample/wage.csv")
}

fn cli() -> Command {
    let mut cmd = Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg(sample())
        .args(["--outcome", "wage", "--group", "gender", "--reference", "F"])
        .args([
            "--predictors",
            "education,experience",
            "--bootstrap-reps",
            "50",
        ]);
    cmd
}

fn tmp(tag: &str, ext: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "oaxaca_cli_icpt_{}_{tag}.{ext}",
        std::process::id()
    ))
}

#[test]
fn the_constants_are_what_the_docs_say() {
    assert_eq!(INTERCEPT_NAME, "__ob_intercept__");
    assert_eq!(INTERCEPT_LABEL, "(intercept)");
}

#[test]
fn the_cli_tables_print_the_label_not_the_token() {
    let out = cli().assert().success().get_output().stdout.clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("(intercept)"), "{text}");
    assert!(!text.contains("__ob_intercept__"), "{text}");
}

#[test]
fn the_html_report_prints_the_label_not_the_token() {
    let page = tmp("report", "html");
    cli()
        .arg("report")
        .arg("--output")
        .arg(&page)
        .assert()
        .success();
    let html = std::fs::read_to_string(&page).unwrap();
    let _ = std::fs::remove_file(&page);
    assert!(html.contains("(intercept)"));
    assert!(!html.contains("__ob_intercept__"));
}

#[test]
fn the_json_output_keeps_the_token() {
    let json = tmp("json", "json");
    cli().arg("--output-json").arg(&json).assert().success();
    let text = std::fs::read_to_string(&json).unwrap();
    let _ = std::fs::remove_file(&json);
    assert!(text.contains("\"__ob_intercept__\""));
    assert!(!text.contains("(intercept)"));
}

#[test]
fn the_library_keeps_the_token_in_its_rows_and_prints_the_label_in_its_summary() {
    let df = df![
        "wage"  => [10.0f64, 12.0, 14.0, 16.0, 11.0, 13.0, 15.0, 30.0,
                    20.0, 22.0, 24.0, 26.0, 21.0, 23.0, 25.0, 40.0],
        "educ"  => [1.0f64, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0,
                    1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0],
        "group" => ["A", "A", "A", "A", "A", "A", "A", "A",
                    "B", "B", "B", "B", "B", "B", "B", "B"],
    ]
    .unwrap();
    let results = OaxacaBuilder::new(df, "wage", "group", "B")
        .predictors(vec!["educ"])
        .bootstrap_reps(0)
        .run()
        .unwrap();
    let rows = results.two_fold().detailed_unexplained();
    assert!(rows.iter().any(|row| row.name() == INTERCEPT_NAME));
    assert!(!rows.iter().any(|row| row.name() == INTERCEPT_LABEL));
    let summary = results.format_summary();
    assert!(summary.contains(INTERCEPT_LABEL));
    assert!(!summary.contains(INTERCEPT_NAME));
    assert!(results.to_json().unwrap().contains(INTERCEPT_NAME));
}
