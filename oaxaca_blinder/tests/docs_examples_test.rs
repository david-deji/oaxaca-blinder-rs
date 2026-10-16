//! The docs site and both READMEs show the output of real runs. This test runs the same programs and compares
//! what they print with the committed text, so a number that changes without the docs changing fails CI.
//!
//! * The three examples in `examples/` are run and compared with `docs/src/outputs/<name>.txt`.
//! * The command-line run on `docs/src/sample/wage.csv` is compared with `docs/src/outputs/cli_run.txt`.
//! * Both READMEs must carry the output of `first_decomposition` word for word.
//! * The five reference schemes on the sample file equal the numbers R `oaxaca` 0.1.5 printed for it, which the
//!   page "Reproducing results in R" shows (R 4.6.1, `oaxaca(wage ~ education + experience | female, R = NULL)`).
//!
//! Regenerate after an intended change: `UPDATE_DOCS=1 cargo test -p oaxaca_blinder --test docs_examples_test`
//! writes the four output files; review the diff before committing it.

use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn outputs_dir() -> PathBuf {
    manifest().join("../docs/src/outputs")
}

fn updating() -> bool {
    std::env::var_os("UPDATE_DOCS").is_some()
}

/// The built example next to this test's own binary (`target/<profile>/examples/<name>`), which a plain
/// `cargo test` has already compiled. A single `--test` run does not rebuild examples, so when the binary is
/// missing or older than its source, build and run it with `cargo run` instead.
fn run_example(name: &str) -> String {
    let mut exe = std::env::current_exe().expect("test exe");
    exe.pop(); // deps
    exe.pop(); // profile directory
    exe.push("examples");
    exe.push(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let source = manifest().join("examples").join(format!("{name}.rs"));
    let fresh = match (std::fs::metadata(&exe), std::fs::metadata(&source)) {
        (Ok(built), Ok(src)) => built.modified().unwrap() >= src.modified().unwrap(),
        _ => false,
    };
    let output = if fresh {
        Command::new(&exe).output().expect("run example")
    } else {
        Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args([
                "run",
                "--quiet",
                "--locked",
                "-p",
                "oaxaca_blinder",
                "--example",
                name,
            ])
            .current_dir(manifest())
            .output()
            .expect("cargo run --example")
    };
    assert!(
        output.status.success(),
        "example {name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 output")
}

fn compare_or_update(file: &str, actual: &str) {
    let path = outputs_dir().join(file);
    if updating() {
        std::fs::write(&path, actual).expect("write output");
        return;
    }
    let expected =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        expected, actual,
        "docs/src/outputs/{file} is out of date. Regenerate with \
         `UPDATE_DOCS=1 cargo test -p oaxaca_blinder --test docs_examples_test` and review the diff."
    );
}

#[test]
fn first_decomposition_output_matches_the_docs() {
    compare_or_update(
        "first_decomposition.txt",
        &run_example("first_decomposition"),
    );
}

#[test]
fn weights_and_categoricals_output_matches_the_docs() {
    compare_or_update(
        "weights_and_categoricals.txt",
        &run_example("weights_and_categoricals"),
    );
}

#[test]
fn quantile_rif_output_matches_the_docs() {
    compare_or_update("quantile_rif.txt", &run_example("quantile_rif"));
}

#[test]
fn the_documented_command_line_run_matches_the_docs() {
    let sample = manifest().join("../docs/src/sample/wage.csv");
    let output = Command::new(env!("CARGO_BIN_EXE_oaxaca-cli"))
        .args(["--data"])
        .arg(&sample)
        .args([
            "--outcome",
            "wage",
            "--group",
            "gender",
            "--reference",
            "F",
            "--predictors",
            "education,experience",
            "--ref-coeffs",
            "pooled",
        ])
        .output()
        .expect("run oaxaca-cli");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    compare_or_update(
        "cli_run.txt",
        &String::from_utf8(output.stdout).expect("utf-8 output"),
    );
}

#[test]
fn the_documented_weighted_command_line_run_succeeds() {
    // The start page shows this line; it must keep working on the sample file.
    let sample = manifest().join("../docs/src/sample/wage.csv");
    let output = Command::new(env!("CARGO_BIN_EXE_oaxaca-cli"))
        .args(["--data"])
        .arg(&sample)
        .args([
            "--outcome",
            "wage",
            "--group",
            "gender",
            "--reference",
            "F",
            "--predictors",
            "education,experience",
            "--weights",
            "headcount",
            "--weights-kind",
            "frequency",
        ])
        .output()
        .expect("run oaxaca-cli");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn both_readmes_show_the_first_decomposition_output() {
    let expected = read(&outputs_dir().join("first_decomposition.txt"));
    let expected = expected.trim_end();
    for readme in ["README.md", "../README.md"] {
        let text = read(&manifest().join(readme));
        assert!(
            text.contains(expected),
            "{readme} must show the output of examples/first_decomposition.rs exactly:\n{expected}"
        );
    }
}

/// The sample file against R `oaxaca`: (scheme, R `group.weight`, explained, unexplained), as R printed them
/// to three decimals. The weight column of R's table is the label of the row the numbers came from.
#[test]
fn the_five_schemes_on_the_sample_equal_r_oaxaca() {
    let sample = manifest().join("../docs/src/sample/wage.csv");
    let df = LazyCsvReader::new(sample)
        .with_has_header(true)
        .finish()
        .and_then(|lf| lf.collect())
        .expect("read the sample");
    let rows = [
        (ReferenceCoefficients::GroupB, 0.0, 1943.891, 6031.109),
        (ReferenceCoefficients::GroupA, 1.0, 2360.853, 5614.147),
        (ReferenceCoefficients::Weighted, 0.5, 2152.372, 5822.628),
        (
            ReferenceCoefficients::PooledNoIndicator,
            -1.0,
            2409.076,
            5565.924,
        ),
        (ReferenceCoefficients::Pooled, -2.0, 2105.824, 5869.176),
    ];
    for (scheme, weight, explained, unexplained) in rows {
        let results = OaxacaBuilder::new(df.clone(), "wage", "gender", "F")
            .predictors(["education", "experience"])
            .reference_coefficients(scheme)
            .bootstrap_reps(2)
            .run()
            .expect("run");
        let e = *results.explained().expect("explained").estimate();
        let u = *results.unexplained().expect("unexplained").estimate();
        assert!(
            (e - explained).abs() < 2e-3 && (u - unexplained).abs() < 2e-3,
            "{scheme:?} (R group.weight {weight}): got {e:.4} / {u:.4}, R printed {explained} / {unexplained}"
        );
    }
}

#[test]
fn both_readmes_show_the_example_source_they_print_the_output_of() {
    let source = read(&manifest().join("examples/first_decomposition.rs"));
    let code = &source[source.find("use oaxaca_blinder").expect("example imports")..];
    let code = code.trim_end();
    for readme in ["README.md", "../README.md"] {
        let text = read(&manifest().join(readme));
        assert!(
            text.contains(code),
            "{readme} must show examples/first_decomposition.rs from its first `use` line, unchanged"
        );
    }
}
