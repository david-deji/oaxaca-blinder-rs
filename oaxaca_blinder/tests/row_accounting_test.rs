//! 0118-MERIDIAN S1 and S2: `get_data_matrices_with_rows` and the group-value check.
//!
//! Every expected number comes from Fixture F's own cells (see
//! `engine/tests/support/mod.rs`), never from the builder's output.

#[path = "../../engine/tests/support/mod.rs"]
mod support;

use oaxaca_blinder::{DataMatricesWithRows, ExclusionReason, OaxacaBuilder, OaxacaError};
use polars::prelude::*;
use std::io::Cursor;
use support::{Col, FixtureF, REFERENCE, TARGET};

/// Parses CSV text the way the engine does, then casts the numeric columns to Float64 as every
/// engine entry point does before handing the frame to the builder (a column with a blank cell
/// is read as Int64).
fn parse(csv: Vec<u8>) -> DataFrame {
    let mut df = CsvReader::new(Cursor::new(csv))
        .finish()
        .expect("fixture csv must parse");
    for name in ["Salary", "Experience", "Level", "Weight"] {
        if let Ok(col) = df.column(name) {
            let cast = col.cast(&DataType::Float64).unwrap();
            df.with_column(cast).unwrap();
        }
    }
    df
}

fn frame(fixture: &FixtureF) -> DataFrame {
    parse(fixture.csv_bytes())
}

fn builder(fixture: &FixtureF, categorical: bool, weighted: bool) -> OaxacaBuilder {
    let mut b = OaxacaBuilder::new(frame(fixture), "Salary", "Gender", REFERENCE);
    b.predictors(vec!["Experience", "Level"]);
    if categorical {
        b.categorical_predictors(vec!["Dept"]);
    }
    if weighted {
        b.weights("Weight")
            .weights_kind(oaxaca_blinder::WeightsKind::Relative);
    }
    b
}

fn reason_name(r: ExclusionReason) -> String {
    serde_json::to_value(r)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

/// Checks one variant against the fixture's own cells: ordinals, the matrix contents at those
/// ordinals, and the excluded list with reasons and columns.
fn assert_variant(label: &str, fixture: &FixtureF, categorical: bool, weighted: bool) {
    let model = FixtureF::model_cols(categorical, weighted);
    let m: DataMatricesWithRows = builder(fixture, categorical, weighted)
        .get_data_matrices_with_rows()
        .unwrap_or_else(|e| panic!("{label}: {e}"));

    let want_reference = fixture.analysed_reference(&model);
    let want_target = fixture.analysed_target(&model);
    assert_eq!(
        m.reference.rows, want_reference,
        "{label}: reference ordinals"
    );
    assert_eq!(m.target.rows, want_target, "{label}: target ordinals");
    assert_eq!(m.total_rows, 100, "{label}: total rows");

    // Matrix rows ARE the employees the ordinals say: outcome and both predictors read back
    // from the fixture's own cells at that ordinal.
    for (group, rows) in [("reference", &m.reference), ("target", &m.target)] {
        assert_eq!(rows.x.nrows(), rows.rows.len(), "{label}: {group} x rows");
        assert_eq!(rows.y.len(), rows.rows.len(), "{label}: {group} y len");
        for (i, &ordinal) in rows.rows.iter().enumerate() {
            let salary = fixture
                .salary_cell(ordinal)
                .expect("analysed row has a wage");
            assert_eq!(
                rows.y[i], salary,
                "{label}: {group} y[{i}] vs Salary[{ordinal}]"
            );
            assert_eq!(rows.x[(i, 0)], 1.0, "{label}: intercept");
            assert_eq!(
                rows.x[(i, 1)],
                fixture.rows[ordinal].experience as f64,
                "{label}: {group} Experience at ordinal {ordinal}"
            );
            assert_eq!(
                rows.x[(i, 2)],
                fixture.rows[ordinal].level as f64,
                "{label}: {group} Level at ordinal {ordinal}"
            );
        }
    }

    let want_excluded = fixture.excluded(&model);
    assert_eq!(
        m.excluded_rows.len(),
        want_excluded.len(),
        "{label}: excluded count"
    );
    for (got, (ordinal, cols)) in m.excluded_rows.iter().zip(&want_excluded) {
        assert_eq!(got.index, *ordinal, "{label}: excluded ordinal");
        let want_columns: Vec<&str> = cols.iter().map(|c| c.header()).collect();
        assert_eq!(
            got.columns.iter().map(String::as_str).collect::<Vec<_>>(),
            want_columns,
            "{label}: excluded columns at {ordinal}"
        );
        let mut want_reasons: Vec<&str> = Vec::new();
        for c in cols {
            if !want_reasons.contains(&c.reason()) {
                want_reasons.push(c.reason());
            }
        }
        assert_eq!(
            got.reasons
                .iter()
                .map(|r| reason_name(*r))
                .collect::<Vec<_>>(),
            want_reasons,
            "{label}: excluded reasons at {ordinal}"
        );
    }
}

#[test]
fn complete_file_lists_every_row_in_file_order() {
    let f = FixtureF::exact();
    assert_variant("complete", &f, false, false);
    let m = builder(&f, false, false)
        .get_data_matrices_with_rows()
        .unwrap();
    assert_eq!(m.reference.rows.len(), 60);
    assert_eq!(m.target.rows.len(), 40);
    assert!(m.excluded_rows.is_empty());
    // Interleaved groups: the group-local position is not the raw ordinal.
    assert_eq!(m.target.rows[0], 1);
    assert_eq!(m.target.rows[39], 98);
    assert_eq!(m.reference.rows[59], 99);
}

#[test]
fn blank_in_the_first_middle_and_last_target_row() {
    // Raw ordinals 1, 51 and 98 are the first, a middle, and the last target employee.
    assert_variant(
        "target first (Experience)",
        &FixtureF::exact().blank(1, Col::Experience),
        false,
        false,
    );
    assert_variant(
        "target middle (Salary)",
        &FixtureF::exact().blank(51, Col::Salary),
        false,
        false,
    );
    assert_variant(
        "target last (Level)",
        &FixtureF::exact().blank(98, Col::Level),
        false,
        false,
    );
}

#[test]
fn blank_in_a_reference_row() {
    assert_variant(
        "reference middle",
        &FixtureF::exact().blank(52, Col::Experience),
        false,
        false,
    );
    // The last raw row is a reference employee.
    assert_variant(
        "reference last",
        &FixtureF::exact().blank(99, Col::Salary),
        false,
        false,
    );
}

#[test]
fn one_blank_per_null_source() {
    assert_variant(
        "outcome",
        &FixtureF::exact().blank(13, Col::Salary),
        false,
        false,
    );
    assert_variant(
        "numeric predictor",
        &FixtureF::exact().blank(13, Col::Experience),
        false,
        false,
    );
    assert_variant(
        "categorical predictor",
        &FixtureF::exact().with_dept().blank(13, Col::Dept),
        true,
        false,
    );
    assert_variant(
        "weights",
        &FixtureF::exact().with_weight().blank(13, Col::Weight),
        false,
        true,
    );
    assert_variant(
        "group value",
        &FixtureF::exact().blank(13, Col::Gender),
        false,
        false,
    );
}

#[test]
fn the_per_column_variants_hold_for_every_group() {
    // The same null sources on a reference row, so a bug that only handled one group would show.
    assert_variant(
        "reference categorical",
        &FixtureF::exact().with_dept().blank(52, Col::Dept),
        true,
        false,
    );
    assert_variant(
        "reference weights",
        &FixtureF::exact().with_weight().blank(52, Col::Weight),
        false,
        true,
    );
    assert_variant(
        "reference group value",
        &FixtureF::exact().blank(52, Col::Gender),
        false,
        false,
    );
}

#[test]
fn a_row_blank_in_two_columns_appears_once_naming_both() {
    let f = FixtureF::exact()
        .blank(13, Col::Salary)
        .blank(13, Col::Experience)
        .blank(40, Col::Level);
    assert_variant("two columns", &f, false, false);
    let m = builder(&f, false, false)
        .get_data_matrices_with_rows()
        .unwrap();
    assert_eq!(m.excluded_rows.len(), 2, "13 once (not twice), and 40");
    assert_eq!(m.excluded_rows[0].index, 13);
    assert_eq!(m.excluded_rows[0].columns, vec!["Salary", "Experience"]);
    assert_eq!(
        m.excluded_rows[0].reasons,
        vec![ExclusionReason::Outcome, ExclusionReason::NumericPredictor]
    );
    // Two blanks that share a reason: the reason is listed once, both columns are named.
    let both_numeric = FixtureF::exact()
        .blank(13, Col::Experience)
        .blank(13, Col::Level);
    let m = builder(&both_numeric, false, false)
        .get_data_matrices_with_rows()
        .unwrap();
    assert_eq!(m.excluded_rows.len(), 1);
    assert_eq!(
        m.excluded_rows[0].reasons,
        vec![ExclusionReason::NumericPredictor]
    );
    assert_eq!(m.excluded_rows[0].columns, vec!["Experience", "Level"]);
}

#[test]
fn an_ordinal_is_the_position_in_the_frame_the_builder_was_given() {
    // The builder sees a parsed frame, so its ordinal is the row position in THAT frame. Making
    // that position the same as the browser app's `csvData` position (physically blank lines
    // skipped) is the CSV reader's job: see `engine/src/rows.rs::read_csv` and the engine test
    // `a_physically_blank_line_does_not_shift_an_ordinal`.
    let f = FixtureF::exact().blank(61, Col::Experience);
    assert_variant("plain", &f, false, false);
    let m = builder(&f, false, false)
        .get_data_matrices_with_rows()
        .unwrap();
    assert_eq!(m.excluded_rows.len(), 1);
    assert_eq!(m.excluded_rows[0].index, 61);
}

#[test]
fn the_legacy_tuple_is_a_thin_wrapper_with_target_first() {
    let f = FixtureF::exact().blank(13, Col::Experience);
    let b = builder(&f, false, false);
    let (x_a, y_a, x_b, y_b, names) = b.get_data_matrices().unwrap();
    let m = b.get_data_matrices_with_rows().unwrap();
    // A = the non-reference (target) group, B = the reference group.
    assert_eq!(x_a, m.target.x);
    assert_eq!(y_a, m.target.y);
    assert_eq!(x_b, m.reference.x);
    assert_eq!(y_b, m.reference.y);
    assert_eq!(names, m.predictor_names);
}

#[test]
fn analysed_rows_agrees_with_the_matrices() {
    let f = FixtureF::exact()
        .blank(13, Col::Experience)
        .blank(52, Col::Salary);
    let b = builder(&f, false, false);
    let acc = b.analysed_rows().unwrap();
    let m = b.get_data_matrices_with_rows().unwrap();
    assert_eq!(acc.reference_rows, m.reference.rows);
    assert_eq!(acc.target_rows, m.target.rows);
    assert_eq!(acc.excluded_rows, m.excluded_rows);
    assert_eq!(acc.total_rows, 100);
}

// ---- S2: more than two group values ------------------------------------------------------

fn with_gender(f: &FixtureF, row: usize, value: &str) -> OaxacaBuilder {
    // Rewrite one Gender cell in the CSV text.
    let csv = f.csv();
    let mut lines: Vec<String> = csv.lines().map(String::from).collect();
    let cells: Vec<&str> = lines[row + 1].split(',').collect();
    let mut new_cells: Vec<String> = cells.iter().map(|s| s.to_string()).collect();
    new_cells[2] = value.to_string(); // Name, Salary, Gender, ...
    lines[row + 1] = new_cells.join(",");
    let df = parse(lines.join("\n").into_bytes());
    let mut b = OaxacaBuilder::new(df, "Salary", "Gender", REFERENCE);
    b.predictors(vec!["Experience", "Level"]).bootstrap_reps(2);
    b
}

fn extra_values(err: OaxacaError) -> Vec<String> {
    match err {
        OaxacaError::TooManyGroupValues {
            group_column,
            reference_group,
            other_values,
        } => {
            assert_eq!(group_column, "Gender");
            assert_eq!(reference_group, REFERENCE);
            other_values
        }
        other => panic!("expected TooManyGroupValues, got {other}"),
    }
}

#[test]
fn a_third_group_value_is_refused_by_name_on_every_builder_entry_point() {
    let f = FixtureF::exact();
    let b = with_gender(&f, 7, "Non-binary");
    let want = vec!["Female".to_string(), "Non-binary".to_string()];

    assert_eq!(extra_values(b.run().map(|_| ()).unwrap_err()), want);
    assert_eq!(
        extra_values(b.decompose_quantile(0.5).map(|_| ()).unwrap_err()),
        want
    );
    assert_eq!(
        extra_values(b.get_data_matrices_with_rows().map(|_| ()).unwrap_err()),
        want
    );
    assert_eq!(
        extra_values(b.get_data_matrices().map(|_| ()).unwrap_err()),
        want
    );
    assert_eq!(
        extra_values(b.analysed_rows().map(|_| ()).unwrap_err()),
        want
    );

    // The message carries the exact values, quoted so whitespace is visible.
    let msg = b.run().map(|_| ()).unwrap_err().to_string();
    assert_eq!(
        msg,
        "TOO_MANY_GROUP_VALUES: column=Gender, reference_group=\"Male\", \
         distinct_other_values=2, other_values=[\"Female\", \"Non-binary\"]"
    );
}

#[test]
fn whitespace_and_case_variants_are_values_not_typos() {
    let f = FixtureF::exact();
    for (label, variant) in [
        ("trailing space", "Female "),
        ("whitespace only", " "),
        ("lower case", "female"),
        ("reference lower case", "male"),
    ] {
        let b = with_gender(&f, 7, variant);
        let got = extra_values(b.get_data_matrices_with_rows().map(|_| ()).unwrap_err());
        assert!(
            got.contains(&variant.to_string()),
            "{label}: {variant:?} must be named in {got:?}"
        );
    }
}

#[test]
fn a_third_value_on_a_row_blank_elsewhere_is_still_an_error() {
    // The check runs on the RAW frame. Row 7 would be dropped by cleaning (blank Experience),
    // but its group value is still in the file.
    let f = FixtureF::exact().blank(7, Col::Experience);
    let b = with_gender(&f, 7, "Unknown");
    let got = extra_values(b.get_data_matrices_with_rows().map(|_| ()).unwrap_err());
    assert_eq!(got, vec!["Female".to_string(), "Unknown".to_string()]);
}

#[test]
fn a_blank_group_cell_is_not_a_third_value() {
    // A blank is an excluded row (reason groupValue), not a value.
    let f = FixtureF::exact().blank(7, Col::Gender);
    let m = builder(&f, false, false)
        .get_data_matrices_with_rows()
        .expect("a blank group cell must not trip the third-value check");
    assert_eq!(m.excluded_rows.len(), 1);
    assert_eq!(
        m.excluded_rows[0].reasons,
        vec![ExclusionReason::GroupValue]
    );
}

#[test]
fn an_absent_reference_group_is_named() {
    let f = FixtureF::exact();
    let mut b = OaxacaBuilder::new(frame(&f), "Salary", "Gender", "Nobody");
    b.predictors(vec!["Experience", "Level"]);
    match b.get_data_matrices_with_rows().map(|_| ()).unwrap_err() {
        OaxacaError::ReferenceGroupAbsent {
            group_column,
            reference_group,
        } => {
            assert_eq!(group_column, "Gender");
            assert_eq!(reference_group, "Nobody");
        }
        other => panic!("expected ReferenceGroupAbsent, got {other}"),
    }
    let _ = TARGET;
}
