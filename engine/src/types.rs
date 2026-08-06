use serde::{Deserialize, Serialize};

use oaxaca_blinder::RunMetadata;

#[derive(Serialize, Deserialize, Debug)]
pub struct DecompositionRequest {
    pub csv_data: Vec<u8>,
    pub outcome_variable: String,
    pub group_variable: String,
    pub reference_group: String,
    pub predictors: Vec<String>,
    pub categorical_predictors: Option<Vec<String>>,
    pub three_fold: Option<bool>,
    pub quantile: Option<f64>,                  // For RIF Regression
    pub reference_coefficients: Option<String>, // "Pooled", "GroupA", "GroupB", "Weighted"
    pub bootstrap_reps: Option<usize>,
}

#[derive(Serialize, Debug)]
pub struct DetailedComponent {
    pub name: String,
    pub estimate: f64,
    pub std_err: Option<f64>,
    pub p_value: Option<f64>,
    pub ci_lower: Option<f64>,
    pub ci_upper: Option<f64>,
}

#[derive(Serialize, Debug)]
pub struct DataSummary {
    pub total_count: usize,
    pub group_a_count: usize,
    pub group_b_count: usize,
    pub group_a_mean: f64,
    pub group_b_mean: f64,
}

#[derive(Serialize, Debug)]
pub struct DecompositionResult {
    pub total_gap: f64,
    pub explained_gap: f64,
    pub unexplained_gap: f64,
    pub interaction_gap: Option<f64>, // For 3-fold
    pub explained_percentage: f64,
    pub unexplained_percentage: f64,
    pub interaction_percentage: Option<f64>,
    pub detailed_explained: Vec<DetailedComponent>,
    pub detailed_unexplained: Vec<DetailedComponent>,
    pub data_summary: Option<DataSummary>,
    pub unexplained_standard_error: Option<f64>,
    /// Provenance of the underlying decomposition run (seed, RNG algorithm, rep accounting).
    pub run_metadata: RunMetadata,
    /// How many inbound `ProposedAdjustment.row_key` values did not resolve and were skipped.
    /// `Some(0)` from `verify_adjustments` when every key resolved; `None` from `decompose`,
    /// which consumes no proposed adjustments. See `crate::row_key`.
    pub unresolved_row_keys: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub enum OptimizationTarget {
    Reference, // Match Group A (Current "Perfect Equity")
    Pooled,    // Match Market Average (Zero Statistical Gap)
}

#[derive(Deserialize, Debug)]
pub enum AllocationStrategy {
    Greedy,    // Sort by Gap Descending
    Equitable, // Distribute budget proportionally
}

#[derive(Deserialize, Debug)]
pub struct OptimizationRequest {
    pub csv_data: Vec<u8>,
    pub outcome_variable: String,
    pub group_variable: String,
    pub reference_group: String,
    pub predictors: Vec<String>,
    pub categorical_predictors: Option<Vec<String>>,
    pub budget: f64,
    pub target_gap: Option<f64>,
    pub target: Option<OptimizationTarget>,
    pub strategy: Option<AllocationStrategy>,
    pub min_gap_pct: Option<f64>,    // Percentage (e.g., 0.02 for 2%)
    pub forensic_mode: Option<bool>, // If true, return ALL adjustments including negative gaps (overpaid)
    pub adjust_both_groups: Option<bool>,
    pub confidence_level: Option<f64>, // e.g., 0.95 for 95% CI
    pub range_target: Option<RangeTarget>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RangeTarget {
    Midpoint,   // Default: Fair Wage (Point Estimate)
    LowerBound, // Minimum Defensible (Lower CI)
    UpperBound, // High Retention (Upper CI)
}

#[derive(Serialize, Debug)]
pub struct Contribution {
    pub name: String,
    pub value: f64,
}

#[derive(Serialize, Debug)]
pub struct Adjustment {
    /// POSITIONAL: the 0-based row ordinal of the parsed CSV DataFrame. Correct as an offset
    /// (seven sites in this crate index a Vec / ChunkedArray / matrix-row map with it), wrong as
    /// an identity across a save boundary. Kept unchanged for every existing consumer and every
    /// payload already written.
    pub index: usize,
    /// STABLE identity for this row — 0017-MERIDIAN P4. Derived from an employee-number column
    /// when the CSV carries one, otherwise from the row's full cell set; never from position.
    /// See `crate::row_key`. `None` means the engine could not mint a key for this ordinal,
    /// which in practice only happens on a pre-P4 engine — a client seeing `undefined` here is
    /// looking at an old build and must fall back to `index`.
    pub row_key: Option<String>,
    pub adjustment: f64,
    pub current_wage: f64,
    pub new_wage: f64,
    pub fair_wage: f64,
    pub fair_wage_lower_bound: Option<f64>,
    pub fair_wage_upper_bound: Option<f64>,
    pub contributions: Vec<Contribution>,
    pub is_defensible: Option<bool>,
    pub defensibility_message: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct OptimizationResult {
    pub adjustments: Vec<Adjustment>,
    pub total_cost: f64,
    pub original_gap: f64,
    pub new_gap: f64,
    pub original_unexplained_gap: f64,
    pub new_unexplained_gap: f64,
    pub required_budget: f64, // Total budget needed to meet target
    pub model_coefficients: Vec<Contribution>,
    /// Literal key-space discriminator for `Adjustment.row_key` — always `"rowKeyV1"`
    /// (`crate::row_key::ROW_KEY_SPACE`). A reader that does not recognise the value must
    /// REFUSE the keys rather than reinterpret them, mirroring P2's `keySpace` guard on the
    /// persisted ledger block.
    pub row_key_space: String,
    /// Which rule minted the keys: `"column"` or `"contentHash"`.
    pub row_key_source: crate::row_key::RowKeySource,
    /// The employee-number column the keys came from, when `row_key_source` is `"column"`.
    pub row_key_column: Option<String>,
    /// How many inbound `ProposedAdjustment.row_key` values did not resolve against this CSV
    /// and were therefore SKIPPED. `None` when the entry point consumes no proposed
    /// adjustments (optimize). A non-zero value means the CSV no longer contains those rows —
    /// the annotations are orphaned, not misplaced.
    pub unresolved_row_keys: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub struct ProposedAdjustment {
    pub index: usize,
    /// Optional stable key (0017-MERIDIAN P4). When present and resolvable it WINS over
    /// `index`; when present and unresolvable the adjustment is skipped and counted in
    /// `OptimizationResult.unresolved_row_keys`. Absent (every pre-P4 caller) means the
    /// `index` path runs unchanged.
    #[serde(default)]
    pub row_key: Option<String>,
    pub value: f64,
    pub predictor_overrides: Option<std::collections::HashMap<String, String>>, // Can handle numbers as string "1.0"
}

#[derive(Deserialize, Debug)]
pub struct VerificationRequest {
    #[serde(flatten)]
    pub decomposition_params: DecompositionRequest,
    pub adjustments: Vec<ProposedAdjustment>,
}

#[derive(Serialize, Debug)]
pub struct FrontierPoint {
    pub budget: f64,
    pub t_statistic: f64,
    pub p_value: f64,
    pub is_significant: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EfficientFrontierRequest {
    #[serde(flatten)]
    pub decomposition_params: DecompositionRequest,
    pub steps: Option<usize>,    // Default 50
    pub max_budget: Option<f64>, // If None, auto-detect
}
