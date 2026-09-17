use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nalgebra::{DMatrix, DVector};
use oaxaca_blinder::matching::logistic::LogisticRegression;

fn bench_logistic_fit(c: &mut Criterion) {
    let n_samples = 2000;
    let n_features = 20;

    // Generate deterministic dummy data
    let mut x_data = Vec::with_capacity(n_samples * n_features);
    for i in 0..n_samples {
        x_data.push(1.0); // Intercept
        for j in 1..n_features {
            x_data.push(((i * 37 + j * 17) % 100) as f64 / 100.0);
        }
    }
    let x = DMatrix::from_row_slice(n_samples, n_features, &x_data);

    let y_data: Vec<f64> = (0..n_samples)
        .map(|i| if (i * 13) % 2 == 0 { 1.0 } else { 0.0 })
        .collect();
    let y = DVector::from_vec(y_data);

    c.bench_function("logistic_fit_2000x20", |b| {
        b.iter(|| {
            let mut model = LogisticRegression::new();
            model.fit(black_box(&x), black_box(&y), 20, 1e-6).unwrap();
            black_box(model)
        })
    });
}

criterion_group!(benches, bench_logistic_fit);
criterion_main!(benches);
