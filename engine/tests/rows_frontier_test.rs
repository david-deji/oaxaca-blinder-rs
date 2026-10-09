//! 0118-MERIDIAN V5: the efficient frontier pays the right employees.
//!
//! The frontier maps each `Adjustment.index` onto a slot of its pooled design. Before 0118 it
//! assigned slots by counting rows in the RAW group column, so with a blank in a row the
//! dollars landed on neighbouring employees' slots and the t statistic it reported belonged to
//! a different pay file.
//!
//! The expected t statistics are computed here, from Fixture F's cells, by a pooled
//! ordinary-least-squares fit written in plain `std` (`support::pooled_group_t`):
//!
//!   y ~ 1 + group + Experience + Level,   group = 1 for target rows
//!
//! over the analysed rows only, after paying the employees named below.

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::{frontier_request, optimization_request};
use pay_equity_engine::analysis::{calculate_efficient_frontier_inner, optimize_inner};
use support::{pooled_group_t, Col, FixtureF};

/// The noisy variant with a blank in a target row (13) and in a reference row (52).
fn fixture() -> FixtureF {
    FixtureF::noisy()
        .blank(13, Col::Experience)
        .blank(52, Col::Level)
}

/// Oracle gaps `(ordinal, fair - wage)` for the analysed target employees, fair wage from the
/// test's own reference fit.
fn oracle_gaps(f: &FixtureF) -> Vec<(usize, f64)> {
    let model = FixtureF::model_cols(false, false);
    let fit = f.reference_fit(&f.analysed_reference(&model));
    f.analysed_target(&model)
        .into_iter()
        .map(|i| (i, f.fair_wage(&fit, i) - f.salary_cell(i).unwrap()))
        .collect()
}

/// The pooled design rows `(ordinal, is_target, outcome)` after `paid` dollars per ordinal.
fn pooled_rows(f: &FixtureF, paid: &dyn Fn(usize) -> f64) -> Vec<(usize, bool, f64)> {
    let model = FixtureF::model_cols(false, false);
    let mut rows = Vec::new();
    for i in f.analysed_reference(&model) {
        rows.push((i, false, f.salary_cell(i).unwrap()));
    }
    for i in f.analysed_target(&model) {
        rows.push((i, true, f.salary_cell(i).unwrap() + paid(i)));
    }
    rows
}

/// Greedy payment at a cumulative `budget`: largest positive gaps first, each paid in full
/// until the money runs out, the last one part-paid. Returns ordinal -> dollars.
fn greedy_payments(gaps: &[(usize, f64)], budget: f64) -> std::collections::BTreeMap<usize, f64> {
    let mut positive: Vec<(usize, f64)> = gaps.iter().copied().filter(|&(_, g)| g > 0.0).collect();
    positive.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let mut remaining = budget;
    let mut paid = std::collections::BTreeMap::new();
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
fn frontier_points_match_an_independent_pooled_regression_after_paying_the_named_employees() {
    let f = fixture();
    let gaps = oracle_gaps(&f);
    let need: f64 = gaps.iter().map(|&(_, g)| g.max(0.0)).sum();
    assert!(need > 10_000.0, "fixture has real gaps to close: {need}");

    let steps = 4;
    let points =
        calculate_efficient_frontier_inner(frontier_request(f.csv_bytes(), steps, Some(need)))
            .expect("frontier failed");
    assert_eq!(points.len(), steps + 1);

    for (k, p) in points.iter().enumerate() {
        let budget = need * k as f64 / steps as f64;
        assert!(
            (p.budget - budget).abs() < 1e-6,
            "point {k} budget {}",
            p.budget
        );

        // Who is paid at this budget, from the cells.
        let paid = greedy_payments(&gaps, budget);
        let want = pooled_group_t(
            &f,
            &pooled_rows(&f, &|i| paid.get(&i).copied().unwrap_or(0.0)),
        );
        assert!(
            (p.t_statistic - want).abs() < 1e-6,
            "point {k} (budget {budget:.2}, {} employees paid): t {} vs oracle {}",
            paid.len(),
            p.t_statistic,
            want
        );
    }
    // Paying people must move the statistic: the check is not vacuous.
    assert!((points[0].t_statistic - points[steps].t_statistic).abs() > 1.0);
}

#[test]
fn the_full_budget_point_matches_the_t_statistic_after_applying_optimises_schedule_by_index() {
    let f = fixture();
    let gaps = oracle_gaps(&f);
    let need: f64 = gaps.iter().map(|&(_, g)| g.max(0.0)).sum();

    // optimise's own schedule, applied to the file by Adjustment.index.
    let schedule = optimize_inner(optimization_request(f.csv_bytes(), false)).unwrap();
    let by_index: std::collections::BTreeMap<usize, f64> = schedule
        .adjustments
        .iter()
        .map(|a| (a.index, a.adjustment))
        .collect();
    let want = pooled_group_t(
        &f,
        &pooled_rows(&f, &|i| by_index.get(&i).copied().unwrap_or(0.0)),
    );

    let points =
        calculate_efficient_frontier_inner(frontier_request(f.csv_bytes(), 4, Some(need))).unwrap();
    let last = points.last().unwrap();
    assert!(last.budget >= need - 1e-6);
    assert!(
        (last.t_statistic - want).abs() < 1e-6,
        "frontier at budget >= need: t {} vs t after applying optimise's schedule {}",
        last.t_statistic,
        want
    );
}

#[test]
fn an_interior_budget_point_pays_the_named_employees() {
    let f = fixture();
    let gaps = oracle_gaps(&f);
    let need: f64 = gaps.iter().map(|&(_, g)| g.max(0.0)).sum();
    let interior = need / 2.0;

    // The employees paid at the interior budget, named from the cells.
    let paid = greedy_payments(&gaps, interior);
    let named: Vec<&str> = paid.keys().map(|&i| f.name(i)).collect();
    assert!(named.len() >= 3 && named.len() < gaps.len(), "{named:?}");

    // optimise at that budget pays the same employees (looked up by index, then named).
    let mut req = optimization_request(f.csv_bytes(), false);
    req.budget = interior;
    let schedule = optimize_inner(req).unwrap();
    let paid_by_engine: std::collections::BTreeMap<usize, f64> = schedule
        .adjustments
        .iter()
        .filter(|a| a.adjustment > 0.0)
        .map(|a| (a.index, a.adjustment))
        .collect();
    let engine_named: Vec<&str> = paid_by_engine.keys().map(|&i| f.name(i)).collect();
    assert_eq!(
        engine_named, named,
        "the employees paid at the interior budget"
    );
    for (i, dollars) in &paid {
        let got = paid_by_engine[i];
        assert!(
            (got - dollars).abs() < 0.01,
            "{} ({i}): paid {got}, oracle {dollars}",
            f.name(*i)
        );
    }

    // The frontier point at that budget is the t statistic of exactly that pay file.
    let points =
        calculate_efficient_frontier_inner(frontier_request(f.csv_bytes(), 4, Some(need))).unwrap();
    let point = &points[2];
    assert!((point.budget - interior).abs() < 1e-6);
    let want = pooled_group_t(
        &f,
        &pooled_rows(&f, &|i| paid.get(&i).copied().unwrap_or(0.0)),
    );
    assert!(
        (point.t_statistic - want).abs() < 1e-6,
        "interior point: t {} vs oracle {}",
        point.t_statistic,
        want
    );
}

#[test]
fn the_frontier_holds_for_a_blank_in_either_group_alone() {
    for (label, f) in [
        (
            "target blank only",
            FixtureF::noisy().blank(13, Col::Experience),
        ),
        (
            "reference blank only",
            FixtureF::noisy().blank(52, Col::Level),
        ),
        (
            "target first and last",
            FixtureF::noisy()
                .blank(1, Col::Level)
                .blank(98, Col::Salary),
        ),
    ] {
        let gaps = oracle_gaps(&f);
        let need: f64 = gaps.iter().map(|&(_, g)| g.max(0.0)).sum();
        let points =
            calculate_efficient_frontier_inner(frontier_request(f.csv_bytes(), 3, Some(need)))
                .unwrap();
        for (k, p) in points.iter().enumerate() {
            let paid = greedy_payments(&gaps, need * k as f64 / 3.0);
            let want = pooled_group_t(
                &f,
                &pooled_rows(&f, &|i| paid.get(&i).copied().unwrap_or(0.0)),
            );
            assert!(
                (p.t_statistic - want).abs() < 1e-6,
                "{label}: point {k}: t {} vs oracle {}",
                p.t_statistic,
                want
            );
        }
    }
}
