//! 0120-MERIDIAN V2: adding-up and relabel-invariance as ONE property, across all six schemes
//! (`GroupA`, `GroupB`, `Pooled`, `Weighted`, `Cotton`, `PooledNoIndicator`) and both
//! conventions, on random designs with 2-5 level factors, unequal and group-specific level
//! mixes, and optional integer weights.
//!
//! The property is the one the normalisation exists for: whatever the restriction, the detailed
//! rows (every level of every factor, the dropped one included, plus the intercept and the
//! numerics) sum to the aggregate explained / unexplained, E + C + I equals the gap, and
//! renaming the alphabetically first level (so a different level becomes the dropped one)
//! changes no row.

use oaxaca_blinder::{
    NormalizationConvention, OaxacaBuilder, OaxacaResults, ReferenceCoefficients,
};
use polars::prelude::*;
use proptest::prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[allow(deprecated)]
const SIX: [ReferenceCoefficients; 6] = [
    ReferenceCoefficients::GroupA,
    ReferenceCoefficients::GroupB,
    ReferenceCoefficients::Pooled,
    ReferenceCoefficients::Weighted,
    ReferenceCoefficients::Cotton,
    ReferenceCoefficients::PooledNoIndicator,
];

struct Design {
    df: DataFrame,
    weighted: bool,
}

/// Deterministic synthetic roster from `seed`. `rename_first` renames the level that sorts
/// first, so the same data has a different dropped level.
fn design(seed: u64, rename_first: bool) -> Design {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let n_a = rng.gen_range(60..140);
    let n_b = rng.gen_range(60..140);
    let k_dept = rng.gen_range(2..=5usize);
    let k_loc = rng.gen_range(2..=4usize);
    let weighted = rng.gen_bool(0.5);

    let mut wage = Vec::new();
    let mut x1 = Vec::new();
    let mut group = Vec::new();
    let mut dept = Vec::new();
    let mut loc = Vec::new();
    let mut w = Vec::new();

    // Level names sort in index order: "La" < "Lb" < ...; idx 0 is the alphabetically first
    // level (the dropped one), and `rename_first` renames it "zz" so it sorts last.
    let names = |k: usize, renamed: bool| -> Vec<String> {
        (0..k)
            .map(|i| {
                if i == 0 && renamed {
                    "zz".to_string()
                } else {
                    format!("L{}", (b'a' + i as u8) as char)
                }
            })
            .collect()
    };
    let dn = names(k_dept, rename_first);
    let ln = names(k_loc, false);

    let effects_a: Vec<f64> = (0..k_dept).map(|_| rng.gen_range(-0.3..0.3)).collect();
    let effects_b: Vec<f64> = (0..k_dept).map(|_| rng.gen_range(-0.3..0.3)).collect();
    let loc_a: Vec<f64> = (0..k_loc).map(|_| rng.gen_range(-0.2..0.2)).collect();
    let loc_b: Vec<f64> = (0..k_loc).map(|_| rng.gen_range(-0.2..0.2)).collect();
    let (slope_a, slope_b) = (rng.gen_range(0.1..0.6), rng.gen_range(0.1..0.6));

    for (g, n) in [("Male", n_a), ("Female", n_b)] {
        // group-specific skew: weights favour a random level
        let skew_d: Vec<f64> = (0..k_dept).map(|_| rng.gen_range(0.2..3.0)).collect();
        let skew_l: Vec<f64> = (0..k_loc).map(|_| rng.gen_range(0.2..3.0)).collect();
        for i in 0..n {
            // the first 3*k rows round-robin guarantee every level at least 3 rows in each group
            let d = if i < 3 * k_dept {
                i % k_dept
            } else {
                weighted_pick(&mut rng, &skew_d)
            };
            let l = if i < 3 * k_loc {
                i % k_loc
            } else {
                weighted_pick(&mut rng, &skew_l)
            };
            let x: f64 = rng.gen_range(-2.0..2.0);
            let (ed, el, s) = if g == "Male" {
                (effects_a[d], loc_a[l], slope_a)
            } else {
                (effects_b[d], loc_b[l], slope_b)
            };
            let noise: f64 = rng.gen_range(-0.25..0.25);
            wage.push(10.0 + s * x + ed + el + noise);
            x1.push(x);
            group.push(g.to_string());
            dept.push(dn[d].clone());
            loc.push(ln[l].clone());
            w.push(rng.gen_range(1..=3) as f64);
        }
    }
    let mut df = df!(
        "wage" => wage, "x1" => x1, "Gender" => group, "Dept" => dept, "Loc" => loc, "w" => w
    )
    .unwrap();
    if !weighted {
        df = df.drop("w").unwrap();
    }
    Design { df, weighted }
}

fn weighted_pick(rng: &mut ChaCha8Rng, weights: &[f64]) -> usize {
    let total: f64 = weights.iter().sum();
    let mut r = rng.gen_range(0.0..total);
    for (i, w) in weights.iter().enumerate() {
        if r < *w {
            return i;
        }
        r -= w;
    }
    weights.len() - 1
}

fn run(d: &Design, scheme: ReferenceCoefficients, conv: NormalizationConvention) -> OaxacaResults {
    let mut b = OaxacaBuilder::new(d.df.clone(), "wage", "Gender", "Female");
    b.predictors(vec!["x1"])
        .categorical_predictors(vec!["Dept", "Loc"])
        .reference_coefficients(scheme)
        .normalization_convention(conv)
        .normalize_all_categoricals()
        .bootstrap_reps(1);
    if d.weighted {
        b.weights("w")
            .weights_kind(oaxaca_blinder::WeightsKind::Frequency);
    }
    b.run().expect("random design is estimable")
}

fn named(v: &[oaxaca_blinder::ComponentResult]) -> Vec<(String, f64)> {
    v.iter().map(|c| (c.name.clone(), c.estimate)).collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn adding_up_and_relabel_invariance_across_all_six_schemes(seed in any::<u64>()) {
        let plain = design(seed, false);
        let renamed = design(seed, true);
        for conv in [NormalizationConvention::PopulationShare, NormalizationConvention::EqualShare] {
            for scheme in SIX {
                let a = run(&plain, scheme, conv);
                let e = a.explained().unwrap().estimate;
                let u = a.unexplained().unwrap().estimate;

                // adding-up: every level of every factor, the dropped one included
                let sum_e: f64 = a.two_fold.detailed_explained().iter().map(|c| c.estimate).sum();
                let sum_u: f64 = a.two_fold.detailed_unexplained().iter().map(|c| c.estimate).sum();
                prop_assert!((sum_e - e).abs() < 1e-9, "{scheme:?}/{conv:?}: detail explained {sum_e} vs {e}");
                prop_assert!((sum_u - u).abs() < 1e-9, "{scheme:?}/{conv:?}: detail unexplained {sum_u} vs {u}");
                prop_assert!((e + u - a.total_gap).abs() < 1e-9, "{scheme:?}/{conv:?}: E+U != gap");

                // three-fold from the raw vectors
                let tf: f64 = a.three_fold.aggregate().iter().map(|c| c.estimate).sum();
                prop_assert!((tf - a.total_gap).abs() < 1e-9, "{scheme:?}/{conv:?}: E+C+I {tf} vs gap {}", a.total_gap);

                // relabel: the alphabetically first Dept level is renamed to sort last, so a
                // different level is dropped; every row must be the same number under its name
                let b = run(&renamed, scheme, conv);
                let map = |n: &str| if n == "Dept_La" { "Dept_zz".to_string() } else { n.to_string() };
                for (which, va, vb) in [
                    ("unexplained", named(a.two_fold.detailed_unexplained()), named(b.two_fold.detailed_unexplained())),
                    ("explained", named(a.two_fold.detailed_explained()), named(b.two_fold.detailed_explained())),
                ] {
                    prop_assert_eq!(va.len(), vb.len());
                    for (n, x) in &va {
                        let y = vb.iter().find(|(m, _)| *m == map(n)).map(|(_, y)| *y);
                        prop_assert!(y.is_some(), "{which}[{n}] missing after relabel");
                        prop_assert!((x - y.unwrap()).abs() < 1e-8, "{scheme:?}/{conv:?} {which}[{n}]: {x} vs {} after relabel", y.unwrap());
                    }
                }
                prop_assert!((a.total_gap - b.total_gap).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn cotton_is_weighted(seed in any::<u64>()) {
        let d = design(seed, false);
        let w = run(&d, ReferenceCoefficients::Weighted, NormalizationConvention::PopulationShare);
        let c = run(&d, ReferenceCoefficients::Cotton, NormalizationConvention::PopulationShare);
        prop_assert_eq!(named(w.two_fold.detailed_unexplained()), named(c.two_fold.detailed_unexplained()));
    }
}
