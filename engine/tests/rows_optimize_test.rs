//! 0118-MERIDIAN V1: `optimize` pays every employee their own gap.
//!
//! Defect under test: the optimiser listed employees by position in the raw file but computed
//! every dollar on the frame left after blank cells were dropped. From the first blank on, each
//! name carried the next employee's fair wage and the last employee was never paid.
//!
//! Every expected figure is computed here from Fixture F's own cells and formula
//! (`tests/support/mod.rs`); none is read from engine output. Results are looked up by
//! `Adjustment.index`, never by position in the returned list, so a shift of dollars between
//! employees cannot pass.

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::{optimization_request, underpaid_targets, with_blank_lines};
use pay_equity_engine::analysis::optimize_inner;
use pay_equity_engine::types::{AllocationStrategy, OptimizationResult, RangeTarget};
use std::collections::BTreeSet;
use support::{same_to_the_cent, Col, FixtureF};

fn model(categorical: bool) -> Vec<Col> {
    FixtureF::model_cols(categorical, false)
}

/// Asserts the schedule against the fixture, employee by employee. `forensic` means every
/// analysed target row is expected in the list (overpaid employees with adjustment 0);
/// otherwise only the underpaid ones.
fn assert_target_schedule(
    label: &str,
    f: &FixtureF,
    res: &OptimizationResult,
    categorical: bool,
    forensic: bool,
) {
    let model = model(categorical);
    let expected_rows: BTreeSet<usize> = if forensic {
        f.analysed_target(&model).into_iter().collect()
    } else {
        underpaid_targets(f, &model).into_iter().collect()
    };

    let target_rows: BTreeSet<usize> = f.target_ordinals().into_iter().collect();
    let got_target_rows: BTreeSet<usize> = res
        .adjustments
        .iter()
        .map(|a| a.index)
        .filter(|i| target_rows.contains(i))
        .collect();
    assert_eq!(
        got_target_rows, expected_rows,
        "{label}: the employees in the schedule must be exactly the expected ones"
    );

    // Excluded employees appear nowhere.
    for (excluded, _) in f.excluded(&model) {
        assert!(
            res.adjustments.iter().all(|a| a.index != excluded),
            "{label}: excluded row {excluded} must not be in any adjustment"
        );
    }

    let mut need = 0.0;
    for a in &res.adjustments {
        let i = a.index;
        let fair = f.formula_wage(i);
        let current = f.salary_cell(i).expect("an adjusted row has a wage");
        assert!(
            same_to_the_cent(a.fair_wage, fair),
            "{label}: index {i} ({}) fair_wage {} != formula {}",
            f.name(i),
            a.fair_wage,
            fair
        );
        assert!(
            same_to_the_cent(a.current_wage, current),
            "{label}: index {i} ({}) current_wage {} != Salary cell {}",
            f.name(i),
            a.current_wage,
            current
        );
        let want_adjustment = (fair - current).max(0.0);
        assert!(
            same_to_the_cent(a.adjustment, want_adjustment),
            "{label}: index {i} ({}) adjustment {} != max(0, fair - current) = {}",
            f.name(i),
            a.adjustment,
            want_adjustment
        );
        assert!(
            same_to_the_cent(a.new_wage, current.max(fair)),
            "{label}: index {i} ({}) new_wage {} != max(current, fair) = {}",
            f.name(i),
            a.new_wage,
            current.max(fair)
        );
        if target_rows.contains(&i) {
            need += want_adjustment;
        }
    }
    assert!(
        (res.required_budget - need).abs() < 0.05,
        "{label}: required_budget {} != sum of the gaps {}",
        res.required_budget,
        need
    );
    assert!(
        (res.total_cost - need).abs() < 0.05,
        "{label}: total_cost {} != sum of the gaps {}",
        res.total_cost,
        need
    );
}

/// The blank variants V1 names, with whether they need the categorical predictor.
fn variants() -> Vec<(&'static str, FixtureF, bool)> {
    vec![
        ("control, no blank", FixtureF::exact(), false),
        (
            "target first blank",
            FixtureF::exact().blank(1, Col::Experience),
            false,
        ),
        (
            "target middle blank",
            FixtureF::exact().blank(51, Col::Salary),
            false,
        ),
        (
            "target last blank",
            FixtureF::exact().blank(98, Col::Level),
            false,
        ),
        (
            "reference middle blank",
            FixtureF::exact().blank(52, Col::Experience),
            false,
        ),
        (
            "last reference row blank",
            FixtureF::exact().blank(99, Col::Salary),
            false,
        ),
        (
            "target group value blank",
            FixtureF::exact().blank(13, Col::Gender),
            false,
        ),
        (
            "row blank in two columns",
            FixtureF::exact()
                .blank(13, Col::Salary)
                .blank(13, Col::Experience),
            false,
        ),
        (
            "several blanks in both groups",
            FixtureF::exact()
                .blank(1, Col::Experience)
                .blank(13, Col::Level)
                .blank(52, Col::Salary)
                .blank(98, Col::Experience),
            false,
        ),
        (
            "physically blank line before a blank cell",
            FixtureF::exact()
                .blank_line_before(30)
                .blank(61, Col::Experience),
            false,
        ),
        (
            "categorical predictor blank (target)",
            FixtureF::exact().with_dept().blank(13, Col::Dept),
            true,
        ),
        (
            "categorical predictor blank (reference)",
            FixtureF::exact().with_dept().blank(52, Col::Dept),
            true,
        ),
    ]
}

#[test]
fn greedy_pays_each_employee_their_own_gap() {
    for (label, f, categorical) in variants() {
        let res = optimize_inner(optimization_request(f.csv_bytes(), categorical))
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_target_schedule(label, &f, &res, categorical, false);
    }
}

#[test]
fn equitable_pays_each_employee_their_own_gap() {
    for (label, f, categorical) in variants() {
        let mut req = optimization_request(f.csv_bytes(), categorical);
        req.strategy = Some(AllocationStrategy::Equitable);
        let res = optimize_inner(req).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_target_schedule(label, &f, &res, categorical, false);
    }
}

#[test]
fn forensic_mode_lists_overpaid_employees_with_zero_adjustment() {
    for (label, f, categorical) in variants() {
        let mut req = optimization_request(f.csv_bytes(), categorical);
        req.forensic_mode = Some(true);
        let res = optimize_inner(req).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_target_schedule(label, &f, &res, categorical, true);
        // Nine target employees are overpaid in Fixture F; those still analysed carry 0.
        let overpaid_in_list = res
            .adjustments
            .iter()
            .filter(|a| f.formula_wage(a.index) < f.salary_cell(a.index).unwrap() - 1e-6)
            .count();
        let expected_overpaid = f
            .analysed_target(&model(categorical))
            .into_iter()
            .filter(|&i| f.formula_wage(i) < f.salary_cell(i).unwrap() - 1e-6)
            .count();
        assert_eq!(overpaid_in_list, expected_overpaid, "{label}");
        assert!(
            expected_overpaid >= 6,
            "{label}: fixture keeps overpaid employees"
        );
    }
}

#[test]
fn the_last_target_employee_is_paid_with_a_blank_earlier_in_the_group() {
    // The pre-0118 engine never paid the last employee of a group once any blank preceded them.
    let f = FixtureF::exact().blank(1, Col::Experience);
    let res = optimize_inner(optimization_request(f.csv_bytes(), false)).unwrap();
    let last = res
        .adjustments
        .iter()
        .find(|a| a.index == 98)
        .expect("ordinal 98 is the last target employee and must be in the schedule");
    // Row 98 is underpaid by exactly 2650 (see `support::shortfall`).
    assert!(
        same_to_the_cent(last.adjustment, 2650.0),
        "{}",
        last.adjustment
    );
    assert!(same_to_the_cent(last.fair_wage, f.formula_wage(98)));
    assert!(same_to_the_cent(
        last.current_wage,
        f.salary_cell(98).unwrap()
    ));
}

#[test]
fn row_key_names_the_csv_row_when_a_key_column_is_supplied() {
    let f = FixtureF::exact()
        .with_employee_id()
        .blank(1, Col::Experience)
        .blank(52, Col::Level);
    let res = optimize_inner(optimization_request(f.csv_bytes(), false)).unwrap();
    assert!(!res.adjustments.is_empty());
    for a in &res.adjustments {
        let want = format!("c:{}", f.rows[a.index].employee_id);
        assert_eq!(
            a.row_key.as_deref(),
            Some(want.as_str()),
            "index {} ({}) must carry its own employee id",
            a.index,
            f.name(a.index)
        );
    }
    // The excluded rows carry their own keys too.
    let excluded: Vec<(usize, Option<&str>)> = res
        .excluded_rows
        .iter()
        .map(|e| (e.index, e.row_key.as_deref()))
        .collect();
    assert_eq!(
        excluded,
        vec![
            (1, Some(format!("c:{}", f.rows[1].employee_id).as_str())),
            (52, Some(format!("c:{}", f.rows[52].employee_id).as_str())),
        ]
    );
}

#[test]
fn adjust_both_groups_with_forensic_pairs_reference_employees_too() {
    // The app always sends forensic_mode: true, so reference employees are in the ledger.
    // A blank in a reference row must not shift any reference employee's figures either.
    let f = FixtureF::exact()
        .blank(52, Col::Experience)
        .blank(1, Col::Level);
    let model = model(false);
    let mut req = optimization_request(f.csv_bytes(), false);
    req.forensic_mode = Some(true);
    req.adjust_both_groups = Some(true);
    let res = optimize_inner(req).unwrap();

    let reference_rows = f.analysed_reference(&model);
    let target_rows = f.analysed_target(&model);
    let want_all: BTreeSet<usize> = reference_rows
        .iter()
        .chain(target_rows.iter())
        .copied()
        .collect();
    let got_all: BTreeSet<usize> = res.adjustments.iter().map(|a| a.index).collect();
    assert_eq!(
        got_all, want_all,
        "every analysed employee, none of the excluded"
    );
    assert!(
        got_all.contains(&99),
        "the last raw row is a reference employee"
    );
    assert!(!got_all.contains(&52) && !got_all.contains(&1));

    for a in &res.adjustments {
        let i = a.index;
        let fair = f.formula_wage(i);
        let current = f.salary_cell(i).unwrap();
        assert!(
            same_to_the_cent(a.fair_wage, fair),
            "index {i}: fair {}",
            a.fair_wage
        );
        assert!(
            same_to_the_cent(a.current_wage, current),
            "index {i}: current {}",
            a.current_wage
        );
        assert!(
            same_to_the_cent(a.adjustment, (fair - current).max(0.0)),
            "index {i}"
        );
        assert!(same_to_the_cent(a.new_wage, current.max(fair)), "index {i}");
    }
    // Reference employees are paid exactly the formula, so each carries a zero adjustment.
    for &i in &reference_rows {
        let a = res.adjustments.iter().find(|a| a.index == i).unwrap();
        assert!(same_to_the_cent(a.adjustment, 0.0), "reference index {i}");
    }
}

#[test]
fn lower_bound_target_pays_up_to_the_lower_bound_of_each_employees_own_interval() {
    // Noisy variant: the reference fit has a real residual variance, so the lower bound sits
    // below the midpoint. The oracle fit and interval are computed here from the cells.
    let f = FixtureF::noisy()
        .blank(1, Col::Experience)
        .blank(52, Col::Level)
        .blank(98, Col::Experience);
    let model = model(false);
    let reference_rows = f.analysed_reference(&model);
    let fit = f.reference_fit(&reference_rows);
    assert!(
        fit.sigma2 > 1.0,
        "noisy fixture must have a residual variance"
    );

    let mut req = optimization_request(f.csv_bytes(), false);
    req.range_target = Some(RangeTarget::LowerBound);
    let res = optimize_inner(req).unwrap();

    let expected: BTreeSet<usize> = f
        .analysed_target(&model)
        .into_iter()
        .filter(|&i| f.interval(&fit, i).0 - f.salary_cell(i).unwrap() > 1e-6)
        .collect();
    let got: BTreeSet<usize> = res.adjustments.iter().map(|a| a.index).collect();
    assert_eq!(got, expected, "employees below their own lower bound");
    assert!(
        expected.iter().max().copied().unwrap() >= 91,
        "employees late in the file are in the schedule"
    );

    for a in &res.adjustments {
        let i = a.index;
        let (lower, upper) = f.interval(&fit, i);
        let current = f.salary_cell(i).unwrap();
        assert!(
            same_to_the_cent(a.fair_wage, f.fair_wage(&fit, i)),
            "index {i} midpoint"
        );
        assert!(
            same_to_the_cent(a.current_wage, current),
            "index {i} current"
        );
        assert!(
            same_to_the_cent(a.fair_wage_lower_bound.unwrap(), lower),
            "index {i} lower bound {} vs oracle {}",
            a.fair_wage_lower_bound.unwrap(),
            lower
        );
        assert!(
            same_to_the_cent(a.fair_wage_upper_bound.unwrap(), upper),
            "index {i} upper bound"
        );
        // Paid up to exactly the lower bound.
        assert!(
            same_to_the_cent(a.new_wage, lower),
            "index {i} new_wage {} must be its own lower bound {}",
            a.new_wage,
            lower
        );
    }
}

#[test]
fn a_physically_blank_line_does_not_shift_an_ordinal() {
    // The browser app skips blank lines when it reads the file, so its row N is the Nth data
    // row. polars' reader turns each blank line into an all-blank ROW, which would put every
    // later employee one ordinal too high. The engine reads through `rows::read_csv`.
    let f = FixtureF::exact().blank(61, Col::Experience);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "one blank line mid-file",
            with_blank_lines(&f.csv(), &[30], 0, false),
        ),
        (
            "blank lines at several places",
            with_blank_lines(&f.csv(), &[0, 30, 61, 99], 0, false),
        ),
        (
            "trailing blank lines",
            with_blank_lines(&f.csv(), &[], 3, false),
        ),
        (
            "CRLF with blank lines",
            with_blank_lines(&f.csv(), &[30, 98], 2, true),
        ),
    ];
    for (label, csv) in cases {
        let res = optimize_inner(optimization_request(csv, false))
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_target_schedule(label, &f, &res, false, false);
        // The last target employee is ordinal 98, not 99 or 100.
        assert!(res.adjustments.iter().any(|a| a.index == 98), "{label}");
        assert_eq!(
            res.excluded_rows.len(),
            1,
            "{label}: only the blank cell is excluded"
        );
        assert_eq!(res.excluded_rows[0].index, 61, "{label}");
        assert_eq!(
            res.analysed_reference_count + res.analysed_target_count,
            99,
            "{label}"
        );
    }
}
