// oaxaca_blinder/src/builder.rs
use std::collections::HashMap;

use nalgebra::{DMatrix, DVector};
use polars::prelude::*;
use rayon::prelude::*;

use crate::decomposition::{
    detailed_decomposition, three_fold_decomposition, two_fold_decomposition, DetailedComponent,
    ReferenceCoefficients, ThreeFoldDecomposition, TwoFoldDecomposition,
};
use crate::error::OaxacaError;
use crate::estimation::{EstimationContext, Estimator, HeckmanEstimator, OlsEstimator};
use crate::formula::Formula;
use crate::inference::bootstrap_stats;
use crate::math::normalization::{
    factor_shares, normalize_categorical_coefficients, NormalizationConvention,
    NormalizationRecord, ShareMap,
};
use crate::math::ols::ols;
use crate::math::rif::{calculate_rif, calculate_rif_relative, calculate_rif_weighted};
use crate::math::weights::{check_weight, rescale_to_count, WeightsKind};
use crate::rng::{
    resample_frequency, resample_indices, unit_rng, RngPurpose, RunMetadata, DEFAULT_SEED,
};
use crate::rows::{
    DataMatricesWithRows, ExcludedRow, ExclusionReason, GroupMatrices, RowAccounting,
};
use crate::types::{ComponentResult, DecompositionDetail, OaxacaResults, TwoFoldResults};

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct SinglePassResult {
    three_fold: ThreeFoldDecomposition,
    two_fold: TwoFoldDecomposition,
    detailed_explained: Vec<DetailedComponent>,
    detailed_unexplained: Vec<DetailedComponent>,
    total_gap: f64,
    residuals_a: DVector<f64>,
    residuals_b: DVector<f64>,
    xa_mean: DVector<f64>,
    xb_mean: DVector<f64>,
    beta_star: DVector<f64>,
    detailed_selection: Vec<DetailedComponent>,
    /// The restriction weights this pass normalised under (empty when it did not normalise).
    /// Built from THIS pass's pooled rows, so a bootstrap replicate carries its own.
    shares: ShareMap,
}

/// Lightweight per-rep bootstrap estimates — only the scalar decomposition
/// components the SE/CI reduction actually consumes. Extracted from the full
/// [`SinglePassResult`] inside the parallel map so each rep's heavy fields
/// (`residuals_a/b`, `xa_mean`, `xb_mean`, `beta_star` — never read from
/// bootstrap reps; the final result reads those from the point estimate)
/// are freed immediately instead of accumulating R times.
///
/// This is the 0014-MERIDIAN Stage-2 memory fix (`mem-profile-report.md`
/// Finding 2): the retained bootstrap pile — not the thread count — was the
/// dominant memory term (~4 MiB/rep at 50k → ~40 GiB at the R=10 000 cap).
/// Retaining only these scalars (~KB/rep) drops peak into the portable
/// SharedArrayBuffer envelope. Field values are copied verbatim from
/// `SinglePassResult`, so the reduction output is numerically identical
/// (INV-02 within-platform byte-identity and the AC-9 mean-path parity
/// baseline both still hold).
#[derive(Clone)]
pub(crate) struct RepEstimates {
    /// Mean gap of the replicate (tests assert E+C+I and explained+unexplained against it).
    #[allow(dead_code)]
    total_gap: f64,
    three_fold: ThreeFoldDecomposition,
    two_fold: TwoFoldDecomposition,
    detailed_explained: Vec<DetailedComponent>,
    detailed_unexplained: Vec<DetailedComponent>,
    detailed_selection: Vec<DetailedComponent>,
}

impl RepEstimates {
    /// Copy only the consumed scalar components out of a full pass result. The
    /// borrowed `SinglePassResult` is dropped by the caller immediately after,
    /// freeing its residual/mean/beta allocations.
    fn from_pass(r: &SinglePassResult) -> Self {
        RepEstimates {
            total_gap: r.total_gap,
            three_fold: r.three_fold.clone(),
            two_fold: r.two_fold.clone(),
            detailed_explained: r.detailed_explained.clone(),
            detailed_unexplained: r.detailed_unexplained.clone(),
            detailed_selection: r.detailed_selection.clone(),
        }
    }
}

/// Outcome of one bootstrap replicate of the `OaxacaBuilder` paths. A failed replicate carries
/// the `variable=level` entries that were absent from one group's resample, if that was the
/// cause (0120-MERIDIAN F3), so the run can name which levels cost replicates.
enum Rep {
    Ok(RepEstimates),
    Failed(Vec<String>),
}

/// Name of the row-ordinal column added to the frame before cleaning (0118-MERIDIAN S1).
/// Reserved: a caller column with this name is a conflict and is refused by polars.
const ROW_ORDINAL_COL: &str = "__ob_row_ordinal__";

/// The weights column of `df` as plain floats. An integer column (a CSV of headcounts) is cast;
/// a column that does not parse as numbers is an error rather than silent nulls.
fn weight_values(df: &DataFrame, col: &str) -> Result<Float64Chunked, OaxacaError> {
    let raw = df.column(col)?.as_materialized_series();
    let cast = raw.cast(&DataType::Float64)?;
    if cast.null_count() > raw.null_count() {
        return Err(OaxacaError::PolarsError(PolarsError::ComputeError(
            format!("weights column '{col}' contains non-numeric values").into(),
        )));
    }
    Ok(cast.f64()?.clone())
}

/// Temporary row-position column used to name a rejected weight when the frame carries no
/// ordinal column. Never survives `clean_dataframe`.
const WEIGHT_POS_COL: &str = "__ob_weight_pos__";

pub struct OaxacaBuilder {
    dataframe: DataFrame,
    outcome: String,
    predictors: Vec<String>,
    categorical_predictors: Vec<String>,
    group: String,
    reference_group: String,
    bootstrap_reps: usize,
    reference_coeffs: ReferenceCoefficients,
    normalization_vars: Vec<String>,
    normalization_convention: NormalizationConvention,
    weights_col: Option<String>,
    weights_kind: Option<WeightsKind>,
    selection_outcome: Option<String>,
    selection_predictors: Vec<String>,
    /// Master seed for bootstrap resampling. `None` resolves to `DEFAULT_SEED` at `run()`.
    seed: Option<u64>,
}

pub(crate) struct GroupSplit {
    pub df_a: DataFrame,
    pub df_b: DataFrame,
    pub group_a_name: String,
    #[allow(dead_code)]
    pub group_b_name: String,
}

impl OaxacaBuilder {
    fn split_groups(&self, df: &DataFrame) -> Result<GroupSplit, OaxacaError> {
        let unique_groups = df.column(&self.group)?.unique()?.sort(SortOptions {
            descending: false,
            nulls_last: false,
            ..Default::default()
        })?;
        if unique_groups.len() < 2 {
            return Err(OaxacaError::InvalidGroupVariable(
                "Not enough groups for comparison".to_string(),
            ));
        }

        let group_b_name = self.reference_group.clone();
        let group_a_name_temp = unique_groups
            .str()?
            .get(0)
            .unwrap_or(self.reference_group.as_str())
            .to_string();
        let group_a_name = if group_a_name_temp == group_b_name {
            unique_groups.str()?.get(1).unwrap_or("").to_string()
        } else {
            group_a_name_temp
        };

        let df_a = df.filter(
            &df.column(&self.group)?
                .as_materialized_series()
                .equal(group_a_name.as_str())?,
        )?;
        let df_b = df.filter(
            &df.column(&self.group)?
                .as_materialized_series()
                .equal(group_b_name.as_str())?,
        )?;

        Ok(GroupSplit {
            df_a,
            df_b,
            group_a_name,
            group_b_name,
        })
    }

    /// 0118-MERIDIAN S2: the group column must hold the reference value and at most ONE other
    /// value. Checked once, on the RAW frame (before `clean_dataframe`), never inside
    /// `split_groups`, which runs per bootstrap replicate on the cleaned frame. Before this, a
    /// third value (`'Non-binary'`, `'Unknown'`, `'F '`) was silently dropped from the
    /// estimation frames by `split_groups` while the optimiser still listed its rows as
    /// target employees, which shifted every pairing after the first such row.
    ///
    /// Values are compared exactly as they sit in the file: no trimming, no case folding. A
    /// group column that is not a string column, or is missing, is left to the existing
    /// downstream errors so their wording is unchanged.
    fn check_group_values(&self) -> Result<(), OaxacaError> {
        let Ok(col) = self.dataframe.column(&self.group) else {
            return Ok(());
        };
        let Ok(values) = col.as_materialized_series().str() else {
            return Ok(());
        };

        let mut reference_present = false;
        let mut others: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for v in values.into_iter().flatten() {
            if v == self.reference_group {
                reference_present = true;
            } else if !others.contains(v) {
                others.insert(v.to_string());
            }
        }

        if !reference_present {
            return Err(OaxacaError::ReferenceGroupAbsent {
                group_column: self.group.clone(),
                reference_group: self.reference_group.clone(),
            });
        }
        if others.len() > 1 {
            return Err(OaxacaError::TooManyGroupValues {
                group_column: self.group.clone(),
                reference_group: self.reference_group.clone(),
                other_values: others.into_iter().collect(),
            });
        }
        Ok(())
    }

    /// D1 (0014-close round-1) pre-flight refusal: a categorical predictor level
    /// present in the full dataset but entirely absent from one comparison group
    /// collapses that group's own design matrix to a singular `X'X` (either as an
    /// exact-zero dummy column for an absent non-reference level, or as an exact
    /// intercept/dummy-sum collinearity when the absent level is the reference
    /// itself) — `math/ols.rs`'s Cholesky check already rejects both shapes, just
    /// without naming the column/level/group. This runs on the RAW categorical
    /// columns (present in `df_a`/`df_b` regardless of dummy encoding), so it
    /// only ever refuses a case that already fails today; data that estimates
    /// today has every level in both groups and passes through untouched.
    ///
    /// Scan order is deterministic: `self.categorical_predictors` column order,
    /// then ascending level order within a column (same sort as
    /// `create_dummies_manual`) — so the same data always names the same
    /// offender first, and group A is checked before group B for a given level.
    ///
    /// The candidate level set is every level present in either comparison frame.
    /// `check_group_values` (0118-MERIDIAN S2) guarantees the group column holds
    /// the reference plus at most one other value, so after cleaning the two frames
    /// together ARE the unsplit frame the dummy columns were encoded from.
    fn check_level_confinement(
        &self,
        df_a: &DataFrame,
        df_b: &DataFrame,
        group_a_name: &str,
        group_b_name: &str,
    ) -> Result<(), OaxacaError> {
        for cat_pred in &self.categorical_predictors {
            let levels_in_a = self.weighted_levels_present(df_a, cat_pred)?;
            let levels_in_b = self.weighted_levels_present(df_b, cat_pred)?;

            let mut full_levels: Vec<&str> = Vec::new();
            for frame in [df_a, df_b] {
                full_levels.extend(
                    frame
                        .column(cat_pred)?
                        .as_materialized_series()
                        .str()?
                        .into_iter()
                        .flatten(),
                );
            }
            full_levels.sort_unstable();
            full_levels.dedup();

            for level in full_levels {
                if !levels_in_a.contains(level) {
                    return Err(OaxacaError::EmptyLevelInGroup {
                        column: cat_pred.clone(),
                        level: level.to_string(),
                        missing_from_group: group_a_name.to_string(),
                    });
                }
                if !levels_in_b.contains(level) {
                    return Err(OaxacaError::EmptyLevelInGroup {
                        column: cat_pred.clone(),
                        level: level.to_string(),
                        missing_from_group: group_b_name.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Levels of `cat_pred` carrying positive total weight in `df`. Without a
    /// weights column this is plain row presence; with one, a level whose rows
    /// all carry zero weight is absent for estimation purposes even though its
    /// rows exist. Nulls are already dropped by `clean_dataframe` upstream.
    fn weighted_levels_present(
        &self,
        df: &DataFrame,
        cat_pred: &str,
    ) -> Result<std::collections::HashSet<String>, OaxacaError> {
        let levels = df.column(cat_pred)?.as_materialized_series().str()?.clone();

        let Some(w_col) = &self.weights_col else {
            return Ok(levels.into_iter().flatten().map(String::from).collect());
        };

        let weights = weight_values(df, w_col)?;
        let mut present = std::collections::HashSet::new();
        for (level, weight) in levels.into_iter().zip(weights.into_iter()) {
            if let (Some(level), Some(weight)) = (level, weight) {
                if weight > 0.0 {
                    present.insert(level.to_string());
                }
            }
        }
        Ok(present)
    }
}

impl OaxacaBuilder {
    /// Creates a new `OaxacaBuilder`.
    ///
    /// # Arguments
    ///
    /// * `dataframe` - A `polars::DataFrame` containing the data for the analysis.
    /// * `outcome` - The name of the column representing the outcome variable (e.g., "wage").
    /// * `group` - The name of the column that divides the data into two groups (e.g., "gender").
    /// * `reference_group` - The value within the `group` column that identifies the reference group, "Group B": the baseline whose pay structure `ReferenceCoefficients::GroupB` applies. It is a choice of baseline and says nothing about which group earns less. Every other value is the compared group, "Group A".
    pub fn new(dataframe: DataFrame, outcome: &str, group: &str, reference_group: &str) -> Self {
        Self {
            dataframe,
            outcome: outcome.to_string(),
            predictors: Vec::new(),
            categorical_predictors: Vec::new(),
            group: group.to_string(),
            reference_group: reference_group.to_string(),
            bootstrap_reps: 20,
            reference_coeffs: ReferenceCoefficients::GroupB,
            normalization_vars: Vec::new(),
            normalization_convention: NormalizationConvention::default(),
            weights_col: None,
            weights_kind: None,
            selection_outcome: None,
            selection_predictors: Vec::new(),
            seed: None,
        }
    }

    /// Creates a new `OaxacaBuilder` using an R-style formula.
    ///
    /// # Arguments
    ///
    /// * `dataframe` - The Polars DataFrame containing the data.
    /// * `formula` - A string representing the model formula (e.g., "wage ~ education + experience + C(sector)").
    /// * `group` - The name of the column defining the two groups.
    /// * `reference_group` - The value in the `group` column representing the reference group (Group B).
    pub fn from_formula(
        dataframe: DataFrame,
        formula: &str,
        group: &str,
        reference_group: &str,
    ) -> Result<Self, OaxacaError> {
        let parsed_formula = Formula::parse(formula)?;
        Ok(Self {
            dataframe,
            outcome: parsed_formula.outcome,
            predictors: parsed_formula.predictors,
            categorical_predictors: parsed_formula.categorical_predictors,
            group: group.to_string(),
            reference_group: reference_group.to_string(),
            bootstrap_reps: 20,
            reference_coeffs: ReferenceCoefficients::GroupB,
            normalization_vars: Vec::new(),
            normalization_convention: NormalizationConvention::default(),
            weights_col: None,
            weights_kind: None,
            selection_outcome: None,
            selection_predictors: Vec::new(),
            seed: None,
        })
    }

    /// Sets the reference coefficients for the decomposition.
    ///
    /// The default is `ReferenceCoefficients::GroupB`.
    pub fn reference_coefficients(&mut self, reference: ReferenceCoefficients) -> &mut Self {
        self.reference_coeffs = reference;
        self
    }

    /// Sets the predictor variables for the model.
    ///
    /// # Arguments
    ///
    /// * `predictors` - An iterator over strings representing the column names of the predictor variables.
    pub fn predictors<I, S>(&mut self, predictors: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.predictors = predictors.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Sets the categorical predictor variables for the model.
    ///
    /// # Arguments
    ///
    /// * `predictors` - An iterator over strings representing the column names of the categorical predictor variables.
    pub fn categorical_predictors<I, S>(&mut self, predictors: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.categorical_predictors = predictors.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Sets the number of bootstrap replications for standard error calculation.
    ///
    /// # Arguments
    ///
    /// * `reps` - The number of bootstrap samples to generate. Defaults to 100.
    pub fn bootstrap_reps(&mut self, reps: usize) -> &mut Self {
        self.bootstrap_reps = reps;
        self
    }

    /// Sets a fixed master seed for all bootstrap resampling. Reproducible-by-default:
    /// the same seed and input always produce byte-identical output within a platform.
    pub fn seed(&mut self, seed: u64) -> &mut Self {
        self.seed = Some(seed);
        self
    }

    /// Copies an `Option<u64>` master seed verbatim (`None` stays `None`, resolving to
    /// `DEFAULT_SEED` at `run()`). Used to forward a seed to an inner builder — e.g.
    /// `decompose_quantile` — so `.seed(X)` reproduces on the RIF quantile path (CV-1).
    pub fn seed_opt(&mut self, seed: Option<u64>) -> &mut Self {
        self.seed = seed;
        self
    }

    /// Draws one master seed from OS entropy and records it in `RunMetadata`, so the
    /// run stays reproducible after the fact via `.seed(recorded_value)`. Native-only:
    /// `oaxaca_blinder` has no direct `getrandom` dependency, so entropy seeding is a
    /// CLI/native affordance; the wasm path uses `DEFAULT_SEED` or an explicit seed (D1).
    #[cfg(not(target_family = "wasm"))]
    pub fn seed_from_entropy(&mut self) -> &mut Self {
        self.seed = Some(crate::rng::draw_entropy_seed());
        self
    }

    /// Sets the categorical variables whose detailed contributions are re-expressed as
    /// deviations from the share-weighted average of ALL their levels (the dropped level
    /// included), instead of from whichever level sorts first. Aggregates do not change.
    ///
    /// Opt-in per call: a builder that never calls this returns the raw treatment-coded
    /// detail (the library goldens stay raw). The shipped engine and CLI call
    /// `.normalize(categorical_predictors)` on every run (0120-MERIDIAN T1).
    ///
    /// Names that are not in `categorical_predictors` are ignored. With `heckman_selection`
    /// set the normalisation is skipped on A, B and the pooled fit together and
    /// `run_metadata.normalization` records `applied: false`.
    ///
    /// # Arguments
    ///
    /// * `vars` - An iterator over strings representing the column names of the categorical variables to normalize.
    pub fn normalize<I, S>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.normalization_vars = vars.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Normalises every categorical predictor: shorthand for
    /// `normalize(categorical_predictors)`. Call it AFTER the categorical predictors are set
    /// (`categorical_predictors(..)`, or `from_formula`, which sets them from `C(..)` terms).
    /// This is what the shipped engine and the CLI do on every run (0120-MERIDIAN T1).
    pub fn normalize_all_categoricals(&mut self) -> &mut Self {
        self.normalization_vars = self.categorical_predictors.clone();
        self
    }

    /// Chooses the restriction weights for [`normalize`](Self::normalize). Default
    /// [`NormalizationConvention::PopulationShare`] (0120-MERIDIAN D1);
    /// [`NormalizationConvention::EqualShare`] reproduces Stata `categorical()`, R `oaxaca`
    /// and `ddecompose(normalize_factors = TRUE)`.
    pub fn normalization_convention(&mut self, convention: NormalizationConvention) -> &mut Self {
        self.normalization_convention = convention;
        self
    }

    /// Sets the column name for sample weights.
    ///
    /// # Arguments
    ///
    /// * `weights` - The name of the column containing sample weights.
    pub fn weights(&mut self, weights: &str) -> &mut Self {
        self.weights_col = Some(weights.to_string());
        self
    }

    /// States what the weights column means (0120-MERIDIAN S9 / T17). REQUIRED whenever
    /// [`weights`](Self::weights) is set; a run without it is refused with
    /// [`OaxacaError::WeightsKindRequired`].
    ///
    /// * [`WeightsKind::Frequency`]: whole-number replication counts; `w = 2` is the row twice,
    ///   in the point estimates and in the bootstrap (a replicate draws `sum(w)` employees, so
    ///   standard errors and p-values match the repeated rows). A fractional weight is refused,
    ///   naming the row.
    /// * [`WeightsKind::Relative`]: relative importance (FTE, design weights). Each regression's
    ///   weights are rescaled to sum to its row count, and the RIF quantile is
    ///   `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)`. Uniform weights change nothing.
    pub fn weights_kind(&mut self, kind: WeightsKind) -> &mut Self {
        self.weights_kind = Some(kind);
        self
    }

    /// Configures the Heckman selection model.
    ///
    /// # Arguments
    ///
    /// * `outcome` - The binary selection variable (e.g., "employed").
    /// * `predictors` - The predictors for the selection equation (should include exclusion restriction).
    pub fn heckman_selection<I, S>(&mut self, outcome: &str, predictors: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.selection_outcome = Some(outcome.to_string());
        self.selection_predictors = predictors.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Exposes the internal data matrices for advanced usage (e.g., optimization).
    /// This method prepares the data (creating dummies, etc.) and returns the matrices for Group A and Group B.
    ///
    /// Returns `(X_A, y_A, X_B, y_B, predictor_names)`. Per `split_groups`, which sets
    /// `group_b_name = reference_group`, the binding is:
    ///   * **A = the NON-reference group** (every group value that is not `reference_group`),
    ///   * **B = the reference group** (the `reference_group` value passed to `new`/`from_formula`).
    ///
    /// A consumer that solves a fair-wage standard from the advantaged/reference group must
    /// therefore bind the *third* returned matrix (`X_B`), not the first. See the engine crate's
    /// `ab_binding_regression_test` for the guardrail that locks this convention against the
    /// recurring "A = reference" mistake.
    ///
    /// A consumer that must map a matrix row back to an employee needs
    /// [`get_data_matrices_with_rows`](Self::get_data_matrices_with_rows) instead: this tuple
    /// carries no row ordinals, and rows with a blank in any model column are dropped from it.
    #[allow(clippy::type_complexity)]
    pub fn get_data_matrices(
        &self,
    ) -> Result<
        (
            DMatrix<f64>,
            DVector<f64>,
            DMatrix<f64>,
            DVector<f64>,
            Vec<String>,
        ),
        OaxacaError,
    > {
        let m = self.get_data_matrices_with_rows()?;
        Ok((
            m.target.x,
            m.target.y,
            m.reference.x,
            m.reference.y,
            m.predictor_names,
        ))
    }

    /// 0118-MERIDIAN S1: the data matrices together with the original row ordinal of every
    /// matrix row and the list of excluded rows, all from ONE `clean_dataframe` +
    /// `split_groups` pass (a row-ordinal column is added before cleaning and read back after
    /// the split), so the ordinals cannot disagree with the matrices.
    ///
    /// Groups are labelled `reference` and `target`, never A/B: `target` holds every row whose
    /// group value is not `reference_group` (at most one other value, see
    /// [`OaxacaError::TooManyGroupValues`]).
    ///
    /// An ordinal is the zero-based position among the parsed data rows of the frame given to
    /// the builder. `GroupMatrices::rows[i]` is the ordinal of matrix row `i`.
    pub fn get_data_matrices_with_rows(&self) -> Result<DataMatricesWithRows, OaxacaError> {
        let (mut df, excluded_rows, total_rows) = self.prepare_clean_frame()?;

        let mut all_dummy_names = Vec::new();

        if !self.categorical_predictors.is_empty() {
            for cat_pred in &self.categorical_predictors {
                let series = df.column(cat_pred)?;
                let (dummies, _, _) =
                    self.create_dummies_manual(series.as_materialized_series())?;
                for s in dummies.get_columns() {
                    all_dummy_names.push(s.name().to_string());
                }
                df = df.hstack(dummies.get_columns())?;
            }
        }

        // `split_groups` names its frames by the legacy A/B convention: `df_a` is the
        // non-reference (target) group, `df_b` the reference group.
        let groups = self.split_groups(&df)?;
        let target_rows = Self::row_ordinals(&groups.df_a)?;
        let reference_rows = Self::row_ordinals(&groups.df_b)?;

        let (x_target, y_target, _, predictor_names) =
            self.prepare_data(&groups.df_a, &all_dummy_names, &[])?;
        let (x_reference, y_reference, _, _) =
            self.prepare_data(&groups.df_b, &all_dummy_names, &[])?;

        Ok(DataMatricesWithRows {
            reference: GroupMatrices {
                x: x_reference,
                y: y_reference,
                rows: reference_rows,
            },
            target: GroupMatrices {
                x: x_target,
                y: y_target,
                rows: target_rows,
            },
            predictor_names,
            excluded_rows,
            total_rows,
        })
    }

    /// 0118-MERIDIAN S1: which original rows are analysed (per group) and which are excluded,
    /// without building the matrices. Same cleaning and split as
    /// [`get_data_matrices_with_rows`](Self::get_data_matrices_with_rows).
    pub fn analysed_rows(&self) -> Result<RowAccounting, OaxacaError> {
        let (df, excluded_rows, total_rows) = self.prepare_clean_frame()?;
        let groups = self.split_groups(&df)?;
        Ok(RowAccounting {
            total_rows,
            reference_rows: Self::row_ordinals(&groups.df_b)?,
            target_rows: Self::row_ordinals(&groups.df_a)?,
            excluded_rows,
        })
    }

    /// The single cleaning pass behind S1: group-value check on the raw frame, ordinal column
    /// added, `clean_dataframe`, and the excluded-row list from the same raw frame. Returns the
    /// cleaned frame (still carrying the ordinal column), the excluded rows, and the raw row
    /// count.
    fn prepare_clean_frame(&self) -> Result<(DataFrame, Vec<ExcludedRow>, usize), OaxacaError> {
        self.check_group_values()?;

        let total_rows = self.dataframe.height();
        let indexed = self
            .dataframe
            .clone()
            .with_row_index(ROW_ORDINAL_COL.into(), None)?;
        let cleaned = self.clean_dataframe(&indexed)?;
        let excluded_rows = self.excluded_rows(&self.dataframe)?;

        // Internal consistency: every raw row is either analysed or excluded, exactly once.
        if cleaned.height() + excluded_rows.len() != total_rows {
            return Err(OaxacaError::PolarsError(PolarsError::ComputeError(
                format!(
                    "row accounting mismatch: {} analysed + {} excluded != {} rows",
                    cleaned.height(),
                    excluded_rows.len(),
                    total_rows
                )
                .into(),
            )));
        }
        Ok((cleaned, excluded_rows, total_rows))
    }

    /// Reads the ordinal column back as `usize`s, in frame row order.
    fn row_ordinals(df: &DataFrame) -> Result<Vec<usize>, OaxacaError> {
        let col = df.column(ROW_ORDINAL_COL)?.as_materialized_series();
        let ca = col.idx()?;
        Ok(ca
            .into_no_null_iter()
            .map(|v| v as usize)
            .collect::<Vec<usize>>())
    }

    /// The columns `clean_dataframe` drops nulls on, in check order, each with the reason a
    /// blank in it excludes a row. One source for both the cleaning and the exclusion report,
    /// so the report names exactly the rules the cleaning applied.
    fn cleaning_columns(&self) -> Vec<(String, ExclusionReason)> {
        let mut cols = vec![
            (self.outcome.clone(), ExclusionReason::Outcome),
            (self.group.clone(), ExclusionReason::GroupValue),
        ];
        cols.extend(
            self.predictors
                .iter()
                .map(|c| (c.clone(), ExclusionReason::NumericPredictor)),
        );
        cols.extend(
            self.categorical_predictors
                .iter()
                .map(|c| (c.clone(), ExclusionReason::CategoricalPredictor)),
        );
        if let Some(w) = &self.weights_col {
            cols.push((w.to_string(), ExclusionReason::Weights));
        }
        if let Some(sel_out) = &self.selection_outcome {
            cols.push((sel_out.to_string(), ExclusionReason::SelectionOutcome));
        }
        cols.extend(
            self.selection_predictors
                .iter()
                .map(|c| (c.clone(), ExclusionReason::SelectionPredictor)),
        );
        cols
    }

    /// Every row with a blank in a cleaning column, one entry per row, ascending by ordinal.
    /// Computed from the raw frame's null masks; `prepare_clean_frame` cross-checks the count
    /// against what `drop_nulls` actually removed.
    fn excluded_rows(&self, raw: &DataFrame) -> Result<Vec<ExcludedRow>, OaxacaError> {
        let mut by_row: std::collections::BTreeMap<usize, ExcludedRow> =
            std::collections::BTreeMap::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (name, reason) in self.cleaning_columns() {
            if !seen.insert(name.clone()) {
                // A column named twice (e.g. a predictor that is also the weights column) is
                // reported once, under the first reason it was checked for.
                continue;
            }
            let mask = raw.column(&name)?.as_materialized_series().is_null();
            for (idx, is_null) in mask.into_iter().enumerate() {
                if is_null == Some(true) {
                    let entry = by_row.entry(idx).or_insert_with(|| ExcludedRow {
                        index: idx,
                        reasons: Vec::new(),
                        columns: Vec::new(),
                    });
                    if !entry.reasons.contains(&reason) {
                        entry.reasons.push(reason);
                    }
                    entry.columns.push(name.clone());
                }
            }
        }
        Ok(by_row.into_values().collect())
    }

    #[allow(clippy::type_complexity)]
    fn prepare_data(
        &self,
        df: &DataFrame,
        all_dummy_names: &[String],
        extra_predictors: &[String],
    ) -> Result<
        (
            DMatrix<f64>,
            DVector<f64>,
            Option<DVector<f64>>,
            Vec<String>,
        ),
        OaxacaError,
    > {
        let y_series = df.column(&self.outcome)?.f64()?;
        // Safe to unwrap because we ran clean_dataframe
        let y_vec: Vec<f64> = y_series
            .into_iter()
            .map(|opt| {
                opt.ok_or_else(|| {
                    OaxacaError::PolarsError(PolarsError::ComputeError(
                        "Null values found in outcome after cleaning".into(),
                    ))
                })
            })
            .collect::<Result<Vec<f64>, _>>()?;
        let y = DVector::from_vec(y_vec);

        let mut current_predictors = self.predictors.clone();
        current_predictors.extend_from_slice(extra_predictors);

        let mut final_predictors: Vec<String> = vec![crate::INTERCEPT_NAME.to_string()];
        final_predictors.extend_from_slice(&current_predictors);
        final_predictors.extend_from_slice(all_dummy_names);

        let mut x_df = df.select(&current_predictors)?;
        let intercept_series = Series::new(crate::INTERCEPT_NAME.into(), vec![1.0; df.height()]);
        x_df.with_column(intercept_series)?;

        for name in all_dummy_names {
            if df
                .get_column_names()
                .iter()
                .any(|s| s.as_str() == name.as_str())
            {
                x_df.with_column(df.column(name)?.clone())?;
            } else {
                let zero_series = Series::new(name.into(), vec![0.0; df.height()]);
                x_df.with_column(zero_series)?;
            }
        }

        let x_df_selected = x_df.select(&final_predictors)?;
        let x_matrix = x_df_selected.to_ndarray::<Float64Type>(IndexOrder::Fortran)?;
        let x_vec: Vec<f64> = x_matrix.iter().copied().collect();
        let final_names = x_df_selected
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        let weights = if let Some(w_col) = &self.weights_col {
            let w_series = weight_values(df, w_col)?;
            let w_vec: Vec<f64> = w_series
                .into_iter()
                .map(|opt| {
                    opt.ok_or_else(|| {
                        OaxacaError::PolarsError(PolarsError::ComputeError(
                            "Null weights found after cleaning".into(),
                        ))
                    })
                })
                .collect::<Result<Vec<f64>, _>>()?;
            // Relative weights are rescaled so each regression's weights sum to its row count
            // (0120-MERIDIAN T17). Frequency weights are used as given: w = 2 is the row twice.
            let w_vec = if self.weights_kind == Some(WeightsKind::Relative) {
                rescale_to_count(&w_vec)
            } else {
                w_vec
            };
            Some(DVector::from_vec(w_vec))
        } else {
            None
        };

        Ok((
            DMatrix::from_row_slice(x_df_selected.height(), x_df_selected.width(), &x_vec),
            y,
            weights,
            final_names,
        ))
    }

    fn create_dummies_manual(
        &self,
        series: &Series,
    ) -> Result<(DataFrame, usize, String), OaxacaError> {
        let unique_vals = series.unique()?.sort(SortOptions {
            descending: false,
            nulls_last: false,
            ..Default::default()
        })?;
        let m = unique_vals.len();
        let mut dummy_vars: Vec<Series> = Vec::new();

        let reference_val = if let Some(s) = unique_vals.str()?.get(0) {
            s
        } else {
            return Err(OaxacaError::InvalidGroupVariable(format!(
                "Could not get reference category for {}",
                series.name()
            )));
        };
        let reference_name = format!("{}_{}", series.name(), reference_val);

        for val in unique_vals.str()?.into_iter().flatten().skip(1) {
            // Skip the first category as the reference
            let dummy_name = format!("{}_{}", series.name(), val);
            let ca = series.equal(val)?;
            let mut dummy_series = ca.into_series();
            dummy_series = dummy_series.cast(&DataType::Float64)?;
            dummy_series.rename(dummy_name.as_str().into());
            dummy_vars.push(dummy_series);
        }

        Ok((
            DataFrame::new(dummy_vars.into_iter().map(Column::Series).collect())
                .map_err(OaxacaError::from)?,
            m,
            reference_name,
        ))
    }

    /// True when this run re-expresses categorical coefficients under a share restriction:
    /// `normalize()` was called and no Heckman selection is configured. The Heckman estimator
    /// ignores normalisation, so with a selection model A, B and the pooled fit all stay raw
    /// together (a half-normalised run would add up wrongly).
    fn normalization_active(&self) -> bool {
        !self.normalization_vars.is_empty() && self.selection_outcome.is_none()
    }

    /// Restriction weights for this pass, built from the POOLED analysed rows of `df` (group A
    /// union group B after cleaning): sum of observation weights per level when a weights
    /// column is set, row counts otherwise, base level included. Returned keyed by variable
    /// name and level name, so no column position is involved. Empty when the run does not
    /// normalise.
    ///
    /// Called once per `run_single_pass`; a bootstrap replicate therefore uses the shares of
    /// its own pooled resample, and `run_metadata.normalization` records the point sample's.
    fn compute_shares(
        &self,
        df: &DataFrame,
        all_dummy_names: &[String],
    ) -> Result<ShareMap, OaxacaError> {
        let mut shares = ShareMap::new();
        if !self.normalization_active() {
            return Ok(shares);
        }
        let weights: Option<Vec<f64>> = match &self.weights_col {
            Some(w_col) => Some(
                weight_values(df, w_col)?
                    .into_iter()
                    .map(|v| {
                        v.ok_or_else(|| {
                            OaxacaError::NormalizationError(
                                "null weight while computing level shares".to_string(),
                            )
                        })
                    })
                    .collect::<Result<Vec<f64>, _>>()?,
            ),
            None => None,
        };
        // Ignore names that are not categorical predictors (nothing was dummy-coded for them).
        let mut vars: Vec<&String> = self
            .normalization_vars
            .iter()
            .filter(|v| self.categorical_predictors.contains(v))
            .collect();
        vars.sort();
        vars.dedup();
        for var in vars {
            let col = df.column(var)?.as_materialized_series().str()?.clone();
            let levels: Vec<&str> = col
                .into_iter()
                .map(|v| {
                    v.ok_or_else(|| {
                        OaxacaError::NormalizationError(format!(
                            "null level in '{}' while computing level shares",
                            var
                        ))
                    })
                })
                .collect::<Result<Vec<&str>, _>>()?;
            shares.insert(
                var.clone(),
                factor_shares(
                    var,
                    &levels,
                    weights.as_deref(),
                    all_dummy_names,
                    self.normalization_convention,
                )?,
            );
        }
        Ok(shares)
    }

    /// The pooled-regression `beta*` of the `Pooled` (with a group indicator, whose row is
    /// dropped) and `PooledNoIndicator` (without one) schemes, normalised under `shares`.
    /// Returns `beta*` and, when normalising, each variable's base-level coefficient.
    fn pooled_beta_star(
        &self,
        df_a: &DataFrame,
        df_b: &DataFrame,
        group_a_name: &str,
        all_dummy_names: &[String],
        with_indicator: bool,
        shares: &ShareMap,
    ) -> Result<(DVector<f64>, HashMap<String, f64>), OaxacaError> {
        let mut df_pooled = df_a.vstack(df_b)?;
        let extra: Vec<String> = if with_indicator {
            let group_indicator = Series::new(
                "__ob_group_indicator__".into(),
                df_pooled
                    .column(&self.group)?
                    .as_materialized_series()
                    .equal(group_a_name)?
                    .into_series()
                    .cast(&DataType::Float64)?,
            );
            df_pooled.with_column(group_indicator)?;
            vec!["__ob_group_indicator__".to_string()]
        } else {
            Vec::new()
        };

        let (x_pooled, y_pooled, w_pooled, pooled_predictor_names) =
            self.prepare_data(&df_pooled, all_dummy_names, &extra)?;

        let mut ols_pooled = ols(&y_pooled, &x_pooled, w_pooled.as_ref())?;

        let base_coeffs = if shares.is_empty() {
            HashMap::new()
        } else {
            normalize_categorical_coefficients(&mut ols_pooled, &pooled_predictor_names, shares)?
        };

        let beta_star = if with_indicator {
            let group_indicator_idx = pooled_predictor_names
                .iter()
                .position(|r| r == "__ob_group_indicator__")
                .ok_or_else(|| {
                    OaxacaError::NalgebraError(
                        "group_indicator not found in pooled model predictors".to_string(),
                    )
                })?;
            ols_pooled.coefficients.remove_row(group_indicator_idx)
        } else {
            ols_pooled.coefficients
        };
        Ok((beta_star, base_coeffs))
    }

    fn run_single_pass(
        &self,
        df: &DataFrame,
        all_dummy_names: &[String],
    ) -> Result<SinglePassResult, OaxacaError> {
        let groups = self.split_groups(df)?;
        let df_a = groups.df_a;
        let df_b = groups.df_b;
        let group_a_name = groups.group_a_name;
        if df_a.height() == 0 || df_b.height() == 0 {
            return Err(OaxacaError::InvalidGroupVariable(
                "One group has no data".to_string(),
            ));
        }

        let (x_a, y_a, w_a, predictor_names) = self.prepare_data(&df_a, all_dummy_names, &[])?;
        let (x_b, y_b, w_b, _) = self.prepare_data(&df_b, all_dummy_names, &[])?;

        // One restriction for the whole pass: beta_A, beta_B, the pooled fit and the weighted
        // mix are all normalised under these shares (0120-MERIDIAN T2).
        let shares = self.compute_shares(df, all_dummy_names)?;

        let ctx = EstimationContext {
            df_a: &df_a,
            df_b: &df_b,
            x_a: &x_a,
            y_a: &y_a,
            w_a: &w_a,
            x_b: &x_b,
            y_b: &y_b,
            w_b: &w_b,
            predictor_names: &predictor_names,
            shares: &shares,
        };

        let estimator: Box<dyn Estimator> = if let Some(sel_outcome) = &self.selection_outcome {
            Box::new(HeckmanEstimator {
                selection_outcome: sel_outcome.clone(),
                selection_predictors: self.selection_predictors.clone(),
            })
        } else {
            Box::new(OlsEstimator)
        };

        let result = estimator.estimate(&ctx)?;

        // ... Calculate beta_star and decompositions ...
        let beta_a = &result.beta_a;
        let beta_b = &result.beta_b;
        let beta_a_raw = &result.beta_a_raw;
        let beta_b_raw = &result.beta_b_raw;
        let xa_mean = result.xa_mean;
        let xb_mean = result.xb_mean;
        let final_predictor_names = result.predictor_names;
        let residuals_a = result.residuals_a;
        let residuals_b = result.residuals_b;
        let base_coeffs_a = result.base_coeffs_a;
        let base_coeffs_b = result.base_coeffs_b;

        // Detailed Selection Decomposition (if Heckman)
        let mut detailed_selection_components = Vec::new();
        if let (
            Some(gamma_a),
            Some(gamma_b),
            Some(z_mean_a),
            Some(z_mean_b),
            Some(delta_a),
            Some(delta_b),
            Some(_sel_names),
        ) = (
            &result.selection_coeffs_a,
            &result.selection_coeffs_b,
            &result.selection_means_a,
            &result.selection_means_b,
            result.imr_delta_a,
            result.imr_delta_b,
            &result.selection_names,
        ) {
            // Identify theta (IMR coefficient). It's the last element of beta.
            // But which reference group?
            // Explained selection = theta_ref * (lambda_a - lambda_b)
            // Approx = theta_ref * delta_ref * gamma_ref * (Z_A - Z_B)

            let (theta_ref, delta_ref, gamma_ref) = match self.reference_coeffs {
                ReferenceCoefficients::GroupA => (beta_a[beta_a.len() - 1], delta_a, gamma_a),
                ReferenceCoefficients::GroupB => (beta_b[beta_b.len() - 1], delta_b, gamma_b),
                _ => (beta_b[beta_b.len() - 1], delta_b, gamma_b), // Default/Simplified
            };

            // Selection names usually include intercept at 0.
            // gamma and z_mean should align with sel_names.
            // However, heckman_two_step probit includes intercept.
            // Check self.selection_predictors.
            // If Estimator logic added intercept, we need to match indices.
            // HeckmanEstimator::prepare_selection_data adds intercept at col 0.

            // We iterate through selection predictors.
            // We assume gamma, z_mean, and sel_names are aligned including intercept.
            // But we might want to skip intercept for "contribution"? Or include it?
            // Usually we show variables.

            // Reconstruct selection variable names: "intercept" + sel_predictors
            let mut full_sel_names = vec![crate::INTERCEPT_NAME.to_string()];
            full_sel_names.extend(self.selection_predictors.clone());

            // Check dimensions
            if gamma_ref.len() == full_sel_names.len() && z_mean_a.len() == full_sel_names.len() {
                for (i, name) in full_sel_names.iter().enumerate() {
                    let diff_z = z_mean_a[i] - z_mean_b[i];
                    let contribution = theta_ref * delta_ref * gamma_ref[i] * diff_z;
                    detailed_selection_components.push(DetailedComponent {
                        variable_name: name.clone(),
                        contribution,
                    });
                }
            }
        }

        let mut base_coeffs_star: HashMap<String, f64> = HashMap::new();
        let beta_star_owned: DVector<f64>;
        #[allow(deprecated)]
        let beta_star: &DVector<f64> = match self.reference_coeffs {
            ReferenceCoefficients::GroupA => {
                base_coeffs_star = base_coeffs_a.clone();
                beta_a
            }
            ReferenceCoefficients::GroupB => {
                base_coeffs_star = base_coeffs_b.clone();
                beta_b
            }
            ReferenceCoefficients::Pooled | ReferenceCoefficients::Neumark => {
                let (b, base) = self.pooled_beta_star(
                    &df_a,
                    &df_b,
                    &group_a_name,
                    all_dummy_names,
                    true,
                    &shares,
                )?;
                beta_star_owned = b;
                base_coeffs_star = base;
                &beta_star_owned
            }
            ReferenceCoefficients::PooledNoIndicator => {
                let (b, base) = self.pooled_beta_star(
                    &df_a,
                    &df_b,
                    &group_a_name,
                    all_dummy_names,
                    false,
                    &shares,
                )?;
                beta_star_owned = b;
                base_coeffs_star = base;
                &beta_star_owned
            }
            ReferenceCoefficients::Weighted | ReferenceCoefficients::Cotton => {
                // The mix weight is each group's share of the pooled population. Under Relative
                // weights `w_a` / `w_b` were rescaled per group to sum to their row counts, which
                // would turn the share into a share of ROWS while the level shares
                // (`compute_shares`) use the raw design weights: two populations in one run. Both
                // use the raw weights here (sum w_A / sum w) (0120-MERIDIAN E-REV-4).
                let group_mass =
                    |w: &Option<DVector<f64>>, group_df: &DataFrame| -> Result<f64, OaxacaError> {
                        match (&self.weights_col, w) {
                            (Some(col), Some(_)) => Ok(weight_values(group_df, col)?
                                .into_iter()
                                .map(|v| v.unwrap_or(0.0))
                                .sum()),
                            (_, Some(w)) => Ok(w.sum()),
                            _ => Ok(group_df.height() as f64),
                        }
                    };
                let n_a = group_mass(&w_a, &df_a)?;
                let n_b = group_mass(&w_b, &df_b)?;
                let total_n = n_a + n_b;
                if total_n == 0.0 {
                    return Err(OaxacaError::InvalidGroupVariable(
                        "No data in groups for weighted coefficients.".to_string(),
                    ));
                }
                let weight_a = n_a / total_n;
                let weight_b = 1.0 - weight_a;
                // beta_a and beta_b were normalised under the same shares, and the restriction
                // is linear, so the mix satisfies it too; its base-level coefficient is the
                // same mix of the two base-level coefficients.
                for var in shares.keys() {
                    let coeff_a = base_coeffs_a.get(var).unwrap_or(&0.0);
                    let coeff_b = base_coeffs_b.get(var).unwrap_or(&0.0);
                    base_coeffs_star.insert(var.clone(), coeff_a * weight_a + coeff_b * weight_b);
                }
                beta_star_owned = beta_a * weight_a + beta_b * weight_b;
                &beta_star_owned
            }
        };

        // Three-fold: from the TREATMENT-CODED vectors. Its aggregate is invariant to the
        // coding of the categoricals; computed from the normalised vectors (which omit the
        // base-level term) it summed to 79% of the gap and moved with the base level (0120 T3).
        let three_fold = three_fold_decomposition(&xa_mean, &xb_mean, beta_a_raw, beta_b_raw);
        let mut two_fold = two_fold_decomposition(&xa_mean, &xb_mean, beta_a, beta_b, beta_star);
        let (mut detailed_explained, mut detailed_unexplained) = detailed_decomposition(
            &xa_mean,
            &xb_mean,
            beta_a,
            beta_b,
            beta_star,
            &final_predictor_names,
        );

        // Base level of every normalised variable: it has no dummy, so its row is built from
        // 1 - sum(dummy means) and its own (new, non-zero) coefficient. Emitting it makes all
        // k levels sum to the aggregate.
        for (var, factor) in &shares {
            let base_dummy_name = format!("{}_{}", var, factor.base_level);

            let dummy_indices: Vec<usize> = factor
                .levels
                .iter()
                .filter(|l| l.level != factor.base_level)
                .map(|l| {
                    let name = format!("{}_{}", var, l.level);
                    final_predictor_names
                        .iter()
                        .position(|n| *n == name)
                        .ok_or_else(|| {
                            OaxacaError::NormalizationError(format!(
                                "design has no dummy column '{}'",
                                name
                            ))
                        })
                })
                .collect::<Result<Vec<usize>, _>>()?;

            let xa_mean_base = 1.0 - dummy_indices.iter().map(|&i| xa_mean[i]).sum::<f64>();
            let xb_mean_base = 1.0 - dummy_indices.iter().map(|&i| xb_mean[i]).sum::<f64>();

            let beta_a_base = base_coeffs_a.get(var).cloned().unwrap_or(0.0);
            let beta_b_base = base_coeffs_b.get(var).cloned().unwrap_or(0.0);
            let beta_star_base = base_coeffs_star.get(var).cloned().unwrap_or(0.0);

            let contribution_unexplained = xa_mean_base * (beta_a_base - beta_star_base)
                + xb_mean_base * (beta_star_base - beta_b_base);

            let contribution_explained = (xa_mean_base - xb_mean_base) * beta_star_base;

            detailed_unexplained.push(DetailedComponent {
                variable_name: base_dummy_name.clone(),
                contribution: contribution_unexplained,
            });

            detailed_explained.push(DetailedComponent {
                variable_name: base_dummy_name,
                contribution: contribution_explained,
            });

            two_fold.explained += contribution_explained;
            two_fold.unexplained += contribution_unexplained;
        }

        let total_gap = if let Some(w) = &w_a {
            y_a.dot(w) / w.sum()
        } else {
            y_a.mean()
        } - if let Some(w) = &w_b {
            y_b.dot(w) / w.sum()
        } else {
            y_b.mean()
        };

        Ok(SinglePassResult {
            three_fold,
            two_fold,
            detailed_explained,
            detailed_unexplained,
            total_gap,
            residuals_a,
            residuals_b,
            xa_mean: xa_mean.clone(),
            xb_mean: xb_mean.clone(),
            beta_star: beta_star.clone(),
            detailed_selection: detailed_selection_components,
            shares,
        })
    }

    /// `variable=level` for every categorical level that occurs in the full group frames but is
    /// absent from the resample of that group. Called only for a replicate that failed, to name
    /// which levels cost replicates (`run_metadata.bootstrap_discard_levels`).
    fn absent_levels(
        &self,
        full_a: &DataFrame,
        full_b: &DataFrame,
        sample_a: &DataFrame,
        sample_b: &DataFrame,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for var in &self.categorical_predictors {
            let levels_of = |df: &DataFrame| -> std::collections::BTreeSet<String> {
                df.column(var)
                    .ok()
                    .and_then(|c| c.as_materialized_series().str().ok().cloned())
                    .map(|ca| ca.into_iter().flatten().map(String::from).collect())
                    .unwrap_or_default()
            };
            let mut all = levels_of(full_a);
            all.extend(levels_of(full_b));
            for sample in [sample_a, sample_b] {
                let present = levels_of(sample);
                for l in &all {
                    if !present.contains(l) {
                        out.push(format!("{}={}", var, l));
                    }
                }
            }
        }
        out
    }

    /// Performs a RIF-Regression decomposition for a specific quantile.
    ///
    /// This method transforms the outcome variable into its Recentered Influence Function (RIF)
    /// for each group separately and then performs the standard Oaxaca-Blinder decomposition
    /// on the transformed variable. This allows for decomposing the difference in quantiles
    /// (e.g., the 90th percentile gap) into explained and unexplained components.
    ///
    /// # Arguments
    ///
    /// * `quantile` - The target quantile (e.g., 0.9 for the 90th percentile).
    pub fn decompose_quantile(&self, quantile: f64) -> Result<OaxacaResults, OaxacaError> {
        // RIF-regression quantile decomposition (Firpo-Fortin-Lemieux 2009), 0014-MERIDIAN
        // rulings a-1 / 4.
        //
        // Ruling 4 (`fixed_rif: false`): the RIF transform is recomputed INSIDE each bootstrap
        // replicate — every resample re-estimates its own quantile q_τ and density f_Y(q_τ)
        // before the RIF-OLS — so the bootstrap CIs reflect the full sampling variability of
        // the quantile estimate (the legacy compute-once-on-the-full-sample form understated
        // them). The POINT estimate still uses the full-sample RIF.
        //
        // Structure mirrors `run()` (shared dummy prep → `run_single_pass` → the
        // bounded-parallel chunked bootstrap → `aggregate_results`), with one addition: each
        // resampled group's outcome is replaced by its per-replicate RIF before the pass.
        // Reusing run()'s chunked loop keeps the stage-2 WASM-OOM bound (peak = H_res +
        // chunk·Sc) AND the fixed rep-index reduction order (so the quantile path is
        // byte-identical across thread counts too, INV-02), and reusing the same
        // `unit_rng(master, Bootstrap, rep*2 [+1])` streams keeps determinism.
        // 0118-MERIDIAN S2: refuse a third group value on the RAW frame, before any cleaning.
        self.check_group_values()?;
        let df_dirty = self.dataframe.clone();
        let mut df = self.clean_dataframe(&df_dirty)?;

        // Categorical one-hot dummies — same construction as run() so run_single_pass sees the
        // one-hot columns and the Gardeazabal-Ugidos normalization applies unchanged (L6).
        let mut all_dummy_names = Vec::new();
        if !self.categorical_predictors.is_empty() {
            for cat_pred in &self.categorical_predictors {
                let series = df.column(cat_pred)?;
                let (dummies, _, _) =
                    self.create_dummies_manual(series.as_materialized_series())?;
                for s in dummies.get_columns() {
                    all_dummy_names.push(s.name().to_string());
                }
                df = df.hstack(dummies.get_columns())?;
            }
        }

        let groups = self.split_groups(&df)?;
        let df_a_global = groups.df_a;
        let df_b_global = groups.df_b;

        // D1 (0014-close round-1): named refusal before estimation reaches Cholesky.
        self.check_level_confinement(
            &df_a_global,
            &df_b_global,
            &groups.group_a_name,
            &groups.group_b_name,
        )?;

        // Point estimate: RIF computed once on the FULL sample of each group.
        let point_df = self
            .rif_replace_outcome(&df_a_global, quantile)?
            .vstack(&self.rif_replace_outcome(&df_b_global, quantile)?)?;
        let point_estimates = self.run_single_pass(&point_df, &all_dummy_names)?;

        let master = self.seed.unwrap_or(DEFAULT_SEED);

        // Bounded-parallel chunked bootstrap (stage-2 invariant — NEVER an unbounded
        // into_par_iter().collect(), which reintroduces the WASM-OOM). Each rep: resample the
        // ORIGINAL groups → recompute the per-group RIF on the resample (ruling 4) → RIF-OLS
        // decomposition pass. Same streams + index-ordered chunk consumption as run().
        let chunk = rayon::current_num_threads().max(1);
        let mut bootstrap_results: Vec<RepEstimates> = Vec::with_capacity(self.bootstrap_reps);
        let mut discard_levels: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        let mut discarded = 0usize;
        let mut start = 0usize;
        while start < self.bootstrap_reps {
            let end = (start + chunk).min(self.bootstrap_reps);
            let chunk_out: Vec<Rep> = (start..end)
                .into_par_iter()
                .map(|rep| {
                    let rep = rep as u64;
                    let mut rng_a = unit_rng(master, RngPurpose::Bootstrap, rep * 2);
                    let mut rng_b = unit_rng(master, RngPurpose::Bootstrap, rep * 2 + 1);

                    let sample_a = self.bootstrap_sample(&df_a_global, &mut rng_a);
                    let sample_b = self.bootstrap_sample(&df_b_global, &mut rng_b);
                    let (sample_a, sample_b) = match (sample_a, sample_b) {
                        (Ok(a), Ok(b)) => (a, b),
                        _ => return Rep::Failed(Vec::new()),
                    };

                    let result = (|| -> Result<SinglePassResult, OaxacaError> {
                        // Ruling 4: recompute the RIF per replicate, per group.
                        let rif_a = self.rif_replace_outcome(&sample_a, quantile)?;
                        let rif_b = self.rif_replace_outcome(&sample_b, quantile)?;
                        let sample_df = rif_a.vstack(&rif_b)?;
                        self.run_single_pass(&sample_df, &all_dummy_names)
                    })();

                    match result {
                        Ok(r) => Rep::Ok(RepEstimates::from_pass(&r)),
                        Err(_) => Rep::Failed(self.absent_levels(
                            &df_a_global,
                            &df_b_global,
                            &sample_a,
                            &sample_b,
                        )),
                    }
                })
                .collect();

            for o in chunk_out {
                match o {
                    Rep::Ok(r) => bootstrap_results.push(r),
                    Rep::Failed(levels) => {
                        discarded += 1;
                        discard_levels.extend(levels);
                    }
                }
            }
            start = end;
        }

        let successful_bootstraps = bootstrap_results.len();
        if successful_bootstraps < self.bootstrap_reps {
            eprintln!(
                "Warning: {} out of {} bootstrap replications failed and were discarded. The analysis is based on {} successful replications.",
                self.bootstrap_reps - successful_bootstraps, self.bootstrap_reps, successful_bootstraps
            );
        }

        // fixed_rif = Some(false): RIF recomputed per replicate (ruling 4).
        Ok(self.aggregate_results(
            &point_estimates,
            &bootstrap_results,
            master,
            successful_bootstraps,
            discarded,
            &discard_levels,
            df_a_global.height(),
            df_b_global.height(),
            Some(false),
        ))
    }

    /// The analysed rows with the outcome column replaced by its per-group RIF at `quantile`:
    /// exactly the outcome `decompose_quantile` regresses on for the point estimate (group A
    /// rows first, then group B rows, each group's RIF computed on its own rows). Diagnostic
    /// export (0120-MERIDIAN V1d): a RIF-OLS on a given outcome vector is plain OLS, so an
    /// external package can be run on this column to check the normalisation of the quantile
    /// path independently of the density estimator.
    pub fn rif_outcome_frame(&self, quantile: f64) -> Result<DataFrame, OaxacaError> {
        self.check_group_values()?;
        let df = self.clean_dataframe(&self.dataframe.clone())?;
        let groups = self.split_groups(&df)?;
        Ok(self
            .rif_replace_outcome(&groups.df_a, quantile)?
            .vstack(&self.rif_replace_outcome(&groups.df_b, quantile)?)?)
    }

    /// One bootstrap replicate of a group frame (0120-MERIDIAN E-REV-1).
    ///
    /// Unweighted and `Relative` runs resample ROWS (weights, if any, travel with their row).
    /// A `Frequency` run resamples the expanded sample: `sum(w)` units drawn with probability
    /// `w_i / sum(w)`, the draw counts replacing the weights, so a replicate on `w = 2` is
    /// distributed as a replicate on the row written twice and the standard errors, intervals and
    /// p-values agree with the expanded sample (a per-row draw leaves the point estimate alone but
    /// inflates every standard error by roughly the square root of the weight).
    fn bootstrap_sample(
        &self,
        g: &DataFrame,
        rng: &mut rand_chacha::ChaCha8Rng,
    ) -> PolarsResult<DataFrame> {
        match (&self.weights_col, self.weights_kind) {
            (Some(col), Some(WeightsKind::Frequency)) => {
                let w: Vec<f64> = weight_values(g, col)
                    .map_err(|e| PolarsError::ComputeError(e.to_string().into()))?
                    .into_iter()
                    .map(|o| o.unwrap_or(0.0))
                    .collect();
                let (idx, counts) = resample_frequency(rng, &w);
                let mut out = g.take(&idx)?;
                out.with_column(Column::new(col.as_str().into(), counts))?;
                Ok(out)
            }
            _ => g.take(&resample_indices(rng, g.height())),
        }
    }

    /// Replace the outcome column of `g` with its Recentered Influence Function at
    /// `quantile` (RIF-regression transform, FFL 2009). Called once per group for the point
    /// estimate and once per group PER bootstrap replicate (ruling 4). `clone()` is an
    /// Arc/COW refcount bump (stage-2 Finding 1), so it adds no meaningful per-rep memory.
    fn rif_replace_outcome(&self, g: &DataFrame, quantile: f64) -> Result<DataFrame, OaxacaError> {
        // 0097 — the seam this issue exists to close. `weights_col` was honoured in
        // `clean_dataframe`, `weighted_levels_present` and `ols()`, but never here, so a weighted
        // quantile run built an UNWEIGHTED RIF transform and then regressed on it WITH weights.
        // Nothing threw; the number was simply wrong. The weights come from this group's own rows,
        // in the same order as the outcome, because `clean_dataframe` has already dropped any row
        // with a null in either column.
        let weights: Option<Vec<f64>> = match &self.weights_col {
            Some(col) => Some(weight_values(g, col)?.into_no_null_iter().collect()),
            None => None,
        };
        let series = g.column(&self.outcome)?.as_materialized_series();
        // Branch rather than always calling the weighted form with `None`: the unweighted path
        // stays literally the original function, so "did the unweighted answer move?" is answered
        // by reading this line rather than by trusting a delegation.
        let rif = match (weights.as_deref(), self.weights_kind) {
            (Some(w), Some(WeightsKind::Relative)) => calculate_rif_relative(series, quantile, w),
            (Some(w), Some(WeightsKind::Frequency)) => {
                calculate_rif_weighted(series, quantile, Some(w))
            }
            (Some(_), None) => {
                return Err(OaxacaError::WeightsKindRequired {
                    column: self.weights_col.clone().unwrap_or_default(),
                })
            }
            (None, _) => calculate_rif(series, quantile),
        }
        .map_err(OaxacaError::PolarsError)?;
        let mut out = g.clone();
        out.with_column(rif)?;
        Ok(out)
    }

    /// Helper to drop rows with missing values in relevant columns.
    fn clean_dataframe(&self, df: &DataFrame) -> Result<DataFrame, OaxacaError> {
        let cols: Vec<String> = self
            .cleaning_columns()
            .into_iter()
            .map(|(name, _)| name)
            .collect();

        // Ensure all columns exist before trying to drop nulls on them
        for c in &cols {
            if df.column(c).is_err() {
                return Err(OaxacaError::ColumnNotFound(c.clone()));
            }
        }

        let Some(w_col) = &self.weights_col else {
            let clean_df = df
                .drop_nulls(Some(&cols))
                .map_err(OaxacaError::PolarsError)?;
            return Ok(clean_df);
        };

        // A weights column: the stated kind is required, and every weight that survives the
        // null drop must be valid under it. A rejected weight is named by its original row
        // ordinal: the ordinal column when the caller added one, otherwise the row position in
        // the frame given here (which is the data-row ordinal of the builder's frame).
        let Some(kind) = self.weights_kind else {
            return Err(OaxacaError::WeightsKindRequired {
                column: w_col.clone(),
            });
        };
        let has_ordinal = df.column(ROW_ORDINAL_COL).is_ok();
        let mut clean_df = if has_ordinal {
            df.drop_nulls(Some(&cols))
                .map_err(OaxacaError::PolarsError)?
        } else {
            df.clone()
                .with_row_index(WEIGHT_POS_COL.into(), None)?
                .drop_nulls(Some(&cols))
                .map_err(OaxacaError::PolarsError)?
        };
        let pos_col = if has_ordinal {
            ROW_ORDINAL_COL
        } else {
            WEIGHT_POS_COL
        };
        let ordinals = Self::ordinals_of(&clean_df, pos_col)?;
        let weights = weight_values(&clean_df, w_col)?;
        let mut total = 0.0;
        for (opt, ordinal) in weights.into_iter().zip(ordinals.iter()) {
            let w = opt.unwrap_or(0.0);
            check_weight(w_col, *ordinal, w, kind)?;
            total += w;
        }
        if clean_df.height() > 0 && total <= 0.0 {
            return Err(OaxacaError::InvalidWeight {
                column: w_col.clone(),
                row: ordinals.first().copied().unwrap_or(0),
                value: 0.0,
                reason: "weights sum to zero".to_string(),
            });
        }
        if !has_ordinal {
            clean_df = clean_df.drop(WEIGHT_POS_COL)?;
        }
        Ok(clean_df)
    }

    /// The `usize` values of an ordinal column, in frame order.
    fn ordinals_of(df: &DataFrame, col: &str) -> Result<Vec<usize>, OaxacaError> {
        let ca = df.column(col)?.as_materialized_series().idx()?.clone();
        Ok(ca.into_no_null_iter().map(|v| v as usize).collect())
    }

    /// Executes the Oaxaca-Blinder decomposition.
    pub fn run(&self) -> Result<OaxacaResults, OaxacaError> {
        // 0118-MERIDIAN S2: refuse a third group value on the RAW frame, before any cleaning.
        self.check_group_values()?;
        let df_dirty = self.dataframe.clone();
        let mut df = self.clean_dataframe(&df_dirty)?;

        let mut all_dummy_names = Vec::new();
        if !self.categorical_predictors.is_empty() {
            for cat_pred in &self.categorical_predictors {
                let series = df.column(cat_pred)?;
                let (dummies, _, _) =
                    self.create_dummies_manual(series.as_materialized_series())?;
                for s in dummies.get_columns() {
                    all_dummy_names.push(s.name().to_string());
                }
                df = df.hstack(dummies.get_columns())?;
            }
        }

        // Checkpoint A (D1, mem-profile only): resident bytes immediately after
        // the categorical dummy hstack — a sub-component of H_res. Additive,
        // side-effect-only; does not touch the RNG/resampling logic below.
        #[cfg(feature = "mem-profile")]
        crate::mem_profile::checkpoint_a();

        let groups = self.split_groups(&df)?;

        // D1 (0014-close round-1): named refusal before estimation reaches Cholesky.
        self.check_level_confinement(
            &groups.df_a,
            &groups.df_b,
            &groups.group_a_name,
            &groups.group_b_name,
        )?;

        let point_estimates = self.run_single_pass(&df, &all_dummy_names)?;

        let df_a_global = groups.df_a;
        let df_b_global = groups.df_b;

        // Resolve the master seed once. `None` -> DEFAULT_SEED (reproducible-by-default).
        let master = self.seed.unwrap_or(DEFAULT_SEED);

        // Checkpoint B (D1, mem-profile only): resident bytes immediately before
        // the bootstrap into_par_iter() loop — the H_res(n) measurement point.
        // Additive, side-effect-only; does not touch the RNG/resampling logic
        // below (unit_rng/resample_indices/take/vstack are untouched).
        #[cfg(feature = "mem-profile")]
        crate::mem_profile::checkpoint_b();

        // Deterministic bootstrap. Each rep's resample and success/failure is a pure
        // function of (master, rep, base frames): group A draws from stream `rep*2`,
        // group B from `rep*2+1`, both via owned index-vector resampling (`take`) — no
        // dependence on execution order. The indexed `into_par_iter().map` collects in
        // order, then a sequential partition records the discard count deterministically
        // (D5, INV-02). `take` borrows the shared base frames; no per-rep clone.
        // Bounded-parallel bootstrap (0014-MERIDIAN memory fix). A single
        // `(0..reps).into_par_iter().map(..).collect()` lets rayon keep many
        // reps' ~Sc-sized `run_single_pass` working sets in flight at once, so
        // peak memory scales with the rep count — harmless natively (the
        // allocator reclaims it) but fatal in wasm, whose linear memory only
        // ever grows. Processing reps in index-ordered chunks of the pool size
        // caps the concurrently-live working sets at `chunk`, so peak =
        // H_res + chunk·Sc (the D2 shared-memory budget model), while each
        // chunk still runs fully in parallel. Chunks and within-chunk results
        // are consumed in strict rep-index order, so the estimates reach the
        // SE/CI reduction in a fixed order independent of thread count →
        // deterministic floating-point reduction (INV-02 byte-identity across
        // thread counts) and the AC-9 parity baseline both hold.
        let chunk = rayon::current_num_threads().max(1);
        let mut bootstrap_results: Vec<RepEstimates> = Vec::with_capacity(self.bootstrap_reps);
        let mut discard_levels: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        let mut discarded = 0usize; // sequential fold over ordered chunks — deterministic
        let mut start = 0usize;
        while start < self.bootstrap_reps {
            let end = (start + chunk).min(self.bootstrap_reps);
            let chunk_out: Vec<Rep> = (start..end)
                .into_par_iter()
                .map(|rep| {
                    let rep = rep as u64;
                    let mut rng_a = unit_rng(master, RngPurpose::Bootstrap, rep * 2);
                    let mut rng_b = unit_rng(master, RngPurpose::Bootstrap, rep * 2 + 1);

                    let sample_a = self.bootstrap_sample(&df_a_global, &mut rng_a);
                    let sample_b = self.bootstrap_sample(&df_b_global, &mut rng_b);
                    let (sample_a, sample_b) = match (sample_a, sample_b) {
                        (Ok(a), Ok(b)) => (a, b),
                        _ => return Rep::Failed(Vec::new()),
                    };

                    let result = (|| -> Result<SinglePassResult, OaxacaError> {
                        let sample_df = sample_a.vstack(&sample_b)?;
                        self.run_single_pass(&sample_df, &all_dummy_names)
                    })();

                    match result {
                        // Extract the small scalar estimates, then let the heavy
                        // SinglePassResult (`r`) drop at the end of this arm so
                        // its residual/mean/beta allocations free before the next
                        // chunk — bounding in-flight memory to `chunk` working sets.
                        Ok(r) => Rep::Ok(RepEstimates::from_pass(&r)),
                        Err(_) => Rep::Failed(self.absent_levels(
                            &df_a_global,
                            &df_b_global,
                            &sample_a,
                            &sample_b,
                        )),
                    }
                })
                .collect();

            for o in chunk_out {
                match o {
                    Rep::Ok(r) => bootstrap_results.push(r),
                    Rep::Failed(levels) => {
                        discarded += 1;
                        discard_levels.extend(levels);
                    }
                }
            }
            start = end;
        }

        let successful_bootstraps = bootstrap_results.len();
        if successful_bootstraps < self.bootstrap_reps {
            eprintln!(
                "Warning: {} out of {} bootstrap replications failed and were discarded. The analysis is based on {} successful replications.",
                self.bootstrap_reps - successful_bootstraps, self.bootstrap_reps, successful_bootstraps
            );
        }

        // fixed_rif = None: mean/OLS path (no quantile RIF transform). The aggregation is
        // shared verbatim with decompose_quantile() via aggregate_results() — one path, no
        // divergent copy (RK2). The mean path is byte-identical to before the extraction
        // (AC-9 / AC-6 baselines) and INV-02 deterministic across thread counts.
        Ok(self.aggregate_results(
            &point_estimates,
            &bootstrap_results,
            master,
            successful_bootstraps,
            discarded,
            &discard_levels,
            df_a_global.height(),
            df_b_global.height(),
            None,
        ))
    }

    /// Shared bootstrap-aggregation: turns the point-estimate pass plus the collected per-rep
    /// scalar estimates into the final `OaxacaResults` (per-component SE / percentile-CI /
    /// p-value / t-stat). Used verbatim by `run()` (mean/OLS, `fixed_rif` = None) and
    /// `decompose_quantile()` (RIF, `fixed_rif` = Some(false)) — one path, no divergent copy
    /// (RK2). The computation and iteration order are identical for both, so the mean path
    /// stays byte-identical and INV-02 within-platform determinism holds.
    #[allow(clippy::too_many_arguments)]
    fn aggregate_results(
        &self,
        point_estimates: &SinglePassResult,
        bootstrap_results: &[RepEstimates],
        master: u64,
        successful_bootstraps: usize,
        discarded: usize,
        discard_levels: &std::collections::BTreeSet<String>,
        n_a: usize,
        n_b: usize,
        fixed_rif: Option<bool>,
    ) -> OaxacaResults {
        let process_component = |name: &str, point: f64, estimates: Vec<f64>| {
            let (std_err, p_value, (ci_lower, ci_upper)) = bootstrap_stats(&estimates, point);
            let t_stat = if std_err.abs() > 1e-9 {
                point / std_err
            } else {
                0.0
            };
            ComponentResult {
                name: name.to_string(),
                estimate: point,
                std_err,
                t_stat,
                p_value,
                ci_lower,
                ci_upper,
            }
        };

        let two_fold_agg = vec![
            process_component(
                "explained",
                point_estimates.two_fold.explained,
                bootstrap_results
                    .iter()
                    .map(|r| r.two_fold.explained)
                    .collect(),
            ),
            process_component(
                "unexplained",
                point_estimates.two_fold.unexplained,
                bootstrap_results
                    .iter()
                    .map(|r| r.two_fold.unexplained)
                    .collect(),
            ),
        ];
        let three_fold_agg = vec![
            process_component(
                "endowments",
                point_estimates.three_fold.endowments,
                bootstrap_results
                    .iter()
                    .map(|r| r.three_fold.endowments)
                    .collect(),
            ),
            process_component(
                "coefficients",
                point_estimates.three_fold.coefficients,
                bootstrap_results
                    .iter()
                    .map(|r| r.three_fold.coefficients)
                    .collect(),
            ),
            process_component(
                "interaction",
                point_estimates.three_fold.interaction,
                bootstrap_results
                    .iter()
                    .map(|r| r.three_fold.interaction)
                    .collect(),
            ),
        ];

        let detailed_explained = self.process_detailed_components(
            &point_estimates.detailed_explained,
            bootstrap_results,
            |r| &r.detailed_explained,
            &process_component,
        );
        let detailed_unexplained = self.process_detailed_components(
            &point_estimates.detailed_unexplained,
            bootstrap_results,
            |r| &r.detailed_unexplained,
            &process_component,
        );

        let detailed_selection = self.process_detailed_components(
            &point_estimates.detailed_selection,
            bootstrap_results,
            |r| &r.detailed_selection,
            &process_component,
        );

        let mut run_metadata = RunMetadata::new(
            master,
            self.bootstrap_reps,
            successful_bootstraps,
            discarded,
        );
        if let Some(fr) = fixed_rif {
            run_metadata = run_metadata.with_fixed_rif(fr);
        }
        run_metadata.bootstrap_discard_levels = discard_levels.iter().cloned().collect();
        // Present only when the caller asked for normalisation, so a raw run's serialized
        // bytes (AC-9 / AC-6 baselines) do not change.
        if !self.normalization_vars.is_empty() {
            let applied = self.normalization_active();
            run_metadata.normalization = Some(NormalizationRecord {
                convention: self.normalization_convention.name(),
                share_basis: if self.weights_col.is_some() {
                    "observation-weights"
                } else {
                    "row-counts"
                },
                applied,
                skipped_reason: if applied {
                    None
                } else {
                    Some("heckman_selection")
                },
                variables: point_estimates.shares.values().cloned().collect(),
            });
        }

        OaxacaResults {
            total_gap: point_estimates.total_gap,
            two_fold: TwoFoldResults {
                aggregate: two_fold_agg,
                detailed_explained,
                detailed_unexplained,
                detailed_selection,
            },
            three_fold: DecompositionDetail {
                aggregate: three_fold_agg,
                detailed: Vec::new(),
            },
            n_a,
            n_b,
            residuals: point_estimates.residuals_b.iter().copied().collect(),
            xa_mean: point_estimates.xa_mean.clone(),
            xb_mean: point_estimates.xb_mean.clone(),
            beta_star: point_estimates.beta_star.clone(),
            run_metadata,
        }
    }

    fn process_detailed_components<'a, F>(
        &self,
        point_components: &[DetailedComponent],
        bootstrap_results: &'a [RepEstimates],
        extract_fn: F,
        process_component: &dyn Fn(&str, f64, Vec<f64>) -> ComponentResult,
    ) -> Vec<ComponentResult>
    where
        F: Fn(&'a RepEstimates) -> &'a Vec<DetailedComponent> + Sync,
    {
        let mut bootstrap_map: HashMap<String, Vec<f64>> = HashMap::new();
        for r in bootstrap_results.iter() {
            for comp in extract_fn(r) {
                bootstrap_map
                    .entry(comp.variable_name.clone())
                    .or_default()
                    .push(comp.contribution);
            }
        }

        point_components
            .iter()
            .map(|comp| {
                let estimates = bootstrap_map
                    .get(&comp.variable_name)
                    .cloned()
                    .unwrap_or_else(Vec::new);
                process_component(&comp.variable_name, comp.contribution, estimates)
            })
            .collect()
    }
}

impl OaxacaResults {}

#[cfg(test)]
mod tests {
    use super::*;

    // 0097 — isolates the RIF wire itself. The end-to-end quantile test cannot do this:
    // `weights_col` also weights the OLS, so `total_gap` moves whether or not the RIF ever sees
    // the weights. A first draft asserted on `total_gap` and passed with the wire deliberately
    // cut — vacuous. This calls `rif_replace_outcome` directly, so the only thing that can move
    // the transformed column is the weights reaching `calculate_rif_weighted`.
    #[test]
    fn rif_replace_outcome_honours_weights_col() {
        use polars::prelude::*;
        let df = df![
            "wage" => [10.0f64, 12.0, 14.0, 16.0, 18.0, 40.0],
            "educ" => [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0],
            "group" => ["A", "A", "A", "A", "A", "A"],
            "hc"   => [1.0f64, 1.0, 1.0, 1.0, 1.0, 30.0],
        ]
        .unwrap();

        let rif_of = |weighted: bool| -> Vec<f64> {
            let mut b = OaxacaBuilder::new(df.clone(), "wage", "group", "A");
            b.predictors(vec!["educ"]);
            if weighted {
                b.weights("hc").weights_kind(WeightsKind::Frequency);
            }
            let out = b
                .rif_replace_outcome(&df, 0.5)
                .expect("rif_replace_outcome");
            out.column("wage")
                .unwrap()
                .f64()
                .unwrap()
                .into_no_null_iter()
                .collect()
        };

        let bare = rif_of(false);
        let weighted = rif_of(true);
        assert_eq!(bare.len(), weighted.len());
        assert!(
            bare.iter()
                .zip(weighted.iter())
                .any(|(a, b)| (a - b).abs() > 1e-9),
            "the RIF column is identical with and without weights_col — the weights are being \
             dropped inside rif_replace_outcome again (bare={bare:?}, weighted={weighted:?})"
        );
    }

    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }

    // ---- 0120-MERIDIAN V2: the bootstrap REPLICATE path --------------------------------------
    //
    // A replicate is `run_single_pass` on a resampled frame, so it builds its restriction
    // weights from THAT resample's pooled rows (the point-estimate shares are what
    // `run_metadata.normalization` records). The pass-level tests below call `run_single_pass`
    // exactly as the replicate loop does, on a frame the loop's own RNG streams resampled.

    fn skewed_frame() -> DataFrame {
        let path = format!(
            "{}/tests/fixtures/norm_skewed_fixture.csv",
            env!("CARGO_MANIFEST_DIR")
        );
        LazyCsvReader::new(path)
            .with_has_header(true)
            .finish()
            .unwrap()
            .collect()
            .unwrap()
    }

    /// The cleaned frame with dummy columns, plus their names: what `run` hands to
    /// `run_single_pass`.
    fn prepared(b: &OaxacaBuilder) -> (DataFrame, Vec<String>) {
        let mut df = b.clean_dataframe(&b.dataframe.clone()).unwrap();
        let mut names = Vec::new();
        for cat in &b.categorical_predictors {
            let (dummies, _, _) = b
                .create_dummies_manual(df.column(cat).unwrap().as_materialized_series())
                .unwrap();
            for s in dummies.get_columns() {
                names.push(s.name().to_string());
            }
            df = df.hstack(dummies.get_columns()).unwrap();
        }
        (df, names)
    }

    fn level_counts(df: &DataFrame, var: &str) -> std::collections::BTreeMap<String, f64> {
        let mut m = std::collections::BTreeMap::new();
        for v in df.column(var).unwrap().str().unwrap().into_iter().flatten() {
            *m.entry(v.to_string()).or_insert(0.0) += 1.0;
        }
        let n: f64 = m.values().sum();
        m.values_mut().for_each(|v| *v /= n);
        m
    }

    #[test]
    fn a_replicate_adds_up_and_normalises_under_its_own_resample_shares() {
        for scheme in [
            ReferenceCoefficients::GroupA,
            ReferenceCoefficients::GroupB,
            ReferenceCoefficients::Pooled,
            ReferenceCoefficients::PooledNoIndicator,
            ReferenceCoefficients::Weighted,
        ] {
            let mut b = OaxacaBuilder::new(skewed_frame(), "log_salary", "Gender", "Female");
            b.predictors(["Age", "Experience_Years"])
                .categorical_predictors(["Department", "Location"])
                .reference_coefficients(scheme)
                .normalize_all_categoricals();
            let (df, dummies) = prepared(&b);
            let point = b.run_single_pass(&df, &dummies).unwrap();
            let groups = b.split_groups(&df).unwrap();

            let mut passes = 0;
            let mut differs_from_point = 0;
            for rep in 0..40u64 {
                let mut rng_a = unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, rep * 2);
                let mut rng_b = unit_rng(DEFAULT_SEED, RngPurpose::Bootstrap, rep * 2 + 1);
                let sample = groups
                    .df_a
                    .take(&resample_indices(&mut rng_a, groups.df_a.height()))
                    .unwrap()
                    .vstack(
                        &groups
                            .df_b
                            .take(&resample_indices(&mut rng_b, groups.df_b.height()))
                            .unwrap(),
                    )
                    .unwrap();
                // a replicate that loses a level is discarded by the loop; skip it here too
                let Ok(r) = b.run_single_pass(&sample, &dummies) else {
                    continue;
                };
                passes += 1;

                // (1) E + C + I == gap and explained + unexplained == gap, per replicate
                let est = RepEstimates::from_pass(&r);
                let tf = est.three_fold.endowments
                    + est.three_fold.coefficients
                    + est.three_fold.interaction;
                assert!(
                    (tf - est.total_gap).abs() < 1e-9,
                    "{scheme:?} rep {rep}: E+C+I {tf} vs gap {}",
                    est.total_gap
                );
                let tw = est.two_fold.explained + est.two_fold.unexplained;
                assert!(
                    (tw - est.total_gap).abs() < 1e-9,
                    "{scheme:?} rep {rep}: explained+unexplained {tw}"
                );
                let sum_u: f64 = est
                    .detailed_unexplained
                    .iter()
                    .map(|c| c.contribution)
                    .sum();
                assert!(
                    (sum_u - est.two_fold.unexplained).abs() < 1e-9,
                    "{scheme:?} rep {rep}: detail does not add up"
                );

                // (2) the shares are the replicate's own pooled resample, counted independently
                for var in ["Department", "Location"] {
                    let want = level_counts(&sample, var);
                    let got = &r.shares[var];
                    assert_eq!(
                        got.levels.len(),
                        want.len(),
                        "{scheme:?} rep {rep} {var}: level set"
                    );
                    for l in &got.levels {
                        assert!(
                            (l.share - want[&l.level]).abs() < 1e-12,
                            "{scheme:?} rep {rep} {var}[{}]: {} vs {}",
                            l.level,
                            l.share,
                            want[&l.level]
                        );
                    }
                }
                if r.shares["Department"] != point.shares["Department"] {
                    differs_from_point += 1;
                }
            }
            assert!(
                passes >= 30,
                "{scheme:?}: only {passes} of 40 replicates estimable"
            );
            assert!(
                differs_from_point >= passes / 2,
                "{scheme:?}: replicate shares equal the point sample's in {} of {passes} replicates; \
                 they should be the replicate's own",
                passes - differs_from_point
            );
        }
    }
}
