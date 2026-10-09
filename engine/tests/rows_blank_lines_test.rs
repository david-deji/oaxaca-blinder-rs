//! 0118-MERIDIAN: physically blank lines in the CSV, through every engine entry point.
//!
//! The browser app reads the file with `skipEmptyLines: true`, so its row N is the Nth data row.
//! polars' reader would turn each blank line into an all-blank row and push every later employee
//! one ordinal up. Every entry point reads through `rows::read_csv`; `verify_adjustments` and
//! `check_defensibility` take `Adjustment.index` from the app's `csvData`, so a blank line that
//! one of them failed to skip would name the neighbour. These tests run the same cases through
//! decompose, verify, defensibility and the frontier (optimize has its own in `rows_optimize_test`).
//!
//! Every expected figure comes from Fixture F's cells and formula, never from engine output.

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::{
    decomposition_request, frontier_request, proposed, verification_request, with_blank_lines,
};
use pay_equity_engine::analysis::{
    calculate_efficient_frontier_inner, decompose_inner, verify_inner,
};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::types::ProposedAdjustment;
use std::collections::BTreeMap;
use support::{pooled_group_t, Col, FixtureF};

fn model() -> Vec<Col> {
    FixtureF::model_cols(false, false)
}

/// The shapes the app can hand over. The cell blank on raw row 61 (a target employee) makes the
/// excluded ordinal observable: it must stay 61 in every variant.
fn variants(csv: &str) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "one blank line mid-file",
            with_blank_lines(csv, &[30], 0, false),
        ),
        (
            "blank lines at several places",
            with_blank_lines(csv, &[0, 30, 61, 99], 0, false),
        ),
        ("trailing blank lines", with_blank_lines(csv, &[], 3, false)),
        (
            "CRLF with blank lines",
            with_blank_lines(csv, &[30, 98], 2, true),
        ),
        (
            "blank line directly before a blank cell",
            with_blank_lines(csv, &[61], 0, false),
        ),
    ]
}

fn base() -> FixtureF {
    FixtureF::exact().blank(61, Col::Experience)
}

#[test]
fn decompose_skips_blank_lines() {
    let f = base();
    let mm = model();
    let want_a = f.analysed_reference(&mm);
    let want_b = f.analysed_target(&mm);
    assert_eq!((want_a.len(), want_b.len()), (60, 39));
    let mean = |rows: &[usize]| -> f64 {
        rows.iter().map(|&i| f.salary_cell(i).unwrap()).sum::<f64>() / rows.len() as f64
    };

    for (label, csv) in variants(&f.csv()) {
        let res = decompose_inner(decomposition_request(csv, false))
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        let excluded: Vec<usize> = res.excluded_rows.iter().map(|e| e.index).collect();
        assert_eq!(excluded, vec![61], "{label}: the literal excluded ordinal");
        assert_eq!(res.analysed_reference_count, 60, "{label}");
        assert_eq!(res.analysed_target_count, 39, "{label}");
        let s = res.data_summary.unwrap();
        assert_eq!(s.total_count, 100, "{label}: blank lines are not rows");
        assert_eq!((s.group_a_count, s.group_b_count), (60, 39), "{label}");
        assert!((s.group_a_mean - mean(&want_a)).abs() < 1e-6, "{label}");
        assert!((s.group_b_mean - mean(&want_b)).abs() < 1e-6, "{label}");
    }
}

/// The schedule that pays every analysed target employee their own gap, named by the ordinal in
/// the fixture's own (blank-line-free) numbering.
fn own_gap_schedule(f: &FixtureF) -> Vec<ProposedAdjustment> {
    f.analysed_target(&model())
        .into_iter()
        .map(|i| {
            let gap = f.formula_wage(i) - f.salary_cell(i).unwrap();
            proposed(i, gap.max(0.0))
        })
        .collect()
}

#[test]
fn verify_skips_blank_lines_and_pays_the_named_employees() {
    let f = base();
    let schedule = own_gap_schedule(&f);
    assert!(schedule.iter().any(|a| a.index == 98), "98 is analysed");

    // Oracle: paying every analysed target employee their own gap leaves, under the reference
    // coefficients, an unexplained gap of mean(wage + paid - fair) over the analysed targets.
    let rows = f.analysed_target(&model());
    let want: f64 = rows
        .iter()
        .map(|&i| {
            let paid = schedule.iter().find(|a| a.index == i).unwrap().value;
            f.salary_cell(i).unwrap() + paid - f.formula_wage(i)
        })
        .sum::<f64>()
        / rows.len() as f64;

    // A schedule that pays only the last target employee: the gap moves by exactly that
    // employee's pay over the analysed count, and only if index 98 names employee 98.
    let own_98 = f.formula_wage(98) - f.salary_cell(98).unwrap();
    assert!(own_98 > 1000.0, "{own_98}");

    for (label, csv) in variants(&f.csv()) {
        let run = |adjustments: Vec<ProposedAdjustment>| {
            let mut req = verification_request(csv.clone(), false, adjustments);
            req.decomposition_params.reference_coefficients = Some("GroupB".to_string());
            req.decomposition_params.bootstrap_reps = Some(5);
            verify_inner(req).unwrap_or_else(|e| panic!("{label}: {e}"))
        };

        let full = run(schedule
            .iter()
            .map(|a| proposed(a.index, a.value))
            .collect());
        assert!(
            (full.unexplained_gap - want).abs() < 1e-6,
            "{label}: unexplained gap {} vs oracle {}",
            full.unexplained_gap,
            want
        );
        assert_eq!(full.adjustments_on_excluded_rows, 0, "{label}");
        let excluded: Vec<usize> = full.excluded_rows.iter().map(|e| e.index).collect();
        assert_eq!(excluded, vec![61], "{label}");
        assert_eq!(full.analysed_reference_count, 60, "{label}");
        assert_eq!(full.analysed_target_count, 39, "{label}");

        let none = run(vec![]);
        let only_98 = run(vec![proposed(98, own_98)]);
        let moved = only_98.unexplained_gap - none.unexplained_gap;
        assert!(
            (moved - own_98 / 39.0).abs() < 1e-6,
            "{label}: paying employee 98 moved the gap by {moved}, oracle {}",
            own_98 / 39.0
        );
        assert_eq!(only_98.adjustments_on_excluded_rows, 0, "{label}");
    }
}

#[test]
fn defensibility_skips_blank_lines_and_scores_the_named_employees() {
    let f = base();
    for (label, csv) in variants(&f.csv()) {
        // 98: the last target employee. 66: a target employee after the blank cell (61).
        // 18: before it.
        let res = check_defensibility_inner(verification_request(
            csv,
            false,
            vec![proposed(98, 0.0), proposed(66, 0.0), proposed(18, 0.0)],
        ))
        .unwrap_or_else(|e| panic!("{label}: {e}"));

        let excluded: Vec<usize> = res.excluded_rows.iter().map(|e| e.index).collect();
        assert_eq!(excluded, vec![61], "{label}");
        assert_eq!(res.analysed_reference_count, 60, "{label}");
        assert_eq!(res.analysed_target_count, 39, "{label}");
        assert_eq!(res.adjustments_on_excluded_rows, 0, "{label}");
        assert_eq!(res.adjustments.len(), 3, "{label}");
        for adj in &res.adjustments {
            let i = adj.index;
            assert!(
                support::same_to_the_cent(adj.fair_wage, f.formula_wage(i)),
                "{label}: index {i} ({}) fair {} vs oracle {}",
                f.name(i),
                adj.fair_wage,
                f.formula_wage(i)
            );
            assert!(
                support::same_to_the_cent(adj.current_wage, f.salary_cell(i).unwrap()),
                "{label}: index {i} current {}",
                adj.current_wage
            );
        }
        // And the oracle wage of the last target employee, literally.
        let last = res.adjustments.iter().find(|a| a.index == 98).unwrap();
        assert!(
            support::same_to_the_cent(last.current_wage, f.salary_cell(98).unwrap()),
            "{label}"
        );
    }
}

// ---- frontier ---------------------------------------------------------------------------

fn frontier_fixture() -> FixtureF {
    FixtureF::noisy()
        .blank(61, Col::Experience)
        .blank(52, Col::Level)
}

fn oracle_gaps(f: &FixtureF) -> Vec<(usize, f64)> {
    let mm = model();
    let fit = f.reference_fit(&f.analysed_reference(&mm));
    f.analysed_target(&mm)
        .into_iter()
        .map(|i| (i, f.fair_wage(&fit, i) - f.salary_cell(i).unwrap()))
        .collect()
}

fn greedy_payments(gaps: &[(usize, f64)], budget: f64) -> BTreeMap<usize, f64> {
    let mut positive: Vec<(usize, f64)> = gaps.iter().copied().filter(|&(_, g)| g > 0.0).collect();
    positive.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let mut remaining = budget;
    let mut paid = BTreeMap::new();
    for (i, gap) in positive {
        if remaining <= 0.0 {
            break;
        }
        let pay = gap.min(remaining);
        paid.insert(i, pay);
        remaining -= pay;
    }
    paid
}

#[test]
fn the_frontier_skips_blank_lines_and_pays_the_named_employees() {
    let f = frontier_fixture();
    let mm = model();
    let gaps = oracle_gaps(&f);
    let need: f64 = gaps.iter().map(|&(_, g)| g.max(0.0)).sum();
    assert!(need > 10_000.0, "{need}");
    let steps = 4;

    for (label, csv) in variants(&f.csv()) {
        let points = calculate_efficient_frontier_inner(frontier_request(csv, steps, Some(need)))
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(points.len(), steps + 1, "{label}");
        for (k, p) in points.iter().enumerate() {
            let budget = need * k as f64 / steps as f64;
            let paid = greedy_payments(&gaps, budget);
            let mut rows = Vec::new();
            for i in f.analysed_reference(&mm) {
                rows.push((i, false, f.salary_cell(i).unwrap()));
            }
            for i in f.analysed_target(&mm) {
                rows.push((
                    i,
                    true,
                    f.salary_cell(i).unwrap() + paid.get(&i).copied().unwrap_or(0.0),
                ));
            }
            let want = pooled_group_t(&f, &rows);
            assert!(
                (p.t_statistic - want).abs() < 1e-6,
                "{label}: point {k}: t {} vs oracle {}",
                p.t_statistic,
                want
            );
        }
    }
}
