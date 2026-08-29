//! Bootstrap-SE golden — R-vs-Rust on the SAME resamples (0097; 0014-MERIDIAN D-3/W5).
//!
//! The engine's point estimates carry two independent external oracles at 1e-6. Its *inference*
//! carried none: this file did not exist, while the golden it consumes and the 182 KB index matrix
//! it replays were both generated and committed. `gen_trust_goldens.R` said why — the golden was
//! missing `bootstrap.sub_idx_0based`, without which the Rust side cannot reconstruct the 800-row
//! subset R drew, so the test "stays BLOCKED until this golden is regenerated". It was regenerated
//! on 2026-08-29, 39 days later, and this is the consumer it was waiting for.
//!
//! This is an EXACT cross-check, not a statistical comparison: both sides compute replicate
//! estimates over the *same committed within-group resample indices*, so agreement is a property of
//! the arithmetic rather than of sampling luck. Tolerance is `_meta.tolerances.bootstrap_se`
//! (rel 1e-3) — loose only because the two sides differ in float summation order, not in method.
//!
//! Regenerate both sides with `Rscript verification/gen_trust_goldens.R`
//! (R >= 4.1 + oaxaca 0.1.5 + quantreg + ddecompose; `TRUST_RAW_CSV` overrides the raw input).

use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;
use serde_json::Value;

const FIXTURE: &str = "tests/fixtures/employers_trust_fixture.csv";
const GOLDEN: &str = "tests/fixtures/trust_goldens_r.json";
const INDICES: &str = "tests/fixtures/resample_indices.csv";

/// `_meta.tolerances.bootstrap_se`.
const REL_TOL: f64 = 1e-3;

fn load_fixture() -> DataFrame {
    LazyCsvReader::new(FIXTURE)
        .with_has_header(true)
        .finish()
        .expect("fixture readable")
        .collect()
        .expect("fixture parses")
}

fn take_rows(df: &DataFrame, rows: &[u32]) -> DataFrame {
    let idx = IdxCa::from_vec("idx".into(), rows.to_vec());
    df.take(&idx).expect("take rows")
}

/// 60 rows x (n_a + n_b) 0-based within-group indices, header `a1..aN,b1..bM`.
fn load_indices(n_a: usize, n_b: usize, reps: usize) -> Vec<(Vec<u32>, Vec<u32>)> {
    let text = std::fs::read_to_string(INDICES).expect("resample_indices.csv committed + readable");
    let mut lines = text.lines();
    let header = lines.next().expect("indices header");
    assert_eq!(
        header.split(',').count(),
        n_a + n_b,
        "index matrix width must equal n_a + n_b from the golden"
    );
    let out: Vec<(Vec<u32>, Vec<u32>)> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Vec<u32> = l
                .split(',')
                .map(|c| c.trim().parse::<u32>().expect("index parses"))
                .collect();
            assert_eq!(v.len(), n_a + n_b, "every replicate row is full width");
            (v[..n_a].to_vec(), v[n_a..].to_vec())
        })
        .collect();
    assert_eq!(
        out.len(),
        reps,
        "index matrix row count must equal golden reps"
    );
    out
}

/// The engine's own single-pass two-fold decomposition on one replicate.
fn estimate(df: DataFrame) -> (f64, f64, f64) {
    let mut b = OaxacaBuilder::new(df, "log_salary", "Gender", "Female");
    b.predictors(vec!["Age", "Experience_Years"])
        .categorical_predictors(vec!["Education_Level", "Department", "Location"])
        // beta* = beta_B, matching the generator's `bstar <- bB`.
        .reference_coefficients(ReferenceCoefficients::GroupB)
        .bootstrap_reps(1);
    let r = b.run().expect("replicate decomposition");
    (
        r.explained().expect("explained").estimate,
        r.unexplained().expect("unexplained").estimate,
        r.total_gap,
    )
}

/// Sample standard deviation, `n - 1` denominator — R's `sd()`.
fn sd(xs: &[f64]) -> f64 {
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    (xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
}

fn assert_close(label: &str, engine: f64, golden: f64) {
    let tol = REL_TOL * golden.abs();
    let diff = (engine - golden).abs();
    assert!(
        diff <= tol,
        "{label}: engine={engine:.12}, R={golden:.12}, diff={diff:.3e} > tol {tol:.3e}"
    );
}

#[test]
fn bootstrap_standard_errors_match_the_r_oracle_on_identical_resamples() {
    let golden: Value =
        serde_json::from_str(&std::fs::read_to_string(GOLDEN).expect("golden readable"))
            .expect("golden parses");
    let b = &golden["bootstrap"];

    let sub_idx: Vec<u32> = b["sub_idx_0based"]
        .as_array()
        .expect(
            "golden must carry bootstrap.sub_idx_0based — regenerate with \
             `Rscript verification/gen_trust_goldens.R` (see this file's header)",
        )
        .iter()
        .map(|v| v.as_u64().expect("subset index") as u32)
        .collect();

    let n_a = b["n_a"].as_u64().unwrap() as usize;
    let n_b = b["n_b"].as_u64().unwrap() as usize;
    let reps = b["reps"].as_u64().unwrap() as usize;
    assert_eq!(
        sub_idx.len(),
        b["subset_n"].as_u64().unwrap() as usize,
        "sub_idx_0based length must equal subset_n"
    );

    // Rebuild R's `sub` — the deterministic 800-row subset, in ascending fixture order.
    let subset = take_rows(&load_fixture(), &sub_idx);

    // Split it exactly as R did: `which(sub$Gender == "Male")` / `== "Female"`, order preserved.
    let gender = subset.column("Gender").unwrap().str().unwrap();
    let mut rows_a: Vec<u32> = Vec::new();
    let mut rows_b: Vec<u32> = Vec::new();
    for (i, g) in gender.into_iter().enumerate() {
        match g {
            Some("Male") => rows_a.push(i as u32),
            Some("Female") => rows_b.push(i as u32),
            other => panic!("unexpected Gender value in fixture subset: {other:?}"),
        }
    }
    assert_eq!(rows_a.len(), n_a, "subset male count must match golden n_a");
    assert_eq!(
        rows_b.len(),
        n_b,
        "subset female count must match golden n_b"
    );

    let sub_a = take_rows(&subset, &rows_a);
    let sub_b = take_rows(&subset, &rows_b);

    // Replay the committed resamples through the engine.
    let mut explained = Vec::with_capacity(reps);
    let mut unexplained = Vec::with_capacity(reps);
    let mut total_gap = Vec::with_capacity(reps);
    for (idx_a, idx_b) in load_indices(n_a, n_b, reps) {
        let mut rep = take_rows(&sub_a, &idx_a);
        rep.vstack_mut(&take_rows(&sub_b, &idx_b))
            .expect("vstack replicate");
        let (ex, un, gap) = estimate(rep);
        explained.push(ex);
        unexplained.push(un);
        total_gap.push(gap);
    }

    assert_close(
        "se(explained)",
        sd(&explained),
        b["se_explained"].as_f64().unwrap(),
    );
    assert_close(
        "se(unexplained)",
        sd(&unexplained),
        b["se_unexplained"].as_f64().unwrap(),
    );
    assert_close(
        "se(total_gap)",
        sd(&total_gap),
        b["se_total_gap"].as_f64().unwrap(),
    );
}
