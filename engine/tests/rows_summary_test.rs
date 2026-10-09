//! 0118-MERIDIAN V6: the results say which rows were analysed.
//!
//! The expectations are written out by hand from the CSV cells below: the literal list of
//! excluded ordinals, columns and reasons, and the counts and means over the analysed rows.
//! `raw = analysed + excluded` is only a secondary check.

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::{
    decomposition_request, optimization_request, proposed, verification_request,
};
use oaxaca_blinder::OaxacaBuilder;
use pay_equity_engine::analysis::{decompose_inner, optimize_inner, verify_inner};
use pay_equity_engine::defensibility::check_defensibility_inner;
use polars::prelude::*;
use serde_json::json;
use std::collections::BTreeSet;
use std::io::Cursor;
use support::{Col, FixtureF};

/// Five excluded rows:
///   1   target     Experience blank
///   13  target     Salary AND Level blank (one row, two columns)
///   52  reference  Gender blank (the group value itself)
///   98  target     Level blank (the last target employee)
fn fixture() -> FixtureF {
    FixtureF::exact()
        .blank(1, Col::Experience)
        .blank(13, Col::Salary)
        .blank(13, Col::Level)
        .blank(52, Col::Gender)
        .blank(98, Col::Level)
}

/// Written out by hand from the blanks above. Columns follow the order the model is checked in:
/// outcome, group, numeric predictors.
fn literal_excluded_rows() -> serde_json::Value {
    json!([
        {"index": 1,  "row_key": null, "reasons": ["numericPredictor"], "columns": ["Experience"]},
        {"index": 13, "row_key": null, "reasons": ["outcome", "numericPredictor"], "columns": ["Salary", "Level"]},
        {"index": 52, "row_key": null, "reasons": ["groupValue"], "columns": ["Gender"]},
        {"index": 98, "row_key": null, "reasons": ["numericPredictor"], "columns": ["Level"]},
    ])
}

const ANALYSED_REFERENCE: usize = 59; // 60 reference rows, minus row 52 (group value blank)
const ANALYSED_TARGET: usize = 37; // 40 target rows, minus rows 1, 13 and 98

#[test]
fn the_literal_expectations_match_the_fixtures_own_cells() {
    // Guards the hand-written numbers above against a fixture change.
    let f = fixture();
    let model = FixtureF::model_cols(false, false);
    assert_eq!(f.analysed_reference(&model).len(), ANALYSED_REFERENCE);
    assert_eq!(f.analysed_target(&model).len(), ANALYSED_TARGET);
    let excluded: Vec<usize> = f.excluded(&model).iter().map(|(i, _)| *i).collect();
    assert_eq!(excluded, vec![1, 13, 52, 98]);
}

#[test]
fn analysed_counts_equal_a_complete_case_count_and_the_matrix_row_counts() {
    let f = fixture();
    let model = FixtureF::model_cols(false, false);

    // Complete-case count the test computes from the cells.
    let complete_reference = f.analysed_reference(&model).len();
    let complete_target = f.analysed_target(&model).len();
    assert_eq!(
        (complete_reference, complete_target),
        (ANALYSED_REFERENCE, ANALYSED_TARGET)
    );

    // The matrices the optimiser actually works on (A = target, B = reference).
    let mut df = CsvReader::new(Cursor::new(f.csv_bytes())).finish().unwrap();
    // The engine casts the numeric columns before building (a column with a blank is Int64).
    for name in ["Salary", "Experience", "Level"] {
        let cast = df.column(name).unwrap().cast(&DataType::Float64).unwrap();
        df.with_column(cast).unwrap();
    }
    let mut builder = OaxacaBuilder::new(df, "Salary", "Gender", support::REFERENCE);
    builder.predictors(vec!["Experience", "Level"]);
    let (x_target, _, x_reference, _, _) = builder.get_data_matrices().unwrap();
    assert_eq!(x_target.nrows(), complete_target);
    assert_eq!(x_reference.nrows(), complete_reference);

    // Every entry point reports those same counts.
    let opt = optimize_inner(optimization_request(f.csv_bytes(), false)).unwrap();
    assert_eq!(opt.analysed_reference_count, complete_reference);
    assert_eq!(opt.analysed_target_count, complete_target);

    let dec = decompose_inner(decomposition_request(f.csv_bytes(), false)).unwrap();
    assert_eq!(dec.analysed_reference_count, complete_reference);
    assert_eq!(dec.analysed_target_count, complete_target);

    let ver = verify_inner(verification_request(f.csv_bytes(), false, vec![])).unwrap();
    assert_eq!(ver.analysed_reference_count, complete_reference);
    assert_eq!(ver.analysed_target_count, complete_target);

    let def =
        check_defensibility_inner(verification_request(f.csv_bytes(), false, vec![])).unwrap();
    assert_eq!(def.analysed_reference_count, complete_reference);
    assert_eq!(def.analysed_target_count, complete_target);

    // Secondary: raw = analysed + excluded.
    assert_eq!(
        100,
        opt.analysed_reference_count + opt.analysed_target_count + opt.excluded_rows.len()
    );
}

#[test]
fn the_excluded_list_is_the_literal_ordinals_reasons_and_columns_on_every_entry_point() {
    let f = fixture();
    let want = literal_excluded_rows();

    let opt = optimize_inner(optimization_request(f.csv_bytes(), false)).unwrap();
    assert_eq!(
        serde_json::to_value(&opt.excluded_rows).unwrap(),
        want,
        "optimize"
    );

    let mut quantile = decomposition_request(f.csv_bytes(), false);
    quantile.quantile = Some(0.5);
    let dec = decompose_inner(decomposition_request(f.csv_bytes(), false)).unwrap();
    assert_eq!(
        serde_json::to_value(&dec.excluded_rows).unwrap(),
        want,
        "decompose"
    );
    let dec_q = decompose_inner(quantile).unwrap();
    assert_eq!(
        serde_json::to_value(&dec_q.excluded_rows).unwrap(),
        want,
        "decompose (quantile)"
    );

    let ver = verify_inner(verification_request(f.csv_bytes(), false, vec![])).unwrap();
    assert_eq!(
        serde_json::to_value(&ver.excluded_rows).unwrap(),
        want,
        "verify"
    );

    let def =
        check_defensibility_inner(verification_request(f.csv_bytes(), false, vec![])).unwrap();
    assert_eq!(
        serde_json::to_value(&def.excluded_rows).unwrap(),
        want,
        "defensibility"
    );
}

#[test]
fn forensic_with_adjust_both_lists_every_analysed_employee_and_only_them() {
    let f = fixture();
    let mut req = optimization_request(f.csv_bytes(), false);
    req.forensic_mode = Some(true);
    req.adjust_both_groups = Some(true);
    let res = optimize_inner(req).unwrap();

    assert_eq!(res.adjustments.len(), ANALYSED_REFERENCE + ANALYSED_TARGET);
    let got: BTreeSet<usize> = res.adjustments.iter().map(|a| a.index).collect();
    assert_eq!(got.len(), res.adjustments.len(), "no ordinal listed twice");
    let excluded: BTreeSet<usize> = [1usize, 13, 52, 98].into_iter().collect();
    let want: BTreeSet<usize> = (0..100usize).filter(|i| !excluded.contains(i)).collect();
    assert_eq!(got, want, "every ordinal except the excluded ones");
    // The last raw row is a reference employee and is listed.
    assert!(got.contains(&99));
}

#[test]
fn data_summary_counts_and_means_are_over_the_analysed_rows() {
    let f = fixture();
    let res = decompose_inner(decomposition_request(f.csv_bytes(), false)).unwrap();
    let s = res.data_summary.expect("summary");

    // total_count stays the raw row count.
    assert_eq!(s.total_count, 100);
    // group_a is the reference group, group_b the target group (the shape the app reads).
    assert_eq!(s.group_a_count, ANALYSED_REFERENCE);
    assert_eq!(s.group_b_count, ANALYSED_TARGET);

    // Hand calculation from the cells: sum of Salary over the analysed rows of each group.
    let model = FixtureF::model_cols(false, false);
    let mean = |rows: Vec<usize>| -> f64 {
        rows.iter().map(|&i| f.salary_cell(i).unwrap()).sum::<f64>() / rows.len() as f64
    };
    let want_a = mean(f.analysed_reference(&model));
    let want_b = mean(f.analysed_target(&model));
    assert!(
        (s.group_a_mean - want_a).abs() < 1e-6,
        "{} vs {}",
        s.group_a_mean,
        want_a
    );
    assert!(
        (s.group_b_mean - want_b).abs() < 1e-6,
        "{} vs {}",
        s.group_b_mean,
        want_b
    );

    // And the target mean differs from the mean over every raw target row that has a wage,
    // which is what the pre-0118 summary used: the check is not vacuous. (The reference side
    // is pinned in `the_reference_mean_excludes_a_row_blank_in_a_predictor`: row 52 here is out
    // by its blank Gender, so it is in neither group under the old or the new logic.)
    let raw_b: Vec<usize> = f
        .target_ordinals()
        .into_iter()
        .filter(|&i| f.salary_cell(i).is_some())
        .collect();
    assert!(
        (mean(raw_b) - want_b).abs() > 1.0,
        "target mean must move when rows are excluded"
    );
}

#[test]
fn the_reference_mean_excludes_a_row_blank_in_a_predictor() {
    // Reference employees 52 and 57 have a wage and a Gender, but a blank predictor: they are
    // excluded from the model, so `group_a_mean` must leave their wages out. Only the group
    // column and the wage are needed to place them in the reference group, which is the
    // distinction a mean over raw rows would miss.
    let f = FixtureF::exact()
        .blank(52, Col::Experience)
        .blank(57, Col::Level)
        .blank(13, Col::Level);
    let model = FixtureF::model_cols(false, false);
    let ref_rows = f.analysed_reference(&model);
    let tgt_rows = f.analysed_target(&model);
    assert_eq!((ref_rows.len(), tgt_rows.len()), (58, 39));

    let mean = |rows: &[usize]| -> f64 {
        rows.iter().map(|&i| f.salary_cell(i).unwrap()).sum::<f64>() / rows.len() as f64
    };
    let raw_a: Vec<usize> = f
        .reference_ordinals()
        .into_iter()
        .filter(|&i| f.salary_cell(i).is_some())
        .collect();
    let raw_b: Vec<usize> = f
        .target_ordinals()
        .into_iter()
        .filter(|&i| f.salary_cell(i).is_some())
        .collect();
    assert_eq!(raw_a.len(), 60);
    assert!(
        (mean(&raw_a) - mean(&ref_rows)).abs() > 1.0,
        "the raw-row reference mean {} must differ from the analysed one {}",
        mean(&raw_a),
        mean(&ref_rows)
    );
    assert!((mean(&raw_b) - mean(&tgt_rows)).abs() > 1.0);

    let res = decompose_inner(decomposition_request(f.csv_bytes(), false)).unwrap();
    let s = res.data_summary.expect("summary");
    assert_eq!((s.group_a_count, s.group_b_count), (58, 39));
    assert!(
        (s.group_a_mean - mean(&ref_rows)).abs() < 1e-6,
        "group_a_mean {} vs hand mean over analysed reference rows {}",
        s.group_a_mean,
        mean(&ref_rows)
    );
    assert!(
        (s.group_a_mean - mean(&raw_a)).abs() > 1.0,
        "group_a_mean {} must not be the raw-row mean {}",
        s.group_a_mean,
        mean(&raw_a)
    );
    assert!((s.group_b_mean - mean(&tgt_rows)).abs() < 1e-6);
}

#[test]
fn a_complete_file_reports_nothing_excluded() {
    let f = FixtureF::exact();
    let res = decompose_inner(decomposition_request(f.csv_bytes(), false)).unwrap();
    assert_eq!(res.analysed_reference_count, 60);
    assert_eq!(res.analysed_target_count, 40);
    assert!(res.excluded_rows.is_empty());
    assert_eq!(res.adjustments_on_excluded_rows, 0);
    let s = res.data_summary.unwrap();
    assert_eq!(
        (s.total_count, s.group_a_count, s.group_b_count),
        (100, 60, 40)
    );

    // And the serialized shape: the new fields are always present, never omitted.
    let v =
        serde_json::to_value(optimize_inner(optimization_request(f.csv_bytes(), false)).unwrap())
            .unwrap();
    for key in [
        "analysed_reference_count",
        "analysed_target_count",
        "excluded_rows",
        "adjustments_on_excluded_rows",
    ] {
        assert!(v.get(key).is_some(), "{key} must be on every result");
    }
    let _ = proposed;
}
