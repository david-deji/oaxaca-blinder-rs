//! 0122-MERIDIAN D6 / T10 / F-11 / F-12: the efficient frontier follows the remedy on the screen.
//!
//! Each point of the curve is the pooled regression with a compared-group indicator on the wages
//! the remedy's schedule produces at that point's budget. The oracle is `optimize` (the schedule at
//! that budget) refitted by a plain-std OLS of Fixture F's own cells (`support::ols`, no engine
//! code), and `check_defensibility`'s group test on the same schedule (a second engine statement).
//!
//! Each setting is also tried ALONE against the default curve, so a request field that is accepted
//! but ignored (the frontier used to hard-code Reference / Greedy / budget 0 / no threshold) turns
//! a gate red on its own, and not only in combination with another field that happens to differ.

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::{decomposition_request, optimization_request};
use pay_equity_engine::analysis::{calculate_efficient_frontier_inner, optimize_inner};
use pay_equity_engine::defensibility::check_defensibility_on;
use pay_equity_engine::types::*;
use support::FixtureF;

#[derive(Clone, Copy)]
struct Settings {
    strategy: AllocationStrategy,
    target: OptimizationTarget,
    range: RangeTarget,
    min_pct: f64,
    both: bool,
}

const BASE: Settings = Settings {
    strategy: AllocationStrategy::Greedy,
    target: OptimizationTarget::Reference,
    range: RangeTarget::Midpoint,
    min_pct: 0.0,
    both: false,
};

fn frontier(s: Settings, steps: usize, max_budget: Option<f64>) -> Vec<FrontierPoint> {
    calculate_efficient_frontier_inner(EfficientFrontierRequest {
        decomposition_params: decomposition_request(FixtureF::noisy().csv_bytes(), false),
        steps: Some(steps),
        max_budget,
        confidence_level: None,
        strategy: Some(s.strategy),
        target: Some(s.target),
        range_target: Some(s.range),
        min_gap_pct: Some(s.min_pct),
        adjust_both_groups: Some(s.both),
    })
    .unwrap()
}

fn remedy(s: Settings, budget: f64) -> OptimizationResult {
    let mut req = optimization_request(FixtureF::noisy().csv_bytes(), false);
    req.budget = budget;
    req.strategy = Some(s.strategy);
    req.target = Some(s.target);
    req.range_target = Some(s.range);
    req.min_gap_pct = Some(s.min_pct);
    req.adjust_both_groups = Some(s.both);
    optimize_inner(req).unwrap()
}

/// Plain-std pooled indicator fit of Fixture F's salaries after `pay` (ordinal -> dollars):
/// `(coefficient, t)`.
fn oracle(pay: &std::collections::HashMap<usize, f64>) -> (f64, f64) {
    let f = FixtureF::noisy();
    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut rows = Vec::new();
    for i in 0..support::N_ROWS {
        let w = f.salary_cell(i).unwrap() + pay.get(&i).copied().unwrap_or(0.0);
        x.push(vec![
            1.0,
            if support::is_target_row(i) { 1.0 } else { 0.0 },
            f.rows[i].experience as f64,
            f.rows[i].level as f64,
        ]);
        y.push(w);
        rows.push((i, support::is_target_row(i), w));
    }
    let fit = support::ols(&x, &y);
    (fit.beta[1], support::pooled_group_t(&f, &rows))
}

fn schedule_of(r: &OptimizationResult) -> std::collections::HashMap<usize, f64> {
    r.adjustments
        .iter()
        .filter(|a| a.adjustment != 0.0)
        .map(|a| (a.index, a.adjustment))
        .collect()
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * 1.0_f64.max(a.abs()).max(b.abs())
}

/// One setting changed from the default, so a field the frontier ignores is caught alone.
fn variants() -> Vec<(&'static str, Settings)> {
    vec![
        (
            "strategy alone",
            Settings {
                strategy: AllocationStrategy::Equitable,
                ..BASE
            },
        ),
        (
            "target alone",
            Settings {
                target: OptimizationTarget::Pooled,
                ..BASE
            },
        ),
        (
            "lower bound alone",
            Settings {
                range: RangeTarget::LowerBound,
                ..BASE
            },
        ),
        (
            "upper bound alone",
            Settings {
                range: RangeTarget::UpperBound,
                ..BASE
            },
        ),
        (
            "threshold alone",
            Settings {
                min_pct: 0.05,
                ..BASE
            },
        ),
        ("reference raises alone", Settings { both: true, ..BASE }),
        (
            "everything at once",
            Settings {
                strategy: AllocationStrategy::Equitable,
                target: OptimizationTarget::Pooled,
                range: RangeTarget::UpperBound,
                min_pct: 0.02,
                both: true,
            },
        ),
    ]
}

#[test]
fn every_point_is_the_pooled_fit_of_the_schedule_the_remedy_pays_at_that_budget() {
    for (label, s) in std::iter::once(("default", BASE)).chain(variants()) {
        let points = frontier(s, 10, None);
        assert_eq!(points.len(), 11, "{label}");
        assert_eq!(points[0].budget, 0.0, "{label}");
        for (k, p) in points.iter().enumerate() {
            // The schedule the remedy pays at this budget (0 is no schedule at all).
            let r = if p.budget > 0.0 {
                remedy(s, p.budget)
            } else {
                remedy(s, 1e-12)
            };
            let pay = schedule_of(&r);
            let (gamma, t) = oracle(&pay);
            assert!(
                near(p.group_coefficient, gamma, 1e-7),
                "{label} point {k} (budget {}): frontier {} vs refit {gamma}",
                p.budget,
                p.group_coefficient
            );
            assert!(
                near(p.t_statistic, t, 1e-7),
                "{label} point {k}: t {} vs {t}",
                p.t_statistic
            );
            // A second statement: check_defensibility's group test on the very same schedule.
            let sched: Vec<ProposedAdjustment> = pay
                .iter()
                .map(|(&index, &value)| ProposedAdjustment {
                    index,
                    row_key: None,
                    value,
                    predictor_overrides: None,
                })
                .collect();
            let d = check_defensibility_on(
                VerificationRequest {
                    decomposition_params: decomposition_request(
                        FixtureF::noisy().csv_bytes(),
                        false,
                    ),
                    adjustments: sched,
                    confidence_level: None,
                },
                &BASE.target,
            )
            .unwrap();
            let g = d.group_test.unwrap();
            assert!(
                near(g.group_coefficient, p.group_coefficient, 1e-7),
                "{label} point {k}"
            );
            assert!(
                near(g.t_statistic, p.t_statistic, 1e-7),
                "{label} point {k}"
            );
            assert!(near(g.p_value, p.p_value, 1e-6), "{label} point {k}");
            assert_eq!(g.degrees_of_freedom, p.degrees_of_freedom, "{label}");
        }
    }
}

#[test]
fn the_budget_axis_ends_where_the_remedy_stops_spending() {
    // F-11: it used to run 10 % past the need, a flat tail.
    for (label, s) in std::iter::once(("default", BASE)).chain(variants()) {
        let full = remedy(s, 0.0).total_cost;
        let points = frontier(s, 25, None);
        assert_eq!(points.len(), 26, "{label}");
        assert_eq!(
            points.last().unwrap().budget,
            full,
            "{label}: the axis must end at the remedy's full cost"
        );
        // And the last point is the remedy paid in full.
        let (gamma, _) = oracle(&schedule_of(&remedy(s, 0.0)));
        assert!(
            near(points.last().unwrap().group_coefficient, gamma, 1e-7),
            "{label}"
        );
        // Strictly rising budgets.
        for w in points.windows(2) {
            assert!(w[1].budget > w[0].budget, "{label}");
        }
    }
    // An explicit `max_budget` is still honoured.
    let points = frontier(BASE, 4, Some(8_000.0));
    assert_eq!(points.last().unwrap().budget, 8_000.0);
}

#[test]
fn each_setting_alone_changes_the_curve() {
    // Same axis for every curve, so the comparison is point for point. It runs past the compared
    // group's need (51,030) so that the reference raises, paid last under Greedy, are reached.
    let base = frontier(BASE, 10, Some(54_000.0));
    for (label, s) in variants() {
        let curve = frontier(s, 10, Some(54_000.0));
        let widest = base
            .iter()
            .zip(&curve)
            .map(|(a, b)| (a.group_coefficient - b.group_coefficient).abs())
            .fold(0.0_f64, f64::max);
        assert!(
            widest > 1.0,
            "{label}: the curve is the default curve (largest difference {widest}); the request \
             field is accepted and ignored"
        );
    }
    // Equitable alone, at half its budget, is not Greedy's point.
    let eq = frontier(
        Settings {
            strategy: AllocationStrategy::Equitable,
            ..BASE
        },
        2,
        Some(30_000.0),
    );
    let gr = frontier(BASE, 2, Some(30_000.0));
    assert!((eq[1].group_coefficient - gr[1].group_coefficient).abs() > 1.0);
}

#[test]
fn equal_shortfalls_are_paid_in_the_order_optimize_pays_them() {
    // Three compared employees short by exactly 2000 and one short by 3000, on an exact line. At a
    // budget that stops in the middle of the tie, the frontier must have paid the same people as
    // `optimize` (compared rows in row order), or its coefficient is another schedule's.
    let csv = "wage,group,x\n40000,R,0\n45000,R,5\n50000,R,10\n41000,R,1\n\
               40000,T,2\n44000,T,6\n43000,T,3\n40000,T,2\n49000,T,12\n"
        .as_bytes()
        .to_vec();
    let decomposition = || DecompositionRequest {
        csv_data: csv.clone(),
        outcome_variable: "wage".into(),
        group_variable: "group".into(),
        reference_group: "R".into(),
        predictors: vec!["x".into()],
        categorical_predictors: None,
        three_fold: None,
        quantile: None,
        reference_coefficients: Some("Pooled".into()),
        bootstrap_reps: Some(2),
    };
    for target in [OptimizationTarget::Reference, OptimizationTarget::Pooled] {
        for budget in [1_000.0, 2_500.0, 4_000.0, 6_500.0] {
            let pts = calculate_efficient_frontier_inner(EfficientFrontierRequest {
                decomposition_params: decomposition(),
                steps: Some(1),
                max_budget: Some(budget),
                confidence_level: None,
                strategy: None,
                target: Some(target),
                range_target: None,
                min_gap_pct: None,
                adjust_both_groups: None,
            })
            .unwrap();
            let opt = optimize_inner(OptimizationRequest {
                csv_data: csv.clone(),
                outcome_variable: "wage".into(),
                group_variable: "group".into(),
                reference_group: "R".into(),
                predictors: vec!["x".into()],
                categorical_predictors: None,
                budget,
                target_gap: None,
                target: Some(target),
                strategy: Some(AllocationStrategy::Greedy),
                min_gap_pct: None,
                forensic_mode: None,
                adjust_both_groups: None,
                confidence_level: None,
                range_target: None,
            })
            .unwrap();
            let sched = opt
                .adjustments
                .iter()
                .filter(|a| a.adjustment != 0.0)
                .map(|a| ProposedAdjustment {
                    index: a.index,
                    row_key: None,
                    value: a.adjustment,
                    predictor_overrides: None,
                })
                .collect();
            let d = check_defensibility_on(
                VerificationRequest {
                    decomposition_params: decomposition(),
                    adjustments: sched,
                    confidence_level: None,
                },
                &target,
            )
            .unwrap();
            let g = d.group_test.unwrap();
            assert!(
                near(pts[1].group_coefficient, g.group_coefficient, 1e-9),
                "{target:?} budget {budget}: frontier {} vs optimize's schedule {}",
                pts[1].group_coefficient,
                g.group_coefficient
            );
        }
    }
}

#[test]
fn a_tie_between_a_compared_and_a_reference_employee_is_paid_compared_first() {
    // F-12: `optimize` sorts by shortfall and keeps compared rows ahead of reference rows on a tie.
    // Here a compared employee and two reference employees share the identical row (x = 2, wage
    // 118), so their shortfalls to the reference line are equal to the last bit. Reference rows come
    // FIRST in the file, so a pay order by row number would pay a reference employee ahead of the
    // compared one. At a budget that stops inside the tie the two orders buy different schedules
    // and different group coefficients.
    let csv = "wage,group,x\n118,R,2\n100,R,0\n112,R,1\n128,R,3\n141,R,4\n150,R,5\n118,R,2\n\
               118,T,2\n170,T,6\n175,T,7\n126,T,3\n148,T,5\n"
        .as_bytes()
        .to_vec();
    let decomposition = || DecompositionRequest {
        csv_data: csv.clone(),
        outcome_variable: "wage".into(),
        group_variable: "group".into(),
        reference_group: "R".into(),
        predictors: vec!["x".into()],
        categorical_predictors: None,
        three_fold: None,
        quantile: None,
        reference_coefficients: Some("Pooled".into()),
        bootstrap_reps: Some(2),
    };
    let optimize = |budget: f64| {
        optimize_inner(OptimizationRequest {
            csv_data: csv.clone(),
            outcome_variable: "wage".into(),
            group_variable: "group".into(),
            reference_group: "R".into(),
            predictors: vec!["x".into()],
            categorical_predictors: None,
            budget,
            target_gap: None,
            target: Some(OptimizationTarget::Reference),
            strategy: Some(AllocationStrategy::Greedy),
            min_gap_pct: None,
            forensic_mode: None,
            adjust_both_groups: Some(true),
            confidence_level: None,
            range_target: None,
        })
        .unwrap()
    };
    // 3.55 (the largest shortfall) + 1.59 (the tied pair) + a little into the next of the tie.
    let budget = 5.94;
    let opt = optimize(budget);
    let paid: Vec<(usize, f64)> = opt
        .adjustments
        .iter()
        .filter(|a| a.adjustment > 0.0)
        .map(|a| (a.index, a.adjustment))
        .collect();
    // optimize pays the compared copy (row 7) in full before either reference copy (rows 0, 6).
    assert!(
        paid.iter().any(|&(i, v)| i == 7 && v > 1.58),
        "optimize's own order changed: {paid:?}"
    );
    let sched = opt
        .adjustments
        .iter()
        .filter(|a| a.adjustment != 0.0)
        .map(|a| ProposedAdjustment {
            index: a.index,
            row_key: None,
            value: a.adjustment,
            predictor_overrides: None,
        })
        .collect();
    let d = check_defensibility_on(
        VerificationRequest {
            decomposition_params: decomposition(),
            adjustments: sched,
            confidence_level: None,
        },
        &OptimizationTarget::Reference,
    )
    .unwrap();
    let pts = calculate_efficient_frontier_inner(EfficientFrontierRequest {
        decomposition_params: decomposition(),
        steps: Some(1),
        max_budget: Some(budget),
        confidence_level: None,
        strategy: None,
        target: Some(OptimizationTarget::Reference),
        range_target: None,
        min_gap_pct: None,
        adjust_both_groups: Some(true),
    })
    .unwrap();
    assert!(
        near(
            pts[1].group_coefficient,
            d.group_test.unwrap().group_coefficient,
            1e-9
        ),
        "the frontier paid the tie in another order than optimize"
    );
}

#[test]
fn a_roster_with_nothing_to_pay_returns_the_single_baseline_point() {
    let csv = "wage,group,x\n40000,R,0\n45000,R,5\n50000,R,10\n43000,T,2\n47000,T,6\n53000,T,12\n"
        .as_bytes()
        .to_vec();
    let pts = calculate_efficient_frontier_inner(EfficientFrontierRequest {
        decomposition_params: DecompositionRequest {
            csv_data: csv,
            outcome_variable: "wage".into(),
            group_variable: "group".into(),
            reference_group: "R".into(),
            predictors: vec!["x".into()],
            categorical_predictors: None,
            three_fold: None,
            quantile: None,
            reference_coefficients: Some("Pooled".into()),
            bootstrap_reps: Some(2),
        },
        steps: Some(5),
        max_budget: None,
        confidence_level: None,
        strategy: None,
        target: None,
        range_target: None,
        min_gap_pct: None,
        adjust_both_groups: None,
    })
    .unwrap();
    assert_eq!(pts.len(), 1);
    assert_eq!(pts[0].budget, 0.0);
}
