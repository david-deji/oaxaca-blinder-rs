//! Request builders for the engine's entry points over Fixture F (0118-MERIDIAN).
//!
//! Included next to `support` by the engine integration tests:
//! `mod support; #[path = "support/engine_requests.rs"] mod engine_requests;`

#![allow(dead_code)]

use crate::support::{FixtureF, REFERENCE};
use pay_equity_engine::types::{
    AllocationStrategy, DecompositionRequest, EfficientFrontierRequest, OptimizationRequest,
    OptimizationTarget, ProposedAdjustment, RangeTarget, VerificationRequest,
};

pub fn decomposition_request(csv: Vec<u8>, categorical: bool) -> DecompositionRequest {
    DecompositionRequest {
        csv_data: csv,
        outcome_variable: "Salary".to_string(),
        group_variable: "Gender".to_string(),
        reference_group: REFERENCE.to_string(),
        predictors: vec!["Experience".to_string(), "Level".to_string()],
        categorical_predictors: if categorical {
            Some(vec!["Dept".to_string()])
        } else {
            None
        },
        three_fold: None,
        quantile: None,
        reference_coefficients: None,
        bootstrap_reps: Some(2),
    }
}

/// The settings V1 names: budget 0 (so the engine funds the full need), Midpoint, Greedy,
/// target Reference, minimum gap 0, no forensic rows, target group only.
pub fn optimization_request(csv: Vec<u8>, categorical: bool) -> OptimizationRequest {
    OptimizationRequest {
        csv_data: csv,
        outcome_variable: "Salary".to_string(),
        group_variable: "Gender".to_string(),
        reference_group: REFERENCE.to_string(),
        predictors: vec!["Experience".to_string(), "Level".to_string()],
        categorical_predictors: if categorical {
            Some(vec!["Dept".to_string()])
        } else {
            None
        },
        budget: 0.0,
        target_gap: None,
        target: Some(OptimizationTarget::Reference),
        strategy: Some(AllocationStrategy::Greedy),
        min_gap_pct: Some(0.0),
        forensic_mode: Some(false),
        adjust_both_groups: Some(false),
        confidence_level: Some(0.95),
        range_target: Some(RangeTarget::Midpoint),
    }
}

pub fn verification_request(
    csv: Vec<u8>,
    categorical: bool,
    adjustments: Vec<ProposedAdjustment>,
) -> VerificationRequest {
    VerificationRequest {
        decomposition_params: decomposition_request(csv, categorical),
        adjustments,
    }
}

pub fn frontier_request(
    csv: Vec<u8>,
    steps: usize,
    max_budget: Option<f64>,
) -> EfficientFrontierRequest {
    EfficientFrontierRequest {
        decomposition_params: decomposition_request(csv, false),
        steps: Some(steps),
        max_budget,
    }
}

pub fn proposed(index: usize, value: f64) -> ProposedAdjustment {
    ProposedAdjustment {
        index,
        row_key: None,
        value,
        predictor_overrides: None,
    }
}

/// Inserts blank lines before the given data rows and `trailing` blank lines at the end, with
/// `\n` or `\r\n` line endings.
pub fn with_blank_lines(
    csv: &str,
    before_data_rows: &[usize],
    trailing: usize,
    crlf: bool,
) -> Vec<u8> {
    let nl = if crlf { "\r\n" } else { "\n" };
    let mut out = String::new();
    for (n, line) in csv.lines().enumerate() {
        // n == 0 is the header; data row r is line n = r + 1.
        if n >= 1 && before_data_rows.contains(&(n - 1)) {
            out.push_str(nl);
        }
        out.push_str(line);
        out.push_str(nl);
    }
    for _ in 0..trailing {
        out.push_str(nl);
    }
    out.into_bytes()
}

/// Strips an `Oaxaca Error: ` prefix, which the data-matrix path adds to builder errors other
/// than the two 0118 group-value refusals. Not used for those refusals: their text is asserted
/// exactly, per entry point.
pub fn bare_error(e: &str) -> &str {
    e.strip_prefix("Oaxaca Error: ").unwrap_or(e)
}

/// Ordinals of every target employee whose fair wage (Fixture F's formula) exceeds their pay,
/// among the analysed target rows of `model`.
pub fn underpaid_targets(f: &FixtureF, model: &[crate::support::Col]) -> Vec<usize> {
    f.analysed_target(model)
        .into_iter()
        .filter(|&i| f.formula_wage(i) - f.salary_cell(i).unwrap() > 1e-6)
        .collect()
}
