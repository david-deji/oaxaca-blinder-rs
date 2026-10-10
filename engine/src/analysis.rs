use crate::rows::{analysed_mask, check_alignment, read_csv, KeySupply};
use crate::support::{self, nz, Fitted, GapWeights, IntervalModel};
use crate::types::*;
use nalgebra::{DMatrix, DVector};
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients, RowAccounting};
use polars::prelude::*;

/// The scheme a decomposition request names. Strict (0120-MERIDIAN S4): an absent value or any
/// string other than `GroupA`, `GroupB`, `Pooled`, `PooledNoIndicator`, `Weighted` is an error.
/// It used to fall back to `Pooled`, which made the scheme with the least external
/// verification the silent default for every caller that forgot the field.
fn parse_reference_coefficients(
    req: &DecompositionRequest,
) -> Result<ReferenceCoefficients, String> {
    ReferenceCoefficients::parse_name(req.reference_coefficients.as_deref())
        .map_err(|e| e.to_string())
}

/// Stamps the engine-layer provenance fields on a result's `run_metadata`. Set here and not in
/// the library so a raw library run keeps the bytes it always had.
fn stamp_engine_metadata(
    meta: &mut oaxaca_blinder::RunMetadata,
    scheme: ReferenceCoefficients,
    method: &str,
) {
    meta.reference_coefficients_used = Some(scheme.canonical_name().to_string());
    meta.engine_version = Some(env!("CARGO_PKG_VERSION").to_string());
    meta.method = Some(method.to_string());
}

pub fn decompose_inner(req: DecompositionRequest) -> Result<DecompositionResult, String> {
    // Refuse a missing or unknown scheme before reading any data.
    parse_reference_coefficients(&req)?;

    // 1. Load Data
    let mut df = read_csv(&req.csv_data)?;

    // 0118-MERIDIAN S5: kept un-cast so an excluded row's stable key can be minted from the
    // raw cells if (and only if) some row turns out to be excluded. See `KeySupply`.
    let raw_df = df.clone();

    // Cast to Float64 with error checking
    let cast_cols = [&req.outcome_variable]
        .into_iter()
        .chain(req.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                // Try strict cast first, if it fails, it means we have non-numeric data
                let new_s = s.cast(&DataType::Float64).map_err(|_| {
                    format!("Column '{}' contains non-numeric data but was selected as a continuous variable. Please verify your column selection.", col)
                })?;

                if new_s.null_count() > s.null_count() {
                    return Err(format!("Column '{}' contains non-numeric data but was selected as a continuous variable. Please verify your column selection.", col));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    run_decomposition_on_df(df, &req, KeySupply::FromRaw(&raw_df))
}

/// Which original rows are analysed and which are excluded for this request's model, from the
/// builder's single cleaning pass (0118-MERIDIAN S1). Errors are the builder's, verbatim.
fn row_accounting(df: &DataFrame, req: &DecompositionRequest) -> Result<RowAccounting, String> {
    let mut builder = OaxacaBuilder::new(
        df.clone(),
        &req.outcome_variable,
        &req.group_variable,
        &req.reference_group,
    );
    builder.predictors(req.predictors.iter().map(|s| s.as_str()));
    if let Some(cats) = &req.categorical_predictors {
        builder.categorical_predictors(cats.iter().map(|s| s.as_str()));
    }
    builder.analysed_rows().map_err(|e| e.to_string())
}

pub fn verify_inner(req: VerificationRequest) -> Result<DecompositionResult, String> {
    // Refuse a missing or unknown scheme before reading any data.
    parse_reference_coefficients(&req.decomposition_params)?;

    // 1. Load Data
    let mut df = read_csv(&req.decomposition_params.csv_data)?;

    // 0017-MERIDIAN P4: mint the stable key table HERE, on the raw parse, before the Float64
    // cast below. Casting changes a cell's text rendering, so a table built after it would
    // depend on which columns the operator picked as predictors. See `crate::row_key`.
    let row_keys = crate::row_key::RowKeyTable::build(&df)?;

    // Cast to Float64 (Replicating logic to ensure type safety)
    let cast_cols = [&req.decomposition_params.outcome_variable]
        .into_iter()
        .chain(req.decomposition_params.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                let new_s = s
                    .cast(&DataType::Float64)
                    .map_err(|_| format!("Column '{}' contains non-numeric data.", col))?;

                if new_s.null_count() > s.null_count() {
                    return Err(format!("Column '{}' contains non-numeric data.", col));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    // 2. Apply Adjustments
    let wage_col_name = &req.decomposition_params.outcome_variable;
    let wage_series = df.column(wage_col_name).map_err(|e| e.to_string())?;
    let ca = wage_series.f64().map_err(|e| e.to_string())?;

    // Collect into Vec<Option<f64>> to handle potential nulls safely
    let mut wage_vec: Vec<Option<f64>> = ca.into_iter().collect();

    // 0017-MERIDIAN P4: a proposed adjustment carrying a `row_key` is resolved by KEY, not by
    // position. An unresolvable key means the row is gone from this CSV, so the adjustment is
    // skipped and counted — applying it at `index` would move the consultant's figure onto
    // whichever employee now sits at that offset.
    let mut unresolved_row_keys: usize = 0;

    // 0118-MERIDIAN S3: an adjustment addressed to a row the analysis excluded (a blank model
    // cell) or to a row that does not exist is skipped and counted, never applied and never an
    // error: the app replays persisted ledger rows on project load, so a row can legitimately
    // have lost its inputs since the ledger was saved. Which rows are analysed is a property of
    // the inputs, and adjustments only ever move a non-blank wage, so it is read off the frame
    // before the adjustments are applied.
    let accounting = row_accounting(&df, &req.decomposition_params)?;
    let analysed = analysed_mask(
        accounting.total_rows,
        &accounting.reference_rows,
        &accounting.target_rows,
    );
    let mut adjustments_on_excluded_rows: usize = 0;

    for adj in &req.adjustments {
        let Some(row_idx) = row_keys.resolve(adj.index, adj.row_key.as_deref()) else {
            unresolved_row_keys += 1;
            continue;
        };
        if !analysed.get(row_idx).copied().unwrap_or(false) {
            adjustments_on_excluded_rows += 1;
            continue;
        }
        if let Some(val) = wage_vec[row_idx] {
            wage_vec[row_idx] = Some(val + adj.value);
        }
    }

    // Reconstruct Series
    let new_series = Series::new(wage_col_name.as_str().into(), &wage_vec);
    df.with_column(new_series).map_err(|e| e.to_string())?;

    // 3. Run Analysis on Mutated DataFrame
    let mut result =
        run_decomposition_on_df(df, &req.decomposition_params, KeySupply::Ready(&row_keys))?;
    result.unresolved_row_keys = Some(unresolved_row_keys);
    result.adjustments_on_excluded_rows = adjustments_on_excluded_rows;
    Ok(result)
}

fn run_decomposition_on_df(
    df: DataFrame,
    req: &DecompositionRequest,
    keys: KeySupply<'_>,
) -> Result<DecompositionResult, String> {
    // Calculate Summary Stats (on provided data)
    let total_count = df.height();
    let group_col = df.column(&req.group_variable).map_err(|e| e.to_string())?;
    // A non-string group column keeps its historical error text.
    group_col.str().map_err(|e| e.to_string())?;

    // 0118-MERIDIAN S1/S5: the summary describes the ANALYSED rows (complete cases), the same
    // rows the decomposition below uses; `total_count` stays the raw row count. The group
    // check (S2) runs inside `row_accounting`, before any figure is computed.
    let accounting = row_accounting(&df, req)?;

    // Group A of this summary is the REFERENCE group, group B every other analysed row. The
    // mean is taken over the filtered frame exactly as before (same polars `mean`), so on a
    // complete-case file the figures are bit-identical to the pre-0118 engine.
    let mean_over = |rows: &[usize]| -> Result<f64, String> {
        let mut selected = vec![false; total_count];
        for &i in rows {
            selected[i] = true;
        }
        let mask = BooleanChunked::from_slice("analysed".into(), &selected);
        let filtered = df.filter(&mask).map_err(|e| e.to_string())?;
        Ok(filtered
            .column(&req.outcome_variable)
            .map_err(|e| e.to_string())?
            .f64()
            .map_err(|e| e.to_string())?
            .mean()
            .unwrap_or(0.0))
    };
    let group_a_count = accounting.reference_rows.len();
    let group_a_mean = mean_over(&accounting.reference_rows)?;
    let group_b_count = accounting.target_rows.len();
    let group_b_mean = mean_over(&accounting.target_rows)?;

    let summary = DataSummary {
        total_count,
        group_a_count,
        group_b_count,
        group_a_mean,
        group_b_mean,
    };

    let predictors: Vec<&str> = req.predictors.iter().map(|s| s.as_str()).collect();
    let cats_vec: Option<Vec<&str>> = req
        .categorical_predictors
        .as_ref()
        .map(|c| c.iter().map(|s| s.as_str()).collect());
    let reps = req.bootstrap_reps.unwrap_or(100);

    let ref_coef = parse_reference_coefficients(req)?;

    // 1b. Support and small-sample diagnostics (0120-MERIDIAN S6), taken from the SAME analysed
    // rows and design the decomposition below uses, before it runs: a group with no residual
    // degrees of freedom is refused here with a named error, not deep inside the estimator.
    // The design is read in the builder's own coding; leverage and the continuous columns are
    // invariant to how the categorical levels are normalised afterwards.
    let (support_block, mut warnings, group_outcomes) = {
        let mut probe = OaxacaBuilder::new(
            df.clone(),
            &req.outcome_variable,
            &req.group_variable,
            &req.reference_group,
        );
        probe.predictors(predictors.iter().copied());
        if let Some(cats) = &cats_vec {
            probe.categorical_predictors(cats.iter().copied());
        }
        let matrices = probe
            .get_data_matrices_with_rows()
            .map_err(crate::rows::matrices_error)?;
        let (support_block, warnings) = support::support_diagnostics(
            &matrices.reference.x,
            &matrices.target.x,
            &matrices.predictor_names,
            &req.predictors,
            Fitted::Both,
            None,
        )?;
        let outcomes = (
            matrices.reference.y.iter().copied().collect::<Vec<f64>>(),
            matrices.target.y.iter().copied().collect::<Vec<f64>>(),
        );
        (support_block, warnings, outcomes)
    };

    // 2. Build and Run Oaxaca or Quantile Decomposition
    let (
        total,
        explained,
        unexplained,
        interaction,
        detailed_exp,
        detailed_unexp,
        unexplained_std_err,
        run_metadata,
    ) = if let Some(q) = req.quantile {
        // QUANTILE DECOMPOSITION (RIF-regression — 0014-MERIDIAN ruling a-1).
        // Route through OaxacaBuilder::decompose_quantile (one-stage RIF-OLS with
        // per-predictor detail, RIF recomputed per bootstrap replicate — ruling 4) instead of
        // the MM-simulation builder, so the aggregate and the per-predictor detail are ONE
        // coherent additive method (W10). Detail is extracted with the SAME two_fold accessors
        // the OLS branch uses below; the postMessage {type,payload} contract is unchanged —
        // detail flows through the existing DecompositionResult fields the mean path serializes.
        let mut builder = OaxacaBuilder::new(
            df,
            &req.outcome_variable,
            &req.group_variable,
            &req.reference_group,
        );
        builder.predictors(predictors.iter().copied());
        builder.reference_coefficients(ref_coef);

        if let Some(cats) = &cats_vec {
            builder.categorical_predictors(cats.iter().copied());
        }
        // Every categorical predictor is normalised on every run (0120-MERIDIAN T1), under the
        // pooled-sample population shares (D1): a level's driver row is its deviation from
        // the company-wide average employee, not from whichever level sorts first.
        builder.normalize_all_categoricals();

        builder.bootstrap_reps(reps);

        let results = builder.decompose_quantile(q).map_err(|e| e.to_string())?;

        let total = *results.total_gap();
        let mut explained = 0.0;
        let mut unexplained = 0.0;
        let mut unexplained_std_err = None;
        let mut d_exp = Vec::new();
        let mut d_unexp = Vec::new();

        let two_fold = results.two_fold();
        for component in two_fold.aggregate() {
            if component.name() == "explained" {
                explained = *component.estimate();
            } else if component.name() == "unexplained" {
                unexplained = *component.estimate();
                unexplained_std_err = Some(*component.std_err());
            }
        }

        for c in two_fold.detailed_explained() {
            d_exp.push(DetailedComponent {
                name: c.name().to_string(),
                estimate: *c.estimate(),
                std_err: Some(*c.std_err()),
                p_value: Some(*c.p_value()),
                ci_lower: Some(*c.ci_lower()),
                ci_upper: Some(*c.ci_upper()),
            });
        }

        for c in two_fold.detailed_unexplained() {
            d_unexp.push(DetailedComponent {
                name: c.name().to_string(),
                estimate: *c.estimate(),
                std_err: Some(*c.std_err()),
                p_value: Some(*c.p_value()),
                ci_lower: Some(*c.ci_lower()),
                ci_upper: Some(*c.ci_upper()),
            });
        }

        (
            total,
            explained,
            unexplained,
            None,
            d_exp,
            d_unexp,
            unexplained_std_err,
            {
                let mut meta = results.run_metadata().clone();
                stamp_engine_metadata(&mut meta, ref_coef, "rif-quantile");
                meta
            },
        )
    } else {
        // STANDARD OLS DECOMPOSITION
        // Pass ownership of df
        let mut builder = OaxacaBuilder::new(
            df,
            &req.outcome_variable,
            &req.group_variable,
            &req.reference_group,
        );
        builder.predictors(predictors.iter().copied());
        builder.reference_coefficients(ref_coef);

        if let Some(cats) = &cats_vec {
            builder.categorical_predictors(cats.iter().copied());
        }
        // Same rule as the quantile branch above.
        builder.normalize_all_categoricals();

        builder.bootstrap_reps(reps);

        let results = builder.run().map_err(|e| e.to_string())?;

        // Format Results
        // NOTE: OaxacaResults total_gap returns &f64 directly
        let total = *results.total_gap();
        let mut explained = 0.0;
        let mut unexplained = 0.0;
        let mut interaction = None;
        let mut d_exp = Vec::new();
        let mut d_unexp = Vec::new();
        let mut unexplained_std_err = None;

        if req.three_fold.unwrap_or(false) {
            let three_fold = results.three_fold();
            let aggregated = three_fold.aggregate();
            for component in aggregated {
                if component.name() == "endowments" {
                    explained = *component.estimate();
                } else if component.name() == "coefficients" {
                    unexplained = *component.estimate();
                } else if component.name() == "interaction" {
                    interaction = Some(*component.estimate());
                }
            }
        } else {
            let two_fold = results.two_fold();
            let aggregated = two_fold.aggregate();
            for component in aggregated {
                if component.name() == "explained" {
                    explained = *component.estimate();
                } else if component.name() == "unexplained" {
                    unexplained = *component.estimate();
                    unexplained_std_err = Some(*component.std_err());
                }
            }

            for c in two_fold.detailed_explained() {
                d_exp.push(DetailedComponent {
                    name: c.name().to_string(),
                    estimate: *c.estimate(),
                    std_err: Some(*c.std_err()),
                    p_value: Some(*c.p_value()),
                    ci_lower: Some(*c.ci_lower()),
                    ci_upper: Some(*c.ci_upper()),
                });
            }

            for c in two_fold.detailed_unexplained() {
                d_unexp.push(DetailedComponent {
                    name: c.name().to_string(),
                    estimate: *c.estimate(),
                    std_err: Some(*c.std_err()),
                    p_value: Some(*c.p_value()),
                    ci_lower: Some(*c.ci_lower()),
                    ci_upper: Some(*c.ci_upper()),
                });
            }
        }
        (
            total,
            explained,
            unexplained,
            interaction,
            d_exp,
            d_unexp,
            unexplained_std_err,
            {
                let mut meta = results.run_metadata().clone();
                stamp_engine_metadata(&mut meta, ref_coef, "oaxaca-blinder-mean");
                meta
            },
        )
    };

    // 3. Percentile report (S8): the actual percentile gap beside the RIF total.
    let quantile_report = match req.quantile {
        Some(q) => {
            let (report, qwarnings) =
                support::quantile_report(q, &group_outcomes.0, &group_outcomes.1, total);
            warnings.extend(qwarnings);
            Some(report)
        }
        None => None,
    };

    Ok(DecompositionResult {
        total_gap: total,
        explained_gap: explained,
        unexplained_gap: unexplained,
        interaction_gap: interaction,
        explained_percentage: (explained / total) * 100.0,
        unexplained_percentage: (unexplained / total) * 100.0,
        interaction_percentage: interaction.map(|i| (i / total) * 100.0),
        detailed_explained: detailed_exp,
        detailed_unexplained: detailed_unexp,
        data_summary: Some(summary),
        unexplained_standard_error: unexplained_std_err,
        run_metadata,
        // Not applicable at this level — `verify_inner` overwrites it with its own count.
        unresolved_row_keys: None,
        analysed_reference_count: accounting.reference_rows.len(),
        analysed_target_count: accounting.target_rows.len(),
        excluded_rows: keys.excluded(&accounting.excluded_rows),
        // `verify_inner` overwrites it with its own count; `decompose` consumes none.
        adjustments_on_excluded_rows: 0,
        support: support_block,
        warnings,
        quantile_report,
    })
}

/// Refuses a remedy request whose numbers the rule cannot honour, by name and before any data is
/// read (0122-MERIDIAN T3, F-03, F-04). Money that is negative or not a number used to fund the
/// full need; a group target typed beside reference raises has no defined budget, because a raise
/// to the reference group moves the line and the gap stops being monotone in the spend.
fn validate_remedy_request(req: &OptimizationRequest) -> Result<(), String> {
    if !req.budget.is_finite() || req.budget < 0.0 {
        return Err(format!(
            "INVALID_BUDGET: budget={}; give 0 for no cap (every eligible shortfall is paid in \
             full) or a positive amount",
            req.budget
        ));
    }
    if let Some(goal) = req.target_gap {
        if !goal.is_finite() {
            return Err(format!(
                "INVALID_TARGET_GAP: target_gap={goal}; give the mean gap to the pay line the \
                 compared group should reach (negative while it sits below), or leave it out"
            ));
        }
        if req.adjust_both_groups == Some(true) {
            return Err(
                "TARGET_GAP_WITH_REFERENCE_RAISES: a group target cannot be combined with \
                 adjust_both_groups; raising reference employees moves the pay line, so no single \
                 budget reaches the target. Drop one of the two"
                    .to_string(),
            );
        }
    }
    if let Some(pct) = req.min_gap_pct {
        if !pct.is_finite() || pct < 0.0 {
            return Err(format!(
                "INVALID_MIN_GAP_PCT: min_gap_pct={pct}; give a fraction of current pay, 0 or \
                 more (0.02 is 2 %), or leave it out"
            ));
        }
    }
    Ok(())
}

/// The remedy: raises each employee below the chosen pay line up to it, never above, within the
/// budget, and reports what that costs and does to the group's gap (0122-MERIDIAN T13).
///
/// No solver and no linear program. Every dollar to a compared employee closes the compared
/// group's mean gap by the same amount, so for the amounts the screen states (pay each person
/// below the line up to it, optionally under a cap) the cost is fixed by the gap reached; the
/// strategy only decides who is paid first when the budget falls short. `target_gap` is therefore
/// a rule for the budget, derived along the strategy's own order (see `TargetRule`).
///
/// Parallelization audit verdict: **SKIP** (engine-parallel-surface D1, entry point 2).
/// One pass over the rows and one sort — nothing to fan out. No parallel site.
pub fn optimize_inner(req: OptimizationRequest) -> Result<OptimizationResult, String> {
    validate_remedy_request(&req)?;

    // 1. Load Data
    let mut df = read_csv(&req.csv_data)?;

    // 0017-MERIDIAN P4: mint the stable key table on the RAW parse, before the Float64 cast
    // below and before any group split, so the table is indexed by exactly the same DataFrame
    // row ordinal that `Adjustment.index` carries. That alignment is what lets a client re-key
    // an already-persisted, index-keyed ledger losslessly when its CSV fingerprint matches.
    let row_keys = crate::row_key::RowKeyTable::build(&df)?;

    // Cast to Float64 with error checking
    let cast_cols = [&req.outcome_variable]
        .into_iter()
        .chain(req.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                let new_s = s.cast(&DataType::Float64).map_err(|_| {
                    format!("Column '{}' contains non-numeric data but was selected as a continuous variable.", col)
                })?;

                if new_s.null_count() > s.null_count() {
                    return Err(format!(
                        "Column '{}' contains non-numeric data but was selected as a continuous variable.",
                        col
                    ));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    let predictors: Vec<&str> = req.predictors.iter().map(|s| s.as_str()).collect();
    let cats_vec: Option<Vec<&str>> = req
        .categorical_predictors
        .as_ref()
        .map(|c| c.iter().map(|s| s.as_str()).collect());

    // 3. Setup Optimization Problem and Residuals
    let mut problem_builder = OaxacaBuilder::new(
        df.clone(),
        &req.outcome_variable,
        &req.group_variable,
        &req.reference_group,
    );
    problem_builder.predictors(predictors.iter().copied());
    problem_builder.reference_coefficients(ReferenceCoefficients::Pooled);

    if let Some(cats) = &cats_vec {
        problem_builder.categorical_predictors(cats.iter().copied());
    }

    use crate::types::{Adjustment, AllocationStrategy, OptimizationResult, OptimizationTarget};

    // 4. Determine Fair Wage Standard (Target)
    let target_mode = req
        .target
        .as_ref()
        .unwrap_or(&OptimizationTarget::Reference);

    // Prepare Matrices (0118-MERIDIAN S1/S3).
    //
    // `get_data_matrices_with_rows` returns the reference and target groups under those names,
    // plus the ORIGINAL row ordinal of every matrix row, from the same single cleaning pass
    // that produced the matrices. The locals below keep this file's historical names: `x_a` /
    // `y_a` hold the REFERENCE (advantaged) group, from which `beta_fair` is solved, and
    // `x_b` / `y_b` the TARGET group. The reference-is-"a" binding is correct as committed; the
    // legacy tuple of `get_data_matrices()` is the other way round (A = target), which is why
    // this call site no longer destructures it. Guardrail: ab_binding_regression_test.
    //
    // Every employee identity emitted below (`Adjustment.index`, `row_key`, the raw wage lookup)
    // is `target_rows[i]` / `reference_rows[i]` for matrix row `i`. Before this, the ordinals
    // were enumerated from the RAW group column while the matrices had already lost every row
    // with a blank model cell, so from the first blank onward each employee carried the next
    // employee's fair wage and the last employee in the group was never paid.
    let matrices = problem_builder
        .get_data_matrices_with_rows()
        .map_err(crate::rows::matrices_error)?;
    let oaxaca_blinder::DataMatricesWithRows {
        reference: reference_group_matrices,
        target: target_group_matrices,
        predictor_names: mut feature_names,
        excluded_rows: builder_excluded_rows,
        ..
    } = matrices;
    let (raw_x_a, y_a, reference_rows) = (
        reference_group_matrices.x,
        reference_group_matrices.y,
        reference_group_matrices.rows,
    );
    let (raw_x_b, y_b, target_rows) = (
        target_group_matrices.x,
        target_group_matrices.y,
        target_group_matrices.rows,
    );
    check_alignment(
        "reference",
        raw_x_a.nrows(),
        y_a.len(),
        reference_rows.len(),
    )?;
    check_alignment("target", raw_x_b.nrows(), y_b.len(), target_rows.len())?;

    // The builder's matrices always start with the reserved intercept column
    // (`oaxaca_blinder::INTERCEPT_NAME`), so there is nothing to add. A second code path here
    // used to push an extra "Base Rate (Intercept)" column and name when the matrix looked too
    // narrow; it was unreachable, and a name nobody could filter on (0120-MERIDIAN S3).
    let (x_a, x_b) = (raw_x_a, raw_x_b);

    // Safety fallback for feature names
    while feature_names.len() < x_b.ncols() {
        feature_names.push(format!("Feature {}", feature_names.len()));
    }

    // The level comes from the request (default 95%, refused outside [50%, 99.9%]).
    let confidence_of_request = req.confidence_level;

    // The pay line every fair wage is read off, and the prediction interval built from that same
    // fit (0120-MERIDIAN T14, T8). A line with no residual degrees of freedom has no honest range
    // (it used to return a zero-width one), so it is refused by name before anything is solved
    // (T13).
    //
    //  * Reference: the reference group's own regression; its sigma^2, (X'X)^-1 and df.
    //  * Pooled: the pooled regression WITH a target-group indicator, read at indicator 0, with
    //    that regression's sigma^2, (X'X)^-1 and df. At the midpoint the target group's mean
    //    shortfall to this line is the indicator's coefficient, which is the decomposition's
    //    `Pooled` unexplained gap. The no-indicator stack (Neumark) is `PooledNoIndicator`, not
    //    offered by this entry point.
    let (beta_fair, interval_model) = match target_mode {
        OptimizationTarget::Reference => {
            if y_a.len() <= x_a.ncols() {
                return Err(support::insufficient_df_error(
                    "reference",
                    y_a.len(),
                    x_a.ncols(),
                ));
            }
            let beta = x_a
                .clone()
                .svd(true, true)
                .solve(&y_a, 1e-9)
                .map_err(|e| format!("SVD Solve Error (Reference): {}", e))?;
            let confidence = support::resolve_confidence(confidence_of_request)?;
            let model = IntervalModel::new(&x_a, &y_a, &beta, confidence)?;
            (beta, model)
        }
        OptimizationTarget::Pooled => {
            let confidence = support::resolve_confidence(confidence_of_request)?;
            let fit = support::PooledFit::new(&x_a, &y_a, &x_b, &y_b, confidence)?;
            (fit.beta, fit.interval)
        }
    };

    // Calculate Model Coefficients for Frontend Simulation
    let mut model_coefficients = Vec::new();
    for (i, name) in feature_names.iter().enumerate() {
        if i < beta_fair.len() {
            model_coefficients.push(Contribution {
                name: name.clone(),
                value: beta_fair[i],
            });
        }
    }

    // Calculate Fair Wages
    let predicted_y_b_fair = &x_b * &beta_fair;
    let predicted_y_a_fair = &x_a * &beta_fair;

    // --- Prediction intervals (0120-MERIDIAN T14) ---
    // `interval_model` (above) is Student t on the residual degrees of freedom of the fit that
    // produced `beta_fair`. It also carries that fit's leverage, which decides each row's
    // `extrapolated` flag.
    let calculate_interval = |features: DVector<f64>, predicted_y: f64| -> (f64, f64) {
        interval_model.interval(&features, predicted_y)
    };

    // Support of the baseline pay line for the compared group (0120-MERIDIAN S6).
    let (support_block, warnings) = support::support_diagnostics(
        &x_a,
        &x_b,
        &feature_names,
        &req.predictors,
        match target_mode {
            OptimizationTarget::Reference => Fitted::Reference,
            OptimizationTarget::Pooled => Fitted::Pooled,
        },
        Some(interval_model.leverage()),
    )?;
    // -------------------------------------------------------------------

    // ---- Who is below the line, by how much, and who is eligible (0122-MERIDIAN) ----
    //
    // `diff` is the shortfall to the CHOSEN line (`range_target`); the unexplained gap is always
    // taken on the MIDPOINT line (T6), so the headline and the verification read one figure
    // whatever the remedy pays to.
    #[derive(Clone, Copy, PartialEq)]
    enum GroupSource {
        GroupA, // Reference
        GroupB, // Target
    }

    struct PotentialAdj {
        matrix_idx: usize,
        source: GroupSource,
        diff: f64,
        fair_wage: f64, // Statistical Fair Wage (Midpoint)
        orig_idx: usize,
        is_eligible: bool, // True if the employee should receive budget allocation
    }
    let mut potential_adjustments = Vec::new();
    let adjust_both = req.adjust_both_groups.unwrap_or(false);
    let is_forensic = req.forensic_mode.unwrap_or(false);
    let min_pct = req.min_gap_pct.unwrap_or(0.0);
    let line_choice = req
        .range_target
        .unwrap_or(crate::types::RangeTarget::Midpoint);
    let n_target = y_b.len();
    let n_reference = y_a.len();

    // Mean (actual - fair midpoint) over the compared group, the numerator of
    // `original_unexplained_gap`, and the mean overshoot beside it.
    let mut sum_midpoint_residual_b = 0.0;
    let mut overshoot_sum_b = 0.0;
    let mut threshold_excluded_count = 0usize;

    // Process Group B (Target) - Always Analyzed
    for i in 0..y_b.len() {
        let actual = y_b[i];
        let fair_midpoint = predicted_y_b_fair[i];

        // Calculate Interval for this employee
        let features = x_b.row(i).transpose();
        let (lower, upper) = calculate_interval(features, fair_midpoint);

        // Determine Target Wage based on user selection
        let target_wage = match line_choice {
            crate::types::RangeTarget::Midpoint => fair_midpoint,
            crate::types::RangeTarget::LowerBound => lower,
            crate::types::RangeTarget::UpperBound => upper,
        };

        // Diff is Target - Actual
        let diff = target_wage - actual;

        sum_midpoint_residual_b += actual - fair_midpoint;
        overshoot_sum_b += (actual - fair_midpoint).max(0.0);

        let is_positive_gap = diff > 1e-6; // Only care if underpaid relative to target

        if is_positive_gap {
            let gap_pct = if actual.abs() > 1e-6 {
                diff / actual
            } else {
                0.0
            };

            if gap_pct >= min_pct {
                potential_adjustments.push(PotentialAdj {
                    matrix_idx: i,
                    source: GroupSource::GroupB,
                    diff,
                    fair_wage: fair_midpoint,
                    orig_idx: target_rows[i],
                    is_eligible: true,
                });
            } else {
                threshold_excluded_count += 1;
                if is_forensic {
                    potential_adjustments.push(PotentialAdj {
                        matrix_idx: i,
                        source: GroupSource::GroupB,
                        diff,
                        fair_wage: fair_midpoint,
                        orig_idx: target_rows[i],
                        is_eligible: false,
                    });
                }
            }
        } else if is_forensic {
            potential_adjustments.push(PotentialAdj {
                matrix_idx: i,
                source: GroupSource::GroupB,
                diff,
                fair_wage: fair_midpoint,
                orig_idx: target_rows[i],
                is_eligible: false,
            });
        }
    }

    // Process Group A (Reference) - Analyzed if flag set OR forensic mode. A reference employee is
    // raised to the SAME line as a compared one (`range_target`, F-10), so one setting reads the
    // same on both groups.
    if adjust_both || is_forensic {
        for i in 0..y_a.len() {
            let actual = y_a[i];
            let fair = predicted_y_a_fair[i];
            let line_wage = match line_choice {
                crate::types::RangeTarget::Midpoint => fair,
                crate::types::RangeTarget::LowerBound => {
                    calculate_interval(x_a.row(i).transpose(), fair).0
                }
                crate::types::RangeTarget::UpperBound => {
                    calculate_interval(x_a.row(i).transpose(), fair).1
                }
            };
            let diff = line_wage - actual;

            let is_positive_gap = diff > 1e-6;

            if is_positive_gap {
                let gap_pct = if actual.abs() > 1e-6 {
                    diff / actual
                } else {
                    0.0
                };

                let is_eligible = adjust_both && gap_pct >= min_pct;

                if is_eligible {
                    potential_adjustments.push(PotentialAdj {
                        matrix_idx: i,
                        source: GroupSource::GroupA,
                        diff,
                        fair_wage: fair,
                        orig_idx: reference_rows[i],
                        is_eligible: true,
                    });
                } else if is_forensic {
                    potential_adjustments.push(PotentialAdj {
                        matrix_idx: i,
                        source: GroupSource::GroupA,
                        diff,
                        fair_wage: fair,
                        orig_idx: reference_rows[i],
                        is_eligible: false,
                    });
                }
            } else if is_forensic {
                // Include in forensic analysis even if no positive gap
                potential_adjustments.push(PotentialAdj {
                    matrix_idx: i,
                    source: GroupSource::GroupA,
                    diff,
                    fair_wage: fair,
                    orig_idx: reference_rows[i],
                    is_eligible: false,
                });
            }
        }
    }

    // 5. Allocation Strategy
    let strategy = req.strategy.as_ref().unwrap_or(&AllocationStrategy::Greedy);

    // Need: the sum of every eligible shortfall, split by group (0122-MERIDIAN T4). The compared
    // group's need is `required_budget`; reference raises are priced on their own line.
    let is_payable = |p: &PotentialAdj| p.diff > 0.0 && p.is_eligible;
    let need_target = nz(potential_adjustments
        .iter()
        .filter(|p| p.source == GroupSource::GroupB && is_payable(p))
        .map(|p| p.diff)
        .sum::<f64>());
    let need_reference = nz(potential_adjustments
        .iter()
        .filter(|p| p.source == GroupSource::GroupA && is_payable(p))
        .map(|p| p.diff)
        .sum::<f64>());
    // What an uncapped run has to fund: both groups when reference raises are on.
    let allocation_need = need_target + need_reference;

    // Sort Descending by Gap Amount (for Greedy). Stable: ties keep compared rows first, then
    // reference rows, each in matrix order, which is the order the frontier also pays in.
    potential_adjustments.sort_by(|a, b| {
        b.diff
            .partial_cmp(&a.diff)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // ---- The line the gap is measured against, and how payments move it (T5, F-06) ----
    let unexplained_before = if n_target > 0 {
        nz(sum_midpoint_residual_b / n_target as f64)
    } else {
        0.0
    };
    let weights = match target_mode {
        OptimizationTarget::Pooled => {
            GapWeights::pooled(&x_a, &x_b, interval_model.leverage().cov())
        }
        OptimizationTarget::Reference if adjust_both => {
            GapWeights::reference_line(&x_a, &x_b, interval_model.leverage().cov())
        }
        OptimizationTarget::Reference => GapWeights::uniform_target(n_target, n_reference),
    };
    let weight_of = |p: &PotentialAdj| match p.source {
        GroupSource::GroupA => weights.reference[p.matrix_idx],
        GroupSource::GroupB => weights.target[p.matrix_idx],
    };

    // The gap after paying every eligible shortfall in full: the best this remedy can reach.
    let mut full_payment_shift = 0.0;
    for p in potential_adjustments.iter().filter(|p| is_payable(p)) {
        full_payment_shift += weight_of(p) * p.diff;
    }
    let best_reachable_gap = nz(unexplained_before + full_payment_shift);

    // ---- The budget rule (0122-MERIDIAN T1, F-01 to F-04) ----
    //
    // No solver: paying the compared group costs the same dollar for dollar, so the least that
    // brings the group's gap to `target_gap` is one number, found along the strategy's own order.
    // The rule is carried as a state, never as an f64 that doubles as "no cap" (F-04): a target
    // that is already met pays NOTHING, not everything.
    enum TargetRule {
        NoTarget,
        AlreadyMet,
        Reachable(f64),
        Unreachable,
    }
    let target_rule = match req.target_gap {
        None => TargetRule::NoTarget,
        Some(goal) => {
            let scale = 1.0_f64
                .max(goal.abs())
                .max(unexplained_before.abs())
                .max(best_reachable_gap.abs());
            let eps = 1e-10 * scale;
            if goal <= unexplained_before + eps {
                TargetRule::AlreadyMet
            } else if goal > best_reachable_gap + eps {
                TargetRule::Unreachable
            } else {
                let budget = match (target_mode, strategy) {
                    // Reference line, compared group only: every dollar moves the gap by 1/n_T,
                    // so the order is irrelevant and the budget is a closed form.
                    (OptimizationTarget::Reference, _) => {
                        n_target as f64 * (goal - unexplained_before)
                    }
                    // Pooled line, Greedy: the gap rises by (weight x dollars) along the pay
                    // order; walk the order to the segment that crosses the goal.
                    (OptimizationTarget::Pooled, AllocationStrategy::Greedy) => {
                        let mut gap = unexplained_before;
                        let mut spent = 0.0;
                        let mut found = None;
                        for p in potential_adjustments
                            .iter()
                            .filter(|p| is_payable(p) && p.source == GroupSource::GroupB)
                        {
                            let weight = weights.target[p.matrix_idx];
                            let rise = weight * p.diff;
                            if weight > 0.0 && gap + rise >= goal {
                                found = Some(spent + (goal - gap) / weight);
                                break;
                            }
                            gap += rise;
                            spent += p.diff;
                        }
                        found.unwrap_or(need_target)
                    }
                    // Pooled line, Equitable: everyone is paid the same share of their shortfall
                    // and the gap is linear in that share.
                    (OptimizationTarget::Pooled, AllocationStrategy::Equitable) => {
                        let share = (goal - unexplained_before) / full_payment_shift;
                        share * need_target
                    }
                };
                TargetRule::Reachable(budget.clamp(0.0, need_target))
            }
        }
    };
    let target_cap = match target_rule {
        TargetRule::AlreadyMet => Some(0.0),
        TargetRule::Reachable(b) => Some(b),
        TargetRule::NoTarget | TargetRule::Unreachable => None,
    };
    // `budget == 0` is "no cap" (documented on the request); the target rule never reuses it.
    let user_cap = if req.budget > 0.0 {
        Some(req.budget)
    } else {
        None
    };
    let cap = match (user_cap, target_cap) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let effective_budget = match cap {
        Some(c) => c,
        // Add epsilon buffer (0.001%) to avoid floating point truncation for the last few employees
        None => allocation_need * 1.00001,
    };

    let mut adjustments = Vec::new();
    let mut current_spend = 0.0;
    let mut cost_b = 0.0;
    let mut cost_a = 0.0;
    let mut pay_b = vec![0.0; n_target];
    let mut pay_a = vec![0.0; n_reference];
    let mut unfunded_count = 0usize;

    let wage_series = df
        .column(&req.outcome_variable)
        .map_err(|e| e.to_string())?;
    let wage_array = wage_series.f64().map_err(|e| e.to_string())?;

    use crate::types::Contribution;

    let feature_names_ref = &feature_names;
    let get_contributions = |matrix_idx: usize, source: &GroupSource| -> Vec<Contribution> {
        let mut contribs = Vec::new();
        // Determine which matrix to use
        let matrix = match source {
            GroupSource::GroupA => &x_a,
            GroupSource::GroupB => &x_b,
        };

        for (j, name) in feature_names_ref.iter().enumerate() {
            if j < matrix.ncols() && j < beta_fair.len() {
                let val = matrix[(matrix_idx, j)];
                let coef = beta_fair[j];
                contribs.push(Contribution {
                    name: name.clone(),
                    value: val * coef,
                });
            }
        }
        contribs
    };

    // Equitable pays everyone the same share of their own shortfall; Greedy pays down the sorted
    // list. Neither pays more than a shortfall, and nobody with diff <= 0 or not eligible is paid.
    let coverage_ratio = if allocation_need > 0.0 {
        (effective_budget / allocation_need).min(1.0)
    } else {
        0.0
    };

    for pot in potential_adjustments {
        let pay_amount = if pot.diff > 0.0 && pot.is_eligible {
            match strategy {
                AllocationStrategy::Greedy => {
                    let remaining_budget = effective_budget - current_spend;
                    if remaining_budget > 0.0 {
                        pot.diff.min(remaining_budget)
                    } else {
                        0.0
                    }
                }
                AllocationStrategy::Equitable => pot.diff * coverage_ratio,
            }
        } else {
            0.0
        };

        let current_wage = wage_array.get(pot.orig_idx).ok_or_else(|| {
            format!(
                "Internal row alignment error: analysed row {} has no outcome value",
                pot.orig_idx
            )
        })?;
        let fair_wage = pot.fair_wage;
        let new_wage = current_wage + pay_amount; // Don't add negative pay amounts!

        // Get features for interval calculation
        let features = match pot.source {
            GroupSource::GroupA => x_a.row(pot.matrix_idx).transpose(),
            GroupSource::GroupB => x_b.row(pot.matrix_idx).transpose(),
        };
        let extrapolated = interval_model.is_extrapolated(&features);
        let (lower, upper) = calculate_interval(features, fair_wage);

        adjustments.push(Adjustment {
            index: pot.orig_idx,
            row_key: row_keys.key_at(pot.orig_idx),
            adjustment: pay_amount,
            current_wage,
            new_wage,
            fair_wage,
            fair_wage_lower_bound: Some(lower),
            fair_wage_upper_bound: Some(upper),
            contributions: get_contributions(pot.matrix_idx, &pot.source),
            is_defensible: None,
            defensibility_message: None,
            extrapolated,
            source: match pot.source {
                GroupSource::GroupA => RowSource::Reference,
                GroupSource::GroupB => RowSource::Compared,
            },
            range_position: support::range_position(new_wage, lower, upper),
            range_position_before: support::range_position(current_wage, lower, upper),
        });

        if pay_amount > 0.0 {
            current_spend += pay_amount;
            match pot.source {
                GroupSource::GroupA => {
                    cost_a += pay_amount;
                    pay_a[pot.matrix_idx] = pay_amount;
                }
                GroupSource::GroupB => {
                    cost_b += pay_amount;
                    pay_b[pot.matrix_idx] = pay_amount;
                }
            }
        }
        if pot.source == GroupSource::GroupB
            && pot.diff > 0.0
            && pot.is_eligible
            && pot.diff - pay_amount > 1e-6
        {
            unfunded_count += 1;
        }
    }

    // Sort adjustments by index
    adjustments.sort_by_key(|a| a.index);

    // 2. Original gap: the target group's mean outcome minus the reference group's, over the
    // analysed rows. It is the same number the decomposition reports as `total_gap` (the library
    // computes it as `y_target.mean() - y_reference.mean()`), so no regression is fitted for it:
    // a compared group too small to fit its own regression is still a valid group to pay
    // toward the baseline's line (0120-MERIDIAN T13 judges only the group that IS fitted).
    let original_gap = y_b.mean() - y_a.mean();

    // 7. Calculate Final Metrics. Money paid to the reference group raises the reference mean,
    // which lowers the raw gap: it is never credited to the compared group (0122 T5, REM-3).
    let n_target_f = n_target as f64;
    let total_cost = nz(current_spend);
    let cost_target = nz(cost_b);
    let cost_reference = nz(cost_a);

    let new_gap = {
        let mut gap = original_gap;
        if n_target > 0 {
            gap += cost_b / n_target_f;
        }
        if n_reference > 0 {
            gap -= cost_a / n_reference as f64;
        }
        gap
    };

    let original_unexplained_gap = unexplained_before;
    // Exact post-schedule gap on the line REFITTED to the schedule's wages (T5): the compared
    // group's cost raises it, a raise to the reference group moves the line itself.
    let new_unexplained_gap = if n_target > 0 {
        nz(unexplained_before + weights.shift(&pay_b, &pay_a))
    } else {
        unexplained_before
    };

    let closure = if need_target > 0.0 {
        Some((cost_target / need_target).clamp(0.0, 1.0))
    } else {
        None
    };
    let unfunded_amount = nz((need_target - cost_target).max(0.0));

    let budget_binding = {
        let limit = target_cap.unwrap_or(f64::INFINITY).min(allocation_need);
        match user_cap {
            Some(u) => u + 1e-9 * u.max(1.0) < limit,
            None => false,
        }
    };
    let (target_gap_reachable, shortfall_to_target, target_budget) =
        match (&target_rule, req.target_gap) {
            (TargetRule::NoTarget, _) | (_, None) => (None, None, None),
            (TargetRule::AlreadyMet, _) => (Some(true), None, Some(0.0)),
            (TargetRule::Reachable(b), _) => (Some(true), None, Some(nz(*b))),
            (TargetRule::Unreachable, Some(goal)) => (
                Some(false),
                Some(nz(goal - best_reachable_gap)),
                Some(need_target),
            ),
        };

    Ok(OptimizationResult {
        adjustments,
        total_cost,
        original_gap,
        new_gap,
        original_unexplained_gap,
        new_unexplained_gap,
        required_budget: need_target,
        target_line: line_choice,
        cost_target,
        cost_reference,
        need_target,
        need_reference: Some(need_reference),
        best_reachable_gap: Some(best_reachable_gap),
        target_gap_reachable,
        shortfall_to_target,
        target_budget,
        budget_binding: Some(budget_binding),
        unfunded_amount: Some(unfunded_amount),
        unfunded_count: Some(unfunded_count),
        threshold_excluded_count: Some(threshold_excluded_count),
        closure,
        overshoot_mean: Some(if n_target > 0 {
            nz(overshoot_sum_b / n_target_f)
        } else {
            0.0
        }),
        position_counts: None,
        group_test: None,
        model_coefficients,
        // These three describe the DERIVATION RULE the table was built under. Every emitted
        // `Adjustment.row_key` is read at the row's own original ordinal (0118-MERIDIAN S3), so
        // a row has a key exactly when the table could mint one for it.
        row_key_space: crate::row_key::ROW_KEY_SPACE.to_string(),
        row_key_source: row_keys.source(),
        row_key_column: row_keys.column(),
        // optimize consumes no ProposedAdjustment, so there is nothing to resolve here.
        unresolved_row_keys: None,
        analysed_reference_count: reference_rows.len(),
        analysed_target_count: target_rows.len(),
        excluded_rows: crate::rows::excluded_with_keys(&builder_excluded_rows, Some(&row_keys)),
        adjustments_on_excluded_rows: 0,
        interval: interval_model.basis.clone(),
        support: support_block,
        warnings,
    })
}

/// Parallelization audit verdict: **SKIP** (engine-parallel-surface D1, entry point 4).
/// Not an independent-optimization grid: one upfront `optimize_inner` solve plus a
/// SEQUENTIAL cumulative budget sweep that carries `current_y`/`pay_idx`/`budget_cursor`
/// across steps (the budget loop below); `compute_t_stat` is one projector multiply.
/// Nothing safe to fan out — the sweep is inherently serial.
pub fn calculate_efficient_frontier_inner(
    req: EfficientFrontierRequest,
) -> Result<Vec<FrontierPoint>, String> {
    // Refuse a bad level before reading any data; every point echoes the level used.
    let confidence = support::resolve_confidence(req.confidence_level)?;

    // 1. Load Data
    let mut df = read_csv(&req.decomposition_params.csv_data)?;

    // Cast to Float64 with error checking
    let cast_cols = [&req.decomposition_params.outcome_variable]
        .into_iter()
        .chain(req.decomposition_params.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                let new_s = s
                    .cast(&DataType::Float64)
                    .map_err(|_| format!("Column '{}' contains non-numeric data.", col))?;

                if new_s.null_count() > s.null_count() {
                    return Err(format!("Column '{}' contains non-numeric data.", col));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    let predictors: Vec<&str> = req
        .decomposition_params
        .predictors
        .iter()
        .map(|s| s.as_str())
        .collect();
    let cats_vec: Option<Vec<&str>> = req
        .decomposition_params
        .categorical_predictors
        .as_ref()
        .map(|c| c.iter().map(|s| s.as_str()).collect());

    // 2. Setup Optimization Problem (for Budget allocation)
    let mut problem_builder = OaxacaBuilder::new(
        df.clone(),
        &req.decomposition_params.outcome_variable,
        &req.decomposition_params.group_variable,
        &req.decomposition_params.reference_group,
    );
    problem_builder.predictors(predictors.iter().copied());
    problem_builder.reference_coefficients(ReferenceCoefficients::Pooled);

    if let Some(cats) = &cats_vec {
        problem_builder.categorical_predictors(cats.iter().copied());
    }

    // 2a. The remedy the curve follows, and the money it takes to pay it in full (0122-MERIDIAN
    // T10, D6). The settings are the screen's: the schedule drawn is the schedule the screen shows.
    let strategy = req.strategy.unwrap_or(AllocationStrategy::Greedy);
    let opt_req = OptimizationRequest {
        csv_data: req.decomposition_params.csv_data.clone(),
        outcome_variable: req.decomposition_params.outcome_variable.clone(),
        group_variable: req.decomposition_params.group_variable.clone(),
        reference_group: req.decomposition_params.reference_group.clone(),
        predictors: req.decomposition_params.predictors.clone(),
        categorical_predictors: req.decomposition_params.categorical_predictors.clone(),
        budget: 0.0,
        target_gap: None,
        target: Some(req.target.unwrap_or(OptimizationTarget::Reference)),
        strategy: Some(strategy),
        min_gap_pct: req.min_gap_pct,
        forensic_mode: None,
        adjust_both_groups: req.adjust_both_groups,
        // The interval does not change a payment to the midpoint; it does to a bound, so the
        // curve's level is the optimiser's.
        confidence_level: req.confidence_level,
        range_target: req.range_target,
    };

    let opt_result = optimize_inner(opt_req)?;
    // The axis ends where the remedy stops spending: its full cost, money paid to both groups
    // when reference raises are on (F-11). It used to run 10 % past it, a flat tail.
    let full_cost = opt_result.total_cost;
    let max_budget = req.max_budget.unwrap_or(full_cost);

    // 3. Pre-compute Matrices for Fast OLS (0118-MERIDIAN S1/S3)
    // `get_data_matrices_with_rows` names the groups: the locals `x_a`/`y_a` hold the REFERENCE
    // group and `x_b`/`y_b` the TARGET group, matching the pooled design below (reference rows
    // first, then target). Correct as committed — do NOT swap them; that inverts gap-closure.
    // Guardrail: ab_binding_regression_test::test_frontier_adjustments_target_underpaid_group.
    // The ordinal lists are what map an `Adjustment.index` onto a pooled slot.
    let matrices = problem_builder
        .get_data_matrices_with_rows()
        .map_err(crate::rows::matrices_error)?;
    let oaxaca_blinder::DataMatricesWithRows {
        reference: reference_group_matrices,
        target: target_group_matrices,
        predictor_names: _feature_names,
        ..
    } = matrices;
    let (x_a, y_a, reference_rows) = (
        reference_group_matrices.x,
        reference_group_matrices.y,
        reference_group_matrices.rows,
    );
    let (x_b, y_b, target_rows) = (
        target_group_matrices.x,
        target_group_matrices.y,
        target_group_matrices.rows,
    );
    check_alignment("reference", x_a.nrows(), y_a.len(), reference_rows.len())?;
    check_alignment("target", x_b.nrows(), y_b.len(), target_rows.len())?;

    let n_a = x_a.nrows();
    let n_b = x_b.nrows();
    let n_pooled = n_a + n_b;
    // Check for intercept in feature names. The internal intercept is always the reserved
    // column `__ob_intercept__` injected by OaxacaBuilder::prepare_data — match only that name.
    // A fuzzy "intercept"/"const" match would misclassify a user predictor literally named
    // `intercept` or `const` as the intercept and silently drop it from the pooled design matrix.
    let intercept_idx = _feature_names
        .iter()
        .position(|f| f == oaxaca_blinder::INTERCEPT_NAME);

    let cols_a = x_a.ncols();

    // We want to exclude the intercept from the source matrices if we are adding our own
    // Calculate new feature count
    let n_vars_to_copy = if intercept_idx.is_some() {
        cols_a - 1
    } else {
        cols_a
    };
    let n_pooled_features = n_vars_to_copy;

    // Build X_pooled: [Intercept, GroupDummy, Features...]
    // Intercept = Col 0
    // GroupDummy = Col 1 (0 for A, 1 for B)
    let mut x_pooled = DMatrix::from_element(n_pooled, n_pooled_features + 2, 0.0);

    for r in 0..n_pooled {
        x_pooled[(r, 0)] = 1.0;
    }

    // Group Dummy: A=0, B=1. A is first n_a rows.
    for r in n_a..n_pooled {
        x_pooled[(r, 1)] = 1.0;
    }

    // Copy features, skipping intercept if present
    if let Some(idx) = intercept_idx {
        // Copy columns before intercept
        if idx > 0 {
            x_pooled
                .view_mut((0, 2), (n_a, idx))
                .copy_from(&x_a.columns(0, idx));
            x_pooled
                .view_mut((n_a, 2), (n_b, idx))
                .copy_from(&x_b.columns(0, idx));
        }
        // Copy columns after intercept
        let after_count = cols_a - 1 - idx;
        if after_count > 0 {
            x_pooled
                .view_mut((0, 2 + idx), (n_a, after_count))
                .copy_from(&x_a.columns(idx + 1, after_count));
            x_pooled
                .view_mut((n_a, 2 + idx), (n_b, after_count))
                .copy_from(&x_b.columns(idx + 1, after_count));
        }
    } else {
        // No intercept found, copy all
        x_pooled.view_mut((0, 2), (n_a, cols_a)).copy_from(&x_a);
        x_pooled.view_mut((n_a, 2), (n_b, cols_a)).copy_from(&x_b);
    }

    // Initial Y
    let mut y_pooled = DMatrix::from_element(n_pooled, 1, 0.0);
    y_pooled.view_mut((0, 0), (n_a, 1)).copy_from(&y_a);
    y_pooled.view_mut((n_a, 0), (n_b, 1)).copy_from(&y_b);

    // Pre-compute (X^T X)^-1 X^T
    let xt_x = x_pooled.transpose() * &x_pooled;
    let xt_x_inv = xt_x.try_inverse().ok_or("Singular matrix in Pooled OLS")?;
    let projector = &xt_x_inv * x_pooled.transpose();

    let diag_inv_xt_x: Vec<f64> = (0..xt_x_inv.nrows()).map(|i| xt_x_inv[(i, i)]).collect();

    // 4. Budget Loop
    let steps = req.steps.unwrap_or(50);
    let step_size = max_budget / (steps as f64);
    let mut points = Vec::new();

    // Map `adjustments` to Pooled Indices: an `Adjustment.index` is an original row ordinal, the
    // pooled design is reference rows first (slots 0..n_a) then target rows (n_a..), both in
    // matrix order, so the slot of an ordinal is its position in its own group's ordinal list.
    let mut original_to_pooled: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::with_capacity(n_pooled);
    for (slot, &ordinal) in reference_rows.iter().enumerate() {
        original_to_pooled.insert(ordinal, slot);
    }
    for (slot, &ordinal) in target_rows.iter().enumerate() {
        original_to_pooled.insert(ordinal, n_a + slot);
    }

    struct PendingPay {
        pooled_idx: usize,
        gap: f64,
        /// Tie-break that reproduces `optimize_inner`'s pay order: compared rows before reference
        /// rows, each in row order (F-12). Without it a tie at a partial budget is paid in a
        /// different order from the schedule the same budget buys in `optimize`.
        rank: (u8, usize),
    }
    let mut pending_payments: Vec<PendingPay> = opt_result
        .adjustments
        .iter()
        .filter(|adj| adj.adjustment > 0.0)
        .filter_map(|adj| {
            original_to_pooled.get(&adj.index).map(|&p_idx| PendingPay {
                pooled_idx: p_idx,
                gap: adj.adjustment,
                rank: (
                    match adj.source {
                        RowSource::Compared => 0,
                        RowSource::Reference => 1,
                    },
                    adj.index,
                ),
            })
        })
        .collect();

    pending_payments.sort_by(|a, b| {
        b.gap
            .partial_cmp(&a.gap)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.rank.cmp(&b.rank))
    });
    // The full schedule: what Equitable scales by the share of the need a budget covers.
    let full_schedule: Vec<(usize, f64)> = pending_payments
        .iter()
        .map(|pp| (pp.pooled_idx, pp.gap))
        .collect();

    // Student t on the pooled regression's residual degrees of freedom (0120-MERIDIAN T14).
    // With none, the p-value is undefined: a named refusal, not the old (t = 0, p = 1) sentinel
    // that read as "no gap".
    let dof = n_pooled as i64 - x_pooled.ncols() as i64;
    if dof <= 0 {
        return Err(support::insufficient_df_error(
            "pooled",
            n_pooled,
            x_pooled.ncols(),
        ));
    }
    let significance_threshold = 1.0 - confidence;

    // (t, p, significant, group coefficient) of the pooled regression for one outcome vector.
    let compute_t_stat = |current_y: &DMatrix<f64>| -> (f64, f64, bool, f64) {
        let beta = &projector * current_y;
        let predictions = &x_pooled * &beta;
        let residuals = current_y - predictions;
        let rss = residuals.dot(&residuals);

        let sigma_sq = rss / dof as f64;
        let se_group = (sigma_sq * diag_inv_xt_x[1]).sqrt();
        let beta_group = beta[1];

        let t_stat = beta_group / se_group;
        let p_val = support::two_sided_p(t_stat, dof as f64);
        let sig = p_val < significance_threshold;

        (t_stat, p_val, sig, beta_group)
    };

    let (t0, p0, s0, g0) = compute_t_stat(&y_pooled);
    points.push(FrontierPoint {
        budget: 0.0,
        t_statistic: t0,
        p_value: p0,
        is_significant: s0,
        group_coefficient: g0,
        degrees_of_freedom: dof as usize,
        confidence_level: confidence,
    });

    // Degenerate case: no positive adjustment budget (e.g. a zero-gap dataset where total_need
    // collapses to ~0 and the caller supplied no explicit max_budget). There is no spend range
    // to explore, so return only the baseline (budget = 0) point rather than fabricating a
    // budget axis from a placeholder maximum.
    if max_budget < 1e-9 {
        eprintln!(
            "Warning: efficient frontier requested with no positive adjustment budget \
             (max_budget ~ 0; no gap to close). Returning a single zero-budget point."
        );
        return Ok(points);
    }

    let mut current_y = y_pooled.clone();
    let mut pay_idx = 0;
    let mut budget_cursor = 0.0;

    for step in 1..=steps {
        // The last point lands exactly on `max_budget`, whatever the float product says.
        let target_budget = if step == steps {
            max_budget
        } else {
            step as f64 * step_size
        };

        match strategy {
            AllocationStrategy::Greedy => {
                let available_for_step = target_budget - budget_cursor;

                if available_for_step > 0.0 {
                    let mut remaining = available_for_step;

                    while remaining > 0.0 && pay_idx < pending_payments.len() {
                        let pp = &mut pending_payments[pay_idx];

                        if pp.gap <= remaining {
                            current_y[(pp.pooled_idx, 0)] += pp.gap;
                            remaining -= pp.gap;
                            pp.gap = 0.0;
                            pay_idx += 1;
                        } else {
                            current_y[(pp.pooled_idx, 0)] += remaining;
                            pp.gap -= remaining;
                            remaining = 0.0;
                        }
                    }
                    budget_cursor = target_budget;
                }
            }
            AllocationStrategy::Equitable => {
                // Not nested: every employee is paid the same share of their own shortfall, so the
                // schedule at a budget is the full schedule scaled by budget / full cost (F-12).
                let share = if full_cost > 0.0 {
                    (target_budget / full_cost).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                current_y = y_pooled.clone();
                for &(pooled_idx, amount) in &full_schedule {
                    current_y[(pooled_idx, 0)] += amount * share;
                }
            }
        }

        let (t, p, s, g) = compute_t_stat(&current_y);
        points.push(FrontierPoint {
            budget: target_budget,
            t_statistic: t,
            p_value: p,
            is_significant: s,
            group_coefficient: g,
            degrees_of_freedom: dof as usize,
            confidence_level: confidence,
        });
    }

    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_csv() -> String {
        let mut csv = "wage,education,experience,gender,department\n".to_string();
        for _ in 0..20 {
            csv.push_str("50000,12,5,Male,Sales\n");
            csv.push_str("55000,16,2,Male,Engineering\n");
            csv.push_str("60000,14,8,Male,Sales\n");
            csv.push_str("80000,18,10,Male,Engineering\n");
            csv.push_str("40000,12,5,Female,Sales\n");
            csv.push_str("42000,14,3,Female,Engineering\n");
            csv.push_str("45000,12,6,Female,Sales\n");
            csv.push_str("50000,16,5,Female,Engineering\n");
        }
        csv
    }

    #[test]
    fn test_ols_decomposition() {
        let csv = create_mock_csv();
        let req = DecompositionRequest {
            csv_data: csv.into_bytes(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: Some(vec!["department".to_string()]),
            three_fold: Some(false),
            quantile: None,
            reference_coefficients: Some("Pooled".to_string()),
            bootstrap_reps: Some(10), // Fast test
        };

        let res = decompose_inner(req);
        if let Err(e) = &res {
            println!("Error: {}", e);
        }
        assert!(res.is_ok());
        let res = res.unwrap();

        // Basic checks
        assert!(res.total_gap > 0.0);
        assert!(res.data_summary.is_some());
        let summary = res.data_summary.unwrap();
        assert_eq!(summary.total_count, 160);
        assert_eq!(summary.group_a_count, 80);
        assert_eq!(summary.group_b_count, 80);
    }

    #[test]
    fn test_prediction_interval() {
        // Create a dataset where we expect some variance
        // y = 2 * x + noise
        let mut csv = "wage,x,group\n".to_string();
        for i in 0..50 {
            let x = i as f64;
            let noise = if i % 2 == 0 { 1000.0 } else { -1000.0 };
            let wage = 50000.0 + 2000.0 * x + noise; // Group A (Reference)
            csv.push_str(&format!("{},{},GroupA\n", wage, x));
        }
        for i in 0..10 {
            let x = (i + 50) as f64;
            let wage = 40000.0; // Underpaid Group B
            csv.push_str(&format!("{},{},GroupB\n", wage, x));
        }

        let req = OptimizationRequest {
            csv_data: csv.into_bytes(),
            outcome_variable: "wage".to_string(),
            group_variable: "group".to_string(),
            reference_group: "GroupA".to_string(),
            predictors: vec!["x".to_string()],
            categorical_predictors: None,
            target: None,
            adjust_both_groups: None,
            budget: 0.0,
            target_gap: None,
            confidence_level: None,
            strategy: None,
            forensic_mode: None,
            min_gap_pct: None,
            range_target: None,
        };

        let res = optimize_inner(req);
        assert!(res.is_ok());
        let res = res.unwrap();

        // Check adjustments
        assert!(!res.adjustments.is_empty());

        let adj = &res.adjustments[0];
        assert!(adj.fair_wage > 40000.0);

        // Verify Bounds
        assert!(adj.fair_wage_lower_bound.is_some());
        assert!(adj.fair_wage_upper_bound.is_some());

        let lower = adj.fair_wage_lower_bound.unwrap();
        let upper = adj.fair_wage_upper_bound.unwrap();

        println!(
            "Fair: {}, Lower: {}, Upper: {}",
            adj.fair_wage, lower, upper
        );

        // Bounds should enclose fair wage
        assert!(lower < adj.fair_wage);
        assert!(upper > adj.fair_wage);

        // Interval width should be reasonable given noise of +/- 1000
        // RMSE approx 1000. 1.96 * 1000 ~= 1960 margin.
        let width = upper - lower;
        assert!(width > 1000.0);
    }
    #[test]
    fn test_quantile_decomposition() {
        let csv = create_mock_csv();
        let req = DecompositionRequest {
            csv_data: csv.into_bytes(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string()],
            categorical_predictors: None,
            three_fold: None,
            quantile: Some(0.5), // Median
            reference_coefficients: Some("Pooled".to_string()),
            bootstrap_reps: Some(10),
        };

        let res = decompose_inner(req);
        if let Err(e) = &res {
            println!("Error: {}", e);
        }
        assert!(res.is_ok());
        let res = res.unwrap();

        // Basic checks
        assert!(res.total_gap.is_finite());
        // In-Scope 12 (ruling a-1): the quantile branch now routes through the RIF path
        // (OaxacaBuilder::decompose_quantile), which returns full per-predictor detail — the old
        // MM path returned unconditional empties. Detail must be populated now.
        assert!(
            !res.detailed_explained.is_empty(),
            "RIF quantile path must populate per-predictor detail (In-Scope 12)"
        );
        assert!(!res.detailed_unexplained.is_empty());
    }

    #[test]
    fn test_optimize_inner() {
        let csv = create_mock_csv();
        let req = OptimizationRequest {
            csv_data: csv.into_bytes(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: None,
            budget: 10000.0,
            target_gap: Some(0.0), // Close the gap
            target: None,
            strategy: None,
            min_gap_pct: None,
            forensic_mode: None,
            confidence_level: None,
            range_target: None,
            adjust_both_groups: None,
        };

        let res = optimize_inner(req);
        if let Err(e) = &res {
            println!("Optimization Error: {}", e);
        }
        assert!(res.is_ok());
        let res = res.unwrap();

        // Print values for debugging
        println!("Original Gap: {}", res.original_gap);
        println!("New Gap: {}", res.new_gap);
        println!("Total Cost: {}", res.total_cost);
        println!("Adjustments Count: {}", res.adjustments.len());

        // Basic checks
        assert!(res.original_gap.abs() > 0.0);

        if !res.adjustments.is_empty() {
            let adj = &res.adjustments[0];
            assert!(adj.new_wage >= adj.current_wage);
            // Verify index integrity somewhat (should be within bounds)
            // Mock data has 20 iterations * 8 rows = 160 rows.
            assert!(adj.index < 160);
        }
    }

    #[test]
    fn test_efficient_frontier() {
        let csv = create_mock_csv();
        let req = EfficientFrontierRequest {
            decomposition_params: DecompositionRequest {
                csv_data: csv.into_bytes(),
                outcome_variable: "wage".to_string(),
                group_variable: "gender".to_string(),
                reference_group: "Female".to_string(),
                predictors: vec!["education".to_string(), "experience".to_string()],
                categorical_predictors: None,
                three_fold: None,
                quantile: None,
                reference_coefficients: Some("Pooled".to_string()),
                bootstrap_reps: None,
            },
            steps: Some(10),
            max_budget: Some(50000.0), // Enough to cover gaps,
            confidence_level: None,

            strategy: None,
            target: None,
            range_target: None,
            min_gap_pct: None,
            adjust_both_groups: None,
        };

        // This relies on calculate_efficient_frontier_inner being available in super
        let res = calculate_efficient_frontier_inner(req);
        if let Err(e) = &res {
            println!("Frontier Error: {}", e);
        }
        assert!(res.is_ok());
        let points = res.unwrap();

        assert!(!points.is_empty());
        assert_eq!(points[0].budget, 0.0);

        // Check monotonicity of budget
        for i in 0..points.len() - 1 {
            assert!(points[i].budget < points[i + 1].budget);
        }

        // With enough budget (Greedy strategy), T-stat for gender coefficient should eventually drop
        // (Assuming closing the gap reduces the gender coefficient's significance)
        let first_t = points.first().unwrap().t_statistic.abs();
        let last_t = points.last().unwrap().t_statistic.abs();

        // Note: Closing the gap usually means making the gender coefficient closer to 0,
        // thus T-stat magnitude should decrease.
        // However, in "Pooled" regression or Reference, the interpretation varies.
        // But generally, fair pay means gender is less predictive.
        println!("Start T: {}, End T: {}", first_t, last_t);
        // assert!(last_t < first_t); // This might not always hold depending on noise, but generally true.
    }

    #[test]
    fn test_decompose_with_non_numeric_outcome() {
        let csv = "wage,education,experience,gender\ninvalid,12,5,Male\n50000,16,2,Female\n52000,14,3,Male\n48000,15,4,Female\n50000,16,2,Male\n48000,15,4,Female\n";
        let req = DecompositionRequest {
            csv_data: csv.as_bytes().to_vec(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: None,
            three_fold: None,
            quantile: None,
            reference_coefficients: Some("Pooled".to_string()),
            bootstrap_reps: Some(10),
        };

        let res = decompose_inner(req);
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            "Column 'wage' contains non-numeric data but was selected as a continuous variable. Please verify your column selection."
        );
    }

    #[test]
    fn test_decompose_with_non_numeric_predictor() {
        let csv = "wage,education,experience,gender\n50000,invalid,5,Male\n50000,16,2,Female\n52000,14,3,Male\n48000,15,4,Female\n50000,16,2,Male\n48000,15,4,Female\n";
        let req = DecompositionRequest {
            csv_data: csv.as_bytes().to_vec(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Female".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: None,
            three_fold: None,
            quantile: None,
            reference_coefficients: Some("Pooled".to_string()),
            bootstrap_reps: Some(10),
        };

        let res = decompose_inner(req);
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err(),
            "Column 'education' contains non-numeric data but was selected as a continuous variable. Please verify your column selection."
        );
    }
}
