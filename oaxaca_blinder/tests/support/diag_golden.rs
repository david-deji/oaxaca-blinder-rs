//! Reader for `diag_goldens_r.json` (0120-MERIDIAN V6 / V7 / V8 / V9).
//!
//! Shared by the library test (`weights_kind_test.rs`) and the engine tests
//! (`support_diagnostics_test.rs`, `intervals_test.rs`, `quantile_report_test.rs`), which include
//! it with `#[path]`.
//!
//! NO EXPECTED VALUE IN THESE TESTS COMES FROM ENGINE OUTPUT. Every number is produced by
//! `verification/gen_diag_goldens.R` (base R `lm` / `predict.lm` / `quantile`, `ddecompose`,
//! `Hmisc`). `load` refuses a golden whose recorded sha256 of the generator script or of any
//! fixture it was generated from no longer matches the file on disk: a stale golden is an error,
//! not a pass.

#![allow(dead_code)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct DiagGolden {
    pub json: Value,
    pub fixtures_dir: PathBuf,
    repo_root: PathBuf,
}

fn sha256_file(path: &Path) -> String {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut h = Sha256::new();
    h.update(&bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

impl DiagGolden {
    /// `repo_root` is the workspace root; the fixtures live under
    /// `<root>/oaxaca_blinder/tests/fixtures`.
    pub fn load(repo_root: &Path) -> DiagGolden {
        let fixtures_dir = repo_root.join("oaxaca_blinder/tests/fixtures");
        let text = std::fs::read_to_string(fixtures_dir.join("diag_goldens_r.json"))
            .expect("diag_goldens_r.json committed and readable");
        let json: Value = serde_json::from_str(&text).expect("diag_goldens_r.json parses");
        assert_eq!(
            json["_meta"]["expected_values_from_engine_output"].as_bool(),
            Some(false),
            "the golden must declare that no expected value comes from engine output"
        );
        let g = DiagGolden {
            json,
            fixtures_dir,
            repo_root: repo_root.to_path_buf(),
        };
        assert_eq!(
            g.json["_meta"]["generator_sha256"].as_str().unwrap(),
            sha256_file(&repo_root.join("verification/gen_diag_goldens.R")),
            "stale golden: verification/gen_diag_goldens.R changed since the golden was \
             generated. Re-run it and commit the result."
        );
        for (name, want) in g.json["_meta"]["fixture_sha256"]
            .as_object()
            .expect("fixture_sha256")
        {
            assert_eq!(
                want.as_str()
                    .unwrap_or_else(|| panic!("no hash for {name}")),
                sha256_file(&g.fixture_path(name)),
                "stale golden: fixture {name} changed since the golden was generated. \
                 Re-run verification/gen_diag_goldens.R and commit the result."
            );
        }
        assert_eq!(
            g.json["_meta"]["packages"]["ddecompose"].as_str(),
            Some("1.0.0")
        );
        assert_eq!(g.json["_meta"]["packages"]["Hmisc"].as_str(), Some("5.2.6"));
        g
    }

    /// The file a fixture name refers to. Two of the inputs live outside the fixtures dir.
    pub fn fixture_path(&self, name: &str) -> PathBuf {
        match name {
            "0118-fixture-f.csv" => self.repo_root.join("engine/tests/fixtures").join(name),
            "wage.csv" => self.repo_root.join("oaxaca_blinder/tests/data").join(name),
            _ => self.fixtures_dir.join(name),
        }
    }

    pub fn csv(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.fixture_path(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    pub fn block(&self, path: &[&str]) -> &Value {
        let mut v = &self.json;
        for p in path {
            v = &v[*p];
            assert!(!v.is_null(), "golden has no block {path:?}");
        }
        v
    }
}

pub fn f(v: &Value, key: &str) -> f64 {
    v[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{key} is not a number in {v}"))
}

/// Relative difference with a floor of 1 on the scale, so values near zero compare absolutely.
pub fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / 1.0_f64.max(a.abs()).max(b.abs())
}

pub fn assert_close(label: &str, got: f64, want: f64, tol: f64) {
    assert!(
        rel(got, want) <= tol,
        "{label}: engine={got:.17e} oracle={want:.17e} rel diff {:.3e} > {tol:.1e}",
        rel(got, want)
    );
}
