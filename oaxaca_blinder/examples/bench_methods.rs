//! Rust-vs-R timing harness (Rust side). Serial, warm-up discarded, median of N.
//! Build/run with RELEASE optimizations (debug is 10-50x slower — an unfair comparison):
//!   cargo run --release -p oaxaca_blinder --example bench_methods
//! Same fixture + model as verification/bench_methods.R. A benchmark, not a golden.
use oaxaca_blinder::{qr_coefficients, OaxacaBuilder};
use polars::prelude::*;
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn bench<F: FnMut()>(mut f: F, n: usize, warm: usize) -> f64 {
    for _ in 0..warm {
        f();
    }
    let mut t = Vec::with_capacity(n);
    for _ in 0..n {
        let s = Instant::now();
        f();
        t.push(s.elapsed().as_secs_f64() * 1000.0);
    }
    median(t)
}

fn main() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/employers_trust_fixture.csv"
    );
    let df = LazyCsvReader::new(path)
        .with_has_header(true)
        .finish()
        .unwrap()
        .collect()
        .unwrap();
    let preds = vec!["Age", "Experience_Years"];
    let cats = vec!["Education_Level", "Department", "Location"];

    let build = |reps: usize| {
        let mut b = OaxacaBuilder::new(df.clone(), "log_salary", "Gender", "Female");
        b.predictors(preds.clone())
            .categorical_predictors(cats.clone())
            .bootstrap_reps(reps);
        b
    };

    let r1 = bench(
        || {
            build(1).run().unwrap();
        },
        7,
        1,
    );
    println!("Rust_mean_ob_point_ms {:.2}", r1);

    let r2 = bench(
        || {
            build(100).run().unwrap();
        },
        3,
        1,
    );
    println!("Rust_mean_ob_boot100_ms {:.2}", r2);

    let r3 = bench(
        || {
            build(1).decompose_quantile(0.5).unwrap();
        },
        3,
        1,
    );
    println!("Rust_rif_quantile_point_ms {:.2}", r3);

    let age: Vec<f64> = df
        .column("Age")
        .unwrap()
        .cast(&DataType::Float64)
        .unwrap()
        .f64()
        .unwrap()
        .into_iter()
        .map(|o| o.unwrap())
        .collect();
    let y: Vec<f64> = df
        .column("log_salary")
        .unwrap()
        .f64()
        .unwrap()
        .into_iter()
        .map(|o| o.unwrap())
        .collect();
    let design: Vec<Vec<f64>> = age.iter().map(|&a| vec![1.0, a]).collect();
    let r4 = bench(
        || {
            qr_coefficients(&design, &y, 0.5).unwrap();
        },
        5,
        1,
    );
    println!("Rust_qr_tau50_ms {:.2}", r4);
}
