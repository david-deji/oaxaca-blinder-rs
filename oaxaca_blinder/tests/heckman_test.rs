use oaxaca_blinder::OaxacaBuilder;
use polars::prelude::*;
use rand::distributions::Distribution;
use rand::prelude::*;
use statrs::distribution::Normal;

#[test]
fn test_heckman_correction() -> Result<(), Box<dyn std::error::Error>> {
    // Generate data with selection bias
    // Z ~ N(0, 1)
    // u ~ N(0, 1), e ~ N(0, 1), corr(u, e) = 0.8
    // Selection: S = 1 if 0.5 * Z + u > 0
    // Outcome: Y = 1.0 + 2.0 * X + e (observed if S=1)
    // X is correlated with Z? Let's say X = Z + noise.

    let n = 2000;
    let mut rng = StdRng::seed_from_u64(42);
    let normal = Normal::new(0.0, 1.0).unwrap();

    let mut z_vals = Vec::new();
    let mut x_vals = Vec::new();
    let mut s_vals = Vec::new();
    let mut y_vals = Vec::new();
    let mut group_vals = Vec::new();

    for _ in 0..n {
        let z: f64 = normal.sample(&mut rng);
        let x: f64 = z + 0.5 * normal.sample(&mut rng);

        let u: f64 = normal.sample(&mut rng);
        let e_uncorr: f64 = normal.sample(&mut rng);
        let rho = 0.8;
        let e = rho * u + (1.0 - rho * rho).sqrt() * e_uncorr;

        let s_latent = 0.5 * z + u;
        let s = if s_latent > 0.0 { 1.0 } else { 0.0 };

        let y = 1.0 + 2.0 * x + e;

        // Randomly assign group
        let group = if rng.gen_bool(0.5) { "A" } else { "B" };

        z_vals.push(z);
        x_vals.push(x);
        s_vals.push(s);
        y_vals.push(if s == 1.0 { Some(y) } else { None });
        group_vals.push(group);
    }

    let df = df!(
        "outcome" => y_vals,
        "x" => x_vals,
        "z" => z_vals,
        "selection" => s_vals,
        "group" => group_vals
    )?;

    // Run Oaxaca with Heckman
    let res = OaxacaBuilder::new(df, "outcome", "group", "B")
        .predictors(vec!["x"])
        .heckman_selection("selection", vec!["z"]) // Z is exclusion restriction
        .bootstrap_reps(0)
        .run()?;

    // Check that we have IMR in the results
    let explained = res.two_fold().detailed_explained();
    let has_imr = explained.iter().any(|c| c.name() == "IMR");
    assert!(has_imr, "IMR should be in detailed decomposition");

    println!("Heckman Decomposition Summary:");
    res.summary();

    Ok(())
}

/// 0120-MERIDIAN S1: the Heckman estimator ignores normalisation, so with a selection model the
/// categorical coefficients must stay raw on A, B and the pooled fit TOGETHER (a half-normalised
/// run would not add up), and the run must say it did not normalise.
#[test]
fn test_heckman_with_categoricals_skips_normalisation_and_says_so(
) -> Result<(), Box<dyn std::error::Error>> {
    let n = 2000;
    let mut rng = StdRng::seed_from_u64(7);
    let normal = Normal::new(0.0, 1.0).unwrap();
    let (mut z_vals, mut x_vals, mut s_vals, mut y_vals) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut group_vals, mut dept_vals) = (Vec::new(), Vec::new());
    for _ in 0..n {
        let z: f64 = normal.sample(&mut rng);
        let x: f64 = z + 0.5 * normal.sample(&mut rng);
        let u: f64 = normal.sample(&mut rng);
        let e = 0.8 * u + 0.6 * normal.sample(&mut rng);
        let s = if 0.5 * z + u > 0.0 { 1.0 } else { 0.0 };
        let dept = ["d1", "d2", "d3"][rng.gen_range(0..3)];
        let y = 1.0 + 2.0 * x + if dept == "d2" { 0.5 } else { 0.0 } + e;
        z_vals.push(z);
        x_vals.push(x);
        s_vals.push(s);
        y_vals.push(if s == 1.0 { Some(y) } else { None });
        group_vals.push(if rng.gen_bool(0.5) { "A" } else { "B" });
        dept_vals.push(dept);
    }
    let df = df!(
        "outcome" => y_vals, "x" => x_vals, "z" => z_vals, "selection" => s_vals,
        "group" => group_vals, "dept" => dept_vals
    )?;

    let run = |normalize: bool| {
        let mut b = OaxacaBuilder::new(df.clone(), "outcome", "group", "B");
        b.predictors(vec!["x"])
            .categorical_predictors(vec!["dept"])
            .heckman_selection("selection", vec!["z"])
            .bootstrap_reps(0);
        if normalize {
            b.normalize_all_categoricals();
        }
        b.run().expect("heckman run")
    };
    let raw = run(false);
    let asked = run(true);
    let rec = asked
        .run_metadata
        .normalization
        .as_ref()
        .expect("a request to normalise is recorded");
    assert!(!rec.applied);
    assert_eq!(rec.skipped_reason, Some("heckman_selection"));
    assert!(rec.variables.is_empty());
    let vec_of = |r: &oaxaca_blinder::OaxacaResults| -> Vec<(String, f64)> {
        r.two_fold()
            .detailed_unexplained()
            .iter()
            .map(|c| (c.name().clone(), *c.estimate()))
            .collect()
    };
    assert_eq!(
        vec_of(&raw),
        vec_of(&asked),
        "a skipped normalisation must leave every row raw"
    );
    assert!(raw.run_metadata.normalization.is_none());
    Ok(())
}
