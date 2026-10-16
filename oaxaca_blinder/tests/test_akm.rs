#![cfg(feature = "experimental")]

use oaxaca_blinder::AkmBuilder;
use polars::prelude::*;
use rand::Rng;

#[test]
fn test_akm_synthetic() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Generate Synthetic Data
    // 100 workers, 20 firms, 1000 observations
    let n_workers = 100;
    let n_firms = 20;
    let n_obs = 1000;

    let mut rng = rand::thread_rng();

    // True effects
    let alpha: Vec<f64> = (0..n_workers).map(|_| rng.gen_range(-1.0..1.0)).collect();
    let psi: Vec<f64> = (0..n_firms).map(|_| rng.gen_range(-0.5..0.5)).collect();
    let beta = 2.5;

    let mut worker_ids = Vec::new();
    let mut firm_ids = Vec::new();
    let mut x_vals = Vec::new();
    let mut y_vals = Vec::new();

    // Ensure connectivity: Create a "spanning tree" first
    // Connect worker 0 to firm 0
    // Connect worker 1 to firm 0
    // Connect worker 1 to firm 1
    // ...
    // Simple way: Randomly assign, but ensure large component.
    // With 1000 obs and 20 firms, it should be connected.

    for _ in 0..n_obs {
        let w_idx = rng.gen_range(0..n_workers);
        let f_idx = rng.gen_range(0..n_firms);

        worker_ids.push(format!("w{}", w_idx));
        firm_ids.push(format!("f{}", f_idx));

        let x = rng.gen_range(0.0..10.0);
        let epsilon = rng.gen_range(-0.01..0.01); // Small noise

        let y = x * beta + alpha[w_idx] + psi[f_idx] + epsilon;

        x_vals.push(x);
        y_vals.push(y);
    }

    let df = df!(
        "worker" => worker_ids,
        "firm" => firm_ids,
        "x" => x_vals,
        "y" => y_vals
    )?;

    // 2. Run AKM
    let result = AkmBuilder::new(df, "y", "worker", "firm")
        .controls(vec!["x"])
        .tolerance(1e-8)
        .max_iters(100)
        .run()?;

    // 3. Verify Beta
    println!("True Beta: {}, Estimated Beta: {}", beta, result.beta[0]);
    assert!((result.beta[0] - beta).abs() < 0.05);

    // 4. Verify R2
    println!("R2: {}", result.r2);
    assert!(result.r2 > 0.99); // Should be very high with low noise

    Ok(())
}

#[test]
fn test_lcs_filtering() -> Result<(), Box<dyn std::error::Error>> {
    // Create a disconnected graph
    // Component 1: w1-f1, w2-f1
    // Component 2: w3-f2

    let df = df!(
        "worker" => &["w1", "w2", "w3"],
        "firm" => &["f1", "f1", "f2"],
        "y" => &[10.0, 11.0, 12.0]
    )?;

    // The builder calls LCS internally.
    // If we run AKM, it should filter out w3-f2 (size 1 edge vs size 2 edges? No, nodes.)
    // Comp 1: {w1, w2, f1} -> size 3
    // Comp 2: {w3, f2} -> size 2
    // So w3 should be dropped.

    let result = AkmBuilder::new(df, "y", "worker", "firm").run()?;

    // Check that we got a result (it didn't fail)
    // And check effects.
    // w3 should not be in worker_effects

    let w_effs = result
        .worker_effects
        .column("worker")?
        .cast(&DataType::String)?;
    let w_vec: Vec<&str> = w_effs.str()?.into_iter().flatten().collect();

    assert!(w_vec.contains(&"w1"));
    assert!(w_vec.contains(&"w2"));
    assert!(!w_vec.contains(&"w3"));

    Ok(())
}
