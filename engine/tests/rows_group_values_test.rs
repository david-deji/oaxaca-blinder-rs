//! 0118-MERIDIAN V3: more than two group values is a named error on every engine surface.
//!
//! A third value in the group column (`'Non-binary'`, `'Unknown'`, `'F '`) used to shift every
//! pairing with no blank anywhere: the estimation dropped the third group's rows while the
//! optimiser still listed them as target employees. It is now refused by name, checked once on
//! the raw frame, on all five entry points (the WASM and MCP surfaces are thin wrappers over
//! them) and on the quantile and three-fold decomposition paths.

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::{
    decomposition_request, frontier_request, optimization_request, proposed, verification_request,
};
use pay_equity_engine::analysis::{
    calculate_efficient_frontier_inner, decompose_inner, optimize_inner, verify_inner,
};
use pay_equity_engine::defensibility::check_defensibility_inner;
use support::{Col, FixtureF, REFERENCE};

/// F's CSV with the Gender cell of `row` replaced by `value` (the Gender column is third).
fn csv_with_gender(f: &FixtureF, row: usize, value: &str) -> Vec<u8> {
    let csv = f.csv();
    let mut lines: Vec<String> = csv.lines().map(String::from).collect();
    let mut cells: Vec<String> = lines[row + 1].split(',').map(String::from).collect();
    cells[2] = value.to_string();
    lines[row + 1] = cells.join(",");
    let mut out = lines.join("\n");
    out.push('\n');
    out.into_bytes()
}

/// The message every surface must produce: variant prefix, the column, the reference, and the
/// extra values (everything except the reference), ascending, quoted so whitespace shows.
fn expected_message(extra: &[&str]) -> String {
    let mut values: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
    values.sort();
    format!(
        "TOO_MANY_GROUP_VALUES: column=Gender, reference_group={:?}, \
         distinct_other_values={}, other_values={:?}",
        REFERENCE,
        values.len(),
        values
    )
}

/// Runs the input through all seven paths and returns `(path, error)` for each. Panics if any
/// path succeeds or panics.
fn errors_on_every_path(csv: &[u8]) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();

    out.push((
        "decompose (OLS, pooled)",
        decompose_inner(decomposition_request(csv.to_vec(), false)).map(|_| ()),
    ));
    let mut three_fold = decomposition_request(csv.to_vec(), false);
    three_fold.three_fold = Some(true);
    out.push((
        "decompose (three-fold)",
        decompose_inner(three_fold).map(|_| ()),
    ));
    let mut quantile = decomposition_request(csv.to_vec(), false);
    quantile.quantile = Some(0.5);
    out.push((
        "decompose (quantile)",
        decompose_inner(quantile).map(|_| ()),
    ));
    out.push((
        "optimize",
        optimize_inner(optimization_request(csv.to_vec(), false)).map(|_| ()),
    ));
    out.push((
        "verify_adjustments",
        verify_inner(verification_request(
            csv.to_vec(),
            false,
            vec![proposed(0, 0.0)],
        ))
        .map(|_| ()),
    ));
    out.push((
        "check_defensibility",
        check_defensibility_inner(verification_request(
            csv.to_vec(),
            false,
            vec![proposed(3, 0.0)],
        ))
        .map(|_| ()),
    ));
    out.push((
        "calculate_efficient_frontier",
        calculate_efficient_frontier_inner(frontier_request(csv.to_vec(), 4, Some(1000.0)))
            .map(|_| ()),
    ));

    out.into_iter()
        .map(|(path, r)| match r {
            Err(e) => (path, e),
            Ok(()) => panic!("{path}: a third group value must be refused, but the call succeeded"),
        })
        .collect()
}

fn assert_refused_everywhere(label: &str, csv: &[u8], extra: &[&str]) {
    let want = expected_message(extra);
    for (path, err) in errors_on_every_path(csv) {
        // Exact text, no prefix stripping: every surface returns the same wire string, which
        // the app matches on.
        assert_eq!(
            err, want,
            "{label}: {path} must refuse with the named error"
        );
    }
}

#[test]
fn a_third_group_value_is_refused_by_name_on_every_path() {
    let f = FixtureF::exact();
    // 'Non-binary' on a row in the middle of the file.
    assert_refused_everywhere(
        "Non-binary",
        &csv_with_gender(&f, 51, "Non-binary"),
        &["Female", "Non-binary"],
    );
    // 'Unknown' on the very last row.
    assert_refused_everywhere(
        "Unknown on the last row",
        &csv_with_gender(&f, 99, "Unknown"),
        &["Female", "Unknown"],
    );
}

#[test]
fn whitespace_and_case_variants_are_values_not_typos() {
    let f = FixtureF::exact();
    // 'Female ' (trailing space), ' ' (only whitespace), 'female' (case): each is a value, none
    // is trimmed or folded into 'Female'.
    for variant in ["Female ", " ", "female"] {
        assert_refused_everywhere(
            &format!("{variant:?}"),
            &csv_with_gender(&f, 8, variant),
            &["Female", variant],
        );
    }
    // The reference spelled differently is also a third value.
    assert_refused_everywhere("male", &csv_with_gender(&f, 8, "male"), &["Female", "male"]);
}

#[test]
fn a_third_value_that_only_appears_on_a_row_blank_elsewhere_is_still_refused() {
    // Row 7 would be dropped by cleaning (blank Experience), so a check on the cleaned frame
    // would never see 'Unknown'. The check runs on the raw frame.
    let f = FixtureF::exact().blank(7, Col::Experience);
    assert_refused_everywhere(
        "third value on a blank row",
        &csv_with_gender(&f, 7, "Unknown"),
        &["Female", "Unknown"],
    );
}

#[test]
fn a_blank_group_cell_is_an_excluded_row_not_a_third_value() {
    let f = FixtureF::exact().blank(7, Col::Gender);
    let res = optimize_inner(optimization_request(f.csv_bytes(), false))
        .expect("a blank group cell must not be refused as a third value");
    assert_eq!(res.excluded_rows.len(), 1);
    assert_eq!(res.excluded_rows[0].index, 7);
}

#[test]
fn an_absent_reference_group_is_a_named_error() {
    let f = FixtureF::exact();
    let mut req = optimization_request(f.csv_bytes(), false);
    req.reference_group = "Nobody".to_string();
    let err = optimize_inner(req).unwrap_err();
    assert_eq!(
        err,
        "REFERENCE_GROUP_ABSENT: column=Gender, reference_group=\"Nobody\""
    );
}

#[test]
fn an_absent_reference_group_has_the_same_text_on_every_path() {
    let f = FixtureF::exact();
    let csv = f.csv_bytes();
    let want = "REFERENCE_GROUP_ABSENT: column=Gender, reference_group=\"Nobody\"";

    let mut dec = decomposition_request(csv.clone(), false);
    dec.reference_group = "Nobody".to_string();
    assert_eq!(decompose_inner(dec).map(|_| ()).unwrap_err(), want);

    let mut opt = optimization_request(csv.clone(), false);
    opt.reference_group = "Nobody".to_string();
    assert_eq!(optimize_inner(opt).map(|_| ()).unwrap_err(), want);

    let mut ver = verification_request(csv.clone(), false, vec![proposed(0, 0.0)]);
    ver.decomposition_params.reference_group = "Nobody".to_string();
    assert_eq!(verify_inner(ver).map(|_| ()).unwrap_err(), want);

    let mut def = verification_request(csv.clone(), false, vec![proposed(3, 0.0)]);
    def.decomposition_params.reference_group = "Nobody".to_string();
    assert_eq!(
        check_defensibility_inner(def).map(|_| ()).unwrap_err(),
        want
    );

    let mut fro = frontier_request(csv, 4, Some(1000.0));
    fro.decomposition_params.reference_group = "Nobody".to_string();
    assert_eq!(
        calculate_efficient_frontier_inner(fro)
            .map(|_| ())
            .unwrap_err(),
        want
    );
}

#[test]
fn the_wrong_group_column_does_not_dump_every_employee_name_into_the_error() {
    // Name picked as the group variable: ~99 distinct non-reference values. The message names
    // the count and the first five, never the whole list.
    let f = FixtureF::exact();
    let csv = f.csv_bytes();
    let reference = f.name(0).to_string();

    let mut dec = decomposition_request(csv.clone(), false);
    dec.group_variable = "Name".to_string();
    dec.reference_group = reference.clone();
    let err = decompose_inner(dec).map(|_| ()).unwrap_err();
    assert!(
        err.starts_with("TOO_MANY_GROUP_VALUES: column=Name, "),
        "{err}"
    );
    assert!(err.contains("distinct_other_values=99,"), "{err}");
    assert!(err.ends_with(" and 94 more"), "{err}");
    assert!(err.len() < 400, "message must stay short: {err}");
    assert!(!err.contains(f.name(60)), "{err}");

    // The same text on a second surface.
    let mut opt = optimization_request(csv, false);
    opt.group_variable = "Name".to_string();
    opt.reference_group = reference;
    assert_eq!(optimize_inner(opt).map(|_| ()).unwrap_err(), err);
}
