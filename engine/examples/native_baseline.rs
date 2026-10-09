//! D3 (0014-close round-1) / INV-02 native↔wasm tolerance leg: emit the native
//! `decompose_inner` JSON for the SAME request `engine/tests/mode_parity_test.rs`
//! and `verification/browser-parity/compute.worker.mjs` run against, so the
//! browser-parity CI job (and the local harness) can diff it against the WASM
//! `threads=1` run at a per-numeric-field <=1e-6 tolerance
//! (`verification/browser-parity/parity.spec.mjs`). Same fixture, same request
//! shape, same seed: `DecompositionRequest` carries no seed field, so both this
//! example and the wasm `decompose()` call resolve to the same DEFAULT_SEED.
//!
//! Thread count is irrelevant here: the within-platform leg
//! (`ac2_mode_parity_native_byte_identity_across_threads`) already proves native
//! output is byte-identical across thread counts (INV-02), so this runs on
//! rayon's default global pool rather than pinning one.
//!
//! Output: `{"three_fold": <result>, "two_fold": <result>}`, the same two requests compute.worker.mjs runs.
//!
//! Run: `cargo run -p pay-equity-engine --example native_baseline > native-baseline.json`

use pay_equity_engine::analysis::decompose_inner;
use pay_equity_engine::types::DecompositionRequest;

const FIXTURE: &[u8] = include_bytes!("../../oaxaca_blinder/tests/fixtures/parity_fixture.csv");

/// The request both legs run. `three_fold` selects the decomposition: the three-fold result carries the
/// interaction term but no per-predictor detail and no standard error, the two-fold result carries the
/// detail rows and the bootstrap standard error. Both are compared (0119 review F2), because a comparison
/// of the three-fold payload alone covered 18 scalars and no coefficient-level number.
fn request(three_fold: bool) -> DecompositionRequest {
    DecompositionRequest {
        csv_data: FIXTURE.to_vec(),
        outcome_variable: "log_wage".to_string(),
        group_variable: "gender".to_string(),
        reference_group: "F".to_string(),
        predictors: vec![
            "education".to_string(),
            "experience".to_string(),
            "tenure".to_string(),
        ],
        categorical_predictors: None,
        three_fold: Some(three_fold),
        quantile: None,
        reference_coefficients: Some("Pooled".to_string()),
        bootstrap_reps: Some(64), // matches compute.worker.mjs + mode_parity_test.rs
    }
}

fn main() {
    let three_fold =
        decompose_inner(request(true)).expect("native baseline decompose (three-fold)");
    let two_fold = decompose_inner(request(false)).expect("native baseline decompose (two-fold)");
    let both = serde_json::json!({
        "three_fold": serde_json::to_value(&three_fold).expect("serialize three-fold"),
        "two_fold": serde_json::to_value(&two_fold).expect("serialize two-fold"),
    });
    print!("{}", serde_json::to_string(&both).expect("serialize"));
}
