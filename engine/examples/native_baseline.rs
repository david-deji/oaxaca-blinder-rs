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
//! Run: `cargo run -p pay-equity-engine --example native_baseline > native-baseline.json`

use pay_equity_engine::analysis::decompose_inner;
use pay_equity_engine::types::DecompositionRequest;

const FIXTURE: &[u8] = include_bytes!("../../oaxaca_blinder/tests/fixtures/parity_fixture.csv");

fn main() {
    let req = DecompositionRequest {
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
        three_fold: Some(true),
        quantile: None,
        reference_coefficients: None,
        bootstrap_reps: Some(64), // matches compute.worker.mjs + mode_parity_test.rs
    };
    let result = decompose_inner(req).expect("native baseline decompose");
    print!("{}", serde_json::to_string(&result).expect("serialize"));
}
