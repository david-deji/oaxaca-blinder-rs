use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::types::{DecompositionRequest, ProposedAdjustment, VerificationRequest};
use std::time::Instant;

fn generate_large_csv(rows: usize) -> Vec<u8> {
    let mut csv = String::with_capacity(rows * 60);
    csv.push_str("wage,education,experience,gender,department\n");
    for i in 0..rows {
        let gender = if i % 2 == 0 { "Male" } else { "Female" };
        let dept = if i % 3 == 0 {
            "Sales"
        } else if i % 3 == 1 {
            "Engineering"
        } else {
            "HR"
        };
        let wage = 40000 + (i % 1000) * 50;
        let edu = 10 + (i % 10);
        let exp = 1 + (i % 30);
        csv.push_str(&format!("{},{},{},{},{}\n", wage, edu, exp, gender, dept));
    }
    csv.into_bytes()
}

fn make_request(csv_data: Vec<u8>) -> VerificationRequest {
    let adjustments: Vec<ProposedAdjustment> = (0..5_000)
        .map(|i| ProposedAdjustment {
            index: i * 2,
            row_key: None,
            value: 500.0,
            predictor_overrides: None,
        })
        .collect();

    VerificationRequest {
        decomposition_params: DecompositionRequest {
            csv_data,
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: Some(vec!["department".to_string()]),
            three_fold: Some(false),
            quantile: None,
            reference_coefficients: None,
            bootstrap_reps: Some(10),
        },
        adjustments,
    }
}

#[test]
fn bench_defensibility_large_dataset() {
    let num_rows = 50_000;
    let csv_data = generate_large_csv(num_rows);

    // Warm-up run
    let _ = check_defensibility_inner(make_request(csv_data.clone())).unwrap();

    // Measured runs
    let mut durations = Vec::new();
    for _ in 0..5 {
        let req = make_request(csv_data.clone());
        let start = Instant::now();
        let res = check_defensibility_inner(req);
        let duration = start.elapsed();
        assert!(res.is_ok());
        durations.push(duration);
    }

    let avg_millis = durations.iter().map(|d| d.as_secs_f64() * 1000.0).sum::<f64>() / durations.len() as f64;
    println!("PERF_METRIC: check_defensibility_inner (50k rows, 5k adjustments) average execution time: {:.2} ms", avg_millis);
}
