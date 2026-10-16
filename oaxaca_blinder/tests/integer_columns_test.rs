//! A whole-number outcome, selection outcome or weights column (what a CSV of salaries and headcounts parses to) is
//! converted to `Float64` by the library, so the library and the command line accept the same data. The numbers
//! equal those of the same values written as decimals.

use oaxaca_blinder::{OaxacaBuilder, OaxacaError, WeightsKind};
use polars::prelude::*;
use rand::distributions::Distribution;
use rand::prelude::*;
use statrs::distribution::Normal;

fn frame() -> DataFrame {
    df![
        "wage"  => [10.0f64, 12.0, 14.0, 16.0, 11.0, 13.0, 15.0, 30.0,
                    20.0, 22.0, 24.0, 26.0, 21.0, 23.0, 25.0, 40.0],
        "educ"  => [1.0f64, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0,
                    1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0],
        "group" => ["A", "A", "A", "A", "A", "A", "A", "A",
                    "B", "B", "B", "B", "B", "B", "B", "B"],
        "hc"    => [1.0f64, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0, 2.0,
                    3.0, 1.0, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0],
    ]
    .unwrap()
}

/// `frame()` with the named columns cast to Int64.
fn as_integers(columns: &[&str]) -> DataFrame {
    let mut df = frame();
    for name in columns {
        let cast = df.column(name).unwrap().cast(&DataType::Int64).unwrap();
        df.with_column(cast).unwrap();
    }
    for name in columns {
        assert!(matches!(df.column(name).unwrap().dtype(), DataType::Int64));
    }
    df
}

fn builder(df: DataFrame) -> OaxacaBuilder {
    let mut b = OaxacaBuilder::new(df, "wage", "group", "B");
    b.predictors(vec!["educ"]).bootstrap_reps(0);
    b
}

fn gaps(df: DataFrame) -> Result<(f64, f64, f64), OaxacaError> {
    let r = builder(df).run()?;
    let two = r.two_fold();
    let agg = two.aggregate();
    let get = |n: &str| {
        agg.iter()
            .find(|c| c.name() == n)
            .map(|c| *c.estimate())
            .unwrap()
    };
    Ok((*r.total_gap(), get("explained"), get("unexplained")))
}

#[test]
fn a_whole_number_outcome_gives_the_numbers_of_the_decimal_one() {
    let decimal = gaps(frame()).unwrap();
    let whole = gaps(as_integers(&["wage"])).expect("an integer outcome is converted, not refused");
    assert_eq!(decimal, whole);
    assert!((whole.0 + 10.0).abs() < 1e-9, "total gap {whole:?}");
}

#[test]
fn a_whole_number_outcome_works_in_the_percentile_decomposition() {
    let decimal = builder(frame()).decompose_quantile(0.5).unwrap().total_gap;
    let whole = builder(as_integers(&["wage"]))
        .decompose_quantile(0.5)
        .expect("an integer outcome is converted in the RIF path")
        .total_gap;
    assert_eq!(decimal, whole);
}

#[test]
fn a_whole_number_outcome_works_with_weights_and_with_a_bootstrap() {
    let run = |df: DataFrame| {
        let mut b = OaxacaBuilder::new(df, "wage", "group", "B");
        b.predictors(vec!["educ"])
            .weights("hc")
            .weights_kind(WeightsKind::Frequency)
            .bootstrap_reps(50);
        let r = b.run().unwrap();
        let two = r.two_fold();
        let agg = two.aggregate();
        agg.iter()
            .map(|c| (*c.estimate(), *c.std_err()))
            .collect::<Vec<_>>()
    };
    let decimal = run(frame());
    assert_eq!(decimal, run(as_integers(&["wage", "hc"])));
}

#[test]
fn a_whole_number_selection_outcome_is_converted() {
    let n = 600;
    let mut rng = StdRng::seed_from_u64(7);
    let normal = Normal::new(0.0, 1.0).unwrap();
    let (mut y, mut x, mut z, mut s, mut g) = (vec![], vec![], vec![], vec![], vec![]);
    for _ in 0..n {
        let zi: f64 = normal.sample(&mut rng);
        let xi = zi + 0.5 * normal.sample(&mut rng);
        let u: f64 = normal.sample(&mut rng);
        let e = 0.8 * u + 0.6 * normal.sample(&mut rng);
        let selected = 0.5 * zi + u > 0.0;
        y.push(if selected {
            Some(1.0 + 2.0 * xi + e)
        } else {
            None
        });
        x.push(xi);
        z.push(zi);
        s.push(i64::from(selected));
        g.push(if rng.gen_bool(0.5) { "A" } else { "B" });
    }
    let run = |selection: Column| {
        let df =
            df!("outcome" => y.clone(), "x" => x.clone(), "z" => z.clone(), "group" => g.clone())
                .unwrap()
                .hstack(&[selection])
                .unwrap();
        OaxacaBuilder::new(df, "outcome", "group", "B")
            .predictors(vec!["x"])
            .heckman_selection("selection", vec!["z"])
            .bootstrap_reps(0)
            .run()
    };
    let whole = run(Column::new("selection".into(), s.clone())).expect("an integer 0/1 column");
    let decimal = run(Column::new(
        "selection".into(),
        s.iter().map(|&v| v as f64).collect::<Vec<f64>>(),
    ))
    .unwrap();
    assert_eq!(whole.total_gap(), decimal.total_gap());
    let imr = |r: &oaxaca_blinder::OaxacaResults| {
        r.two_fold()
            .detailed_explained()
            .iter()
            .find(|c| c.name() == "IMR")
            .map(|c| *c.estimate())
            .expect("IMR row")
    };
    assert_eq!(imr(&whole), imr(&decimal));
}
