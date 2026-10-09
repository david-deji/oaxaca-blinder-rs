//! 0120-MERIDIAN S9 / T17 / V9: what a weights column MEANS.
//!
//! No single convention makes both "uniform fractional weights are a no-op" and "a weight of 2 is
//! the row twice" true, so a weights column always carries a `WeightsKind`:
//!
//!  * `Frequency`: whole-number counts. Equals repeating the row; a fractional value is refused,
//!    naming the row.
//!  * `Relative`: rescaled to sum to the row count; the quantile is
//!    `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)`. Uniform weights change nothing.
//!
//! The quantile values are checked against R (`verification/gen_diag_goldens.R`), never against
//! engine output.

#[path = "support/diag_golden.rs"]
mod diag_golden;

use diag_golden::*;
use oaxaca_blinder::{weighted_quantile, OaxacaBuilder, OaxacaError, WeightsKind};
use polars::prelude::*;
use std::path::Path;

fn golden() -> DiagGolden {
    DiagGolden::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
}

fn vec_of(v: &serde_json::Value) -> Vec<f64> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|x| x.as_f64().expect("number"))
        .collect()
}

const TAUS: [(&str, f64); 5] = [
    ("0.10", 0.10),
    ("0.25", 0.25),
    ("0.50", 0.50),
    ("0.75", 0.75),
    ("0.90", 0.90),
];

/// R `quantile(type = 7)` written out here, only to state the no-op property.
fn type7(y: &[f64], p: f64) -> f64 {
    let mut s = y.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let h = (s.len() as f64 - 1.0) * p;
    let (lo, hi) = (h.floor() as usize, h.ceil() as usize);
    s[lo] + (h - h.floor()) * (s[hi] - s[lo])
}

#[test]
fn v9_relative_quantile_equals_hmisc_wtd_quantile_normwt() {
    let g = golden();
    let mut worst = 0.0_f64;
    for case in g.block(&["weights", "relative"]).as_array().unwrap() {
        let (y, w) = (vec_of(&case["y"]), vec_of(&case["w"]));
        let name = case["name"].as_str().unwrap();
        for (key, tau) in TAUS {
            let want = f(&case["relative"], key);
            let got = weighted_quantile(&y, &w, tau, WeightsKind::Relative).unwrap();
            assert_close(&format!("{name} tau={tau}"), got, want, 1e-9);
            worst = worst.max(rel(got, want));
        }
    }
    println!("relative vs Hmisc: worst relative difference {worst:e}");
}

#[test]
fn v9_the_relative_cases_have_teeth() {
    // Weights must matter in the mixed-FTE case, or a unit-weight implementation would pass.
    let g = golden();
    let mixed = &g.block(&["weights", "relative"]).as_array().unwrap()[0];
    let (y, w) = (vec_of(&mixed["y"]), vec_of(&mixed["w"]));
    assert_eq!(mixed["name"].as_str(), Some("mixed FTE"));
    let moved = TAUS
        .iter()
        .map(|(key, tau)| (f(&mixed["relative"], key) - type7(&y, *tau)).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        moved > 0.5,
        "the mixed-FTE weights must move some percentile by more than 0.5, moved {moved}"
    );
    // ...and the same weights treated as FREQUENCY counts are refused (they are fractional).
    assert!(weighted_quantile(&y, &w, 0.5, WeightsKind::Frequency).is_err());
}

#[test]
fn v9_frequency_quantile_equals_the_repeated_rows_in_r() {
    let g = golden();
    for case in g.block(&["weights", "frequency"]).as_array().unwrap() {
        let (y, w) = (vec_of(&case["y"]), vec_of(&case["w"]));
        let name = case["name"].as_str().unwrap();
        for (key, tau) in TAUS {
            let want = f(&case["frequency"], key);
            let got = weighted_quantile(&y, &w, tau, WeightsKind::Frequency).unwrap();
            assert_close(&format!("{name} tau={tau}"), got, want, 1e-9);
        }
    }
}

#[test]
fn v9_uniform_fractional_weights_are_a_no_op_under_relative() {
    let y = [20.0, 24.0, 28.0, 32.0];
    for w in [0.5, 0.4, 0.8, 2.0, 1.0] {
        for (_, tau) in TAUS {
            let got = weighted_quantile(&y, &[w; 4], tau, WeightsKind::Relative).unwrap();
            assert!(
                (got - type7(&y, tau)).abs() < 1e-12,
                "uniform weight {w} at tau={tau}: {got} != {}",
                type7(&y, tau)
            );
        }
    }
    // The R oracle says the same: its uniform-0.5 case is the unweighted type-7.
    let g = golden();
    let uniform = g
        .block(&["weights", "relative"])
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "uniform 0.5")
        .unwrap();
    for (key, tau) in TAUS {
        assert_close(
            "R uniform",
            f(&uniform["relative"], key),
            type7(&y, tau),
            1e-12,
        );
    }
}

#[test]
fn zero_weight_rows_carry_no_mass() {
    let y = [1.0, 2.0, 3.0, 4.0, 100.0];
    let w = [1.0, 1.0, 1.0, 1.0, 0.0];
    for kind in [WeightsKind::Frequency, WeightsKind::Relative] {
        let with_zero = weighted_quantile(&y, &w, 0.5, kind).unwrap();
        let without = weighted_quantile(&y[..4], &w[..4], 0.5, kind).unwrap();
        assert!((with_zero - without).abs() < 1e-12, "{kind:?}");
    }
}

// ---- builder level ---------------------------------------------------------------------------

fn frame(weights: Vec<f64>) -> DataFrame {
    df![
        "wage"  => [10.0f64, 12.0, 14.0, 16.0, 11.0, 13.0, 15.0, 30.0,
                    20.0, 22.0, 24.0, 26.0, 21.0, 23.0, 25.0, 40.0],
        "educ"  => [1.0f64, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0,
                    1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0],
        "group" => ["A", "A", "A", "A", "A", "A", "A", "A",
                    "B", "B", "B", "B", "B", "B", "B", "B"],
        "hc"    => weights,
    ]
    .unwrap()
}

fn mean_gap(df: DataFrame, weights: Option<WeightsKind>) -> Result<(f64, f64, f64), OaxacaError> {
    let mut b = OaxacaBuilder::new(df, "wage", "group", "B");
    b.predictors(vec!["educ"]).bootstrap_reps(0);
    if let Some(kind) = weights {
        b.weights("hc").weights_kind(kind);
    }
    let r = b.run()?;
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

fn quantile_gap(df: DataFrame, weights: Option<WeightsKind>) -> Result<f64, OaxacaError> {
    let mut b = OaxacaBuilder::new(df, "wage", "group", "B");
    b.predictors(vec!["educ"]).bootstrap_reps(0);
    if let Some(kind) = weights {
        b.weights("hc").weights_kind(kind);
    }
    Ok(b.decompose_quantile(0.5)?.total_gap)
}

#[test]
fn uniform_fractional_weights_change_nothing_under_relative_in_either_decomposition() {
    let plain = mean_gap(frame(vec![1.0; 16]), None).unwrap();
    let rel_half = mean_gap(frame(vec![0.5; 16]), Some(WeightsKind::Relative)).unwrap();
    for (a, b) in [
        (plain.0, rel_half.0),
        (plain.1, rel_half.1),
        (plain.2, rel_half.2),
    ] {
        assert!(
            (a - b).abs() < 1e-10,
            "mean path moved: {plain:?} vs {rel_half:?}"
        );
    }
    let q_plain = quantile_gap(frame(vec![1.0; 16]), None).unwrap();
    let q_half = quantile_gap(frame(vec![0.5; 16]), Some(WeightsKind::Relative)).unwrap();
    assert!(
        (q_plain - q_half).abs() < 1e-10,
        "quantile path moved: {q_plain} vs {q_half}"
    );
    // Under frequency the same fractional column is refused.
    assert!(matches!(
        mean_gap(frame(vec![0.5; 16]), Some(WeightsKind::Frequency)),
        Err(OaxacaError::InvalidWeight { .. })
    ));
}

#[test]
fn relative_weights_are_scale_free() {
    let w: Vec<f64> = (0..16).map(|i| 0.4 + (i % 5) as f64 * 0.3).collect();
    let scaled: Vec<f64> = w.iter().map(|x| x * 7.5).collect();
    let a = quantile_gap(frame(w.clone()), Some(WeightsKind::Relative)).unwrap();
    let b = quantile_gap(frame(scaled.clone()), Some(WeightsKind::Relative)).unwrap();
    assert!(
        (a - b).abs() < 1e-10,
        "relative weights must be scale-free: {a} vs {b}"
    );
    let ma = mean_gap(frame(w), Some(WeightsKind::Relative)).unwrap();
    let mb = mean_gap(frame(scaled), Some(WeightsKind::Relative)).unwrap();
    assert!((ma.0 - mb.0).abs() < 1e-10 && (ma.2 - mb.2).abs() < 1e-10);
}

#[test]
fn an_integer_frequency_weight_is_the_row_repeated() {
    // Weights 1,2,3 on the first three rows of each group; the expansion repeats those rows.
    let weights = vec![
        2.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0,
    ];
    let base = frame(weights.clone());
    let mut idx: Vec<u32> = Vec::new();
    for (i, w) in weights.iter().enumerate() {
        for _ in 0..(*w as usize) {
            idx.push(i as u32);
        }
    }
    let expanded = base
        .take(&IdxCa::from_vec("i".into(), idx))
        .unwrap()
        .drop("hc")
        .unwrap();
    let weighted = mean_gap(base.clone(), Some(WeightsKind::Frequency)).unwrap();
    let repeated = mean_gap(
        expanded
            .hstack(&[Column::new("hc".into(), vec![1.0f64; expanded.height()])])
            .unwrap(),
        None,
    )
    .unwrap();
    for (a, b) in [
        (weighted.0, repeated.0),
        (weighted.1, repeated.1),
        (weighted.2, repeated.2),
    ] {
        assert!(
            (a - b).abs() < 1e-9,
            "frequency weights != repeated rows: {weighted:?} vs {repeated:?}"
        );
    }
    // The quantile decomposition keeps the same identity (type 7 on the expansion).
    let q_w = quantile_gap(base, Some(WeightsKind::Frequency)).unwrap();
    let q_r = quantile_gap(
        expanded
            .hstack(&[Column::new("hc".into(), vec![1.0f64; expanded.height()])])
            .unwrap(),
        None,
    )
    .unwrap();
    assert!((q_w - q_r).abs() < 1e-8, "{q_w} vs {q_r}");
}

#[test]
fn a_fractional_frequency_weight_is_refused_naming_its_original_row() {
    // Row 2 has a blank predictor (it is excluded), row 5 carries the fractional weight: the
    // error must name ordinal 5, not the cleaned-frame position 4.
    let mut educ = vec![
        Some(1.0f64),
        Some(2.0),
        None,
        Some(4.0),
        Some(1.0),
        Some(2.0),
        Some(3.0),
        Some(4.0),
    ];
    educ.extend([
        Some(1.0),
        Some(2.0),
        Some(3.0),
        Some(4.0),
        Some(1.0),
        Some(2.0),
        Some(3.0),
        Some(4.0),
    ]);
    let mut w = vec![1.0f64; 16];
    w[5] = 1.5;
    let df = df![
        "wage"  => [10.0f64, 12.0, 14.0, 16.0, 11.0, 13.0, 15.0, 30.0,
                    20.0, 22.0, 24.0, 26.0, 21.0, 23.0, 25.0, 40.0],
        "educ"  => educ,
        "group" => ["A","A","A","A","A","A","A","A","B","B","B","B","B","B","B","B"],
        "hc"    => w,
    ]
    .unwrap();
    let mut b = OaxacaBuilder::new(df, "wage", "group", "B");
    b.predictors(vec!["educ"])
        .weights("hc")
        .weights_kind(WeightsKind::Frequency)
        .bootstrap_reps(0);
    match b.run() {
        Err(OaxacaError::InvalidWeight {
            column,
            row,
            value,
            reason,
        }) => {
            assert_eq!((column.as_str(), row, value), ("hc", 5, 1.5));
            assert!(reason.contains("relative"), "{reason}");
        }
        other => panic!("expected InvalidWeight, got {:?}", other.map(|_| ())),
    }
    // The quantile entry point refuses it too.
    let mut b = OaxacaBuilder::new(
        frame({
            let mut w = vec![1.0; 16];
            w[9] = 2.25;
            w
        }),
        "wage",
        "group",
        "B",
    );
    b.predictors(vec!["educ"])
        .weights("hc")
        .weights_kind(WeightsKind::Frequency)
        .bootstrap_reps(0);
    let e = b.decompose_quantile(0.5).err().unwrap().to_string();
    assert!(
        e.starts_with("INVALID_WEIGHT: column=hc, row=9, value=2.25"),
        "{e}"
    );
}

#[test]
fn a_weights_column_without_a_kind_is_refused() {
    let mut b = OaxacaBuilder::new(frame(vec![1.0; 16]), "wage", "group", "B");
    b.predictors(vec!["educ"]).weights("hc").bootstrap_reps(0);
    let e = b.run().err().unwrap();
    assert!(matches!(e, OaxacaError::WeightsKindRequired { .. }), "{e}");
    assert!(
        e.to_string()
            .starts_with("WEIGHTS_KIND_REQUIRED: column=hc"),
        "{e}"
    );
    assert!(b.decompose_quantile(0.5).is_err());
}

#[test]
fn negative_and_all_zero_weights_are_refused_under_both_kinds() {
    for kind in [WeightsKind::Frequency, WeightsKind::Relative] {
        let mut w = vec![1.0; 16];
        w[3] = -1.0;
        assert!(matches!(
            mean_gap(frame(w), Some(kind)),
            Err(OaxacaError::InvalidWeight { row: 3, .. })
        ));
        assert!(matches!(
            mean_gap(frame(vec![0.0; 16]), Some(kind)),
            Err(OaxacaError::InvalidWeight { .. })
        ));
    }
}

#[test]
fn an_integer_typed_weights_column_is_accepted() {
    // A CSV of headcounts parses as Int64; the builder used to demand Float64.
    let df = frame(vec![1.0; 16])
        .lazy()
        .with_column(col("hc").cast(DataType::Int64))
        .collect()
        .unwrap();
    assert!(matches!(df.column("hc").unwrap().dtype(), DataType::Int64));
    assert!(mean_gap(df.clone(), Some(WeightsKind::Frequency)).is_ok());
    assert!(quantile_gap(df, Some(WeightsKind::Frequency)).is_ok());
}

#[test]
fn the_wire_names_are_exact() {
    assert_eq!(
        WeightsKind::parse_name("frequency").unwrap(),
        WeightsKind::Frequency
    );
    assert_eq!(
        WeightsKind::parse_name("relative").unwrap(),
        WeightsKind::Relative
    );
    for bad in ["Frequency", "freq", "", "RELATIVE"] {
        assert!(WeightsKind::parse_name(bad).is_err(), "{bad:?}");
    }
}

// ---- the RIF the quantile decomposition regresses on -----------------------------------------

/// The percentile the RIF transform used, recovered from the exported RIF column. A RIF value is
/// `q + (tau - 1{y <= q}) / f`, so the two levels it takes are `q + (tau - 1)/f` and `q + tau/f`
/// and `q = hi - tau * (hi - lo)`.
fn percentile_used_by_the_rif(y: &[f64], w: &[f64], kind: WeightsKind, tau: f64) -> f64 {
    let n = y.len();
    let mut wage = y.to_vec();
    let mut group = vec!["A"; n];
    let mut weight = w.to_vec();
    // A second group so the builder has two to compare; its own RIF is not read.
    for i in 0..8 {
        wage.push(100.0 + i as f64);
        group.push("B");
        weight.push(1.0);
    }
    let x: Vec<f64> = (0..wage.len()).map(|i| (i % 5) as f64).collect();
    let df = df![
        "wage" => wage,
        "group" => group,
        "x" => x,
        "hc" => weight,
    ]
    .unwrap();
    let mut b = OaxacaBuilder::new(df, "wage", "group", "B");
    b.predictors(vec!["x"]).weights("hc").weights_kind(kind);
    let frame = b.rif_outcome_frame(tau).unwrap();
    // `rif_outcome_frame` lists the non-reference group (A) first.
    let rif: Vec<f64> = frame
        .column("wage")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .take(n)
        .collect();
    let lo = rif.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = rif.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if hi == lo {
        // Every row is at or below the percentile (it is the sample maximum): one RIF level, and
        // the percentile cannot be read back. Report the maximum; the caller's oracle must agree.
        return y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    }
    hi - tau * (hi - lo)
}

#[test]
fn v9_the_relative_rif_is_built_on_the_hmisc_percentile() {
    // The public `weighted_quantile` is one function; the RIF transform the decomposition
    // regresses on is another path to it. This pins the second against R.
    let g = golden();
    for case in g.block(&["weights", "relative"]).as_array().unwrap() {
        let (y, w) = (vec_of(&case["y"]), vec_of(&case["w"]));
        for (key, tau) in TAUS {
            let want = f(&case["relative"], key);
            let got = percentile_used_by_the_rif(&y, &w, WeightsKind::Relative, tau);
            assert_close(
                &format!("{} RIF percentile at tau={tau}", case["name"]),
                got,
                want,
                1e-9,
            );
        }
    }
}

#[test]
fn v9_the_frequency_rif_is_built_on_the_repeated_rows_percentile() {
    let g = golden();
    for case in g.block(&["weights", "frequency"]).as_array().unwrap() {
        let (y, w) = (vec_of(&case["y"]), vec_of(&case["w"]));
        for (key, tau) in TAUS {
            let want = f(&case["frequency"], key);
            let got = percentile_used_by_the_rif(&y, &w, WeightsKind::Frequency, tau);
            assert_close(
                &format!("{} RIF percentile at tau={tau}", case["name"]),
                got,
                want,
                1e-9,
            );
        }
    }
}

// ---- Relative weights and the Weighted / Cotton mix (E-REV-4) -----------------------------------

/// Weighted least squares of `y` on a constant and `x`: (weighted mean x, weighted mean y, slope).
fn wls_line(x: &[f64], y: &[f64], w: &[f64]) -> (f64, f64, f64) {
    let sw: f64 = w.iter().sum();
    let mx = x.iter().zip(w).map(|(a, b)| a * b).sum::<f64>() / sw;
    let my = y.iter().zip(w).map(|(a, b)| a * b).sum::<f64>() / sw;
    let sxy: f64 = (0..x.len()).map(|i| w[i] * (x[i] - mx) * (y[i] - my)).sum();
    let sxx: f64 = (0..x.len()).map(|i| w[i] * (x[i] - mx).powi(2)).sum();
    (mx, my, sxy / sxx)
}

/// Groups with different education ranges and different pay slopes, so the mix matters.
fn mix_frame(weights: Vec<f64>) -> DataFrame {
    df![
        "wage"  => [10.0f64, 12.0, 15.0, 16.0, 19.0, 21.0, 26.0, 30.0,
                    20.0, 22.0, 22.0, 30.0, 35.0, 45.0, 50.0, 62.0],
        "educ"  => [1.0f64, 2.0, 3.0, 4.0, 4.0, 5.0, 6.0, 7.0,
                    2.0, 3.0, 3.0, 5.0, 6.0, 7.0, 8.0, 9.0],
        "group" => ["A", "A", "A", "A", "A", "A", "A", "A",
                    "B", "B", "B", "B", "B", "B", "B", "B"],
        "hc"    => weights,
    ]
    .unwrap()
}

fn mix_explained(
    df: DataFrame,
    kind: WeightsKind,
    scheme: oaxaca_blinder::ReferenceCoefficients,
) -> f64 {
    let mut b = OaxacaBuilder::new(df, "wage", "group", "B");
    b.predictors(vec!["educ"])
        .bootstrap_reps(0)
        .reference_coefficients(scheme)
        .weights("hc")
        .weights_kind(kind);
    let r = b.run().unwrap();
    let two = r.two_fold();
    let agg = two.aggregate();
    *agg.iter()
        .find(|c| c.name() == "explained")
        .unwrap()
        .estimate()
}

#[test]
fn relative_weights_mix_the_weighted_scheme_by_pooled_weight_not_row_count() {
    // Eight rows per group, so a row-count mix is 50/50; the design weights make group A carry
    // 16 of 24 units of weight. The mix must be sum(w_A) / sum(w).
    let w: Vec<f64> = vec![
        1.0, 1.0, 1.0, 1.0, 3.0, 3.0, 3.0, 3.0, // A
        1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, // B
    ];
    let df = mix_frame(w.clone());
    let wage: Vec<f64> = df
        .column("wage")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();
    let educ: Vec<f64> = df
        .column("educ")
        .unwrap()
        .f64()
        .unwrap()
        .into_no_null_iter()
        .collect();
    // Independent oracle: per-group WLS by closed form, mixed by raw weight share.
    let (xa, _, sa) = wls_line(&educ[..8], &wage[..8], &w[..8]);
    let (xb, _, sb) = wls_line(&educ[8..], &wage[8..], &w[8..]);
    let share_a = 16.0 / 24.0;
    let oracle = (xa - xb) * (share_a * sa + (1.0 - share_a) * sb);
    let row_count_mix = (xa - xb) * (0.5 * sa + 0.5 * sb);
    assert!(
        (oracle - row_count_mix).abs() > 0.05,
        "the fixture must tell the two mixes apart: {oracle} vs {row_count_mix}"
    );
    for scheme in [
        oaxaca_blinder::ReferenceCoefficients::Weighted,
        oaxaca_blinder::ReferenceCoefficients::Cotton,
    ] {
        let relative = mix_explained(mix_frame(w.clone()), WeightsKind::Relative, scheme);
        assert!(
            (relative - oracle).abs() < 1e-9,
            "{scheme:?}: relative {relative} vs oracle {oracle}"
        );
        // Whole-number weights mean the same under both kinds, so frequency agrees too.
        let frequency = mix_explained(mix_frame(w.clone()), WeightsKind::Frequency, scheme);
        assert!(
            (frequency - oracle).abs() < 1e-9,
            "{scheme:?}: frequency {frequency}"
        );
    }
}
