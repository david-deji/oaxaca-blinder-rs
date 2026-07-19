//! AC-13 CLI<->library/WASM numeric parity test (0014-MERIDIAN).
//!
//! main.rs:234-239 documents, BY COMMENT ONLY, that the CLI quantile path and the WASM/MCP
//! surface share the exact same `OaxacaBuilder::decompose_quantile` code path. The browser leg
//! of that claim was proven separately in stage 4D. This test proves the other half
//! numerically: CLI-subprocess-output == direct-in-process-library-call output, on the same
//! fixture, same predictors, same (default) seed. Because `decompose_quantile` is exactly the
//! function the WASM/MCP surface calls too, CLI==library proves CLI==WASM transitively without
//! needing a browser.
//!
//! Determinism note: neither side passes `--seed` / `.seed(..)` (the CLI exposes no seed flag),
//! so both resolve to the fixed `DEFAULT_SEED` constant (oaxaca_blinder/src/rng.rs) and are
//! byte-identical regardless of thread count (builder.rs, INV-02) — so a tight tolerance is
//! legitimate here, not a loose statistical comparison. The CLI's `--ref-coeffs` also defaults
//! to `group-b` (main.rs), matching the explicit `ReferenceCoefficients::GroupB` below.

#![allow(deprecated)] // assert_cmd's `Command::cargo_bin` is deprecated upstream; matches the
                      // suppression in the sibling cli_test.rs. This test uses no other
                      // deprecated API.
use assert_cmd::prelude::*;
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;
use std::process::Command;

const TOL: f64 = 1e-9;

fn assert_close(label: &str, cli: f64, lib: f64) {
    assert!(
        (cli - lib).abs() < TOL,
        "{} mismatch: cli={} lib={} diff={}",
        label,
        cli,
        lib,
        (cli - lib).abs()
    );
}

#[test]
fn cli_quantile_output_matches_direct_library_call() {
    let out_path = std::env::temp_dir().join(format!(
        "oaxaca_cli_wasm_parity_{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&out_path);

    // 1. Run the CLI on the committed fixture -- the exact path a real user hits.
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
        .arg("--quantiles")
        .arg("0.5")
        .arg("--bootstrap-reps")
        .arg("2")
        .arg("--ref-coeffs")
        .arg("group-b")
        .arg("--output-json")
        .arg(&out_path);
    cmd.assert().success();

    let cli_json = std::fs::read_to_string(&out_path)
        .expect("CLI --output-json did not write the expected file for a single quantile");
    let _ = std::fs::remove_file(&out_path);
    let cli_value: serde_json::Value =
        serde_json::from_str(&cli_json).expect("CLI JSON output did not parse");

    // 2. Call the exact same library entry point in-process, on the same inputs, no CLI
    //    subprocess involved. This is the code path WASM/MCP also calls.
    let df = LazyCsvReader::new("tests/data/wage.csv")
        .with_has_header(true)
        .finish()
        .unwrap()
        .collect()
        .unwrap();
    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    builder
        .predictors(["education"])
        .bootstrap_reps(2)
        .reference_coefficients(ReferenceCoefficients::GroupB);
    let lib_results = builder
        .decompose_quantile(0.5)
        .expect("library decompose_quantile failed");
    let lib_json_str = lib_results.to_json().expect("library to_json failed");
    let lib_value: serde_json::Value =
        serde_json::from_str(&lib_json_str).expect("library JSON did not parse");

    // 3. Compare every numeric field CLI emitted vs. the library's own serialization.
    assert_close(
        "total_gap",
        cli_value["total_gap"].as_f64().unwrap(),
        lib_value["total_gap"].as_f64().unwrap(),
    );
    assert_eq!(cli_value["n_a"], lib_value["n_a"], "n_a mismatch");
    assert_eq!(cli_value["n_b"], lib_value["n_b"], "n_b mismatch");

    let cli_components = cli_value["two_fold"]["aggregate"]
        .as_array()
        .expect("cli two_fold.aggregate missing");
    let lib_components = lib_value["two_fold"]["aggregate"]
        .as_array()
        .expect("lib two_fold.aggregate missing");
    assert_eq!(
        cli_components.len(),
        lib_components.len(),
        "component count mismatch between CLI and library output"
    );

    for (c, l) in cli_components.iter().zip(lib_components.iter()) {
        let name = c["name"].as_str().expect("cli component missing name");
        assert_eq!(
            name,
            l["name"].as_str().expect("lib component missing name"),
            "component name mismatch"
        );
        for field in ["estimate", "std_err", "p_value", "ci_lower", "ci_upper"] {
            let cli_v = c[field]
                .as_f64()
                .unwrap_or_else(|| panic!("cli component '{}' missing field {}", name, field));
            let lib_v = l[field]
                .as_f64()
                .unwrap_or_else(|| panic!("lib component '{}' missing field {}", name, field));
            assert_close(&format!("{}.{}", name, field), cli_v, lib_v);
        }
    }
}
