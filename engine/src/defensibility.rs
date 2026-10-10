use crate::rows::{check_alignment, excluded_with_keys, read_csv};
use crate::support::{self, nz, Fitted, GapWeights, IntervalModel, DEFENSIBLE_TOLERANCE};
use crate::types::*;
use nalgebra::DVector;
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;
// D14 (0017-P1): every map in this function is a BTreeMap, never a std HashMap. std HashMap
// iterates in RandomState order (seeded per process), and three f64 sums below are accumulated
// by iterating a row-index map — `required_budget`, `original_unexplained_gap` and
// `new_unexplained_gap`. Hash order makes those tolerance-equal but not byte-identical across
// runs, and P1 persists them as aggregates that a recompute must reproduce bit for bit.
// BTreeMap fixes the reduction order to ascending row index, so the sums are byte-reproducible.
use std::collections::BTreeMap;

/// Every inbound `ProposedAdjustment` that resolves to one DataFrame row, folded into the single
/// entry that row is scored as. See the collapse in `check_defensibility_inner`.
struct MergedAdjustment {
    /// DataFrame row ordinal this entry scores.
    row_idx: usize,
    /// Sum of every inbound delta naming `row_idx`, accumulated in request order so the sum is
    /// byte-reproducible (D14).
    value: f64,
    /// Per-key union of every inbound override map naming `row_idx`, later entries winning.
    /// BTreeMap, not the inbound `HashMap`: this map is iterated when the predictor columns are
    /// rewritten below (D14).
    predictor_overrides: BTreeMap<String, f64>,
}

/// Parallelization audit verdict: **SKIP** (engine-parallel-surface D1, entry point 5).
/// Scalar defensibility scoring over the decompose output (the scoring arithmetic below) —
/// trivial and serial; no solve, no fan-out site.
pub fn check_defensibility_inner(req: VerificationRequest) -> Result<OptimizationResult, String> {
    check_defensibility_on(req, &OptimizationTarget::Reference)
}

/// `check_defensibility` against the pay line the remedy targeted (review N8). `Reference` is
/// `check_defensibility_inner` exactly; `Pooled` reads every fair wage, bound, `extrapolated` flag
/// and df warning off the pooled regression with a group indicator (`support::PooledFit`), the same
/// fit `optimize_inner` uses for its Pooled target.
pub fn check_defensibility_on(
    req: VerificationRequest,
    target: &OptimizationTarget,
) -> Result<OptimizationResult, String> {
    // 1. Load Data
    let mut df = read_csv(&req.decomposition_params.csv_data)?;

    // 0017-MERIDIAN P4: stable key table, built on the raw parse before the Float64 cast and
    // before the predictor overrides below mutate any cell. Same bytes in, same keys out, so
    // this table is identical to the one `optimize_inner` minted for the same CSV.
    let row_keys = crate::row_key::RowKeyTable::build(&df)?;

    // Resolve every proposed adjustment to a DataFrame row ordinal ONCE, up front: the
    // overrides pass and the scoring loop below must agree on which row each adjustment names,
    // and resolving twice invites them to drift. A `row_key` that does not resolve is skipped
    // and counted rather than falling back to `index` — see `crate::row_key::resolve`.
    //
    // Resolution is many-to-one: two inbound adjustments may name the same row without looking
    // alike on the wire — one by `index`, another by a stale `index` plus that row's `row_key`.
    // They are COLLAPSED here, into one entry per ordinal, because a row has one wage and one
    // defensibility verdict, and every consumer downstream of a non-collapsed duplicate
    // disagreed about which of the two it was:
    //   - the predictor-override pass was last-wins (whole-map replace);
    //   - `result_pos_by_index` (D15) is first-wins, so only the FIRST duplicate's `new_wage`
    //     ever reached `new_gap` / `new_unexplained_gap`;
    //   - the scoring loop emitted one `Adjustment` PER duplicate, and `total_cost` summed them
    //     all, so the per-row array and `total_cost` carried money the headline gap did not —
    //     with `unresolved_row_keys: 0` beside them asserting nothing had been dropped. On a
    //     figure that goes to the CNESST that is a silent failure, not a rounding difference.
    // Two duplicates also emitted two rows sharing one `index` AND one `row_key`, which trips
    // the browser client's own duplicate-key refusal (frontend/src/utils/engineRowKey.js) and
    // made the whole payload unadoptable.
    //
    // Collapse policy, applied in request order so it is byte-reproducible (D14):
    //   - deltas SUM. Nothing is dropped, and this matches `verify_inner`, which already
    //     accumulates repeated deltas onto one row (`analysis.rs`).
    //   - predictor overrides merge PER KEY, later inbound entries winning. Previously a second
    //     duplicate's whole override map replaced the first's; a per-key union keeps both
    //     callers' stated facts and only arbitrates a genuine collision.
    // First-appearance order is preserved, so a request with no duplicates — every current
    // caller — produces byte-identical output to the pre-collapse engine.
    let mut unresolved_row_keys: usize = 0;
    let mut merged: Vec<MergedAdjustment> = Vec::with_capacity(req.adjustments.len());
    // Row ordinal -> slot in `merged`. BTreeMap, not HashMap (D14).
    let mut slot_by_row: BTreeMap<usize, usize> = BTreeMap::new();

    for adj in req.adjustments {
        let Some(row_idx) = row_keys.resolve(adj.index, adj.row_key.as_deref()) else {
            unresolved_row_keys += 1;
            continue;
        };
        let slot = match slot_by_row.get(&row_idx) {
            Some(&slot) => slot,
            None => {
                merged.push(MergedAdjustment {
                    row_idx,
                    value: 0.0,
                    predictor_overrides: BTreeMap::new(),
                });
                let slot = merged.len() - 1;
                slot_by_row.insert(row_idx, slot);
                slot
            }
        };
        let entry = &mut merged[slot];
        entry.value += adj.value;
        if let Some(ovr) = adj.predictor_overrides {
            // `ovr` is the inbound `HashMap`, so this iterates in hash order — harmless, because
            // a key appears at most once in a single map and the destination is keyed by name.
            // Cross-adjustment precedence is fixed by the request-order loop above, not by this.
            for (k, v) in ovr {
                if let Ok(val) = v.parse::<f64>() {
                    entry.predictor_overrides.insert(k, val);
                }
            }
        }
    }

    // Cast to Float64
    let cast_cols = [&req.decomposition_params.outcome_variable]
        .into_iter()
        .chain(req.decomposition_params.predictors.iter());

    for col in cast_cols {
        if let Ok(s) = df.column(col) {
            if s.dtype() != &DataType::Float64 {
                let new_s = s
                    .cast(&DataType::Float64)
                    .map_err(|_| format!("Column '{}' contains non-numeric data.", col))?;

                // 0118-MERIDIAN S4: a non-strict cast turns an unparseable cell ("N/A") into a
                // NULL, which would silently drop that employee from the model and shift every
                // later pairing. Refuse it by name, exactly as `optimize`, `verify_adjustments`
                // and `calculate_efficient_frontier` do.
                if new_s.null_count() > s.null_count() {
                    return Err(format!("Column '{}' contains non-numeric data.", col));
                }

                df.with_column(new_s).map_err(|e| e.to_string())?;
            }
        } else {
            return Err(format!("Column '{}' not found in dataset.", col));
        }
    }

    // 2. Apply Predictor Overrides (Before Model Building)
    // Parsing and per-row merging already happened in the collapse above, so each row appears at
    // most once here and no write can be shadowed by a later one for the same cell.
    let has_overrides = merged.iter().any(|m| !m.predictor_overrides.is_empty());

    if has_overrides {
        for col_name in &req.decomposition_params.predictors {
            if let Ok(s) = df.column(col_name) {
                if let Ok(ca) = s.f64() {
                    let mut vec: Vec<Option<f64>> = ca.into_iter().collect();
                    let mut changed = false;

                    for m in &merged {
                        if let Some(new_val) = m.predictor_overrides.get(col_name) {
                            if m.row_idx < vec.len() {
                                vec[m.row_idx] = Some(*new_val);
                                changed = true;
                            }
                        }
                    }

                    if changed {
                        let new_s = Series::new(col_name.as_str().into(), &vec);
                        df.with_column(new_s).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }

    // Initialize Pay Equity Problem
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

    // Get Matrices (0118-MERIDIAN S1/S3). Taken AFTER the predictor-override step above, so a
    // blank predictor cell that an override fills brings its row back into the analysis, and
    // every ordinal below describes the frame that was actually modelled.
    //
    // `get_data_matrices_with_rows` names the groups: `x_a`/`y_a` below hold the REFERENCE
    // (advantaged) group, from which `beta_fair` is solved, and `x_b` the TARGET group. Correct
    // as committed — do NOT swap them. Guardrail: ab_binding_regression_test.
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
    let (raw_x_b, target_rows) = (target_group_matrices.x, target_group_matrices.rows);
    check_alignment(
        "reference",
        raw_x_a.nrows(),
        y_a.len(),
        reference_rows.len(),
    )?;
    check_alignment(
        "target",
        raw_x_b.nrows(),
        target_group_matrices.y.len(),
        target_rows.len(),
    )?;

    // The builder's matrices always start with the reserved intercept column
    // (`oaxaca_blinder::INTERCEPT_NAME`); see the same note in `optimize_inner`. The unreachable
    // "Base Rate (Intercept)" fallback that used to sit here is removed (0120-MERIDIAN S3).
    let (x_a, x_b) = (raw_x_a, raw_x_b);

    while feature_names.len() < x_b.ncols() {
        feature_names.push(format!("Feature {}", feature_names.len()));
    }

    // The pay line every fair wage is read off, and the prediction interval built from that same fit
    // (0120-MERIDIAN T14, T8, N8). With no residual degrees of freedom there is no honest range (the
    // old code returned a zero-width one), so it is refused by name before anything is solved (T13).
    //
    //  * Reference: the baseline group's own regression, its sigma^2, (X'X)^-1 and df.
    //  * Pooled: the pooled regression WITH a target-group indicator, read at indicator 0, with that
    //    regression's own sigma^2, (X'X)^-1 and df: the line the optimiser's Pooled target prices.
    let (beta_fair, interval_model, fitted) = match target {
        OptimizationTarget::Reference => {
            if y_a.len() <= x_a.ncols() {
                return Err(support::insufficient_df_error(
                    "reference",
                    y_a.len(),
                    x_a.ncols(),
                ));
            }
            // Calculate Fair Beta (Reference Target for "Defensibility")
            let beta = x_a
                .clone()
                .svd(true, true)
                .solve(&y_a, 1e-9)
                .map_err(|e| format!("SVD Solve Error: {}", e))?;
            // The level comes from the request (default 95%, refused outside [50%, 99.9%]).
            let confidence = support::resolve_confidence(req.confidence_level)?;
            let model = IntervalModel::new(&x_a, &y_a, &beta, confidence)?;
            (beta, model, Fitted::Reference)
        }
        OptimizationTarget::Pooled => {
            let confidence = support::resolve_confidence(req.confidence_level)?;
            let fit =
                support::PooledFit::new(&x_a, &y_a, &x_b, &target_group_matrices.y, confidence)?;
            (fit.beta, fit.interval, Fitted::Pooled)
        }
    };
    let calculate_interval = |features: DVector<f64>, predicted_y: f64| -> (f64, f64) {
        interval_model.interval(&features, predicted_y)
    };

    let (support_block, warnings) = support::support_diagnostics(
        &x_a,
        &x_b,
        &feature_names,
        &req.decomposition_params.predictors,
        fitted,
        Some(interval_model.leverage()),
    )?;

    // The level the group test and the intervals are read at (already validated above).
    let confidence = support::resolve_confidence(req.confidence_level)?;

    // How a dollar paid to each analysed row moves the compared group's unexplained gap: one
    // shared function with `optimize` (0122-MERIDIAN T5, F-06), so a schedule has one
    // `new_unexplained_gap` whichever entry point prices it.
    let weights = match target {
        OptimizationTarget::Reference => {
            GapWeights::reference_line(&x_a, &x_b, interval_model.leverage().cov())
        }
        OptimizationTarget::Pooled => {
            GapWeights::pooled(&x_a, &x_b, interval_model.leverage().cov())
        }
    };

    // Process Specific Adjustments
    let mut results = Vec::new();
    let mut adjustments_on_excluded_rows: usize = 0;

    // Mapping Original Row Ordinal -> (Matrix Row, IsReference), over ANALYSED rows only: a row
    // with a blank model cell is in neither list, so it can be neither scored nor counted in any
    // aggregate below. BTreeMap, not HashMap: the f64 accumulations below iterate this
    // map, so its order is the float reduction order (D14); ascending ordinal, as before.
    let mut map_orig_to_matrix: BTreeMap<usize, (usize, bool)> = BTreeMap::new();
    for (matrix_row, &ordinal) in reference_rows.iter().enumerate() {
        map_orig_to_matrix.insert(ordinal, (matrix_row, true));
    }
    for (matrix_row, &ordinal) in target_rows.iter().enumerate() {
        map_orig_to_matrix.insert(ordinal, (matrix_row, false));
    }

    let wage_series = df
        .column(&req.decomposition_params.outcome_variable)
        .map_err(|e| e.to_string())?;
    let wage_array = wage_series.f64().map_err(|e| e.to_string())?;

    let feature_names_ref = &feature_names;

    for m in &merged {
        let row_idx = m.row_idx;
        if let Some((matrix_idx, is_group_a)) = map_orig_to_matrix.get(&row_idx) {
            let matrix_idx = *matrix_idx;
            let is_group_a = *is_group_a;

            // Get features (updated with overrides)
            let features = if is_group_a {
                x_a.row(matrix_idx).transpose()
            } else {
                x_b.row(matrix_idx).transpose()
            };

            // Calculate Fair Wage (Point Estimate)
            let fair_wage = (&features.transpose() * &beta_fair)[(0, 0)];

            let extrapolated = interval_model.is_extrapolated(&features);
            let (lower, upper) = calculate_interval(features, fair_wage);

            let current_wage = wage_array.get(row_idx).unwrap_or(0.0);

            // New Wage = Current (from CSV) + Adjustment (Delta)
            // Note: If Predictor Overrides changed the CSV data, current_wage might be weird?
            // No, wage column was NOT modified by overrides (only predictors).
            // But if user meant "wage override" via adjustment, we add it.
            // `m.value` is the SUM of every inbound delta naming this row (see the collapse), so
            // this is the one new wage the per-row verdict and the aggregates both read.
            let new_wage = current_wage + m.value;

            // Defensibility Logic
            let is_defensible = new_wage >= (lower - DEFENSIBLE_TOLERANCE);

            let msg = if is_defensible {
                Some("Wage is within or above the calculated fair range.".to_string())
            } else {
                Some(format!(
                    "Wage is {:.2} below the defensible lower bound ({:.2}).",
                    lower - new_wage,
                    lower
                ))
            };

            // Reconstruct contributions
            let mut contribs = Vec::new();
            let matrix = if is_group_a { &x_a } else { &x_b };
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

            results.push(Adjustment {
                index: row_idx,
                row_key: row_keys.key_at(row_idx),
                adjustment: m.value,
                current_wage,
                new_wage,
                fair_wage,
                fair_wage_lower_bound: Some(lower),
                fair_wage_upper_bound: Some(upper),
                // One entry per feature column (filled by the loop above), NOT empty. This is the
                // dominant term in the serialized payload size: rows x features contributions.
                contributions: contribs,
                is_defensible: Some(is_defensible),
                defensibility_message: msg,
                extrapolated,
                source: if is_group_a {
                    RowSource::Reference
                } else {
                    RowSource::Compared
                },
                range_position: support::range_position(new_wage, lower, upper),
                range_position_before: support::range_position(current_wage, lower, upper),
            });
        } else {
            // 0118-MERIDIAN S3: addressed to an excluded or unknown row. The app replays
            // persisted ledger rows on project load, so this is not an error, but it is never
            // silent: the count rides on the result.
            adjustments_on_excluded_rows += 1;
        }
    }

    // D15 (0017-P1): row index -> position in `results`, built once. `or_insert` keeps first-wins;
    // since the collapse above `results` holds at most one entry per row index.
    let mut result_pos_by_index: BTreeMap<usize, usize> = BTreeMap::new();
    for (pos, adj) in results.iter().enumerate() {
        result_pos_by_index.entry(adj.index).or_insert(pos);
    }

    // Money by group, in request order (D14).
    let mut total_cost = 0.0;
    let mut cost_target = 0.0;
    let mut cost_reference = 0.0;
    for adj in &results {
        total_cost += adj.adjustment;
        match adj.source {
            RowSource::Compared => cost_target += adj.adjustment,
            RowSource::Reference => cost_reference += adj.adjustment,
        }
    }

    // One pass over every ANALYSED row, in ascending ordinal (D14): group means before and after,
    // the compared group's shortfall to the line, where each compared employee stands against
    // their range before and after, and the per-row pay the gap weights and the group test read.
    // A row the schedule does not name counts at adjustment 0, so a partial schedule cannot hide
    // the people it left out (0122-MERIDIAN F-08).
    let mut sum_a = 0.0;
    let mut count_a = 0.0;
    let mut sum_b = 0.0;
    let mut count_b = 0.0;
    let mut new_sum_a = 0.0;
    let mut new_sum_b = 0.0;
    let mut residual_sum_b = 0.0;
    let mut need_target = 0.0;
    let mut pay_a = vec![0.0; reference_rows.len()];
    let mut pay_b = vec![0.0; target_rows.len()];
    let mut y_a_after = y_a.clone();
    let mut y_b_after = target_group_matrices.y.clone();
    let (mut below, mut inside, mut above) = (0usize, 0usize, 0usize);
    let (mut below_before, mut inside_before, mut above_before) = (0usize, 0usize, 0usize);
    let mut newly_above = 0usize;

    for (idx, (matrix_idx, is_group_a)) in &map_orig_to_matrix {
        let Some(v) = wage_array.get(*idx) else {
            continue;
        };
        let (adjustment, adjusted_val) = match result_pos_by_index.get(idx) {
            Some(&pos) => (results[pos].adjustment, results[pos].new_wage),
            None => (0.0, v),
        };
        if *is_group_a {
            sum_a += v;
            new_sum_a += adjusted_val;
            count_a += 1.0;
            pay_a[*matrix_idx] = adjustment;
            y_a_after[*matrix_idx] += adjustment;
        } else {
            sum_b += v;
            new_sum_b += adjusted_val;
            count_b += 1.0;
            pay_b[*matrix_idx] = adjustment;
            y_b_after[*matrix_idx] += adjustment;

            let features = x_b.row(*matrix_idx).transpose();
            let fair = (&features.transpose() * &beta_fair)[(0, 0)];
            residual_sum_b += v - fair;
            if fair - v > 1e-6 {
                need_target += fair - v;
            }
            let (lower, upper) = calculate_interval(features, fair);
            let before = support::range_position(v, lower, upper);
            let after = support::range_position(adjusted_val, lower, upper);
            match before {
                RangePosition::Below => below_before += 1,
                RangePosition::Inside => inside_before += 1,
                RangePosition::Above => above_before += 1,
            }
            match after {
                RangePosition::Below => below += 1,
                RangePosition::Inside => inside += 1,
                RangePosition::Above => {
                    above += 1;
                    if before != RangePosition::Above {
                        newly_above += 1;
                    }
                }
            }
        }
    }

    let mean_a = if count_a > 0.0 { sum_a / count_a } else { 0.0 };
    let mean_b = if count_b > 0.0 { sum_b / count_b } else { 0.0 };
    // Compared minus reference, negative while the compared group is underpaid: the sign
    // `optimize` and the decomposition use (0122-MERIDIAN T7). It used to be reference minus
    // compared.
    let original_gap = mean_b - mean_a;

    let new_mean_a = if count_a > 0.0 {
        new_sum_a / count_a
    } else {
        0.0
    };
    let new_mean_b = if count_b > 0.0 {
        new_sum_b / count_b
    } else {
        0.0
    };
    let new_gap = new_mean_b - new_mean_a;

    // Unexplained gap: the compared group's mean (pay - fair pay) on the baseline line, and the
    // same after the schedule with the line refitted on the schedule's wages (T5).
    let original_unexplained_gap = if count_b > 0.0 {
        residual_sum_b / count_b
    } else {
        0.0
    };
    let new_unexplained_gap = if count_b > 0.0 {
        original_unexplained_gap + weights.shift(&pay_b, &pay_a)
    } else {
        0.0
    };

    let group_test = support::pooled_group_test(&x_a, &y_a_after, &x_b, &y_b_after, confidence);

    // Prepare Coefficients
    let mut model_coefficients = Vec::new();
    for (i, name) in feature_names.iter().enumerate() {
        if i < beta_fair.len() {
            model_coefficients.push(Contribution {
                name: name.clone(),
                value: beta_fair[i],
            });
        }
    }

    Ok(OptimizationResult {
        adjustments: results,
        total_cost: nz(total_cost),
        original_gap: nz(original_gap),
        new_gap: nz(new_gap),
        original_unexplained_gap: nz(original_unexplained_gap),
        new_unexplained_gap: nz(new_unexplained_gap),
        required_budget: nz(need_target),
        target_line: RangeTarget::Midpoint,
        cost_target: nz(cost_target),
        cost_reference: nz(cost_reference),
        need_target: nz(need_target),
        need_reference: None,
        best_reachable_gap: None,
        target_gap_reachable: None,
        shortfall_to_target: None,
        target_budget: None,
        budget_binding: None,
        unfunded_amount: None,
        unfunded_count: None,
        threshold_excluded_count: None,
        closure: None,
        overshoot_mean: None,
        position_counts: Some(PositionCounts {
            below,
            inside,
            above,
            below_before,
            inside_before,
            above_before,
            newly_above,
        }),
        group_test,
        model_coefficients,
        row_key_space: crate::row_key::ROW_KEY_SPACE.to_string(),
        row_key_source: row_keys.source(),
        row_key_column: row_keys.column(),
        unresolved_row_keys: Some(unresolved_row_keys),
        analysed_reference_count: reference_rows.len(),
        analysed_target_count: target_rows.len(),
        excluded_rows: excluded_with_keys(&builder_excluded_rows, Some(&row_keys)),
        adjustments_on_excluded_rows,
        interval: interval_model.basis.clone(),
        support: support_block,
        warnings,
    })
}
