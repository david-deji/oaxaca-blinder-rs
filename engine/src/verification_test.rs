#[cfg(test)]
mod tests {
    use crate::analysis::optimize_inner;
    use crate::types::{AllocationStrategy, OptimizationRequest, OptimizationTarget};
    use polars::prelude::*;

    #[test]
    fn test_log_linear_adjustments_are_non_uniform() {
        // Create mock data:
        // Group A (Ref): High Wages, correlated with X
        // Group B (Target): Lower Wages, SAME X, but paid less.

        // Wages:
        // A: Exp(10 + 0.5*X) -> High Pay
        // B: Exp(9 + 0.5*X)  -> Low Pay

        // X: Education Level
        // Wages:
        // A: Exp(10 + 0.5*X) -> High Pay
        // B: Exp(9 + 0.5*X)  -> Low Pay
        // Gap is massive.

        let w_a_1 = (10.0 + 0.5 * 1.0f64).exp(); // ~36315
        let w_a_2 = (10.0 + 0.5 * 2.0f64).exp(); // ~59874
        let w_a_3 = (10.0 + 0.5 * 3.0f64).exp(); // ~98715

        let w_b_1 = (9.0 + 0.5 * 1.0f64).exp(); // ~13359
        let w_b_2 = (9.0 + 0.5 * 2.0f64).exp(); // ~22026
        let w_b_3 = (9.0 + 0.5 * 3.0f64).exp(); // ~36315

        // Create DataFrame using compatible Polars API
        let df = DataFrame::new(vec![
            Column::new("id".into(), &[1, 2, 3, 4, 5, 6]),
            Column::new("group".into(), &["A", "A", "A", "B", "B", "B"]),
            Column::new("education".into(), &[1.0, 2.0, 3.0, 1.0, 2.0, 3.0]),
            Column::new("wage".into(), &[w_a_1, w_a_2, w_a_3, w_b_1, w_b_2, w_b_3]),
        ])
        .unwrap();

        // Write to CSV buffer
        let mut buffer = Vec::new();
        CsvWriter::new(&mut buffer).finish(&mut df.clone()).unwrap();
        let csv_data = buffer;

        let req = OptimizationRequest {
            csv_data,
            group_variable: "group".to_string(),
            outcome_variable: "wage".to_string(),
            predictors: vec!["education".to_string()],
            categorical_predictors: None,
            reference_group: "A".to_string(),
            budget: 1_000_000.0,
            target: Some(OptimizationTarget::Reference),
            strategy: Some(AllocationStrategy::Greedy),
            min_gap_pct: None,
            forensic_mode: Some(false),
            adjust_both_groups: None,
            target_gap: None,
            confidence_level: None,
            range_target: None,
        };

        let result = optimize_inner(req).unwrap();

        println!(
            "Adjustments: {:?}",
            result
                .adjustments
                .iter()
                .map(|a| a.adjustment)
                .collect::<Vec<_>>()
        );

        // Assertions
        assert_eq!(result.adjustments.len(), 3);

        let adj_1 = result.adjustments[0].adjustment;
        let adj_3 = result.adjustments[2].adjustment;

        // Check 1: Non-Zero Adjustments
        assert!(adj_1 > 100.0, "Adjustment 1 should be significant");

        // Check 2: Non-Uniformity
        // Person 3 (High Education) should get a LARGER adjustment than Person 1 (Low Education)
        // because the gap is a percentage (Log Scale).
        // Diff = Exp(10.5) - Exp(9.5) vs Exp(11.5) - Exp(10.5)
        // Delta is bigger for higher X.

        assert!(
            (adj_3 - adj_1).abs() > 100.0,
            "Adjustments should differ significantly! Found {} vs {}",
            adj_1,
            adj_3
        );

        println!("Test Passed: Non-Uniformity Confirmed.");

        // Check 3: Model Coefficients
        assert!(
            !result.model_coefficients.is_empty(),
            "Model coefficients should not be empty"
        );
        let education_coef = result
            .model_coefficients
            .iter()
            .find(|c| c.name == "education");
        assert!(
            education_coef.is_some(),
            "Education coefficient should be present"
        );
        println!(
            "Coefficient for education: {}",
            education_coef.unwrap().value
        );
    }

    fn create_mock_data() -> (Vec<u8>, DataFrame) {
        // Simple mock:
        // 1000 employees. Groups A and B.
        // A paid more than B for same features.
        // We want gaps.
        let mut ids = Vec::new();
        let mut groups = Vec::new();
        let mut edu = Vec::new();
        let mut exp = Vec::new();
        let mut wage = Vec::new();
        let mut depts = Vec::new();

        for i in 0..1000 {
            // NOT `i` — a column whose values equal the row ordinal is a row counter, and
            // `row_key::is_row_counter` refuses it as identity AND excludes it from the content
            // digest (0017-P4), because renumbering it on an insert would move every key. A real
            // employee number is unique but not positional, so the fixture models one: an offset
            // base keeps the values readable (row 555 -> 500555) while `value != index` holds.
            ids.push(500_000 + i as u64);
            let is_a = i < 500; // 50% Group A
            groups.push(if is_a { "Male" } else { "Female" }); // StandardRef=Male

            let ed = 12.0 + ((i % 10) as f64) * 0.5; // 12-17
            let ex = (i % 30) as f64;
            edu.push(ed);
            exp.push(ex);

            depts.push(if i % 2 == 0 { "Sales" } else { "Eng" });

            // Wages: Base + Coef*Ed + Coef*Ex + Noise
            // A: Base 20000 + 2000*Ed + 500*Ex
            // B: Base 15000 + 2000*Ed + 500*Ex
            // Explicit Gap of 5000.
            let fair_base = 20000.0 + 2000.0 * ed + 500.0 * ex;
            let actual_base = if is_a { fair_base } else { fair_base - 5000.0 };

            // Add deterministic noise
            let noise = ((i % 100) as f64) * 10.0 - 500.0;
            wage.push(actual_base + noise);
        }

        let df = DataFrame::new(vec![
            Column::new("id".into(), &ids),
            Column::new("gender".into(), &groups),
            Column::new("education".into(), &edu),
            Column::new("experience".into(), &exp),
            Column::new("wage".into(), &wage),
            Column::new("department".into(), &depts),
        ])
        .unwrap();

        let mut buffer = Vec::new();
        CsvWriter::new(&mut buffer).finish(&mut df.clone()).unwrap();
        (buffer, df)
    }

    use crate::types::RangeTarget;

    /// Every PAID row ends within a cent of its own lower bound, and there is at least one.
    fn paid_rows_sit_on_the_lower_bound(result: &crate::types::OptimizationResult) -> bool {
        let paid: Vec<_> = result
            .adjustments
            .iter()
            .filter(|a| a.adjustment > 0.0)
            .collect();
        !paid.is_empty()
            && paid
                .iter()
                .all(|a| (a.new_wage - a.fair_wage_lower_bound.unwrap()).abs() <= 0.01)
    }

    fn request_for(range: RangeTarget, budget: f64) -> OptimizationRequest {
        let (csv_data, _) = create_mock_data();
        OptimizationRequest {
            csv_data,
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Male".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: Some(vec!["department".to_string()]),
            strategy: Some(AllocationStrategy::Greedy),
            budget,
            target_gap: None,
            target: None,
            min_gap_pct: Some(0.0),
            adjust_both_groups: Some(false),
            forensic_mode: Some(false),
            confidence_level: Some(0.95),
            range_target: Some(range),
        }
    }

    #[test]
    fn test_lower_bound_optimization() {
        let (csv_data, _) = create_mock_data();

        // Use optimize_inner directly
        use crate::analysis::optimize_inner;

        let req = OptimizationRequest {
            csv_data: csv_data.clone(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Male".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: Some(vec!["department".to_string()]),
            strategy: Some(AllocationStrategy::Greedy),
            budget: 5_000_000.0, // Enough to fix all gaps
            target_gap: None,
            target: None,
            min_gap_pct: Some(0.0),
            adjust_both_groups: Some(false),
            forensic_mode: Some(false),
            confidence_level: Some(0.95),
            range_target: Some(RangeTarget::LowerBound),
        };

        let result = optimize_inner(req).expect("Optimization failed");

        // 0122-MERIDIAN T14 / V12 (TRUST-9): this test used to print "FAIL" and pass. Every row that
        // is paid, with a budget that funds all of them, lands on its OWN lower bound.
        assert!(
            !result.adjustments.is_empty(),
            "the roster has employees below their lower bound"
        );
        assert!(
            paid_rows_sit_on_the_lower_bound(&result),
            "a funded row is not paid exactly to its lower bound"
        );

        // Mutate and confirm: pay the same roster to the MIDPOINT and the same assertion fails, so
        // it can tell the two lines apart (a `new_wage >= lower - tolerance` check cannot: the
        // midpoint is above the lower bound too).
        let mut midpoint = request_for(RangeTarget::Midpoint, 5_000_000.0);
        midpoint.min_gap_pct = Some(0.0);
        let midpoint_result = optimize_inner(midpoint).expect("midpoint run");
        assert!(
            !paid_rows_sit_on_the_lower_bound(&midpoint_result),
            "the lower-bound assertion cannot tell a midpoint payment from a lower-bound one"
        );
    }

    #[test]
    fn test_auto_budget_lower_bound() {
        let (csv_data, _) = create_mock_data();
        use crate::analysis::optimize_inner;

        // Auto Budget (0.0) with LowerBound target
        let req = OptimizationRequest {
            csv_data: csv_data.clone(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Male".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: Some(vec!["department".to_string()]),
            strategy: Some(AllocationStrategy::Greedy),
            budget: 0.0, // Auto Budget!
            target_gap: None,
            target: None,
            min_gap_pct: Some(0.0),
            adjust_both_groups: Some(false),
            forensic_mode: Some(false),
            confidence_level: Some(0.95),
            range_target: Some(RangeTarget::LowerBound),
        };

        let result = optimize_inner(req).expect("Optimization failed");

        assert!(!result.adjustments.is_empty());
        assert!(
            paid_rows_sit_on_the_lower_bound(&result),
            "auto budget: a funded row is not paid exactly to its lower bound"
        );
        // The auto budget is the whole need to the LOWER bound, and it is spent.
        let paid: f64 = result.adjustments.iter().map(|a| a.adjustment).sum();
        assert!((paid - result.total_cost).abs() < 1e-6);
        let to_lower: f64 = result
            .adjustments
            .iter()
            .filter(|a| a.adjustment > 0.0)
            .map(|a| a.fair_wage_lower_bound.unwrap() - a.current_wage)
            .sum();
        assert!(
            (result.total_cost - to_lower).abs() < 1.0,
            "spent {} but the shortfall to the lower bounds sums to {to_lower}",
            result.total_cost
        );
    }

    #[test]
    fn test_defensibility_override() {
        use crate::defensibility::check_defensibility_inner;
        use crate::types::{ProposedAdjustment, VerificationRequest};
        use std::collections::HashMap;

        let (csv_data, _df) = create_mock_data();

        // Find a Group B person (Low Education, Low Experience) who is underpaid.
        // In mock data: B is offset by -5000.
        // Index 5 (Group B): Ed=14.5, Exp=5.
        // Wage ~ 15000 + 2000*14.5 + 500*5 = 15000 + 29000 + 2500 = 46500.
        // Actual in mock is Fair - 5000 + Noise.

        // Let's pick index 555 (Group B, since > 500).
        let target_idx = 555;

        // Baseline Check (No Overrides)
        let req_baseline = VerificationRequest {
            decomposition_params: crate::types::DecompositionRequest {
                csv_data: csv_data.clone(),
                outcome_variable: "wage".to_string(),
                group_variable: "gender".to_string(),
                reference_group: "Male".to_string(),
                predictors: vec!["education".to_string(), "experience".to_string()],
                categorical_predictors: Some(vec!["department".to_string()]),
                three_fold: None,
                quantile: None,
                reference_coefficients: Some("Pooled".to_string()),
                bootstrap_reps: None,
            },
            adjustments: vec![ProposedAdjustment {
                index: target_idx,
                row_key: None,
                value: 0.0,
                predictor_overrides: None,
            }],
            confidence_level: None,
        };

        let res_baseline = check_defensibility_inner(req_baseline).expect("Baseline check failed");
        let adj_baseline = &res_baseline.adjustments[0];

        println!(
            "Baseline: Current={}, Fair={}, Lower={:?}, Defensible={:?}",
            adj_baseline.current_wage,
            adj_baseline.fair_wage,
            adj_baseline.fair_wage_lower_bound,
            adj_baseline.is_defensible
        );

        // The mock data has a gap of 5000.
        // Fair Wage should be ~ Current + 5000.
        // Lower bound should be Fair - Margin (margin usually < 5000 for high confidence).
        // So likely NOT defensible.

        // Now Apply Override: LOWER the education significantly.
        // Say we claim this person actually has Education = 10 (instead of ~15).
        // Fair Wage (Male model) is 20000 + 2000*Ed + 500*Ex.
        // Reducing Ed by 5 units -> Reduces Fair Wage by 10,000!
        // This should make the Current Wage appear HIGHER than the new Fair Wage (or at least valid).

        let mut overrides = HashMap::new();
        overrides.insert("education".to_string(), "10.0".to_string());

        let req_override = VerificationRequest {
            decomposition_params: crate::types::DecompositionRequest {
                csv_data: csv_data.clone(),
                outcome_variable: "wage".to_string(),
                group_variable: "gender".to_string(),
                reference_group: "Male".to_string(),
                predictors: vec!["education".to_string(), "experience".to_string()],
                categorical_predictors: Some(vec!["department".to_string()]),
                three_fold: None,
                quantile: None,
                reference_coefficients: Some("Pooled".to_string()),
                bootstrap_reps: None,
            },
            adjustments: vec![ProposedAdjustment {
                index: target_idx,
                row_key: None,
                value: 0.0,
                predictor_overrides: Some(overrides),
            }],
            confidence_level: None,
        };

        let res_override = check_defensibility_inner(req_override).expect("Override check failed");
        let adj_override = &res_override.adjustments[0];

        println!(
            "Override: Current={}, Fair={}, Lower={:?}, Defensible={:?}",
            adj_override.current_wage,
            adj_override.fair_wage,
            adj_override.fair_wage_lower_bound,
            adj_override.is_defensible
        );

        assert!(
            adj_override.fair_wage < adj_baseline.fair_wage - 5000.0,
            "Fair wage should drop significantly"
        );
        // If Fair Wage dropped by 10k, and gap was 5k, now Current Wage should be > Fair Wage (Overpaid).
        // So Defensible should be TRUE.

        if let Some(def) = adj_override.is_defensible {
            assert!(def, "Should be defensible with lower education override");
        } else {
            panic!("Defensibility check returned None");
        }
    }

    // --- 0017-MERIDIAN P4 row keys, 0118-MERIDIAN row ordinals -------------------------------
    //
    // Six reference (Male) rows on wage = 30000 + 2000*education exactly, and six target
    // (Female) rows on the same line minus 5000. Raw ordinals 0-5 are Male, 6-11 are Female and
    // every Female is underpaid by 5000. A blank cell drops that employee from the analysis; it
    // must not move any OTHER employee's figures or key (0118 S6: these tests used to pin the
    // shifted pairing and the withheld keys as expected output).
    fn row_key_alignment_csv(blank_first_female_education: bool) -> Vec<u8> {
        row_key_alignment_csv_with(blank_first_female_education, false)
    }

    fn row_key_alignment_csv_with(
        blank_first_female_education: bool,
        blank_first_male_education: bool,
    ) -> Vec<u8> {
        let f000_education = if blank_first_female_education {
            ""
        } else {
            "10"
        };
        let m000_education = if blank_first_male_education { "" } else { "10" };
        format!(
            "employee_id,gender,education,wage\n\
             M-000,Male,{},50000\n\
             M-001,Male,12,54000\n\
             M-002,Male,14,58000\n\
             M-003,Male,16,62000\n\
             M-004,Male,18,66000\n\
             M-005,Male,20,70000\n\
             F-000,Female,{},45000\n\
             F-001,Female,12,49000\n\
             F-002,Female,14,53000\n\
             F-003,Female,16,57000\n\
             F-004,Female,18,61000\n\
             F-005,Female,20,65000\n",
            m000_education, f000_education
        )
        .into_bytes()
    }

    fn row_key_alignment_request(csv_data: Vec<u8>) -> OptimizationRequest {
        OptimizationRequest {
            csv_data,
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Male".to_string(),
            predictors: vec!["education".to_string()],
            categorical_predictors: None,
            strategy: Some(AllocationStrategy::Greedy),
            budget: 1_000_000.0,
            target_gap: None,
            target: Some(OptimizationTarget::Reference),
            min_gap_pct: None,
            forensic_mode: Some(false),
            adjust_both_groups: Some(false),
            confidence_level: Some(0.95),
            range_target: None,
        }
    }

    /// Control: with no nulls, the raw frame and the null-dropped model frame agree, so every
    /// emitted row carries the key of the employee whose dollars it holds.
    #[test]
    fn row_keys_are_emitted_when_the_model_frame_matches_the_raw_frame() {
        let result = optimize_inner(row_key_alignment_request(row_key_alignment_csv(false)))
            .expect("optimization failed");

        assert_eq!(
            result.adjustments.len(),
            6,
            "all six target rows survive cleaning, so all six are emitted"
        );

        for adj in &result.adjustments {
            // Raw ordinal 6 is F-000, 7 is F-001, ... — the key must name that same employee.
            let expected = format!("c:F-{:03}", adj.index - 6);
            assert_eq!(
                adj.row_key.as_deref(),
                Some(expected.as_str()),
                "index {} must carry its own employee's key",
                adj.index
            );
            // And the key must name the employee whose fair wage this row actually holds:
            // education = 10 + 2*(index - 6), fair = 30000 + 2000*education.
            let expected_fair = 30000.0 + 2000.0 * (10.0 + 2.0 * ((adj.index - 6) as f64));
            assert!(
                (adj.fair_wage - expected_fair).abs() < 1.0,
                "index {} fair_wage {} should be {}",
                adj.index,
                adj.fair_wage,
                expected_fair
            );
        }
    }

    /// F-000's education is blank, so the model drops F-000 and ONLY F-000. Every other target
    /// employee keeps their own ordinal, their own key and their own fair wage, and the last
    /// target employee (F-005, ordinal 11) is paid. Every expected number below is read off the
    /// CSV cells in `row_key_alignment_csv_with`, never off the engine: employee `n` (0-based
    /// among Females) has education `10 + 2n`, current wage `45000 + 4000n`, and the Male line
    /// gives a fair wage of `30000 + 2000*education = 50000 + 4000n`.
    #[test]
    fn a_blank_target_cell_drops_that_employee_and_nobody_else() {
        let result = optimize_inner(row_key_alignment_request(row_key_alignment_csv(true)))
            .expect("optimization failed");

        let indices: Vec<usize> = result.adjustments.iter().map(|a| a.index).collect();
        assert_eq!(
            indices,
            vec![7, 8, 9, 10, 11],
            "F-000 (ordinal 6) is the only employee missing; F-005 (ordinal 11) is paid"
        );

        for adj in &result.adjustments {
            let n = adj.index - 6; // F-00n
            let education = 10.0 + 2.0 * n as f64;
            let expected_fair = 30000.0 + 2000.0 * education;
            let expected_current = 45000.0 + 4000.0 * n as f64;
            assert!(
                (adj.fair_wage - expected_fair).abs() < 0.01,
                "F-00{n} (index {}) fair_wage {} should be its own {}",
                adj.index,
                adj.fair_wage,
                expected_fair
            );
            assert!(
                (adj.current_wage - expected_current).abs() < 1e-9,
                "F-00{n} current_wage {} should be its own {}",
                adj.current_wage,
                expected_current
            );
            assert!(
                (adj.adjustment - 5000.0).abs() < 0.01,
                "F-00{n} is underpaid by exactly 5000, got {}",
                adj.adjustment
            );
            // The key names the employee whose dollars these are.
            assert_eq!(
                adj.row_key.as_deref(),
                Some(format!("c:F-{:03}", n).as_str()),
                "index {} must carry F-00{n}'s own key",
                adj.index
            );
        }

        // Five employees paid 5000 each.
        assert!(
            (result.total_cost - 25000.0).abs() < 0.05,
            "{}",
            result.total_cost
        );
        assert!((result.required_budget - 25000.0).abs() < 0.05);

        // The excluded employee is reported, by key and by the blank column.
        assert_eq!(result.analysed_target_count, 5);
        assert_eq!(result.analysed_reference_count, 6);
        assert_eq!(result.excluded_rows.len(), 1);
        let excluded = &result.excluded_rows[0];
        assert_eq!(excluded.index, 6);
        assert_eq!(excluded.row_key.as_deref(), Some("c:F-000"));
        assert_eq!(excluded.columns, vec!["education".to_string()]);
        assert_eq!(
            serde_json::to_value(&excluded.reasons).unwrap(),
            serde_json::json!(["numericPredictor"])
        );

        // The derivation rule is reported as before.
        assert_eq!(result.row_key_space, crate::row_key::ROW_KEY_SPACE);
    }

    /// Keys are never withheld: a blank cell anywhere leaves every analysed employee's key in
    /// place, and a run with no blank mints the same keys for the same employees. (Pre-0118 the
    /// engine emitted NO keys on a misaligned run; that gate is gone because there is no longer
    /// a misalignment to guard.)
    #[test]
    fn keys_are_emitted_on_every_run_with_or_without_a_blank() {
        let with_blank = optimize_inner(row_key_alignment_request(row_key_alignment_csv(true)))
            .expect("run with a blank failed");
        let without_blank = optimize_inner(row_key_alignment_request(row_key_alignment_csv(false)))
            .expect("run without a blank failed");

        assert!(with_blank.adjustments.iter().all(|a| a.row_key.is_some()));
        assert!(without_blank
            .adjustments
            .iter()
            .all(|a| a.row_key.is_some()));

        // The same employee has the same key whether or not another employee's cell is blank.
        for adj in &with_blank.adjustments {
            let twin = without_blank
                .adjustments
                .iter()
                .find(|b| b.index == adj.index)
                .expect("every analysed employee is in the complete run too");
            assert_eq!(adj.row_key, twin.row_key, "index {}", adj.index);
        }
    }

    /// A blank in a REFERENCE row drops that reference employee and nothing else. The target
    /// pairing, keys and fair wages are exactly those of the complete file (M-000 sits on the
    /// same line as the other five males, so the reference fit is unchanged), and the excluded
    /// list names the reference employee.
    #[test]
    fn a_reference_side_blank_leaves_the_target_pairing_untouched() {
        let result = optimize_inner(row_key_alignment_request(row_key_alignment_csv_with(
            false, true,
        )))
        .expect("optimization failed");

        assert_eq!(
            result.adjustments.len(),
            6,
            "every target row still survives cleaning"
        );

        for adj in &result.adjustments {
            let expected = format!("c:F-{:03}", adj.index - 6);
            assert_eq!(
                adj.row_key.as_deref(),
                Some(expected.as_str()),
                "index {} is correctly paired and must keep its key",
                adj.index
            );
            // M-000 dropping out does not move the reference regression: the remaining five
            // males still sit exactly on wage = 30000 + 2000*education.
            let expected_fair = 30000.0 + 2000.0 * (10.0 + 2.0 * ((adj.index - 6) as f64));
            assert!(
                (adj.fair_wage - expected_fair).abs() < 1.0,
                "index {} fair_wage {} should be {}",
                adj.index,
                adj.fair_wage,
                expected_fair
            );
        }

        assert_eq!(result.analysed_reference_count, 5);
        assert_eq!(result.analysed_target_count, 6);
        assert_eq!(result.excluded_rows.len(), 1);
        assert_eq!(result.excluded_rows[0].index, 0);
        assert_eq!(result.excluded_rows[0].row_key.as_deref(), Some("c:M-000"));
    }

    // --- check_defensibility: one row, at most one adjustment (defensibility.rs) -------------
    //
    // `create_mock_data` writes an `id` column of 0..999, which `RowKeyTable` accepts as the
    // employee-number column, so raw ordinal N carries key `c:N`. Ordinals 500-999 are Female
    // (the non-reference group the unexplained-gap aggregates are computed over), 500 of them.

    fn defensibility_params(csv_data: &[u8]) -> crate::types::DecompositionRequest {
        crate::types::DecompositionRequest {
            csv_data: csv_data.to_vec(),
            outcome_variable: "wage".to_string(),
            group_variable: "gender".to_string(),
            reference_group: "Male".to_string(),
            predictors: vec!["education".to_string(), "experience".to_string()],
            categorical_predictors: Some(vec!["department".to_string()]),
            three_fold: None,
            quantile: None,
            reference_coefficients: Some("Pooled".to_string()),
            bootstrap_reps: None,
        }
    }

    /// The defect. Two adjustments that look distinct on the wire — one addressed by `index`,
    /// one by a stale `index` plus the `row_key` of the same employee — resolve onto one
    /// ordinal. Before the collapse this emitted TWO `Adjustment` rows carrying an identical
    /// `index` AND an identical `row_key` (555 / `c:500555`), with `unresolved_row_keys: 0`, while
    /// `new_unexplained_gap` came back bit-identical to the one-adjustment run: the second
    /// adjustment's $5,000 was in the per-row array and in `total_cost`, and absent from the
    /// headline gap printed beside them. The duplicate `row_key` also tripped the browser
    /// client's own refusal condition, blocking adoption of the whole payload.
    #[test]
    fn duplicate_row_target_by_stale_index_and_row_key_collapses_to_one_row() {
        use crate::defensibility::check_defensibility_inner;
        use crate::types::{ProposedAdjustment, VerificationRequest};

        let (csv_data, _df) = create_mock_data();

        let res = check_defensibility_inner(VerificationRequest {
            decomposition_params: defensibility_params(&csv_data),
            adjustments: vec![
                ProposedAdjustment {
                    index: 555,
                    row_key: None,
                    value: 1000.0,
                    predictor_overrides: None,
                },
                ProposedAdjustment {
                    index: 999,
                    row_key: Some("c:500555".to_string()),
                    value: 5000.0,
                    predictor_overrides: None,
                },
            ],
            confidence_level: None,
        })
        .expect("a doubly-addressed row is collapsed, not rejected");

        assert_eq!(
            res.adjustments.len(),
            1,
            "one row must yield one verdict — two rows here share an index and a row_key, \
             which is the shape the browser client refuses outright"
        );
        let only = &res.adjustments[0];
        assert_eq!(only.index, 555);
        assert_eq!(only.row_key.as_deref(), Some("c:500555"));
        assert_eq!(res.unresolved_row_keys, Some(0));

        // Nothing is dropped: the emitted delta is the sum, and the wage it implies is the wage
        // the aggregates below are computed from.
        assert!(
            (only.adjustment - 6000.0).abs() < 1e-9,
            "got {}",
            only.adjustment
        );
        assert!(
            (only.new_wage - (only.current_wage + 6000.0)).abs() < 1e-9,
            "new_wage {} should be current {} + 6000",
            only.new_wage,
            only.current_wage
        );
        assert!(
            (res.total_cost - 6000.0).abs() < 1e-9,
            "got {}",
            res.total_cost
        );

        // The headline the per-row array is presented beside must agree with it: row 555 is
        // Female (non-reference), and there are 500 such rows, so 6000/500 = 12.0. Before the
        // collapse this was 2.0 — the first-wins $1,000 only. The gap rises as money is paid
        // (0122-MERIDIAN T7: compared minus the line, negative while underpaid).
        let moved = res.new_unexplained_gap - res.original_unexplained_gap;
        assert!(
            (moved - 12.0).abs() < 1e-6,
            "aggregate must reflect the same 6000 the per-row array reports: expected 12.0, \
             got {}",
            moved
        );
    }

    /// The same collapse on the legacy positional shape: one row named twice by `index`, no keys
    /// involved. Deltas sum, matching `verify_inner`, which already accumulates repeated deltas
    /// onto one row.
    #[test]
    fn duplicate_row_target_by_repeated_index_collapses_to_one_row() {
        use crate::defensibility::check_defensibility_inner;
        use crate::types::{ProposedAdjustment, VerificationRequest};

        let (csv_data, _df) = create_mock_data();

        let res = check_defensibility_inner(VerificationRequest {
            decomposition_params: defensibility_params(&csv_data),
            adjustments: vec![
                ProposedAdjustment {
                    index: 555,
                    row_key: None,
                    value: 1000.0,
                    predictor_overrides: None,
                },
                ProposedAdjustment {
                    index: 555,
                    row_key: None,
                    value: 5000.0,
                    predictor_overrides: None,
                },
            ],
            confidence_level: None,
        })
        .expect("the same index twice is collapsed, not rejected");

        assert_eq!(res.adjustments.len(), 1);
        assert_eq!(res.adjustments[0].index, 555);
        assert!((res.adjustments[0].adjustment - 6000.0).abs() < 1e-9);
        let moved = res.new_unexplained_gap - res.original_unexplained_gap;
        assert!((moved - 12.0).abs() < 1e-6, "got {}", moved);
    }

    /// Predictor overrides on a collapsed row merge per key rather than the later map replacing
    /// the earlier one wholesale. Both callers' stated facts survive; only a genuine collision
    /// is arbitrated (last inbound wins).
    #[test]
    fn overrides_on_a_collapsed_row_merge_per_key() {
        use crate::defensibility::check_defensibility_inner;
        use crate::types::{ProposedAdjustment, VerificationRequest};
        use std::collections::HashMap;

        let (csv_data, _df) = create_mock_data();

        let mut first = HashMap::new();
        first.insert("education".to_string(), "10.0".to_string());
        let mut second = HashMap::new();
        second.insert("experience".to_string(), "0.0".to_string());

        let merged = check_defensibility_inner(VerificationRequest {
            decomposition_params: defensibility_params(&csv_data),
            adjustments: vec![
                ProposedAdjustment {
                    index: 555,
                    row_key: None,
                    value: 0.0,
                    predictor_overrides: Some(first.clone()),
                },
                ProposedAdjustment {
                    index: 555,
                    row_key: None,
                    value: 0.0,
                    predictor_overrides: Some(second.clone()),
                },
            ],
            confidence_level: None,
        })
        .expect("collapsed override run failed");

        // Reference: one adjustment carrying BOTH keys at once.
        let mut both = first;
        both.extend(second);
        let reference = check_defensibility_inner(VerificationRequest {
            decomposition_params: defensibility_params(&csv_data),
            adjustments: vec![ProposedAdjustment {
                index: 555,
                row_key: None,
                value: 0.0,
                predictor_overrides: Some(both),
            }],
            confidence_level: None,
        })
        .expect("reference override run failed");

        assert_eq!(merged.adjustments.len(), 1);
        assert!(
            (merged.adjustments[0].fair_wage - reference.adjustments[0].fair_wage).abs() < 1e-9,
            "merged overrides {} must equal the single-map equivalent {} — a wholesale replace \
             would drop the education override and land elsewhere",
            merged.adjustments[0].fair_wage,
            reference.adjustments[0].fair_wage
        );
    }

    /// The guard must not over-trigger on mixed addressing across DIFFERENT rows — the shape
    /// every current caller sends. Both deltas must reach the aggregate: adjusting two Female
    /// rows by 1000 and 5000 out of 500 non-reference rows moves the unexplained gap by exactly
    /// 6000/500 = 12.0. A first-wins drop would show 2.0 here.
    #[test]
    fn distinct_rows_addressed_by_index_and_row_key_both_reach_the_aggregate() {
        use crate::defensibility::check_defensibility_inner;
        use crate::types::{ProposedAdjustment, VerificationRequest};

        let (csv_data, _df) = create_mock_data();

        let res = check_defensibility_inner(VerificationRequest {
            decomposition_params: defensibility_params(&csv_data),
            adjustments: vec![
                ProposedAdjustment {
                    index: 555,
                    row_key: None,
                    value: 1000.0,
                    predictor_overrides: None,
                },
                // Stale index, resolved by key to a DIFFERENT row than the one above.
                ProposedAdjustment {
                    index: 3,
                    row_key: Some("c:500777".to_string()),
                    value: 5000.0,
                    predictor_overrides: None,
                },
            ],
            confidence_level: None,
        })
        .expect("two adjustments on two distinct rows must verify");

        assert_eq!(res.adjustments.len(), 2);
        let addressed: Vec<usize> = res.adjustments.iter().map(|a| a.index).collect();
        assert_eq!(
            addressed,
            vec![555, 777],
            "the keyed adjustment must land on its key's row, not its stale index"
        );
        assert_eq!(res.unresolved_row_keys, Some(0));

        let moved = res.new_unexplained_gap - res.original_unexplained_gap;
        assert!(
            (moved - 12.0).abs() < 1e-6,
            "both deltas must reach the headline gap: expected 6000/500 = 12.0, got {}",
            moved
        );
    }
}
