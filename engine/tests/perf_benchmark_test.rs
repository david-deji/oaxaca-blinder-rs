use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::types::{DecompositionRequest, ProposedAdjustment, VerificationRequest};
use std::time::Instant;

fn generate_benchmark_csv(num_rows: usize) -> Vec<u8> {
    let mut csv = String::with_capacity(num_rows * 60);
    csv.push_str("wage,education,experience,tenure,gender\n");
    for i in 0..num_rows {
        let gender = if i % 2 == 0 { "M" } else { "F" };
        let wage = 30000.0 + (i as f64 % 500.0) * 100.0;
        let edu = 10.0 + (i % 10) as f64;
        let exp = (i % 30) as f64;
        let ten = (i % 15) as f64;
        csv.push_str(&format!("{wage},{edu},{exp},{ten},{gender}\n"));
    }
    csv.into_bytes()
}

fn create_request(csv_bytes: Vec<u8>) -> VerificationRequest {
    VerificationRequest {
        decomposition_params: DecompositionRequest {
            csv_data: csv_bytes,
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "M".to_string(),
            predictors: vec![
                "education".to_string(),
                "experience".to_string(),
                "tenure".to_string(),
            ],
            categorical_predictors: None,
            three_fold: Some(false),
            quantile: None,
            reference_coefficients: None,
            bootstrap_reps: Some(10),
        },
        adjustments: (0..5000)
            .map(|idx| ProposedAdjustment {
                index: idx * 2,
                row_key: None,
                value: 500.0,
                predictor_overrides: None,
            })
            .collect(),
    }
}

#[test]
fn benchmark_check_defensibility_large_dataset() {
    let num_rows = 10000;
    let csv_bytes = generate_benchmark_csv(num_rows);

    // Warmup
    let req = create_request(csv_bytes.clone());
    let _ = check_defensibility_inner(req).unwrap();

    let iterations = 10;
    let start = Instant::now();
    for _ in 0..iterations {
        let req = create_request(csv_bytes.clone());
        let res = check_defensibility_inner(req);
        assert!(res.is_ok());
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / (iterations as f64);
    println!("PERF_BENCHMARK: check_defensibility_inner (10,000 rows): avg {:.3} ms across {} runs", avg_ms, iterations);
}
