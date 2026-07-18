// Ground-truth verification against hand-computable answers.
// 1. Three-fold: deterministic linear wages per group -> OLS recovers betas exactly,
//    so endowments/coefficients/interaction must match hand-computed values.
// 2. Quantile decomposition: pure location shift (M = F + 5, identical X and noise)
//    -> every quantile gap ~= 5, characteristics ~= 0, coefficients ~= 5.
use oaxaca_blinder::{OaxacaBuilder, QuantileDecompositionBuilder};
use polars::prelude::*;

fn lcg_uniform(seed: &mut u64) -> f64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
}

#[test]
fn test_three_fold_ground_truth() {
    // Group B (reference) = F: wage = 4 + 1.0*edu, mean edu 13
    // Group A            = M: wage = 5 + 1.5*edu, mean edu 15
    // Hand-computed: gap = 27.5 - 17 = 10.5
    //   endowments   = (15-13)*1.0             = 2.0
    //   coefficients = 1*(5-4) + 13*(1.5-1.0)  = 7.5
    //   interaction  = (15-13)*(1.5-1.0)       = 1.0
    let edu_f: Vec<f64> = [10.0, 12.0, 14.0, 16.0].repeat(3);
    let edu_m: Vec<f64> = [12.0, 14.0, 16.0, 18.0].repeat(3);
    let wage_f: Vec<f64> = edu_f.iter().map(|e| 4.0 + 1.0 * e).collect();
    let wage_m: Vec<f64> = edu_m.iter().map(|e| 5.0 + 1.5 * e).collect();

    let mut wage = wage_m.clone();
    wage.extend(&wage_f);
    let mut edu = edu_m.clone();
    edu.extend(&edu_f);
    let mut gender: Vec<&str> = vec!["M"; 12];
    gender.extend(vec!["F"; 12]);

    let df = df!("wage" => wage, "education" => edu, "gender" => gender).unwrap();

    let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
    builder.predictors(vec!["education"]).bootstrap_reps(5);
    let results = builder.run().expect("three-fold run failed");

    assert!(
        (results.total_gap() - 10.5).abs() < 1e-9,
        "total gap {} != 10.5",
        results.total_gap()
    );

    let get = |name: &str| -> f64 {
        *results
            .three_fold()
            .aggregate()
            .iter()
            .find(|c| c.name() == name)
            .unwrap_or_else(|| panic!("component {} missing", name))
            .estimate()
    };
    let endow = get("endowments");
    let coef = get("coefficients");
    let inter = get("interaction");
    println!(
        "endowments={} coefficients={} interaction={} gap={}",
        endow,
        coef,
        inter,
        results.total_gap()
    );
    assert!((endow - 2.0).abs() < 1e-6, "endowments {} != 2.0", endow);
    assert!((coef - 7.5).abs() < 1e-6, "coefficients {} != 7.5", coef);
    assert!((inter - 1.0).abs() < 1e-6, "interaction {} != 1.0", inter);
    assert!(
        (endow + coef + inter - results.total_gap()).abs() < 1e-9,
        "three-fold components do not sum to total gap"
    );
}

#[test]
fn test_quantile_decomposition_location_shift_ground_truth() {
    // Identical education and noise draws for both groups; wage_M = wage_F + 5.
    // True answer at every quantile: gap = 5, characteristics = 0, coefficients = 5.
    let n = 100usize;
    let mut seed: u64 = 42;
    let mut edu = Vec::with_capacity(2 * n);
    let mut wage = Vec::with_capacity(2 * n);
    let mut gender: Vec<&str> = Vec::with_capacity(2 * n);

    let mut base = Vec::with_capacity(n);
    for i in 0..n {
        let e = 10.0 + (i % 11) as f64;
        let noise = (lcg_uniform(&mut seed) - 0.5) * 4.0; // uniform [-2, 2]
        base.push((e, 5.0 + 1.2 * e + noise));
    }
    for &(e, w) in &base {
        edu.push(e);
        wage.push(w + 5.0);
        gender.push("M");
    }
    for &(e, w) in &base {
        edu.push(e);
        wage.push(w);
        gender.push("F");
    }

    let df = df!("wage" => wage, "education" => edu, "gender" => gender).unwrap();

    let mut builder = QuantileDecompositionBuilder::new(df, "wage", "gender", "F");
    let results = builder
        .predictors(vec!["education"])
        .quantiles(&[0.1, 0.5, 0.9])
        // The location-shift property (characteristics -> 0) is asymptotic in the MM
        // simulation count. With the deterministic seeded RNG (0014-MERIDIAN), 500 sims
        // leaves a Monte-Carlo error at the median (~0.8) that exceeds the 0.75 band at the
        // fixed seed; 2500 sims reduces it below the band. (coeffs are recovered exactly and
        // adding-up holds — the decomposition math is correct; only the MC precision changed.)
        .simulations(2500)
        .bootstrap_reps(2)
        .run()
        .expect("quantile decomposition run failed");

    for key in &["q10", "q50", "q90"] {
        let detail = results
            .results_by_quantile()
            .get(*key)
            .unwrap_or_else(|| panic!("missing {}", key));
        let gap = detail.total_gap().estimate();
        let chars = detail.characteristics_effect().estimate();
        let coeffs = detail.coefficients_effect().estimate();
        println!("{}: gap={} chars={} coeffs={}", key, gap, chars, coeffs);
        assert!(
            (chars + coeffs - gap).abs() < 1e-9,
            "{}: components do not sum to gap",
            key
        );
        assert!((gap - 5.0).abs() < 1.0, "{}: gap {} not ~5", key, gap);
        assert!(chars.abs() < 0.75, "{}: characteristics {} not ~0", key, chars);
        assert!((coeffs - 5.0).abs() < 1.0, "{}: coefficients {} not ~5", key, coeffs);
    }
}
