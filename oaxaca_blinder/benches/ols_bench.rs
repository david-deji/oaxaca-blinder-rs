use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nalgebra::{DMatrix, DVector};
use oaxaca_blinder::math::ols::ols;

fn bench_ols_weighted(c: &mut Criterion) {
    let n = 100_000;
    let k = 20;

    // Generate deterministic synthetic data
    let mut x_data = Vec::with_capacity(n * k);
    for j in 0..k {
        for i in 0..n {
            x_data.push(if j == 0 {
                1.0
            } else {
                ((i + j) % 100) as f64 * 0.1
            });
        }
    }
    let x = DMatrix::from_vec(n, k, x_data);

    let y_data: Vec<f64> = (0..n).map(|i| (i % 50) as f64 * 0.5 + 1.0).collect();
    let y = DVector::from_vec(y_data);

    let w_data: Vec<f64> = (0..n).map(|i| ((i % 10) as f64) * 0.1 + 0.5).collect();
    let weights = DVector::from_vec(w_data);

    c.bench_function("ols_weighted_100k_x_20", |b| {
        b.iter(|| {
            black_box(ols(black_box(&y), black_box(&x), Some(black_box(&weights))))
                .expect("ols failed")
        });
    });
}

criterion_group!(benches, bench_ols_weighted);
criterion_main!(benches);
