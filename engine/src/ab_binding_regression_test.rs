//! A/B-binding regression guardrails for `get_data_matrices()` consumers.
//!
//! ## Convention (read before "fixing" any `get_data_matrices()` call site)
//!
//! `OaxacaBuilder::split_groups` (`oaxaca_blinder/src/builder.rs:73`) sets
//! `group_b_name = reference_group`. Therefore:
//!
//! ```text
//! get_data_matrices() -> (X_A, y_A, X_B, y_B, names)
//!     A = the NON-reference group
//!     B = the reference group  (per OaxacaBuilder::new doc, builder.rs:113)
//! ```
//!
//! The three engine call sites (`analysis.rs` optimize_inner ~:391, `analysis.rs`
//! frontier ~:929, `defensibility.rs` ~:102) bind these matrices so that the local
//! `x_a`/`y_a` end up holding the *reference* (advantaged) group, from which `beta_fair`
//! (the fair-wage standard) is solved. **All three bindings are correct as committed.**
//!
//! ## Why this file exists
//!
//! A prior working session began "fixing" the frontier binding at `analysis.rs:929`
//! (uncommitted change `(x_b,y_b,x_a,y_a)` -> `(x_a,y_a,x_b,y_b)`) on the false premise
//! that A = reference. That change is a *regression*: it inverts which group's wages the
//! frontier adjusts, so the statistical gap never closes (empirically: p_max stays 0.0,
//! `is_significant` stays true at full budget). It was reverted; these tests lock the
//! correct behavior in so the same inversion cannot be reintroduced silently.
//!
//! Each test is discriminating: it PASSES on the committed binding and FAILS on the
//! inverted one (verified by flipping each site during Track-0 build, 2026-06-06).

#[cfg(test)]
mod tests {
    use crate::analysis::{calculate_efficient_frontier_inner, optimize_inner};
    use crate::defensibility::check_defensibility_inner;
    use crate::types::{
        AllocationStrategy, DecompositionRequest, EfficientFrontierRequest, OptimizationRequest,
        OptimizationTarget, ProposedAdjustment, VerificationRequest,
    };

    /// Asymmetric fixture: group A has high wage returns, group B low (underpaid).
    /// Education varies 10-19 in both groups (non-singular) with light deterministic
    /// noise (non-degenerate residual variance). `reference_group` is set by the caller.
    fn asymmetric_csv(a_slope: f64, b_slope: f64, a_intercept: f64, b_intercept: f64) -> Vec<u8> {
        let mut csv = "wage,education,group\n".to_string();
        for i in 0..10usize {
            let edu = 10.0 + i as f64;
            let noise = ((i % 4) as f64 - 1.5) * 100.0;
            csv.push_str(&format!(
                "{:.1},{:.1},A\n",
                a_intercept + a_slope * edu + noise,
                edu
            ));
        }
        for i in 0..10usize {
            let edu = 10.0 + i as f64;
            let noise = ((i % 4) as f64 - 1.5) * 100.0;
            csv.push_str(&format!(
                "{:.1},{:.1},B\n",
                b_intercept + b_slope * edu + noise,
                edu
            ));
        }
        csv.into_bytes()
    }

    #[test]
    fn test_optimizer_targets_reference_group_regression() {
        // optimize_inner (analysis.rs:391). Under OptimizationTarget::Reference the fair-wage
        // standard (beta_fair) must be solved from the REFERENCE (advantaged) group.
        // Fixture: reference_group = "A" (high, 3000*edu); B = target (low, 1000*edu).
        //   Committed binding -> beta_fair from A -> fair wage for B ~ 3000*edu -> large budget.
        //   Inverted binding  -> beta_fair from B -> fair wage for B ~ current  -> ~0 budget.
        // PASSES committed (required_budget ~ 290000); FAILS inverted (~0).
        let req = OptimizationRequest {
            csv_data: asymmetric_csv(3000.0, 1000.0, 0.0, 0.0),
            outcome_variable: "wage".to_string(),
            group_variable: "group".to_string(),
            reference_group: "A".to_string(),
            predictors: vec!["education".to_string()],
            categorical_predictors: None,
            budget: 1_000_000.0,
            target_gap: None,
            target: Some(OptimizationTarget::Reference),
            strategy: Some(AllocationStrategy::Greedy),
            min_gap_pct: None,
            forensic_mode: Some(false),
            adjust_both_groups: None,
            confidence_level: None,
            range_target: None,
        };
        let result = optimize_inner(req).expect("optimization must not error");
        assert!(
            result.required_budget > 10_000.0,
            "expected non-trivial required_budget when the reference group has higher wage \
             returns than the target; got {}. Near-zero = inverted binding (target group's own \
             regression used as the fair standard).",
            result.required_budget
        );
    }

    #[test]
    fn test_frontier_adjustments_target_underpaid_group() {
        // calculate_efficient_frontier_inner (analysis.rs:929). As budget rises, frontier
        // adjustments must reduce the group-dummy significance (gap closes). The frontier
        // sources its adjustment amounts from optimize_inner, so this also exercises site 2.
        // Fixture: reference_group = "A" (40000 + 2000*edu); B = target (10000 + 2000*edu);
        // 30000 structural/unexplained gap, identical education distribution (no explained gap).
        //   Committed binding -> B wages rise -> dummy coef -> 0 -> p rises -> not significant.
        //   Inverted binding  -> adjustments land on the reference group -> gap persists.
        // PASSES committed (p rises to ~1.0, not significant); FAILS inverted (p stays 0.0).
        let req = EfficientFrontierRequest {
            decomposition_params: DecompositionRequest {
                csv_data: asymmetric_csv(2000.0, 2000.0, 40000.0, 10000.0),
                outcome_variable: "wage".to_string(),
                group_variable: "group".to_string(),
                reference_group: "A".to_string(),
                predictors: vec!["education".to_string()],
                categorical_predictors: None,
                three_fold: Some(false),
                quantile: None,
                reference_coefficients: None,
                bootstrap_reps: None,
            },
            steps: Some(10),
            max_budget: Some(700_000.0),
        };
        let points =
            calculate_efficient_frontier_inner(req).expect("frontier should compute without error");
        assert!(points.len() >= 2, "need baseline and at least one step");
        let p_zero = points[0].p_value;
        let last = points.last().unwrap();
        assert!(
            last.p_value > p_zero,
            "p_value should rise as the gap closes; p_zero={:.4} p_max={:.4}. \
             Failure = inverted A/B binding (adjustments hitting the reference group).",
            p_zero,
            last.p_value
        );
        assert!(
            !last.is_significant,
            "gap should be statistically insignificant at full budget; is_significant=true, \
             p={:.4} -> inverted binding.",
            last.p_value
        );
    }

    #[test]
    fn test_defensibility_uses_reference_group_regression() {
        // check_defensibility_inner (defensibility.rs:102). The defensibility "fair beta" must
        // come from the REFERENCE (advantaged) group. Fixture: reference_group = "A" (high,
        // 3000*edu); B = target (low, 1000*edu). Score three underpaid B workers (value 0.0).
        //   Committed binding -> beta_fair from A -> B far below fair range -> is_defensible=false.
        //   Inverted binding  -> beta_fair from B's own reg -> B at its standard -> is_defensible=true.
        // PASSES committed (>=1 false); FAILS inverted (all true).
        let req = VerificationRequest {
            decomposition_params: DecompositionRequest {
                csv_data: asymmetric_csv(3000.0, 1000.0, 0.0, 0.0),
                outcome_variable: "wage".to_string(),
                group_variable: "group".to_string(),
                reference_group: "A".to_string(),
                predictors: vec!["education".to_string()],
                categorical_predictors: None,
                three_fold: Some(false),
                quantile: None,
                reference_coefficients: None,
                bootstrap_reps: None,
            },
            adjustments: vec![
                ProposedAdjustment {
                    index: 12,
                    row_key: None,
                    value: 0.0,
                    predictor_overrides: None,
                },
                ProposedAdjustment {
                    index: 15,
                    row_key: None,
                    value: 0.0,
                    predictor_overrides: None,
                },
                ProposedAdjustment {
                    index: 18,
                    row_key: None,
                    value: 0.0,
                    predictor_overrides: None,
                },
            ],
        };
        let result = check_defensibility_inner(req).expect("defensibility must not error");
        let any_indefensible = result
            .adjustments
            .iter()
            .any(|a| a.is_defensible == Some(false));
        assert!(
            any_indefensible,
            "expected >=1 underpaid B worker with is_defensible=Some(false) when the reference \
             (advantaged) group's regression is the fair standard. All-true means beta_fair came \
             from the target group's own regression (inverted binding at defensibility.rs:102)."
        );
    }
}
