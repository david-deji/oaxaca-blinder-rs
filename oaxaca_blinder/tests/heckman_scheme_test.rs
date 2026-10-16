//! A Heckman selection model with `Pooled` or `PooledNoIndicator` used to panic on a matrix dimension mismatch.
//! It now returns an `OaxacaError` that names the combination. The schemes that run keep running.

#![allow(deprecated)] // ReferenceCoefficients::Neumark

use assert_cmd::prelude::*;
use oaxaca_blinder::{OaxacaBuilder, OaxacaError, ReferenceCoefficients};
use polars::prelude::*;
use predicates::prelude::*;
use rand::distributions::Distribution;
use rand::prelude::*;
use statrs::distribution::Normal;

fn frame() -> DataFrame {
    let n = 800;
    let mut rng = StdRng::seed_from_u64(11);
    let normal = Normal::new(0.0, 1.0).unwrap();
    let (mut y, mut x, mut z, mut s, mut g) = (vec![], vec![], vec![], vec![], vec![]);
    for _ in 0..n {
        let zi: f64 = normal.sample(&mut rng);
        let xi = zi + 0.5 * normal.sample(&mut rng);
        let u: f64 = normal.sample(&mut rng);
        let e = 0.8 * u + 0.6 * normal.sample(&mut rng);
        let selected = 0.5 * zi + u > 0.0;
        y.push(if selected {
            Some(1.0 + 2.0 * xi + e)
        } else {
            None
        });
        x.push(xi);
        z.push(zi);
        s.push(if selected { 1.0 } else { 0.0 });
        g.push(if rng.gen_bool(0.5) { "A" } else { "B" });
    }
    df!("outcome" => y, "x" => x, "z" => z, "selection" => s, "group" => g).unwrap()
}

fn run(reference: ReferenceCoefficients) -> Result<oaxaca_blinder::OaxacaResults, OaxacaError> {
    OaxacaBuilder::new(frame(), "outcome", "group", "B")
        .predictors(vec!["x"])
        .heckman_selection("selection", vec!["z"])
        .reference_coefficients(reference)
        .bootstrap_reps(0)
        .run()
}

#[test]
fn pooled_schemes_with_a_selection_model_are_an_error_that_names_the_combination() {
    for (reference, name) in [
        (ReferenceCoefficients::Pooled, "Pooled"),
        (
            ReferenceCoefficients::PooledNoIndicator,
            "PooledNoIndicator",
        ),
        (ReferenceCoefficients::Neumark, "Neumark"),
    ] {
        match run(reference) {
            Err(OaxacaError::UnsupportedCombination { option, with, .. }) => {
                assert_eq!(option, "heckman_selection");
                assert_eq!(with, name);
            }
            other => panic!("{name}: expected UnsupportedCombination, got {other:?}"),
        }
    }
}

#[test]
fn the_message_starts_with_the_code_and_names_both_options_and_the_schemes_that_work() {
    let err = run(ReferenceCoefficients::Pooled).unwrap_err().to_string();
    assert!(err.starts_with("UNSUPPORTED_COMBINATION:"), "{err}");
    assert!(err.contains("heckman_selection"), "{err}");
    assert!(err.contains("with=Pooled"), "{err}");
    for ok in ["GroupA", "GroupB", "Weighted", "Cotton"] {
        assert!(err.contains(ok), "{err}");
    }
}

#[test]
fn the_schemes_that_run_with_a_selection_model_still_run() {
    for reference in [
        ReferenceCoefficients::GroupA,
        ReferenceCoefficients::GroupB,
        ReferenceCoefficients::Weighted,
        ReferenceCoefficients::Cotton,
    ] {
        let results = run(reference).unwrap_or_else(|e| panic!("{reference:?}: {e}"));
        assert!(results.total_gap().is_finite());
    }
}

#[test]
fn the_quantile_path_refuses_the_same_combination() {
    let mut b = OaxacaBuilder::new(frame(), "outcome", "group", "B");
    b.predictors(vec!["x"])
        .heckman_selection("selection", vec!["z"])
        .reference_coefficients(ReferenceCoefficients::PooledNoIndicator)
        .bootstrap_reps(0);
    assert!(matches!(
        b.decompose_quantile(0.5),
        Err(OaxacaError::UnsupportedCombination { .. })
    ));
}

#[test]
fn without_a_selection_model_the_pooled_schemes_are_unaffected() {
    let df = frame();
    for reference in [
        ReferenceCoefficients::Pooled,
        ReferenceCoefficients::PooledNoIndicator,
    ] {
        OaxacaBuilder::new(df.clone(), "outcome", "group", "B")
            .predictors(vec!["x"])
            .reference_coefficients(reference)
            .bootstrap_reps(0)
            .run()
            .unwrap_or_else(|e| panic!("{reference:?}: {e}"));
    }
}

#[test]
fn the_command_line_says_so_instead_of_panicking() {
    let df = frame();
    let col = |name: &str| df.column(name).unwrap().as_materialized_series().clone();
    let (outcome, x, z, selection, group) = (
        col("outcome"),
        col("x"),
        col("z"),
        col("selection"),
        col("group"),
    );
    let mut csv = String::from("outcome,x,z,selection,group\n");
    for i in 0..df.height() {
        let cell = |s: &Series| s.get(i).unwrap().to_string().replace('"', "");
        let y = match outcome.f64().unwrap().get(i) {
            Some(v) => v.to_string(),
            None => String::new(),
        };
        csv.push_str(&format!(
            "{y},{},{},{},{}\n",
            cell(&x),
            cell(&z),
            cell(&selection),
            cell(&group)
        ));
    }
    let path =
        std::env::temp_dir().join(format!("oaxaca_heckman_pooled_{}.csv", std::process::id()));
    std::fs::write(&path, csv).unwrap();
    #[allow(deprecated)]
    let mut cmd = std::process::Command::cargo_bin("oaxaca-cli").unwrap();
    cmd.arg("--data")
        .arg(&path)
        .args([
            "--outcome",
            "outcome",
            "--group",
            "group",
            "--reference",
            "B",
        ])
        .args(["--predictors", "x", "--selection-outcome", "selection"])
        .args(["--selection-predictors", "z", "--ref-coeffs", "pooled"])
        .args(["--bootstrap-reps", "0"]);
    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("UNSUPPORTED_COMBINATION"))
        .stderr(predicate::str::contains("panicked").not());
    let _ = std::fs::remove_file(&path);
}
