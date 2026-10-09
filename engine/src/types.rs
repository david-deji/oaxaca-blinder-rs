use serde::{Deserialize, Serialize};

use oaxaca_blinder::{ExclusionReason, RunMetadata};

#[derive(Serialize, Deserialize, Debug)]
pub struct DecompositionRequest {
    // `serde_bytes` is load-bearing, not a micro-optimization. This struct is #[serde(flatten)]ed
    // into VerificationRequest and EfficientFrontierRequest, and flatten deserializes through a
    // buffer: serde reads each value with deserialize_any into Content, then replays it. For a JS
    // Uint8Array, serde-wasm-bindgen's deserialize_any calls visit_bytes -> Content::Bytes, and
    // replaying Content::Bytes into a plain Vec<u8> calls deserialize_seq, which errors with
    // "invalid type: byte array, expected a sequence".
    //
    // That is not hypothetical. It shipped: the browser's verify_adjustments, check_defensibility
    // and calculate_efficient_frontier were all dead against a Uint8Array payload while
    // `decompose` — the one entry point with no flatten above it — worked, which is exactly why
    // it looked like the engine accepted typed arrays.
    //
    // serde_bytes' Vec<u8> visitor implements visit_bytes, visit_byte_buf AND visit_seq, so both
    // a typed array and a plain JS Array of numbers deserialize. Callers may send either.
    #[serde(with = "serde_bytes")]
    pub csv_data: Vec<u8>,
    pub outcome_variable: String,
    pub group_variable: String,
    pub reference_group: String,
    pub predictors: Vec<String>,
    pub categorical_predictors: Option<Vec<String>>,
    pub three_fold: Option<bool>,
    pub quantile: Option<f64>, // For RIF Regression
    /// REQUIRED by `decompose` and `verify_adjustments`: exactly one of `"GroupA"`, `"GroupB"`,
    /// `"Pooled"`, `"PooledNoIndicator"`, `"Weighted"`. Absent or any other string is an error
    /// (0120-MERIDIAN S4); there is no default scheme. Typed `Option` only because this struct
    /// is flattened into the frontier and defensibility requests, which fit their own pooled
    /// model and ignore the field.
    pub reference_coefficients: Option<String>,
    pub bootstrap_reps: Option<usize>,
}

/// One input row the analysis left out because a model column was blank on it
/// (0118-MERIDIAN S5). Present on every result so a consumer can say which employees were not
/// analysed instead of silently showing fewer rows.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ExcludedRow {
    /// Original row ordinal: the zero-based position among the parsed data rows of the CSV sent
    /// to the engine. The same ordinal `Adjustment.index` uses.
    pub index: usize,
    /// The row's stable key (`Adjustment.row_key`), populated only when the CSV carries an
    /// employee-number column (`row_key_source` = `"column"`). `None` otherwise.
    pub row_key: Option<String>,
    /// Distinct reasons, in the order the columns are checked: `outcome`, `groupValue`,
    /// `numericPredictor`, `categoricalPredictor`, `weights`, `selectionOutcome`,
    /// `selectionPredictor`.
    pub reasons: Vec<ExclusionReason>,
    /// Every blank column on this row. A row blank in two columns appears once, naming both.
    pub columns: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct DetailedComponent {
    /// Predictor name, `{variable}_{level}` for a categorical level (EVERY level, the
    /// alphabetically first included, once the engine normalises), or `intercept_token()` for
    /// the constant, which is not a driver.
    pub name: String,
    pub estimate: f64,
    pub std_err: Option<f64>,
    pub p_value: Option<f64>,
    pub ci_lower: Option<f64>,
    pub ci_upper: Option<f64>,
}

/// Counts and means for the two groups. `group_a_*` is the REFERENCE group and `group_b_*`
/// every other row (the target group); this predates the A/B convention of the data matrices and
/// is the shape the app reads.
///
/// 0118-MERIDIAN S5: `group_a_count`, `group_b_count` and both means are over the ANALYSED rows
/// (complete cases), the same rows the decomposition used. `total_count` stays the raw row count
/// of the file, so `total_count - analysed counts` is the number of excluded rows.
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
    /// Analysed (complete-case) reference-group rows. 0118-MERIDIAN S5.
    pub analysed_reference_count: usize,
    /// Analysed (complete-case) target-group rows. 0118-MERIDIAN S5.
    pub analysed_target_count: usize,
    /// Rows left out of the analysis because a model column was blank. 0118-MERIDIAN S5.
    pub excluded_rows: Vec<ExcludedRow>,
    /// Inbound proposed adjustments addressed to an excluded or unknown row, skipped and
    /// counted rather than applied. `0` from `decompose`, which consumes none.
    pub adjustments_on_excluded_rows: usize,
    /// Where the compared group's characteristics sit against the baseline group's, and how many
    /// residual degrees of freedom each group's regression has (0120-MERIDIAN S6 / T12 / T13).
    /// Numbers only; `warnings` says which of them crossed a threshold.
    pub support: SupportDiagnostics,
    /// Everything on this result a reader should be told about, machine-readable. Empty when
    /// nothing crossed a threshold. See [`DiagnosticWarning`].
    pub warnings: Vec<DiagnosticWarning>,
    /// Percentile-mode report (0120-MERIDIAN S8 / T16): the actual percentile gap beside the
    /// RIF model total, with the tie and ECDF diagnostics. `Some` only when `quantile` was
    /// requested; omitted from the JSON otherwise, so a mean-mode result has no such key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantile_report: Option<QuantileReport>,
}

/// Why a [`DiagnosticWarning`] fired. Serialised as snake_case. The engine says WHAT crossed
/// WHICH threshold; the words a reader sees belong to the app's copy.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    /// More than [`SUPPORT_OUTSIDE_RANGE_SHARE`] of the compared rows lie outside the baseline
    /// group's observed [min, max] of a continuous predictor (`subject` = the predictor).
    OutsideRange,
    /// The Imbens-Rubin normalised difference of a continuous predictor exceeds
    /// [`SUPPORT_NORMALISED_DIFFERENCE`] in absolute value (`subject` = the predictor).
    NormalisedDifference,
    /// A fitted regression has fewer than [`SUPPORT_MIN_RESIDUAL_DF`] residual degrees of freedom
    /// (`subject` = `"reference"`, `"target"`, or `"pooled"` for the pooled fit of the Pooled
    /// optimise target). At zero or below the run is refused instead.
    FewResidualDf,
    /// In percentile mode, more than [`QUANTILE_TIE_SHARE`] of a group's rows equal the group's
    /// own percentile value (`subject` = `"reference"` or `"target"`).
    TieShare,
    /// In percentile mode, a group's empirical CDF at its own percentile value is more than
    /// `max(QUANTILE_ECDF_OFFSET, 1/n)` away from the requested percentile (`subject` as above);
    /// the `1/n` is the most a tie-free group of `n` rows can be off by discreteness alone.
    EcdfOffset,
}

/// Share of compared rows outside the baseline range above which `OutsideRange` fires.
pub const SUPPORT_OUTSIDE_RANGE_SHARE: f64 = 0.05;
/// Absolute Imbens-Rubin normalised difference above which `NormalisedDifference` fires
/// (Imbens & Rubin 2015, ch. 14.2 rule of thumb).
pub const SUPPORT_NORMALISED_DIFFERENCE: f64 = 0.25;
/// Residual degrees of freedom below which `FewResidualDf` fires.
pub const SUPPORT_MIN_RESIDUAL_DF: i64 = 10;
/// Tie share above which `TieShare` fires.
pub const QUANTILE_TIE_SHARE: f64 = 0.05;
/// Absolute `F_n(q_tau) - tau` above which `EcdfOffset` fires, floored at `1/n` for a group of
/// `n` rows (the warning's own `threshold` carries the line that applied).
pub const QUANTILE_ECDF_OFFSET: f64 = 0.01;

/// One thing a reader should be told about a result. `value` is what was measured and
/// `threshold` the line it crossed, so a screen can quote both without recomputing.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct DiagnosticWarning {
    pub code: WarningCode,
    /// The predictor or group the warning is about; `None` when it is about the whole result.
    /// `None` arrives as `undefined` in JS and as `null` over MCP, like `normalised_difference`.
    pub subject: Option<String>,
    pub value: f64,
    pub threshold: f64,
}

/// How far the compared (target) group's values sit from the baseline (reference) group's for
/// one continuous predictor. "Baseline" is the group whose pay line the fair wage extends: the
/// reference group, the one `reference_group` names.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct PredictorSupport {
    pub name: String,
    pub reference_min: f64,
    pub reference_max: f64,
    /// Type-7 1st and 99th percentiles of the reference group's values.
    pub reference_p01: f64,
    pub reference_p99: f64,
    pub target_min: f64,
    pub target_max: f64,
    /// Share of target rows below the reference minimum or above the reference maximum.
    pub target_outside_range_share: f64,
    /// Share of target rows below the reference 1st percentile or above its 99th.
    pub target_outside_p01_p99_share: f64,
    /// Imbens-Rubin normalised difference `(mean_target - mean_reference) /
    /// sqrt((var_target + var_reference) / 2)` with sample variances. `None` when both groups
    /// have zero variance. (`ddecompose` prints the same quantity divided by sqrt(2).)
    ///
    /// `None` arrives as `undefined` in JS (serde-wasm-bindgen) and as `null` over MCP and native
    /// JSON; a consumer must treat both as absent. Pinned by the browser-parity serializer case.
    pub normalised_difference: Option<f64>,
}

/// Support and small-sample diagnostics of one result (0120-MERIDIAN S6 / T12 / T13).
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SupportDiagnostics {
    /// Analysed rows per group.
    pub reference_count: usize,
    pub target_count: usize,
    /// Design columns of each group's regression, intercept and every level dummy included.
    pub model_columns: usize,
    /// `count - model_columns` per group (under the Pooled optimise target the fit that matters
    /// is the pooled one, whose df is `interval.degrees_of_freedom`). Signed: a value at or below zero never reaches a
    /// result, the run is refused (`INSUFFICIENT_RESIDUAL_DF`).
    pub reference_residual_df: i64,
    pub target_residual_df: i64,
    /// One entry per continuous predictor, in the order the request listed them.
    pub predictors: Vec<PredictorSupport>,
    /// Target rows whose leverage `x' (X_ref' X_ref)^-1 x` exceeds the largest leverage among
    /// the reference rows: the fair wage for them extends the reference pay line beyond the
    /// observed range. Under the Pooled optimise target the leverage is the pooled design's,
    /// with the indicator at 0, against its reference rows.
    pub extrapolated_target_count: usize,
}

/// Percentile-mode report (0120-MERIDIAN S8 / T16). `quantile_gap` is the percentile gap the
/// headline card claims; `rif_total` is the number the model decomposes (the difference of mean
/// RIF values, which `ddecompose` also reports) and which `total_gap` carries on this path.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct QuantileReport {
    pub tau: f64,
    /// `Q_tau(target) - Q_tau(reference)`, R `quantile(type = 7)` per group, the same
    /// orientation as `total_gap`.
    pub quantile_gap: f64,
    /// The RIF model's total (`total_gap` on this path), unchanged.
    pub rif_total: f64,
    pub reference: QuantileGroupReport,
    pub target: QuantileGroupReport,
}

/// One group's percentile and how well the data pins it down.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct QuantileGroupReport {
    pub count: usize,
    /// The group's type-7 percentile.
    pub quantile_value: f64,
    /// `F_n(q_tau)`: the share of the group's rows at or below `quantile_value`.
    pub ecdf_at_quantile: f64,
    /// `F_n(q_tau) - tau`.
    pub ecdf_offset: f64,
    /// Share of the group's rows exactly equal to `quantile_value`.
    pub tie_share: f64,
}

#[derive(Deserialize, Debug)]
pub enum OptimizationTarget {
    /// Match the reference group's own pay line.
    Reference,
    /// Match the pooled line with a target-group indicator, read at indicator 0. At the
    /// midpoint, `original_unexplained_gap` is that indicator's coefficient: the
    /// decomposition's `Pooled` unexplained gap (Elder et al. 2010). `model_coefficients` and
    /// each row's `contributions` are the pooled terms of the model's own columns, without the
    /// indicator. (Before T8 this stacked both groups with no indicator, which is
    /// `PooledNoIndicator`, Neumark.)
    Pooled,
}

#[derive(Deserialize, Debug)]
pub enum AllocationStrategy {
    Greedy,    // Sort by Gap Descending
    Equitable, // Distribute budget proportionally
}

#[derive(Deserialize, Debug)]
pub struct OptimizationRequest {
    // Same treatment as DecompositionRequest::csv_data. This struct is not flattened today, so it
    // is not currently broken — annotated for symmetry so that flattening it later cannot
    // reintroduce the bug silently, and so callers may send a Uint8Array here too.
    #[serde(with = "serde_bytes")]
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

/// One term of the fair-wage model: either a coefficient (`OptimizationResult::model_coefficients`)
/// or a coefficient times one employee's value (`Adjustment::contributions`).
///
/// RAW MODEL TERMS, not drivers (0120-MERIDIAN S3 ruling). The model is the reference group's
/// OLS fit under treatment coding, so a categorical level's term is measured against the
/// alphabetically first level, which has no entry: rename that level and every other level's
/// term changes. No consumer may rank these or present them as findings; the normalised
/// per-level numbers are `DecompositionResult::detailed_explained` / `detailed_unexplained`.
/// The constant is the entry whose `name` equals `intercept_token()`.
#[derive(Serialize, Debug)]
pub struct Contribution {
    pub name: String,
    pub value: f64,
}

#[derive(Serialize, Debug)]
pub struct Adjustment {
    /// POSITIONAL: the 0-based row ordinal among the parsed data rows of the CSV sent to the
    /// engine, counting rows the analysis excluded (a row with a blank model cell keeps its
    /// ordinal and simply has no adjustment). Correct as an offset (seven sites in this crate
    /// index a Vec / ChunkedArray / matrix-row map with it), wrong as an identity across a save
    /// boundary. Kept unchanged for every existing consumer and every payload already written.
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
    /// True when this employee's leverage exceeds the largest leverage among the baseline
    /// (reference) group's own rows, so `fair_wage` extends the baseline pay line beyond the
    /// range the baseline group occupies (0120-MERIDIAN S6 / T12). Always `false` for a
    /// reference-group row.
    pub extrapolated: bool,
}

/// The prediction interval behind `fair_wage_lower_bound` / `fair_wage_upper_bound` and
/// `is_defensible` (0120-MERIDIAN S7 / T14): Student t on the baseline regression's residual
/// degrees of freedom, `predict.lm(interval = "prediction")`.
///
/// Exact `predict.lm` on the fit the fair wage is read off, for both optimise targets: the
/// baseline group's regression for Reference, the pooled regression with a group indicator for
/// Pooled (T8), evaluated at indicator 0.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct IntervalBasis {
    /// The level used; 0.95 when the request gave none. A level outside [0.50, 0.999] is refused.
    pub confidence_level: f64,
    /// Residual degrees of freedom of the regression behind the fair wage, always positive here:
    /// `n - k` of the baseline group for Reference, `n_reference + n_target - k - 1` for Pooled.
    pub degrees_of_freedom: usize,
    /// The t quantile that multiplied the prediction standard error.
    pub critical_value: f64,
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
    /// Analysed (complete-case) reference-group rows. 0118-MERIDIAN S5.
    pub analysed_reference_count: usize,
    /// Analysed (complete-case) target-group rows. 0118-MERIDIAN S5.
    pub analysed_target_count: usize,
    /// Rows left out of the analysis because a model column was blank. 0118-MERIDIAN S5.
    pub excluded_rows: Vec<ExcludedRow>,
    /// Inbound proposed adjustments addressed to an excluded or unknown row, skipped and
    /// counted rather than applied (`check_defensibility`). `0` from `optimize`, which
    /// consumes none.
    pub adjustments_on_excluded_rows: usize,
    /// The interval every `fair_wage_lower_bound` / `fair_wage_upper_bound` was built with.
    pub interval: IntervalBasis,
    /// Support of the baseline group's pay line for the compared group (0120-MERIDIAN S6). Only
    /// the baseline group is fitted here, so only its residual degrees of freedom are judged.
    pub support: SupportDiagnostics,
    /// See [`DiagnosticWarning`]. Empty when nothing crossed a threshold.
    pub warnings: Vec<DiagnosticWarning>,
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
    /// Level of the prediction interval `check_defensibility` scores against, e.g. `0.90`.
    /// Refused outside [0.50, 0.999]; `None` means 0.95 (0120-MERIDIAN S7). `verify_adjustments`
    /// builds no interval and ignores it.
    #[serde(default)]
    pub confidence_level: Option<f64>,
}

/// The `check_defensibility` request: a verification request plus the pay line the amounts are
/// judged on (0120-MERIDIAN review N8, "E2-c" in the Track A plan).
///
/// `target` is the one the remedy was priced against: `Reference` (the baseline group's own
/// regression, the default and the only line before this field) or `Pooled` (the pooled regression
/// with a target-group indicator read at indicator 0, the optimiser's Pooled line). With it, the
/// bounds, the `extrapolated` flags and the `few_residual_df` warning describe the line the amounts
/// were computed on, so a remedy and its check can never judge extension on two different designs.
/// A separate struct, not a field of `VerificationRequest`, so `verify_adjustments` (which has no
/// line to choose) does not carry a field it would ignore.
#[derive(Deserialize, Debug)]
pub struct DefensibilityRequest {
    #[serde(flatten)]
    pub verification: VerificationRequest,
    #[serde(default)]
    pub target: Option<OptimizationTarget>,
}

#[derive(Serialize, Debug)]
pub struct FrontierPoint {
    pub budget: f64,
    pub t_statistic: f64,
    /// Two-sided p-value of `t_statistic` under Student t with `degrees_of_freedom` (0120 S7).
    pub p_value: f64,
    /// `p_value < 1 - confidence_level` (0.05 by default).
    pub is_significant: bool,
    /// The group-indicator coefficient of the pooled regression after this budget is paid: how
    /// far the compared group sits from the baseline group at equal characteristics, in outcome
    /// units. The number behind "after adjustment, the group coefficient is X (p = Y)".
    pub group_coefficient: f64,
    /// Residual degrees of freedom of the pooled regression (`n - k`).
    pub degrees_of_freedom: usize,
    /// The level `is_significant` was held against: the request's, or 0.95 when it gave none.
    pub confidence_level: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EfficientFrontierRequest {
    #[serde(flatten)]
    pub decomposition_params: DecompositionRequest,
    pub steps: Option<usize>,    // Default 50
    pub max_budget: Option<f64>, // If None, auto-detect
    /// Level whose complement is the significance threshold of `FrontierPoint::is_significant`.
    /// Refused outside [0.50, 0.999]; `None` means 0.95 (0120-MERIDIAN S7).
    #[serde(default)]
    pub confidence_level: Option<f64>,
}
