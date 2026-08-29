//! Byte-identity determinism for the three engine entry points that are NOT `decompose`
//! (0017-P1, D17).
//!
//! Every determinism test that existed before this file covered `decompose` only
//! (`engine/tests/mode_parity_test.rs`, `oaxaca_blinder/tests/rng_determinism.rs`). The three
//! functions covered here — `optimize_inner`, `check_defensibility_inner` and
//! `calculate_efficient_frontier_inner` — produce the bulk of the persisted payload, which
//! 0017-P1 deletes from disk and regenerates on load. "Regenerate on load" is only a safe trade
//! if regeneration is byte-identical, so byte identity is the assertion, not tolerance.
//!
//! Two axes, both required:
//!   1. Repeat-run identity inside one process. This is the axis that catches std `HashMap`
//!      iteration order: `RandomState` re-keys per map instance within a single thread, so two
//!      maps built from the same inserts in the same process iterate differently. A float sum
//!      accumulated in that order is tolerance-equal but not bit-equal. `check_defensibility_inner`
//!      accumulated `required_budget`, `original_unexplained_gap` and `new_unexplained_gap`
//!      exactly that way until D14 replaced the map with a `BTreeMap`.
//!   2. Thread-count identity across rayon pools of 1/2/4, mirroring `mode_parity_test.rs`.
//!      A reduction that folds in completion order rather than index order fails here.
//!
//! Canonical serializer: `serde_json::to_string` over the `Serialize`-derived result types. Both
//! `OptimizationResult` and `FrontierPoint` are all-`Vec`, no map, so field and element order are
//! fixed and floats go through ryu deterministically.

use pay_equity_engine::analysis::{calculate_efficient_frontier_inner, optimize_inner};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::types::{
    AllocationStrategy, DecompositionRequest, EfficientFrontierRequest, OptimizationRequest,
    OptimizationTarget, ProposedAdjustment, VerificationRequest,
};
use rayon::ThreadPoolBuilder;

const FIXTURE: &[u8] = include_bytes!("../../oaxaca_blinder/tests/fixtures/parity_fixture.csv");

fn predictors() -> Vec<String> {
    vec![
        "education".to_string(),
        "experience".to_string(),
        "tenure".to_string(),
    ]
}

fn decomposition_params() -> DecompositionRequest {
    DecompositionRequest {
        csv_data: FIXTURE.to_vec(),
        outcome_variable: "log_wage".to_string(),
        group_variable: "gender".to_string(),
        // Reference is the advantaged group, so the non-reference group carries the shortfall
        // that `required_budget` accumulates. With "F" as reference the sum is empty and the
        // accumulation this file guards is never exercised — the sanity assertion at the bottom
        // of `defensibility_aggregate_scalars_are_bit_stable_across_runs` enforces that.
        reference_group: "M".to_string(),
        predictors: predictors(),
        categorical_predictors: None,
        three_fold: Some(true),
        quantile: None,
        reference_coefficients: None,
        bootstrap_reps: Some(16),
    }
}

fn optimization_request() -> OptimizationRequest {
    OptimizationRequest {
        csv_data: FIXTURE.to_vec(),
        outcome_variable: "log_wage".to_string(),
        group_variable: "gender".to_string(),
        // Reference is the advantaged group, so the non-reference group carries the shortfall
        // that `required_budget` accumulates. With "F" as reference the sum is empty and the
        // accumulation this file guards is never exercised — the sanity assertion at the bottom
        // of `defensibility_aggregate_scalars_are_bit_stable_across_runs` enforces that.
        reference_group: "M".to_string(),
        predictors: predictors(),
        categorical_predictors: None,
        budget: 25.0,
        target_gap: None,
        target: Some(OptimizationTarget::Reference),
        strategy: Some(AllocationStrategy::Greedy),
        min_gap_pct: None,
        forensic_mode: Some(true),
        adjust_both_groups: Some(false),
        confidence_level: Some(0.95),
        range_target: None,
    }
}

/// Adjustments spread across the fixture, deliberately including duplicate indices, an index
/// with no matching row, and predictor overrides — the branches that feed the row-index maps.
fn proposed_adjustments() -> Vec<ProposedAdjustment> {
    let mut adjustments: Vec<ProposedAdjustment> = (0..300)
        .step_by(3)
        .map(|index| ProposedAdjustment {
            index,
            row_key: None,
            value: 0.01 + (index as f64) * 0.0007,
            predictor_overrides: None,
        })
        .collect();

    let mut overrides = std::collections::HashMap::new();
    overrides.insert("education".to_string(), "16.5".to_string());
    overrides.insert("tenure".to_string(), "7.25".to_string());
    adjustments.push(ProposedAdjustment {
        index: 7,
        row_key: None,
        value: 0.042,
        predictor_overrides: Some(overrides),
    });

    // Duplicate index: the lookup must resolve first-wins, as the linear scans it replaced did.
    adjustments.push(ProposedAdjustment {
        index: 12,
        row_key: None,
        value: 0.5,
        predictor_overrides: None,
    });

    adjustments
}

fn verification_request() -> VerificationRequest {
    VerificationRequest {
        decomposition_params: decomposition_params(),
        adjustments: proposed_adjustments(),
    }
}

fn frontier_request() -> EfficientFrontierRequest {
    EfficientFrontierRequest {
        decomposition_params: decomposition_params(),
        steps: Some(8),
        max_budget: Some(40.0),
    }
}

fn optimize_canonical() -> String {
    serde_json::to_string(&optimize_inner(optimization_request()).unwrap()).unwrap()
}

fn defensibility_canonical() -> String {
    serde_json::to_string(&check_defensibility_inner(verification_request()).unwrap()).unwrap()
}

fn frontier_canonical() -> String {
    serde_json::to_string(&calculate_efficient_frontier_inner(frontier_request()).unwrap()).unwrap()
}

fn in_pool<F: Fn() -> String + Send + Sync>(threads: usize, f: F) -> String {
    ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap()
        .install(f)
}

fn assert_repeat_identity(label: &str, f: fn() -> String) {
    let first = f();
    for run in 2..=4 {
        assert_eq!(
            first,
            f(),
            "{label}: run 1 and run {run} differ — output is not byte-reproducible in-process"
        );
    }
    assert!(!first.is_empty(), "{label}: serialized output is empty");
}

fn assert_thread_invariance(label: &str, f: fn() -> String) {
    let s1 = in_pool(1, f);
    let s2 = in_pool(2, f);
    let s4 = in_pool(4, f);
    assert_eq!(s1, s2, "{label}: 1-thread vs 2-thread byte mismatch");
    assert_eq!(s2, s4, "{label}: 2-thread vs 4-thread byte mismatch");
}

#[test]
fn optimize_inner_is_byte_identical_across_repeat_runs() {
    assert_repeat_identity("optimize_inner", optimize_canonical);
}

#[test]
fn optimize_inner_is_byte_identical_across_thread_counts() {
    assert_thread_invariance("optimize_inner", optimize_canonical);
}

#[test]
fn check_defensibility_inner_is_byte_identical_across_repeat_runs() {
    assert_repeat_identity("check_defensibility_inner", defensibility_canonical);
}

#[test]
fn check_defensibility_inner_is_byte_identical_across_thread_counts() {
    assert_thread_invariance("check_defensibility_inner", defensibility_canonical);
}

#[test]
fn calculate_efficient_frontier_inner_is_byte_identical_across_repeat_runs() {
    assert_repeat_identity("calculate_efficient_frontier_inner", frontier_canonical);
}

#[test]
fn calculate_efficient_frontier_inner_is_byte_identical_across_thread_counts() {
    assert_thread_invariance("calculate_efficient_frontier_inner", frontier_canonical);
}

/// D14 regression, stated at the bit level on the three scalars 0017-P1 persists as aggregates.
/// The full-payload tests above would also fail on a map-order regression, but this one names the
/// fields, so a failure points straight at the accumulation order rather than at "something in a
/// 300-row JSON blob moved".
#[test]
fn defensibility_aggregate_scalars_are_bit_stable_across_runs() {
    let first = check_defensibility_inner(verification_request()).unwrap();
    let baseline = [
        first.required_budget.to_bits(),
        first.original_unexplained_gap.to_bits(),
        first.new_unexplained_gap.to_bits(),
    ];

    for run in 2..=8 {
        let next = check_defensibility_inner(verification_request()).unwrap();
        let observed = [
            next.required_budget.to_bits(),
            next.original_unexplained_gap.to_bits(),
            next.new_unexplained_gap.to_bits(),
        ];
        assert_eq!(
            baseline, observed,
            "run {run}: [required_budget, original_unexplained_gap, new_unexplained_gap] changed \
             bit pattern between runs — the row-index map is iterating in hash order again"
        );
    }

    // Sanity: the sums are non-trivial, so bit-stability is a real assertion and not an artifact
    // of every field being 0.0.
    assert!(
        first.required_budget != 0.0,
        "required_budget is 0.0 — fixture no longer exercises the accumulation this test guards"
    );
    assert!(
        first.original_unexplained_gap != first.new_unexplained_gap,
        "adjustments had no effect on the unexplained gap — fixture is not exercising the path"
    );
}
