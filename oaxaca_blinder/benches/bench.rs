use criterion::{black_box, criterion_group, criterion_main, Criterion};
use oaxaca_blinder::decomposition::{detailed_decomposition, DetailedComponent};
use nalgebra::DVector;

fn bench_detailed_decomposition(c: &mut Criterion) {
    let n = 1000;
    let xa_mean = DVector::from_element(n, 1.0);
    let xb_mean = DVector::from_element(n, 2.0);
    let beta_a = DVector::from_element(n, 0.5);
    let beta_b = DVector::from_element(n, 0.6);
    let beta_star = DVector::from_element(n, 0.55);
    let predictor_names: Vec<String> = (0..n).map(|i| format!("var_{}", i)).collect();

    c.bench_function("detailed_decomposition_1000", |b| {
        b.iter(|| {
            detailed_decomposition(
                black_box(&xa_mean),
                black_box(&xb_mean),
                black_box(&beta_a),
                black_box(&beta_b),
                black_box(&beta_star),
                black_box(&predictor_names),
            )
        })
    });
}

criterion_group!(benches, bench_detailed_decomposition);
criterion_main!(benches);
