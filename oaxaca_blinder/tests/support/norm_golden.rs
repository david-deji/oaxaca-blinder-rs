//! Reader and comparator for `norm_goldens_r.json` (0120-MERIDIAN V1/V1b/V1c/V1d).
//!
//! Shared by the library test (`oaxaca_blinder/tests/normalization_oracle_test.rs`) and the
//! shipped-path test (`engine/tests/normalization_shipped_path_test.rs`), which includes this
//! file with `#[path]`.
//!
//! NO EXPECTED VALUE IN THIS TEST COMES FROM ENGINE OUTPUT. Every number is produced by
//! `verification/gen_norm_goldens.R` (base-R `lm()`, `ddecompose`, `oaxaca`). `load` refuses a
//! golden whose recorded sha256 of the generator script, of any fixture it was generated from,
//! or of the R package versions no longer matches the files on disk: a stale golden is an error,
//! not a pass.

#![allow(dead_code)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const SCHEMES: [&str; 5] = [
    "GroupA",
    "GroupB",
    "Pooled",
    "PooledNoIndicator",
    "Weighted",
];

/// Tolerances, from the golden's `_meta.tolerances`: package comparisons (equal-share, exact to
/// print precision), refit comparisons (population-share), identities.
pub const TOL_PACKAGE: f64 = 1e-10;
/// The 10 000-row employers design with three categoricals (13 columns). Measured worst
/// disagreement with ddecompose / the refit is 2.2e-10, on the `Age` row: the engine solves the
/// normal equations by Cholesky, R by QR, and Age (mean 40) amplifies the difference in its
/// slope. Pinned at 1e-9 (4.5x the measurement); the effects being checked are 1e-3..1e-1.
pub const TOL_PACKAGE_10K: f64 = 1e-9;
pub const TOL_REFIT: f64 = 1e-9;
pub const TOL_IDENTITY: f64 = 1e-12;

pub struct Golden {
    pub json: Value,
    pub fixtures_dir: PathBuf,
}

fn sha256_file(path: &Path) -> String {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut h = Sha256::new();
    h.update(&bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

impl Golden {
    /// Load the golden and verify every recorded hash. `fixtures_dir` holds the fixtures and the
    /// golden; `generator` is the path of `verification/gen_norm_goldens.R`.
    pub fn load(fixtures_dir: &Path, generator: &Path) -> Golden {
        let text = std::fs::read_to_string(fixtures_dir.join("norm_goldens_r.json"))
            .expect("norm_goldens_r.json committed and readable");
        let json: Value = serde_json::from_str(&text).expect("norm_goldens_r.json parses");
        let meta = &json["_meta"];

        assert_eq!(
            meta["expected_values_from_engine_output"].as_bool(),
            Some(false),
            "the golden must declare that no expected value comes from engine output"
        );
        assert_eq!(
            meta["generator_sha256"].as_str().unwrap(),
            sha256_file(generator),
            "stale golden: verification/gen_norm_goldens.R changed since the golden was generated. \
             Re-run verification/regen_norm_goldens.sh and commit the result."
        );
        for (name, want) in meta["fixture_sha256"].as_object().expect("fixture_sha256") {
            let want = want
                .as_str()
                .unwrap_or_else(|| panic!("no hash recorded for {name}"));
            assert_eq!(
                want,
                sha256_file(&fixtures_dir.join(name)),
                "stale golden: fixture {name} changed since the golden was generated. \
                 Re-run verification/regen_norm_goldens.sh and commit the result."
            );
        }
        assert_eq!(
            meta["packages"]["oaxaca"].as_str(),
            Some("0.1.5"),
            "golden must be generated against oaxaca 0.1.5"
        );
        assert_eq!(
            meta["packages"]["ddecompose"].as_str(),
            Some("1.0.0"),
            "golden must be generated against ddecompose 1.0.0"
        );
        Golden {
            json,
            fixtures_dir: fixtures_dir.to_path_buf(),
        }
    }

    pub fn case(&self, name: &str) -> &Value {
        let c = &self.json["cases"][name];
        assert!(!c.is_null(), "golden has no case {name:?}");
        c
    }

    pub fn rif_case(&self, name: &str) -> &Value {
        assert_eq!(self.json["rif"]["available"].as_bool(), Some(true));
        let c = &self.json["rif"][name];
        assert!(!c.is_null(), "golden has no rif case {name:?}");
        c
    }

    pub fn package(&self, name: &str) -> &Value {
        let c = &self.json["packages"][name];
        assert!(!c.is_null(), "golden has no package block {name:?}");
        c
    }

    pub fn fixture(&self, name: &str) -> PathBuf {
        self.fixtures_dir.join(name)
    }
}

/// `(name, value)` pairs of a JSON object of numbers.
pub fn pairs(obj: &Value) -> Vec<(String, f64)> {
    obj.as_object()
        .expect("object of numbers")
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_f64().unwrap_or_else(|| panic!("{k} not a number")),
            )
        })
        .collect()
}

/// Compare an engine vector with a golden object: the NAME SETS are compared first (a missing
/// or extra level fails structurally), then every value to `tol` (absolute). Returns the worst
/// absolute difference seen.
pub fn compare_vector(label: &str, engine: &[(String, f64)], golden: &Value, tol: f64) -> f64 {
    let want = pairs(golden);
    let got_names: BTreeSet<&String> = engine.iter().map(|(n, _)| n).collect();
    let want_names: BTreeSet<&String> = want.iter().map(|(n, _)| n).collect();
    assert_eq!(
        engine.len(),
        got_names.len(),
        "{label}: engine returned a duplicated name: {engine:?}"
    );
    assert_eq!(
        got_names, want_names,
        "{label}: name sets differ (engine vs oracle)"
    );
    let mut worst = 0.0_f64;
    for (name, w) in &want {
        let g = engine.iter().find(|(n, _)| n == name).unwrap().1;
        let d = (g - w).abs();
        worst = worst.max(d);
        assert!(
            d <= tol,
            "{label}[{name}]: engine={g:.15e} oracle={w:.15e} |diff|={d:.3e} > {tol:.1e}"
        );
    }
    worst
}

pub fn assert_close(label: &str, got: f64, want: f64, tol: f64) -> f64 {
    let d = (got - want).abs();
    assert!(
        d <= tol,
        "{label}: engine={got:.15e} oracle={want:.15e} |diff|={d:.3e} > {tol:.1e}"
    );
    d
}

/// Shares echoed by the engine (`run_metadata.normalization.variables`) against the oracle's
/// `shares` object `{variable: {level: share}}`, to 1e-12.
pub fn compare_shares(label: &str, engine: &[(String, Vec<(String, f64)>)], golden: &Value) {
    let want = golden.as_object().expect("shares object");
    let got_vars: BTreeSet<&String> = engine.iter().map(|(v, _)| v).collect();
    let want_vars: BTreeSet<&String> = want.keys().collect();
    assert_eq!(
        got_vars, want_vars,
        "{label}: normalised variable sets differ"
    );
    for (var, levels) in engine {
        compare_vector(
            &format!("{label}.shares[{var}]"),
            levels,
            &want[var],
            TOL_IDENTITY,
        );
    }
}
