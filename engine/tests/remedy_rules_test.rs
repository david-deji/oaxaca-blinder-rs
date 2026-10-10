//! 0122-MERIDIAN: the remedy's budget rule, its refusals, and its figures against oracles that
//! share no code with the engine.
//!
//! Oracles: closed forms worked from the six-row roster's cells (`0122-remedy-tiny.csv`, an exact
//! reference line, so every fair wage is a number read off the file); a brute-force enumeration of
//! every allocation on a 250-dollar grid (`verification/gen_remedy_bruteforce.py`); a plain-std OLS
//! of Fixture F's own cells (`support::ols`, no engine code); a plain-std allocator written in this
//! file. The R oracle lives in `remedy_oracle_test`.
//!
//! Gates: V1 (target-gap rule), V2 (best reachable gap, in the oracle test), V5 (closure), V6
//! (unfunded and threshold counts), V7 (both entry points on one schedule), V11 (refusals), F-17
//! (no signed zero), D5 (no statute cited on a remedy surface).

#[path = "support/engine_requests.rs"]
mod engine_requests;
mod support;

use engine_requests::optimization_request;
use pay_equity_engine::analysis::optimize_inner;
use pay_equity_engine::defensibility::check_defensibility_on;
use pay_equity_engine::types::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use support::FixtureF;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn tiny_csv() -> Vec<u8> {
    std::fs::read(root().join("engine/tests/fixtures/0122-remedy-tiny.csv")).unwrap()
}

fn tiny(edit: &dyn Fn(&mut OptimizationRequest)) -> Result<OptimizationResult, String> {
    let mut req = OptimizationRequest {
        csv_data: tiny_csv(),
        outcome_variable: "wage".into(),
        group_variable: "group".into(),
        reference_group: "R".into(),
        predictors: vec!["x".into()],
        categorical_predictors: None,
        budget: 0.0,
        target_gap: None,
        target: Some(OptimizationTarget::Reference),
        strategy: Some(AllocationStrategy::Greedy),
        min_gap_pct: Some(0.0),
        forensic_mode: Some(false),
        adjust_both_groups: Some(false),
        confidence_level: Some(0.95),
        range_target: Some(RangeTarget::Midpoint),
    };
    edit(&mut req);
    optimize_inner(req)
}

fn near(a: f64, b: f64) -> bool {
    let scale = 1.0_f64.max(a.abs()).max(b.abs());
    if std::env::var_os("MERIDIAN_MEASURE").is_some() {
        eprintln!("MEASURED {:e}", (a - b).abs() / scale);
    }
    (a - b).abs() < 1e-9 * scale
}

fn paid_by_index(r: &OptimizationResult) -> Vec<(usize, f64)> {
    let mut v: Vec<(usize, f64)> = r
        .adjustments
        .iter()
        .map(|a| (a.index, a.adjustment))
        .collect();
    v.sort_by_key(|p| p.0);
    v
}

// ---- hand figures on the six-row roster ----------------------------------------------------------
//
// Reference: wage = 40000 + 1000 x at x = 0, 5, 10. Compared: x = 2 (paid 40000, line 42000, short
// 2000), x = 6 (paid 47000, line 46000, above by 1000), x = 12 (paid 49000, line 52000, short 3000).
// n_T = 3, g0 = (-2000 + 1000 - 3000) / 3 = -4000/3, need = 5000, overshoot mean = 1000/3.

#[test]
fn the_six_row_roster_reports_its_hand_figures() {
    let r = tiny(&|_| {}).unwrap();
    assert!(near(r.original_unexplained_gap, -4000.0 / 3.0));
    assert!(near(r.required_budget, 5000.0));
    assert!(near(r.need_target, 5000.0));
    assert_eq!(r.need_reference, Some(0.0));
    assert!(near(r.overshoot_mean.unwrap(), 1000.0 / 3.0));
    assert!(near(r.best_reachable_gap.unwrap(), 1000.0 / 3.0));
    assert!(near(r.new_unexplained_gap, 1000.0 / 3.0));
    assert!(near(r.total_cost, 5000.0));
    assert_eq!(r.closure, Some(1.0));
    assert_eq!(r.unfunded_count, Some(0));
}

#[test]
fn a_reachable_target_costs_n_times_the_distance_and_both_strategies_agree() {
    // g_T = -500: cost = 3 * (-500 + 4000/3) = 2500.
    let greedy = tiny(&|r| r.target_gap = Some(-500.0)).unwrap();
    let equitable = tiny(&|r| {
        r.target_gap = Some(-500.0);
        r.strategy = Some(AllocationStrategy::Equitable);
    })
    .unwrap();
    for r in [&greedy, &equitable] {
        assert!(near(r.total_cost, 2500.0), "cost {}", r.total_cost);
        assert!(
            near(r.new_unexplained_gap, -500.0),
            "{}",
            r.new_unexplained_gap
        );
        assert_eq!(r.target_gap_reachable, Some(true));
        assert!(near(r.target_budget.unwrap(), 2500.0));
        assert_eq!(r.shortfall_to_target, None);
        assert!(near(r.closure.unwrap(), 0.5));
        assert!(near(r.unfunded_amount.unwrap(), 2500.0));
        assert_eq!(r.budget_binding, Some(false));
    }
    // Greedy pays the largest shortfall first: 2500 of the 3000 row, nothing to the 2000 row.
    assert_eq!(paid_by_index(&greedy).len(), 2);
    let g: Vec<f64> = paid_by_index(&greedy).iter().map(|p| p.1).collect();
    assert!(near(g[0], 0.0) && near(g[1], 2500.0), "{g:?}");
    // Equitable pays half of each shortfall.
    let e: Vec<f64> = paid_by_index(&equitable).iter().map(|p| p.1).collect();
    assert!(near(e[0], 1000.0) && near(e[1], 1500.0), "{e:?}");
    // Both eligible employees are left short under either strategy at half the need.
    assert_eq!(greedy.unfunded_count, Some(2));
    assert_eq!(equitable.unfunded_count, Some(2));
}

#[test]
fn a_target_the_group_already_meets_pays_nothing() {
    // F-04: a budget of 0 used to read as "no cap" and pay the full need.
    for strategy in [AllocationStrategy::Greedy, AllocationStrategy::Equitable] {
        let r = tiny(&|r| {
            r.target_gap = Some(-2000.0);
            r.strategy = Some(strategy);
        })
        .unwrap();
        assert_eq!(r.total_cost, 0.0, "{strategy:?}");
        assert!(r.adjustments.iter().all(|a| a.adjustment == 0.0));
        assert_eq!(r.target_gap_reachable, Some(true));
        assert_eq!(r.target_budget, Some(0.0));
        assert_eq!(r.closure, Some(0.0));
        assert!(near(r.unfunded_amount.unwrap(), 5000.0));
        assert!(near(r.new_unexplained_gap, -4000.0 / 3.0));
    }
    // Exactly at the current gap also pays nothing.
    let r = tiny(&|r| r.target_gap = Some(-4000.0 / 3.0)).unwrap();
    assert_eq!(r.total_cost, 0.0);
}

#[test]
fn a_target_beyond_reach_pays_the_same_as_no_target_and_says_so() {
    // D1 / V15: "unreachable equals no target"; the floor is named, no amount changes.
    let none = tiny(&|_| {}).unwrap();
    for strategy in [AllocationStrategy::Greedy, AllocationStrategy::Equitable] {
        let r = tiny(&|r| {
            r.target_gap = Some(500.0);
            r.strategy = Some(strategy);
        })
        .unwrap();
        assert_eq!(r.target_gap_reachable, Some(false));
        assert!(near(r.best_reachable_gap.unwrap(), 1000.0 / 3.0));
        assert!(near(r.shortfall_to_target.unwrap(), 500.0 - 1000.0 / 3.0));
        assert!(near(r.target_budget.unwrap(), 5000.0));
        assert!(near(r.total_cost, none.total_cost));
        if strategy == AllocationStrategy::Greedy {
            assert_eq!(paid_by_index(&r), paid_by_index(&none));
        }
    }
}

#[test]
fn a_cap_below_the_budget_the_target_asks_for_binds() {
    let r = tiny(&|r| {
        r.target_gap = Some(-500.0);
        r.budget = 1000.0;
    })
    .unwrap();
    assert!(near(r.total_cost, 1000.0));
    assert_eq!(r.budget_binding, Some(true));
    assert!(
        near(r.target_budget.unwrap(), 2500.0),
        "the rule's own figure is reported"
    );
    assert!(r.new_unexplained_gap < -500.0);
    // A cap above both the target's budget and the need does not bind.
    let r = tiny(&|r| {
        r.target_gap = Some(-500.0);
        r.budget = 9000.0;
    })
    .unwrap();
    assert!(near(r.total_cost, 2500.0));
    assert_eq!(r.budget_binding, Some(false));
    let r = tiny(&|r| r.budget = 9000.0).unwrap();
    assert_eq!(r.budget_binding, Some(false), "9000 exceeds the 5000 need");
}

// ---- a pooled roster where paying someone LOWERS the gap (0122-MERIDIAN C-01) ---------------------
//
// `0122-remedy-nonmonotone.csv`: ten reference rows at x 0..9 on a noisy line, five compared rows,
// one of them at x = 42, far beyond the reference group. On the pooled line that row's weight is
// negative (-0.126), and it is the smallest shortfall, so Greedy pays it LAST: the gap climbs to
// 723 at a spend of 23 404 and then falls to 161 when the last row is paid. The numbers in the
// R oracle (`N_pooled_*`) were refitted with `lm`; the sweep here needs none.

fn nonmonotone(edit: &dyn Fn(&mut OptimizationRequest)) -> OptimizationResult {
    let mut req = OptimizationRequest {
        csv_data: std::fs::read(root().join("engine/tests/fixtures/0122-remedy-nonmonotone.csv"))
            .unwrap(),
        outcome_variable: "wage".into(),
        group_variable: "group".into(),
        reference_group: "R".into(),
        predictors: vec!["x".into()],
        categorical_predictors: None,
        budget: 0.0,
        target_gap: None,
        target: Some(OptimizationTarget::Pooled),
        strategy: Some(AllocationStrategy::Greedy),
        min_gap_pct: Some(0.0),
        forensic_mode: Some(false),
        adjust_both_groups: Some(false),
        confidence_level: Some(0.95),
        range_target: Some(RangeTarget::Midpoint),
    };
    edit(&mut req);
    optimize_inner(req).unwrap()
}

#[test]
fn the_best_reachable_gap_is_the_highest_the_gap_gets_not_where_it_ends() {
    let full = nonmonotone(&|_| {});
    let best = full.best_reachable_gap.unwrap();
    // The roster really is non-monotone: paying the last row costs the group 560 dollars of gap.
    assert!(
        best > full.new_unexplained_gap + 400.0,
        "best {best} vs the gap after paying everything {}",
        full.new_unexplained_gap
    );
    // Sweep the cap: no budget does better than `best`, and some budget gets within a dollar of it.
    let need = full.required_budget;
    let mut highest = f64::NEG_INFINITY;
    let mut steps = 0;
    let mut cap = 100.0;
    while cap < need * 1.05 {
        let r = nonmonotone(&|r| r.budget = cap);
        highest = highest.max(r.new_unexplained_gap);
        assert!(
            r.new_unexplained_gap <= best + 1e-6,
            "cap {cap}: the gap reaches {} above best_reachable_gap {best}",
            r.new_unexplained_gap
        );
        cap += 100.0;
        steps += 1;
    }
    assert!(steps > 200);
    assert!(
        best - highest < 2.0,
        "no swept budget gets near best: best {best}, highest {highest}"
    );
}

#[test]
fn a_target_between_the_end_of_the_path_and_its_peak_is_reachable_and_costs_less_than_the_need() {
    let full = nonmonotone(&|_| {});
    let best = full.best_reachable_gap.unwrap();
    let end = full.new_unexplained_gap;
    // Above where the full schedule ends, below the peak: the old rule called it unreachable.
    for target in [end + 10.0, (best + end) / 2.0, best - 100.0, best - 1.0] {
        let r = nonmonotone(&|r| r.target_gap = Some(target));
        assert_eq!(r.target_gap_reachable, Some(true), "target {target}");
        assert!(
            (r.new_unexplained_gap - target).abs() < 1e-6,
            "target {target}: the schedule ends at {}",
            r.new_unexplained_gap
        );
        assert!(
            r.total_cost < full.required_budget - 1000.0,
            "target {target}: it costs {} of a {} need",
            r.total_cost,
            full.required_budget
        );
        assert!(near(r.target_budget.unwrap(), r.total_cost));
        // It is the LEAST budget: a cent less leaves the gap below the target.
        let less = nonmonotone(&|q| {
            q.target_gap = Some(target);
            q.budget = r.total_cost - 0.01;
        });
        assert!(less.new_unexplained_gap < target, "target {target}");
    }
}

#[test]
fn a_target_above_the_peak_pays_up_to_the_peak_and_stops_there() {
    let full = nonmonotone(&|_| {});
    let best = full.best_reachable_gap.unwrap();
    let r = nonmonotone(&|r| r.target_gap = Some(best + 100.0));
    assert_eq!(r.target_gap_reachable, Some(false));
    assert!(near(r.shortfall_to_target.unwrap(), 100.0));
    // The schedule reaches the best the remedy can, which is not the schedule that pays everything.
    assert!(
        (r.new_unexplained_gap - best).abs() < 1e-6,
        "ends at {} but best is {best}",
        r.new_unexplained_gap
    );
    assert!(r.total_cost < full.required_budget - 1000.0);
    assert!(near(r.target_budget.unwrap(), r.total_cost));
    // On a roster where every payment raises the gap, an unreachable target still pays everything.
    let tiny_far = tiny(&|r| r.target_gap = Some(500.0)).unwrap();
    assert!(near(tiny_far.total_cost, tiny_far.required_budget));
}

#[test]
fn equitable_on_the_same_roster_peaks_at_full_payment() {
    // Everyone is paid the same share, so the gap is a straight line in the share: -5571 at 0 and
    // +161 at 1. The peak is the end, and a target in between costs the matching share.
    let r = nonmonotone(&|r| r.strategy = Some(AllocationStrategy::Equitable));
    assert!(near(r.best_reachable_gap.unwrap(), r.new_unexplained_gap));
    let half = nonmonotone(&|q| {
        q.strategy = Some(AllocationStrategy::Equitable);
        q.target_gap = Some((r.original_unexplained_gap + r.new_unexplained_gap) / 2.0);
    });
    assert!(near(half.total_cost, r.total_cost / 2.0));
}

// ---- brute force ---------------------------------------------------------------------------------

fn sha256_file(path: &PathBuf) -> String {
    Sha256::digest(std::fs::read(path).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn the_engine_cost_is_the_enumerated_minimum_for_both_strategies() {
    let path = root().join("engine/tests/fixtures/remedy_bruteforce.json");
    let g: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(g["_meta"]["expected_values_from_engine_output"], false);
    assert_eq!(
        g["_meta"]["generator_sha256"].as_str().unwrap(),
        sha256_file(&root().join("verification/gen_remedy_bruteforce.py")),
        "stale golden: verification/gen_remedy_bruteforce.py changed"
    );
    for (name, want) in g["_meta"]["fixture_sha256"].as_object().unwrap() {
        assert_eq!(
            want.as_str().unwrap(),
            sha256_file(&root().join(name)),
            "{name}"
        );
    }
    let step = g["_meta"]["grid_step"].as_f64().unwrap();
    let cases = g["cases"].as_array().unwrap();
    assert!(cases.len() >= 8);
    let mut reachable_seen = 0;
    let mut unreachable_seen = 0;
    for c in cases {
        let target = c["target_gap"].as_f64().unwrap();
        for strategy in [AllocationStrategy::Greedy, AllocationStrategy::Equitable] {
            let r = tiny(&|r| {
                r.target_gap = Some(target);
                r.strategy = Some(strategy);
            })
            .unwrap();
            match c["min_cost"].as_f64() {
                Some(min) => {
                    reachable_seen += 1;
                    // The enumeration cannot go below the grid; the engine's continuous minimum can
                    // undercut it by less than one step, and equals it when the cost sits on the grid.
                    assert!(
                        r.total_cost <= min + 1e-9 && r.total_cost > min - step,
                        "target {target} {strategy:?}: engine {} vs enumerated minimum {min}",
                        r.total_cost
                    );
                    assert_eq!(r.target_gap_reachable, Some(true), "target {target}");
                    assert!(r.new_unexplained_gap >= target - 1e-9, "target {target}");
                    let analytic = (3.0 * (target + 4000.0 / 3.0)).max(0.0);
                    assert!(
                        near(r.total_cost, analytic),
                        "target {target}: {}",
                        r.total_cost
                    );
                }
                None => {
                    unreachable_seen += 1;
                    assert_eq!(r.target_gap_reachable, Some(false), "target {target}");
                    assert_eq!(c["feasible_points"], 0);
                }
            }
        }
    }
    assert!(reachable_seen > 8 && unreachable_seen >= 2);
}

// ---- refusals (V11, F-03, F-04) --------------------------------------------------------------------

#[test]
fn money_the_rule_cannot_honour_is_refused_by_name() {
    for bad in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1e-9] {
        let e = tiny(&|r| r.budget = bad).unwrap_err();
        assert!(e.starts_with("INVALID_BUDGET"), "{bad}: {e}");
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let e = tiny(&|r| r.target_gap = Some(bad)).unwrap_err();
        assert!(e.starts_with("INVALID_TARGET_GAP"), "{bad}: {e}");
    }
    for bad in [f64::NAN, f64::INFINITY, -0.01] {
        let e = tiny(&|r| r.min_gap_pct = Some(bad)).unwrap_err();
        assert!(e.starts_with("INVALID_MIN_GAP_PCT"), "{bad}: {e}");
    }
    let e = tiny(&|r| {
        r.target_gap = Some(-500.0);
        r.adjust_both_groups = Some(true);
    })
    .unwrap_err();
    assert!(e.starts_with("TARGET_GAP_WITH_REFERENCE_RAISES"), "{e}");
}

#[test]
fn zero_is_no_cap_and_negative_zero_is_zero() {
    let none = tiny(&|_| {}).unwrap();
    let neg_zero = tiny(&|r| r.budget = -0.0).unwrap();
    assert_eq!(paid_by_index(&neg_zero), paid_by_index(&none));
    assert!(
        near(neg_zero.total_cost, 5000.0),
        "0 funds the full eligible need"
    );
    // A cap that is a hair above zero pays a hair, not everything.
    let hair = tiny(&|r| r.budget = 1e-9).unwrap();
    assert!(hair.total_cost <= 1e-9 + 1e-12, "{}", hair.total_cost);
}

// ---- Fixture F: plain-std oracles ----------------------------------------------------------------------

struct Csv {
    f: FixtureF,
    fair: Vec<f64>,
}

fn noisy() -> Csv {
    let f = FixtureF::noisy();
    let fit = f.reference_fit(&f.reference_ordinals());
    let fair = (0..support::N_ROWS).map(|i| f.fair_wage(&fit, i)).collect();
    Csv { f, fair }
}

impl Csv {
    fn salary(&self, i: usize) -> f64 {
        self.f.salary_cell(i).unwrap()
    }
    /// (ordinal, shortfall) of every compared row below the line by more than a millionth and by
    /// at least `min_pct` of current pay, largest shortfall first.
    fn eligible(&self, min_pct: f64) -> Vec<(usize, f64)> {
        let mut v: Vec<(usize, f64)> = self
            .f
            .target_ordinals()
            .into_iter()
            .map(|i| (i, self.fair[i] - self.salary(i)))
            .filter(|&(i, d)| d > 1e-6 && d / self.salary(i) >= min_pct)
            .collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
        v
    }
    fn need(&self, min_pct: f64) -> f64 {
        self.eligible(min_pct).iter().map(|p| p.1).sum()
    }
    fn threshold_excluded(&self, min_pct: f64) -> usize {
        self.f
            .target_ordinals()
            .into_iter()
            .filter(|&i| {
                let d = self.fair[i] - self.salary(i);
                d > 1e-6 && d / self.salary(i) < min_pct
            })
            .count()
    }
}

fn run_noisy(edit: &dyn Fn(&mut OptimizationRequest)) -> OptimizationResult {
    let mut req = optimization_request(FixtureF::noisy().csv_bytes(), false);
    edit(&mut req);
    optimize_inner(req).unwrap()
}

/// Money the engine paid to compared rows, read by group membership from the CSV (not from the
/// result's own `source` flag).
fn paid_to_compared(r: &OptimizationResult) -> f64 {
    r.adjustments
        .iter()
        .filter(|a| support::is_target_row(a.index))
        .map(|a| a.adjustment)
        .sum()
}

#[test]
fn closure_is_the_share_of_the_need_paid_and_rises_with_every_dollar() {
    let csv = noisy();
    let need = csv.need(0.0);
    assert!(need > 40_000.0, "{need}");
    for strategy in [AllocationStrategy::Greedy, AllocationStrategy::Equitable] {
        let mut previous = -1.0;
        // Six rising caps; the old percentage went 100 % -> 58 % as the cap rose past the point
        // where the group crossed its line.
        for cap in [8_000.0, 16_000.0, 24_000.0, 32_000.0, 40_000.0, 50_000.0] {
            let r = run_noisy(&|r| {
                r.budget = cap;
                r.strategy = Some(strategy);
            });
            let closure = r.closure.unwrap();
            let paid = paid_to_compared(&r);
            assert!(
                near(paid, cap.min(need)),
                "{strategy:?} cap {cap}: paid {paid}"
            );
            assert!(
                (closure - paid / need).abs() < 1e-9,
                "{strategy:?} cap {cap}"
            );
            assert!(
                closure > previous,
                "{strategy:?} cap {cap}: {closure} <= {previous}"
            );
            assert!((0.0..=1.0).contains(&closure));
            previous = closure;
        }
    }
    let full = run_noisy(&|_| {});
    assert_eq!(full.closure, Some(1.0), "no cap pays the whole need");
    // The same cost closes the same share under both strategies.
    let g = run_noisy(&|r| r.budget = 20_000.0);
    let e = run_noisy(&|r| {
        r.budget = 20_000.0;
        r.strategy = Some(AllocationStrategy::Equitable);
    });
    assert!((g.closure.unwrap() - e.closure.unwrap()).abs() < 1e-9);
}

#[test]
fn closure_counts_the_compared_group_only_when_reference_raises_are_on() {
    let csv = noisy();
    let r = run_noisy(&|r| r.adjust_both_groups = Some(true));
    assert!(
        r.cost_reference > 0.0,
        "the roster has reference employees below their line"
    );
    assert!(near(r.cost_target, paid_to_compared(&r)));
    assert!(near(r.closure.unwrap() * csv.need(0.0), r.cost_target));
    assert!(r.total_cost > r.cost_target);
    assert!(
        near(r.required_budget, csv.need(0.0)),
        "required_budget is the compared need"
    );
}

#[test]
fn nobody_below_the_line_has_no_closure_and_no_signed_zero() {
    // Raise every compared wage above the line: the need is zero.
    let csv = "wage,group,x\n40000,R,0\n45000,R,5\n50000,R,10\n43000,T,2\n47000,T,6\n53000,T,12\n";
    let r = tiny(&|r| r.csv_data = csv.as_bytes().to_vec()).unwrap();
    assert_eq!(r.closure, None);
    assert_eq!(r.required_budget, 0.0);
    assert!(r.adjustments.is_empty() || r.adjustments.iter().all(|a| a.adjustment == 0.0));
    assert_eq!(r.unfunded_count, Some(0));
    assert!(near(
        r.overshoot_mean.unwrap(),
        (1000.0 + 1000.0 + 1000.0) / 3.0
    ));
    assert!(near(r.new_unexplained_gap, r.original_unexplained_gap));
    // F-17: no numeric field may be a negative zero (an empty f64 sum is one on this toolchain).
    let mut offenders = Vec::new();
    signed_zeroes("", &serde_json::to_value(&r).unwrap(), &mut offenders);
    assert!(offenders.is_empty(), "signed zeroes: {offenders:?}");
    // The same roster through check_defensibility with an empty schedule.
    let d = check_defensibility_on(
        VerificationRequest {
            decomposition_params: DecompositionRequest {
                csv_data: csv.as_bytes().to_vec(),
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
            adjustments: vec![],
            confidence_level: None,
        },
        &OptimizationTarget::Reference,
    )
    .unwrap();
    let mut offenders = Vec::new();
    signed_zeroes("", &serde_json::to_value(&d).unwrap(), &mut offenders);
    assert!(offenders.is_empty(), "signed zeroes: {offenders:?}");
}

fn signed_zeroes(path: &str, v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Number(n) => {
            if let Some(x) = n.as_f64() {
                if x == 0.0 && x.is_sign_negative() {
                    out.push(path.to_string());
                }
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                signed_zeroes(&format!("{path}[{i}]"), x, out);
            }
        }
        Value::Object(m) => {
            for (k, x) in m {
                signed_zeroes(&format!("{path}.{k}"), x, out);
            }
        }
        _ => {}
    }
}

#[test]
fn unfunded_and_threshold_counts_come_from_the_csv_not_the_result() {
    let csv = noisy();

    // Half the need, Greedy: pay down the sorted shortfalls, count the rows left short.
    let need = csv.need(0.0);
    let cap = need / 2.0;
    let mut remaining = cap;
    let mut unfunded = 0;
    for (_, d) in csv.eligible(0.0) {
        let pay = d.min(remaining.max(0.0));
        remaining -= pay;
        if d - pay > 1e-6 {
            unfunded += 1;
        }
    }
    let r = run_noisy(&|r| r.budget = cap);
    assert_eq!(r.unfunded_count, Some(unfunded));
    assert!(unfunded > 0 && unfunded < csv.eligible(0.0).len());
    assert!(near(r.unfunded_amount.unwrap(), need - cap));
    assert_eq!(r.threshold_excluded_count, Some(0));
    assert_eq!(r.budget_binding, Some(true));

    // Equitable at a cap: everyone is paid a share, so every eligible row is still short.
    let e = run_noisy(&|r| {
        r.budget = cap;
        r.strategy = Some(AllocationStrategy::Equitable);
    });
    assert_eq!(e.unfunded_count, Some(csv.eligible(0.0).len()));

    // A 5 % threshold leaves rows out ON PURPOSE: counted apart from the rows the cap leaves short.
    let excluded = csv.threshold_excluded(0.05);
    assert!(
        excluded > 3,
        "the roster must have rows under 5 %: {excluded}"
    );
    let t = run_noisy(&|r| r.min_gap_pct = Some(0.05));
    assert_eq!(t.threshold_excluded_count, Some(excluded));
    assert_eq!(
        t.unfunded_count,
        Some(0),
        "no cap: every eligible row is paid in full"
    );
    assert!(near(t.required_budget, csv.need(0.05)));
    assert!(csv.need(0.05) < need, "the threshold removes need");
    // Threshold AND cap together: the two counts stay separate.
    let tc = run_noisy(&|r| {
        r.min_gap_pct = Some(0.05);
        r.budget = csv.need(0.05) / 2.0;
    });
    assert_eq!(tc.threshold_excluded_count, Some(excluded));
    assert!(tc.unfunded_count.unwrap() > 0);
}

// ---- both entry points price one schedule alike (V7) ---------------------------------------------

fn as_schedule(r: &OptimizationResult) -> Vec<ProposedAdjustment> {
    r.adjustments
        .iter()
        .filter(|a| a.adjustment != 0.0)
        .map(|a| ProposedAdjustment {
            index: a.index,
            row_key: None,
            value: a.adjustment,
            predictor_overrides: None,
        })
        .collect()
}

#[test]
fn optimize_and_check_defensibility_report_one_set_of_figures_for_one_schedule() {
    let scenarios: Vec<(&str, OptimizationTarget, bool, f64)> = vec![
        ("reference", OptimizationTarget::Reference, false, 0.0),
        (
            "reference capped",
            OptimizationTarget::Reference,
            false,
            25_000.0,
        ),
        (
            "reference + reference raises",
            OptimizationTarget::Reference,
            true,
            0.0,
        ),
        (
            "reference + raises capped",
            OptimizationTarget::Reference,
            true,
            53_000.0,
        ),
        ("pooled", OptimizationTarget::Pooled, false, 0.0),
        ("pooled capped", OptimizationTarget::Pooled, false, 25_000.0),
        (
            "pooled + reference raises",
            OptimizationTarget::Pooled,
            true,
            0.0,
        ),
        (
            "pooled + raises capped",
            OptimizationTarget::Pooled,
            true,
            52_000.0,
        ),
    ];
    for (label, target, both, budget) in scenarios {
        let o = run_noisy(&|r| {
            r.target = Some(target);
            r.adjust_both_groups = Some(both);
            r.budget = budget;
        });
        let d = check_defensibility_on(
            VerificationRequest {
                decomposition_params: engine_requests::decomposition_request(
                    FixtureF::noisy().csv_bytes(),
                    false,
                ),
                adjustments: as_schedule(&o),
                confidence_level: None,
            },
            &target,
        )
        .unwrap();
        // Four fields, one sign. Their size is far above the tolerance, so a sign slip is caught.
        assert!(
            o.original_unexplained_gap < -500.0,
            "{label}: {}",
            o.original_unexplained_gap
        );
        for (field, a, b) in [
            ("original_gap", o.original_gap, d.original_gap),
            ("new_gap", o.new_gap, d.new_gap),
            (
                "original_unexplained_gap",
                o.original_unexplained_gap,
                d.original_unexplained_gap,
            ),
            (
                "new_unexplained_gap",
                o.new_unexplained_gap,
                d.new_unexplained_gap,
            ),
            ("required_budget", o.required_budget, d.required_budget),
            ("cost_target", o.cost_target, d.cost_target),
            ("cost_reference", o.cost_reference, d.cost_reference),
            ("total_cost", o.total_cost, d.total_cost),
        ] {
            if std::env::var_os("MERIDIAN_MEASURE").is_some() {
                eprintln!("MEASURED {:e}", (a - b).abs());
            }
            assert!(
                (a - b).abs() < 1e-6,
                "{label}.{field}: optimize {a} vs check_defensibility {b}"
            );
        }
        if both && budget == 0.0 {
            assert!(d.cost_reference > 0.0, "{label}");
        }
        // The group test is always read on the pooled line, and says so on the wire (S-01). Under
        // `Pooled` it is the same figure as the gap; under `Reference` the two are on different
        // lines and are not.
        let test = d.group_test.as_ref().unwrap();
        assert_eq!(test.line, "Pooled", "{label}");
        assert_eq!(
            serde_json::to_value(&d).unwrap()["group_test"]["line"],
            "Pooled",
            "{label}: the line is on the wire"
        );
        let apart = (test.group_coefficient - d.new_unexplained_gap).abs();
        match target {
            OptimizationTarget::Pooled => assert!(apart < 1e-6, "{label}: {apart}"),
            OptimizationTarget::Reference => {
                assert!(apart > 0.05, "{label}: the two lines read alike ({apart})")
            }
        }
    }
}

#[test]
fn required_budget_is_one_figure_only_on_the_default_basis() {
    // C-05 / S-04. `optimize` prices the line and the threshold the request names;
    // `check_defensibility` always prices the midpoint at threshold 0. Same figure by default,
    // another when a threshold or a bound is set, and `target_line` says which one was read.
    let check = |o: &OptimizationResult| {
        check_defensibility_on(
            VerificationRequest {
                decomposition_params: engine_requests::decomposition_request(
                    FixtureF::noisy().csv_bytes(),
                    false,
                ),
                adjustments: as_schedule(o),
                confidence_level: None,
            },
            &OptimizationTarget::Reference,
        )
        .unwrap()
    };
    let plain = run_noisy(&|_| {});
    assert!(near(plain.required_budget, check(&plain).required_budget));
    let threshold = run_noisy(&|r| r.min_gap_pct = Some(0.04));
    let d = check(&threshold);
    assert!(
        threshold.required_budget < d.required_budget - 1000.0,
        "a 4 % threshold leaves people out of the need: {} vs {}",
        threshold.required_budget,
        d.required_budget
    );
    assert_eq!(d.target_line, RangeTarget::Midpoint);
    let lower = run_noisy(&|r| r.range_target = Some(RangeTarget::LowerBound));
    assert_eq!(lower.target_line, RangeTarget::LowerBound);
    assert!(lower.required_budget < check(&lower).required_budget - 1000.0);
}

#[test]
fn raising_the_reference_group_never_counts_toward_the_compared_groups_gap_as_if_the_line_stood_still(
) {
    // REM-3: the old arithmetic credited the reference raise to the compared group.
    let fixed = run_noisy(&|r| r.adjust_both_groups = Some(false));
    let both = run_noisy(&|r| r.adjust_both_groups = Some(true));
    assert!(both.cost_reference > 0.0);
    // The compared group's gain is the SAME dollars either way; the reference raise lifts the line
    // under them, so their remaining gap is lower with the raise than without.
    assert!(near(both.cost_target, fixed.cost_target));
    assert!(
        both.new_unexplained_gap < fixed.new_unexplained_gap - 1.0,
        "raising the line must lower the compared group's gap to it: {} vs {}",
        both.new_unexplained_gap,
        fixed.new_unexplained_gap
    );
    // And the raw gap falls by the reference raise over the reference headcount.
    let expect = fixed.original_gap + both.cost_target / 40.0 - both.cost_reference / 60.0;
    assert!(near(both.new_gap, expect));
}

// ---- reference raises under a range target (F-10) -----------------------------------------------

#[test]
fn reference_raises_follow_the_same_range_target_as_the_compared_group() {
    // Upper bound: every reference employee is below it, and each is raised to ITS OWN upper bound.
    let upper = run_noisy(&|r| {
        r.adjust_both_groups = Some(true);
        r.range_target = Some(RangeTarget::UpperBound);
    });
    let paid: Vec<&Adjustment> = upper
        .adjustments
        .iter()
        .filter(|a| a.source == RowSource::Reference && a.adjustment > 0.0)
        .collect();
    assert!(paid.len() > 20, "reference rows paid: {}", paid.len());
    for a in paid {
        let bound = a.fair_wage_upper_bound.unwrap();
        assert!(
            (a.new_wage - bound).abs() <= 0.01,
            "reference row {} is paid {} against its upper bound {bound}",
            a.index,
            a.new_wage
        );
    }
    // Lower bound: no reference employee sits below their own lower bound, so nobody is raised.
    // Before, they were raised to the MIDPOINT whatever the range target said.
    let lower = run_noisy(&|r| {
        r.adjust_both_groups = Some(true);
        r.range_target = Some(RangeTarget::LowerBound);
    });
    assert_eq!(lower.cost_reference, 0.0);
    let midpoint = run_noisy(&|r| r.adjust_both_groups = Some(true));
    assert!(
        midpoint.cost_reference > 1000.0,
        "the midpoint line does raise them"
    );
}

// ---- the sign of a gap: one convention for the group's own figures (V7) -------------------------------

#[test]
fn a_group_paid_below_the_line_reads_negative_and_rises_with_money() {
    let r = run_noisy(&|r| r.budget = 10_000.0);
    assert!(r.original_unexplained_gap < 0.0);
    assert!(r.new_unexplained_gap > r.original_unexplained_gap);
    assert!(near(
        r.new_unexplained_gap - r.original_unexplained_gap,
        r.total_cost / 40.0
    ));
    assert!(r.original_gap.is_finite());
}

// ---- D5: no statute is cited on a remedy surface -------------------------------------------------------

/// Every file a remedy sentence can live in: the engine, the MCP server and the library sources,
/// and every shipped document (the changelog, the READMEs, the architecture notes, the API and
/// diagnostics references). The research notes, the audit trail and the build scratch directories
/// are history, not a surface.
fn remedy_text_files() -> Vec<PathBuf> {
    fn walk(dir: &std::path::Path, ext: &str, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, ext, out);
            } else if path.extension().is_some_and(|e| e == ext) {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    for dir in ["engine/src", "meridian-mcp/src", "oaxaca_blinder/src"] {
        walk(&root().join(dir), "rs", &mut files);
    }
    for doc in [
        "CHANGELOG.md",
        "README.md",
        "ARCHITECTURE.md",
        "CLAUDE.md",
        "oaxaca_blinder/README.md",
        "docs/API.md",
        "docs/DIAGNOSTICS.md",
        "docs/README.md",
    ] {
        files.push(root().join(doc));
    }
    for readme in ["engine/README.md", "meridian-mcp/README.md"] {
        if root().join(readme).exists() {
            files.push(root().join(readme));
        }
    }
    files
}

/// Statute citations in `text` (already lower-cased): a named Act, its number in the consolidated
/// statutes, `l.r.q.`, an `art.` / `article` followed by a number anywhere, and a `section` / `s.`
/// followed by a number on a line that is talking about legislation.
fn statute_citations(text: &str) -> Vec<String> {
    let banned = [
        "pay equity act",
        "loi sur l'équité",
        "loi sur l’équité",
        "équité salariale",
        "equite salariale",
        "e-12.001",
        "l.r.q",
        "rlrq",
    ];
    let mut found: Vec<String> = banned
        .iter()
        .filter(|b| text.contains(*b))
        .map(|b| format!("`{b}`"))
        .collect();
    let leads_to_number = |after: &str| {
        let after = after
            .trim_start_matches("icles")
            .trim_start_matches("icle")
            .trim_start_matches("tion")
            .trim_start_matches('.')
            .trim_start();
        after.chars().next().is_some_and(|c| c.is_ascii_digit())
    };
    for line in text.lines() {
        let padded = format!(" {line} ");
        let about_law = [
            "equity", "équité", "equite", "statut", "législ", "legislat", " act ", "loi ", "l'act",
        ]
        .iter()
        .any(|w| padded.contains(w));
        let chars: Vec<char> = line.chars().collect();
        for (word, needs_law) in [("art", false), ("section", true), ("s.", true)] {
            for (i, _) in line.match_indices(word) {
                let before = line[..i].chars().last();
                // `start.` is not `art.`; `class. 3` is not `s. 3`.
                if before.is_some_and(|c| c.is_alphanumeric()) {
                    continue;
                }
                if needs_law && !about_law {
                    continue;
                }
                let skip = line[..i].chars().count() + word.chars().count();
                let rest: String = chars.iter().skip(skip).take(12).collect();
                if leads_to_number(&rest) {
                    found.push(format!("`{}{}`", word, rest.trim_end()));
                }
            }
        }
    }
    found
}

#[test]
fn no_remedy_text_cites_a_pay_equity_act_article() {
    // This remedy is not the legislated Quebec pay equity exercise (0122 D5): no article of the
    // Act is cited on any remedy surface, and nothing presents a scenario as a statutory schedule.
    let files = remedy_text_files();
    assert!(
        files.len() > 15,
        "the scan reads every source and shipped document"
    );
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap().to_lowercase();
        let found = statute_citations(&text);
        assert!(found.is_empty(), "{} cites {:?}", f.display(), found);
    }
}

#[test]
fn the_statute_scan_catches_each_way_of_citing_one() {
    // The scan can fail: each citation form is caught, each innocent look-alike is not.
    for cited in [
        "see art. 70 of the act",
        "article 73 applies",
        "per the pay equity act",
        "la loi sur l'équité salariale",
        "section 73 of the pay equity legislation",
        "s. 70 of the act",
        "l.r.q., c. e-12.001",
    ] {
        assert!(
            !statute_citations(cited).is_empty(),
            "`{cited}` was not caught"
        );
    }
    for innocent in [
        "start. 3 rows later",
        "the normalised difference exceeds 0.25 (imbens and rubin 2015, section 14.2)",
        "see section 3 of this guide",
        "classes. 4 of them",
        "article id column",
    ] {
        assert!(
            statute_citations(innocent).is_empty(),
            "`{innocent}` was caught: {:?}",
            statute_citations(innocent)
        );
    }
}

#[test]
fn forensic_and_threshold_modes_keep_the_added_fields_consistent() {
    let r = run_noisy(&|r| {
        r.forensic_mode = Some(true);
        r.min_gap_pct = Some(0.02);
    });
    assert!(
        r.adjustments.len() > 40,
        "forensic mode lists every analysed compared row"
    );
    assert!(near(r.total_cost, r.cost_target + r.cost_reference));
    assert!(near(r.required_budget, r.need_target));
    let csv = noisy();
    assert_eq!(
        r.threshold_excluded_count,
        Some(csv.threshold_excluded(0.02))
    );
}
