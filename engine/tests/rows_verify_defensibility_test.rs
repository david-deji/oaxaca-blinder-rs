//! 0118-MERIDIAN V2 and V4: `verify_adjustments` and `check_defensibility` over a file with
//! blank cells.
//!
//! Expected figures are computed here from Fixture F's cells and formula (`tests/support`),
//! never from engine output.

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::{bare_error, optimization_request, proposed, verification_request};
use pay_equity_engine::analysis::{optimize_inner, verify_inner};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::types::ProposedAdjustment;
use std::collections::HashMap;
use support::{same_to_the_cent, Col, FixtureF};

fn model() -> Vec<Col> {
    FixtureF::model_cols(false, false)
}

// ---- V2: verify_adjustments ---------------------------------------------------------------

/// Settings V2 names. The decomposition uses the REFERENCE group's coefficients
/// (`reference_coefficients = "GroupB"`, two-fold), under which the unexplained gap is exactly
///
///   mean over analysed target employees of ( wage' - fair )     where wage' = wage + paid
///
/// i.e. minus the `mean(fair - (wage + paid))` the spec states. (The decomposition's gap runs
/// target minus reference, so an underpaid target group has a NEGATIVE unexplained gap.)
fn verify_with(
    f: &FixtureF,
    schedule: Vec<ProposedAdjustment>,
) -> pay_equity_engine::types::DecompositionResult {
    let mut req = verification_request(f.csv_bytes(), false, schedule);
    req.decomposition_params.reference_coefficients = Some("GroupB".to_string());
    req.decomposition_params.three_fold = None;
    req.decomposition_params.quantile = None;
    req.decomposition_params.bootstrap_reps = Some(5);
    verify_inner(req).expect("verify_adjustments failed")
}

/// Oracle `mean(fair - (wage + paid))` over the analysed target employees, paying each
/// employee exactly their own gap `max(0, fair - wage)`.
fn oracle_residual(f: &FixtureF, paid_to: impl Fn(usize) -> f64) -> f64 {
    let rows = f.analysed_target(&model());
    let total: f64 = rows
        .iter()
        .map(|&i| f.formula_wage(i) - (f.salary_cell(i).unwrap() + paid_to(i)))
        .sum();
    total / rows.len() as f64
}

fn own_gap(f: &FixtureF) -> impl Fn(usize) -> f64 + '_ {
    move |i| (f.formula_wage(i) - f.salary_cell(i).unwrap()).max(0.0)
}

fn schedule_from_optimize(f: &FixtureF) -> Vec<ProposedAdjustment> {
    optimize_inner(optimization_request(f.csv_bytes(), false))
        .unwrap()
        .adjustments
        .iter()
        .map(|a| proposed(a.index, a.adjustment))
        .collect()
}

#[test]
fn verifying_the_optimisers_schedule_leaves_the_oracle_unexplained_gap() {
    for (label, f) in [
        ("control", FixtureF::exact()),
        (
            "target first blank",
            FixtureF::exact().blank(1, Col::Experience),
        ),
        ("target last blank", FixtureF::exact().blank(98, Col::Level)),
        (
            "blanks in both groups",
            FixtureF::exact()
                .blank(13, Col::Salary)
                .blank(52, Col::Experience)
                .blank(1, Col::Level),
        ),
    ] {
        let schedule = schedule_from_optimize(&f);
        let res = verify_with(&f, schedule);
        let want = -oracle_residual(&f, own_gap(&f));
        assert!(
            (res.unexplained_gap - want).abs() < 1e-6,
            "{label}: unexplained_gap {} vs oracle {}",
            res.unexplained_gap,
            want
        );
        assert_eq!(res.adjustments_on_excluded_rows, 0, "{label}");
        assert_eq!(res.unresolved_row_keys, Some(0), "{label}");
    }
}

#[test]
fn lost_dollars_move_the_verified_gap() {
    // The check can fail: drop the last target employee's dollars from the schedule and the
    // verified unexplained gap must move by that employee's pay over the analysed count.
    let f = FixtureF::exact().blank(1, Col::Experience);
    let full = verify_with(&f, schedule_from_optimize(&f)).unexplained_gap;

    let mut schedule = schedule_from_optimize(&f);
    let pos = schedule
        .iter()
        .position(|a| a.index == 98)
        .expect("schedule must contain the last target employee");
    let dropped = schedule.remove(pos).value;
    assert!(same_to_the_cent(dropped, 2650.0), "{dropped}");

    let without_last = verify_with(&f, schedule).unexplained_gap;
    let analysed = f.analysed_target(&model()).len() as f64;
    assert!(
        ((full - without_last) - dropped / analysed).abs() < 1e-6,
        "dropping {dropped} over {analysed} employees moved the gap by {}",
        full - without_last
    );
}

#[test]
fn a_dollar_addressed_to_an_excluded_row_is_counted_and_not_applied() {
    let f = FixtureF::exact().blank(13, Col::Experience);
    let clean = verify_with(&f, schedule_from_optimize(&f));

    let mut poisoned_schedule = schedule_from_optimize(&f);
    poisoned_schedule.push(proposed(13, 5000.0)); // the excluded employee
    poisoned_schedule.push(proposed(500, 5000.0)); // a row that does not exist
    let poisoned = verify_with(&f, poisoned_schedule);

    assert_eq!(poisoned.adjustments_on_excluded_rows, 2);
    assert_eq!(clean.adjustments_on_excluded_rows, 0);
    // Not applied: the verified figures are exactly those of the clean schedule.
    assert_eq!(poisoned.unexplained_gap, clean.unexplained_gap);
    assert_eq!(poisoned.total_gap, clean.total_gap);
}

#[test]
fn verify_reports_which_rows_were_analysed() {
    let f = FixtureF::exact()
        .blank(13, Col::Experience)
        .blank(52, Col::Salary);
    let res = verify_with(&f, vec![]);
    assert_eq!(res.analysed_target_count, 39);
    assert_eq!(res.analysed_reference_count, 59);
    let excluded: Vec<usize> = res.excluded_rows.iter().map(|e| e.index).collect();
    assert_eq!(excluded, vec![13, 52]);
    let summary = res.data_summary.unwrap();
    assert_eq!(summary.total_count, 100);
    assert_eq!(summary.group_a_count, 59);
    assert_eq!(summary.group_b_count, 39);
}

// ---- V4: check_defensibility ------------------------------------------------------------------

fn defensibility(
    f: &FixtureF,
    adjustments: Vec<ProposedAdjustment>,
) -> Result<pay_equity_engine::types::OptimizationResult, String> {
    check_defensibility_inner(verification_request(f.csv_bytes(), false, adjustments))
}

#[test]
fn defensibility_aggregates_are_over_the_analysed_rows_and_agree_with_optimize() {
    let f = FixtureF::exact().blank(13, Col::Experience);
    let tgt = f.analysed_target(&model());
    assert_eq!(tgt.len(), 39);

    let res = defensibility(&f, vec![]).expect("defensibility failed");
    let opt = optimize_inner(optimization_request(f.csv_bytes(), false)).unwrap();

    // Oracle: sums over the 39 analysed target employees, from the cells.
    let sum_gap: f64 = tgt
        .iter()
        .map(|&i| f.formula_wage(i) - f.salary_cell(i).unwrap())
        .sum();
    let need: f64 = tgt
        .iter()
        .map(|&i| (f.formula_wage(i) - f.salary_cell(i).unwrap()).max(0.0))
        .sum();

    assert!(
        (res.required_budget - need).abs() < 0.05,
        "{}",
        res.required_budget
    );
    assert!(
        (opt.required_budget - need).abs() < 0.05,
        "{}",
        opt.required_budget
    );
    // `required_budget` is the same figure from both entry points.
    assert!((res.required_budget - opt.required_budget).abs() < 0.05);

    // The divisor is the analysed target count (39), not the raw count (40).
    assert_eq!(res.analysed_target_count, 39);
    let want = sum_gap / 39.0;
    assert!(
        (res.original_unexplained_gap - want).abs() < 1e-6,
        "{} vs oracle {}",
        res.original_unexplained_gap,
        want
    );
    assert!(
        (res.original_unexplained_gap - sum_gap / 40.0).abs() > 1.0,
        "a divisor of 40 would give a different figure"
    );

    // Same magnitude as optimize's. The sign convention differs between the two entry points
    // and has since before 0118: defensibility reports mean(fair - wage), optimize reports
    // -mean(fair - wage).
    assert!(
        (res.original_unexplained_gap + opt.original_unexplained_gap).abs() < 1e-6,
        "defensibility {} vs optimize {}",
        res.original_unexplained_gap,
        opt.original_unexplained_gap
    );
}

#[test]
fn defensibility_gap_means_are_over_analysed_rows_of_both_groups() {
    // Raw row 52 is a REFERENCE employee with a wage but a blank Experience; 13 is a target
    // employee with the same kind of blank. The gap means (`original_gap`, `new_gap`) must be
    // taken over the analysed rows of each group, so the excluded reference wage cannot sit in
    // the reference mean.
    let mm = model();
    for (label, f, want_ref, want_tgt) in [
        (
            "reference blank only",
            FixtureF::exact().blank(52, Col::Experience),
            59usize,
            40usize,
        ),
        (
            "reference blank beside a target blank",
            FixtureF::exact()
                .blank(13, Col::Experience)
                .blank(52, Col::Experience),
            59,
            39,
        ),
    ] {
        let ref_rows = f.analysed_reference(&mm);
        let tgt_rows = f.analysed_target(&mm);
        assert_eq!(
            (ref_rows.len(), tgt_rows.len()),
            (want_ref, want_tgt),
            "{label}"
        );

        let mean = |rows: &[usize]| -> f64 {
            rows.iter().map(|&i| f.salary_cell(i).unwrap()).sum::<f64>() / rows.len() as f64
        };
        // Pay the last target employee (98) their own gap, so `new_gap` moves.
        let paid_98 = f.formula_wage(98) - f.salary_cell(98).unwrap();
        assert!(paid_98 > 1000.0, "{label}: {paid_98}");
        let want_orig = mean(&ref_rows) - mean(&tgt_rows);
        let want_new = mean(&ref_rows) - (mean(&tgt_rows) + paid_98 / tgt_rows.len() as f64);

        // Without the exclusion the reference mean would include row 52's wage.
        let raw_ref: Vec<usize> = f.reference_ordinals();
        assert!((mean(&raw_ref) - mean(&ref_rows)).abs() > 1.0, "{label}");

        let res = defensibility(&f, vec![proposed(98, paid_98)]).unwrap();
        assert!(
            (res.original_gap - want_orig).abs() < 1e-6,
            "{label}: original_gap {} vs oracle {}",
            res.original_gap,
            want_orig
        );
        assert!(
            (res.new_gap - want_new).abs() < 1e-6,
            "{label}: new_gap {} vs oracle {}",
            res.new_gap,
            want_new
        );
        assert!(
            (res.original_gap - res.new_gap - paid_98 / tgt_rows.len() as f64).abs() < 1e-6,
            "{label}: new_gap must move by the dollars paid over the analysed target count"
        );
        assert_eq!(res.analysed_reference_count, want_ref, "{label}");
        assert_eq!(res.analysed_target_count, want_tgt, "{label}");
        let excluded: Vec<usize> = res.excluded_rows.iter().map(|e| e.index).collect();
        assert!(excluded.contains(&52), "{label}");
        // The unexplained gap is still over the analysed targets only.
        let sum_gap: f64 = tgt_rows
            .iter()
            .map(|&i| f.formula_wage(i) - f.salary_cell(i).unwrap())
            .sum();
        assert!(
            (res.original_unexplained_gap - sum_gap / tgt_rows.len() as f64).abs() < 1e-6,
            "{label}"
        );
    }
}

#[test]
fn defensibility_scores_the_last_employee_and_one_after_the_blank_against_their_own_fair_wage() {
    let f = FixtureF::exact().blank(13, Col::Experience);
    // 98 is the last target employee; 18 is a target employee after the blank (13), underpaid
    // by exactly 1550 (see `support::shortfall`, k = 7).
    let res = defensibility(&f, vec![proposed(98, 2650.0), proposed(18, 0.0)]).unwrap();
    assert_eq!(res.adjustments.len(), 2);

    for adj in &res.adjustments {
        let i = adj.index;
        let fair = f.formula_wage(i);
        let current = f.salary_cell(i).unwrap();
        assert!(
            same_to_the_cent(adj.fair_wage, fair),
            "index {i} ({}): fair {}",
            f.name(i),
            adj.fair_wage
        );
        assert!(
            same_to_the_cent(adj.current_wage, current),
            "index {i}: current {}",
            adj.current_wage
        );
    }
    let last = res.adjustments.iter().find(|a| a.index == 98).unwrap();
    // 98 is paid up to its fair wage, so it is defensible.
    assert!(same_to_the_cent(last.new_wage, f.formula_wage(98)));
    assert_eq!(last.is_defensible, Some(true));

    let after = res.adjustments.iter().find(|a| a.index == 18).unwrap();
    // 18 is underpaid by 1550 and given nothing: not defensible, and the message says by how
    // much (the reference fit is exact, so the lower bound is the fair wage itself).
    assert_eq!(after.is_defensible, Some(false));
    let message = after.defensibility_message.as_deref().unwrap();
    assert!(message.contains("1550.00"), "{message}");

    // The new unexplained gap reflects exactly the 2650 paid to employee 98, over 39 employees.
    let moved = res.original_unexplained_gap - res.new_unexplained_gap;
    assert!((moved - 2650.0 / 39.0).abs() < 1e-6, "{moved}");
    assert_eq!(res.adjustments_on_excluded_rows, 0);
}

#[test]
fn an_adjustment_addressed_to_the_excluded_row_is_counted() {
    let f = FixtureF::exact().blank(13, Col::Experience);
    let res = defensibility(&f, vec![proposed(13, 1000.0), proposed(16, 0.0)]).unwrap();
    assert_eq!(res.adjustments_on_excluded_rows, 1);
    assert!(
        res.adjustments.iter().all(|a| a.index != 13),
        "the excluded employee must not be scored"
    );
    assert_eq!(res.adjustments.len(), 1);
    assert_eq!(
        res.total_cost, 0.0,
        "the 1000 addressed to row 13 is not spent"
    );
}

#[test]
fn a_predictor_override_that_fills_the_blank_brings_the_row_back() {
    let f = FixtureF::exact().blank(13, Col::Experience);
    let mut overrides = HashMap::new();
    overrides.insert("Experience".to_string(), "7".to_string());
    let res = defensibility(
        &f,
        vec![ProposedAdjustment {
            index: 13,
            row_key: None,
            value: 0.0,
            predictor_overrides: Some(overrides),
        }],
    )
    .unwrap();

    // Row 13 is analysed now: no exclusion, and it is scored against the fair wage of its
    // OVERRIDDEN Experience (7) and its own Level.
    assert!(res.excluded_rows.is_empty());
    assert_eq!(res.analysed_target_count, 40);
    assert_eq!(res.adjustments_on_excluded_rows, 0);
    assert_eq!(res.adjustments.len(), 1);
    let adj = &res.adjustments[0];
    assert_eq!(adj.index, 13);
    let want_fair = 30000.0 + 1000.0 * 7.0 + 4000.0 * f.rows[13].level as f64;
    assert!(
        same_to_the_cent(adj.fair_wage, want_fair),
        "{} vs {}",
        adj.fair_wage,
        want_fair
    );
    assert!(same_to_the_cent(
        adj.current_wage,
        f.salary_cell(13).unwrap()
    ));
}

#[test]
fn a_non_numeric_predictor_cell_is_a_named_error_wherever_it_sits() {
    // "N/A" cannot be cast to a number. Before 0118 the defensibility entry point alone turned
    // it into a silent blank (its cast lacked the null-count guard the other entry points
    // have) and dropped the employee, shifting everything after them.
    let early = FixtureF::exact().cell_text(3, Col::Experience, "N/A");
    let last = FixtureF::exact().cell_text(99, Col::Experience, "N/A");
    let a = defensibility(&early, vec![]).unwrap_err();
    let b = defensibility(&last, vec![]).unwrap_err();
    assert_eq!(a, "Column 'Experience' contains non-numeric data.");
    assert_eq!(a, b, "the same error whether the cell is early or last");
    // And it reads the same through the other surfaces' wording for the same input family.
    let opt = optimize_inner(optimization_request(early.csv_bytes(), false)).unwrap_err();
    assert!(
        bare_error(&opt).contains("'Experience' contains non-numeric data"),
        "{opt}"
    );
}
