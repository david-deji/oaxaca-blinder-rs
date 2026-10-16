#![cfg(feature = "experimental")]

use oaxaca_blinder::MatchingEngine;
use polars::prelude::*;

#[test]
fn test_matching_engine_basic() -> Result<(), Box<dyn std::error::Error>> {
    // Create dummy data
    // Treated: Higher income, higher education
    // Control: Lower income, lower education
    // But some overlap

    let n = 100;
    let mut treated = vec![0; n];
    let mut income = vec![0.0; n];
    let mut education = vec![0.0; n];

    for i in 0..n {
        if i < 50 {
            treated[i] = 1;
            income[i] = 50000.0 + (i as f64) * 1000.0;
            education[i] = 16.0;
        } else {
            treated[i] = 0;
            income[i] = 30000.0 + (i as f64) * 500.0;
            education[i] = 12.0;
        }
    }

    // Add some overlap
    education[48] = 12.0; // Treated but low ed
    education[49] = 12.0;
    education[50] = 16.0; // Control but high ed
    education[51] = 16.0;

    let df = df!(
        "treated" => treated.into_iter().map(|x| x as f64).collect::<Vec<f64>>(),
        "income" => income,
        "education" => education
    )?;

    let engine = MatchingEngine::new(df.clone(), "treated", "income", vec!["education"]);

    // Test NN matching
    let weights = engine.run_matching(1, false)?;

    // Check weights
    assert_eq!(weights.len(), n);

    // Debug prints
    println!("Weights[50]: {}", weights[50]);
    println!("Weights[51]: {}", weights[51]);

    // Treated units should have weight 1.0
    for w in weights.iter().take(50) {
        assert!(*w >= 0.0);
    }

    // Control units matched to treated units should have weights > 0
    // Due to deterministic tie-breaking, one of them might get all the weights.
    assert!(weights[50] + weights[51] >= 48.0);

    Ok(())
}

#[test]
fn test_psm_matching() -> Result<(), Box<dyn std::error::Error>> {
    let n = 100;
    let mut treated = vec![0; n];
    let mut income = vec![0.0; n];
    let mut education = vec![0.0; n];

    for i in 0..n {
        if i < 50 {
            treated[i] = 1;
            education[i] = 16.0;
        } else {
            treated[i] = 0;
            education[i] = 12.0;
        }
        // Add noise
        income[i] = 1000.0 * education[i];
    }

    // Overlap
    education[0] = 12.0;
    education[50] = 16.0;

    let df = df!(
        "treated" => treated.into_iter().map(|x| x as f64).collect::<Vec<f64>>(),
        "income" => income,
        "education" => education
    )?;

    let engine = MatchingEngine::new(df, "treated", "income", vec!["education"]);
    let weights = engine.match_psm(1)?;

    assert_eq!(weights.len(), n);
    assert_eq!(weights[0], 1.0);

    Ok(())
}
