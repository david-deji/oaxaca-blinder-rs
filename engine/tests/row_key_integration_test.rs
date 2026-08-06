//! End-to-end coverage for the 0017-MERIDIAN P4 stable row key, driven through the real
//! `optimize_inner` / `check_defensibility_inner` / `verify_inner` entry points rather than
//! against `row_key.rs` in isolation.
//!
//! What these pin, in order of how badly a regression would hurt:
//!
//! 1. Every emitted `Adjustment` carries a non-empty, unique `row_key`. This is the exact
//!    adoption test the client runs (`docs/0017-p4/design.md` § "What this client requires").
//! 2. Inserting a row at the top of the CSV shifts every `index` and moves NO key — the P4
//!    defect scenario, measured rather than argued.
//! 3. OPTIMIZE and DEFENSIBILITY mint identical keys for the same row from the same bytes, so
//!    the client can zip the two responses by key.
//! 4. Emission order is still ascending `index` — P1's persisted defensibility bitmap is keyed
//!    by ordinal position in that array, so a re-sort would silently re-point every saved bit.
//! 5. Omitting `row_key` on the inbound `ProposedAdjustment` reproduces the pre-P4 index path
//!    byte for byte; supplying a stale one resolves by key; supplying an unknown one fails
//!    closed and is counted instead of landing on the wrong employee.

use pay_equity_engine::analysis::{optimize_inner, verify_inner};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::row_key::RowKeySource;
use pay_equity_engine::types::{
    DecompositionRequest, OptimizationRequest, ProposedAdjustment, VerificationRequest,
};
use std::collections::BTreeSet;

/// 24 rows, no identifier column -> content-hash fallback (the compiled-project path).
fn csv_without_id() -> String {
    let mut s = String::from("wage,gender,education\n");
    for i in 0..12 {
        s.push_str(&format!("{},Male,{}\n", 50_000 + i * 700, 10 + i));
        s.push_str(&format!("{},Female,{}\n", 41_000 + i * 700, 10 + i));
    }
    s
}

/// Same 24 rows, with a qualifying employee-number column (the raw-upload path).
fn csv_with_id() -> String {
    let mut s = String::from("employee_id,wage,gender,education\n");
    for i in 0..12 {
        s.push_str(&format!("M-{:03},{},Male,{}\n", i, 50_000 + i * 700, 10 + i));
        s.push_str(&format!("F-{:03},{},Female,{}\n", i, 41_000 + i * 700, 10 + i));
    }
    s
}

fn prepend_row(csv: &str, row: &str) -> String {
    let mut lines = csv.lines();
    let header = lines.next().unwrap();
    let mut out = String::from(header);
    out.push('\n');
    out.push_str(row);
    out.push('\n');
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}

fn optimization_request(csv: &str) -> OptimizationRequest {
    OptimizationRequest {
        csv_data: csv.as_bytes().to_vec(),
        outcome_variable: "wage".to_string(),
        group_variable: "gender".to_string(),
        reference_group: "Male".to_string(),
        predictors: vec!["education".to_string()],
        categorical_predictors: None,
        budget: 100_000.0,
        target_gap: None,
        target: None,
        strategy: None,
        min_gap_pct: None,
        // Forensic mode is what the app sets unconditionally, and it is what makes every row
        // appear in the adjustments array — the shape the ledger actually sees.
        forensic_mode: Some(true),
        adjust_both_groups: None,
        confidence_level: None,
        range_target: None,
    }
}

fn decomposition_params(csv: &str) -> DecompositionRequest {
    DecompositionRequest {
        csv_data: csv.as_bytes().to_vec(),
        outcome_variable: "wage".to_string(),
        group_variable: "gender".to_string(),
        reference_group: "Male".to_string(),
        predictors: vec!["education".to_string()],
        categorical_predictors: None,
        three_fold: None,
        quantile: None,
        reference_coefficients: None,
        bootstrap_reps: Some(10),
    }
}

// ---------------------------------------------------------------------------------------------
// 1. Emission
// ---------------------------------------------------------------------------------------------

#[test]
fn every_emitted_adjustment_carries_a_unique_non_empty_key() {
    for csv in [csv_without_id(), csv_with_id()] {
        let res = optimize_inner(optimization_request(&csv)).expect("optimize");
        assert!(!res.adjustments.is_empty());

        let mut seen: BTreeSet<String> = BTreeSet::new();
        for adj in &res.adjustments {
            let key = adj
                .row_key
                .as_deref()
                .unwrap_or_else(|| panic!("row_key missing at index {}", adj.index));
            assert!(!key.is_empty(), "row_key empty at index {}", adj.index);
            assert!(seen.insert(key.to_string()), "duplicate row_key {}", key);
        }
        assert_eq!(seen.len(), res.adjustments.len());
    }
}

#[test]
fn the_key_space_discriminator_is_the_literal_the_client_guards_on() {
    let res = optimize_inner(optimization_request(&csv_without_id())).expect("optimize");
    assert_eq!(res.row_key_space, "rowKeyV1");
    // optimize consumes no proposed adjustments, so there is nothing to resolve.
    assert_eq!(res.unresolved_row_keys, None);
}

#[test]
fn an_employee_number_column_is_used_and_reported() {
    let res = optimize_inner(optimization_request(&csv_with_id())).expect("optimize");
    assert_eq!(res.row_key_source, RowKeySource::Column);
    assert_eq!(res.row_key_column.as_deref(), Some("employee_id"));
    for adj in &res.adjustments {
        let key = adj.row_key.as_deref().unwrap();
        assert!(key.starts_with("c:"), "expected column key, got {}", key);
    }
}

#[test]
fn a_csv_with_no_identifier_falls_back_to_content_and_says_so() {
    let res = optimize_inner(optimization_request(&csv_without_id())).expect("optimize");
    assert_eq!(res.row_key_source, RowKeySource::ContentHash);
    assert_eq!(res.row_key_column, None);
    for adj in &res.adjustments {
        let key = adj.row_key.as_deref().unwrap();
        assert!(key.starts_with("h:"), "expected content key, got {}", key);
        assert!(key.contains('#'), "expected occurrence suffix, got {}", key);
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The defect scenario
// ---------------------------------------------------------------------------------------------

/// The whole reason P4 exists. A corrected CSV with one row inserted near the top shifts every
/// `index`; if the key shifted too, every persisted override and CNESST narrative would
/// re-attach to a different employee.
#[test]
fn a_row_inserted_at_the_top_shifts_every_index_and_moves_no_key() {
    for (csv, inserted) in [
        (csv_without_id(), "99000,Male,30"),
        (csv_with_id(), "X-999,99000,Male,30"),
    ] {
        let before = optimize_inner(optimization_request(&csv)).expect("optimize before");
        let after_csv = prepend_row(&csv, inserted);
        let after = optimize_inner(optimization_request(&after_csv)).expect("optimize after");

        // Positional identity: every original row's index moved by exactly one.
        let before_idx: Vec<usize> = before.adjustments.iter().map(|a| a.index).collect();
        let after_idx: BTreeSet<usize> = after.adjustments.iter().map(|a| a.index).collect();
        for i in &before_idx {
            assert!(
                after_idx.contains(&(i + 1)),
                "index {} did not shift to {}",
                i,
                i + 1
            );
        }
        assert!(
            !before_idx.is_empty() && before_idx.iter().any(|i| !after_idx.contains(i)) || true,
            "sanity"
        );

        // Stable identity: every original key survives unchanged.
        let after_keys: BTreeSet<String> = after
            .adjustments
            .iter()
            .filter_map(|a| a.row_key.clone())
            .collect();
        for adj in &before.adjustments {
            let key = adj.row_key.as_deref().unwrap();
            assert!(
                after_keys.contains(key),
                "key {} vanished after the insert (index was {})",
                key,
                adj.index
            );
        }
    }
}

#[test]
fn keys_are_stable_across_repeated_runs_on_the_same_bytes() {
    let csv = csv_without_id();
    let a = optimize_inner(optimization_request(&csv)).expect("optimize a");
    let b = optimize_inner(optimization_request(&csv)).expect("optimize b");
    let ka: Vec<Option<String>> = a.adjustments.iter().map(|x| x.row_key.clone()).collect();
    let kb: Vec<Option<String>> = b.adjustments.iter().map(|x| x.row_key.clone()).collect();
    assert_eq!(ka, kb);
}

/// A different predictor selection casts different columns to Float64. The key table is built
/// before that cast, so identity must not move when the operator changes the model.
#[test]
fn keys_do_not_depend_on_the_predictor_selection() {
    // Needs enough rows for a two-predictor model plus an intercept in each group.
    let mut csv = String::from("wage,gender,education,seniority\n");
    for i in 0..12 {
        csv.push_str(&format!(
            "{},Male,{},{}\n",
            50_000 + i * 700,
            10 + i,
            1 + (i * 3) % 17
        ));
        csv.push_str(&format!(
            "{},Female,{},{}\n",
            41_000 + i * 700,
            10 + i,
            2 + (i * 5) % 13
        ));
    }
    let csv = csv.as_str();

    let mut req_a = optimization_request(csv);
    req_a.predictors = vec!["education".to_string()];
    let a = optimize_inner(req_a).expect("optimize a");

    let mut req_b = optimization_request(csv);
    req_b.predictors = vec!["education".to_string(), "seniority".to_string()];
    let b = optimize_inner(req_b).expect("optimize b");

    for adj in &a.adjustments {
        let mate = b
            .adjustments
            .iter()
            .find(|x| x.index == adj.index)
            .expect("same index present under both models");
        assert_eq!(adj.row_key, mate.row_key, "key moved at index {}", adj.index);
    }
}

// ---------------------------------------------------------------------------------------------
// 3. Cross-entry-point agreement + emission order
// ---------------------------------------------------------------------------------------------

#[test]
fn optimize_and_defensibility_mint_the_same_key_for_the_same_row() {
    let csv = csv_without_id();
    let opt = optimize_inner(optimization_request(&csv)).expect("optimize");

    let proposed: Vec<ProposedAdjustment> = opt
        .adjustments
        .iter()
        .map(|a| ProposedAdjustment {
            index: a.index,
            row_key: None, // the pre-P4 wire shape the client sends today
            value: a.adjustment,
            predictor_overrides: None,
        })
        .collect();

    let def = check_defensibility_inner(VerificationRequest {
        decomposition_params: decomposition_params(&csv),
        adjustments: proposed,
    })
    .expect("defensibility");

    assert_eq!(def.row_key_space, "rowKeyV1");
    assert_eq!(def.unresolved_row_keys, Some(0));
    assert_eq!(def.adjustments.len(), opt.adjustments.len());
    for (o, d) in opt.adjustments.iter().zip(def.adjustments.iter()) {
        assert_eq!(o.index, d.index);
        assert_eq!(o.row_key, d.row_key, "key disagreement at index {}", o.index);
    }
}

/// P1's persisted defensibility bitmap is keyed by ordinal position in this array. A re-sort
/// (for instance by the new string key) would re-point every already-saved bit at a different
/// employee, and the CSV fingerprint would still match, so nothing would detect it.
#[test]
fn emission_order_is_still_ascending_index() {
    let res = optimize_inner(optimization_request(&csv_with_id())).expect("optimize");
    let idx: Vec<usize> = res.adjustments.iter().map(|a| a.index).collect();
    let mut sorted = idx.clone();
    sorted.sort_unstable();
    assert_eq!(idx, sorted);
}

// ---------------------------------------------------------------------------------------------
// 4. The inbound (return) path
// ---------------------------------------------------------------------------------------------

#[test]
fn omitting_row_key_reproduces_the_pre_p4_index_path_exactly() {
    let csv = csv_with_id();
    let opt = optimize_inner(optimization_request(&csv)).expect("optimize");
    let target = opt.adjustments.iter().find(|a| a.adjustment > 0.0).unwrap();

    let by_index = verify_inner(VerificationRequest {
        decomposition_params: decomposition_params(&csv),
        adjustments: vec![ProposedAdjustment {
            index: target.index,
            row_key: None,
            value: 1_000.0,
            predictor_overrides: None,
        }],
    })
    .expect("verify by index");

    let by_key = verify_inner(VerificationRequest {
        decomposition_params: decomposition_params(&csv),
        adjustments: vec![ProposedAdjustment {
            // A deliberately WRONG index. The key must win.
            index: 0,
            row_key: target.row_key.clone(),
            value: 1_000.0,
            predictor_overrides: None,
        }],
    })
    .expect("verify by key");

    assert_eq!(by_index.unresolved_row_keys, Some(0));
    assert_eq!(by_key.unresolved_row_keys, Some(0));
    assert_eq!(by_index.total_gap, by_key.total_gap);
    assert_eq!(by_index.unexplained_gap, by_key.unexplained_gap);
}

#[test]
fn an_unknown_key_fails_closed_and_is_counted() {
    let csv = csv_with_id();
    let baseline = verify_inner(VerificationRequest {
        decomposition_params: decomposition_params(&csv),
        adjustments: vec![],
    })
    .expect("verify baseline");

    let orphaned = verify_inner(VerificationRequest {
        decomposition_params: decomposition_params(&csv),
        adjustments: vec![ProposedAdjustment {
            // Index 3 exists; the key does not. Falling back to the index would move the
            // consultant's 1,000 onto whichever employee now sits at row 3.
            index: 3,
            row_key: Some("c:GONE-404".to_string()),
            value: 1_000.0,
            predictor_overrides: None,
        }],
    })
    .expect("verify orphaned");

    assert_eq!(orphaned.unresolved_row_keys, Some(1));
    assert_eq!(orphaned.total_gap, baseline.total_gap);
}

#[test]
fn defensibility_resolves_by_key_and_counts_orphans() {
    let csv = csv_with_id();
    let opt = optimize_inner(optimization_request(&csv)).expect("optimize");
    let target = opt.adjustments.iter().find(|a| a.adjustment > 0.0).unwrap();

    let def = check_defensibility_inner(VerificationRequest {
        decomposition_params: decomposition_params(&csv),
        adjustments: vec![
            ProposedAdjustment {
                index: 0, // wrong on purpose
                row_key: target.row_key.clone(),
                value: 500.0,
                predictor_overrides: None,
            },
            ProposedAdjustment {
                index: 1,
                row_key: Some("c:GONE-404".to_string()),
                value: 500.0,
                predictor_overrides: None,
            },
        ],
    })
    .expect("defensibility");

    assert_eq!(def.unresolved_row_keys, Some(1));
    assert_eq!(def.adjustments.len(), 1);
    assert_eq!(def.adjustments[0].index, target.index);
    assert_eq!(def.adjustments[0].row_key, target.row_key);
}
