//! MODE-parity — byte-identical decompose output across thread counts (0014-MERIDIAN,
//! verification-benchmark D-1/D-2, AC-1/AC-2 native leg; INV-02).
//!
//! Threading changes the execution mode but must not change a single output bit. This test
//! runs the SAME seeded engine decompose (DEFAULT_SEED) inside rayon pools of 1/2/4 threads
//! and asserts the canonical serialization is byte-identical — proving the bounded-parallel
//! bootstrap's float reduction is rep-index-ordered (thread-count-invariant), not merely
//! close. This is the METHODS-independent MODES gate: a math bug would pass here (all modes
//! wrong-identically) and fail the trust goldens; a threading bug fails HERE.
//!
//! Canonical serializer: `serde_json::to_string` of the Serialize-derived `DecompositionResult`
//! (all-`Vec`, no HashMap → stable field + element order + deterministic ryu floats). The
//! double-serialize self-test (AC-1) guards the map-order assumption explicitly.
//!
//! The native↔wasm tolerance-parity leg (council MJ-1) + browser seq/t2/t4 modes run in the
//! Playwright/COI CI job (verification-benchmark D-6). The raw-wasm sha256 baseline (ci.yml,
//! stage 3) covers the wasm side's cross-build reproducibility.

use pay_equity_engine::analysis::decompose_inner;
use pay_equity_engine::types::DecompositionRequest;
use rayon::ThreadPoolBuilder;

const FIXTURE: &[u8] = include_bytes!("../../oaxaca_blinder/tests/fixtures/parity_fixture.csv");

fn request(reps: usize) -> DecompositionRequest {
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
        three_fold: Some(true),
        quantile: None,
        reference_coefficients: None,
        bootstrap_reps: Some(reps),
    }
}

fn canonical(threads: usize, reps: usize) -> String {
    let pool = ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    pool.install(|| serde_json::to_string(&decompose_inner(request(reps)).unwrap()).unwrap())
}

#[test]
fn ac2_mode_parity_native_byte_identity_across_threads() {
    let reps = 64; // enough bootstrap work that a non-ordered reduction would diverge
    let s1 = canonical(1, reps);
    let s2 = canonical(2, reps);
    let s4 = canonical(4, reps);
    assert_eq!(
        s1, s2,
        "native 1-thread vs 2-thread byte mismatch — bootstrap reduction is not rep-index-ordered"
    );
    assert_eq!(s2, s4, "native 2-thread vs 4-thread byte mismatch");
}

#[test]
fn ac1_serializer_double_serialize_determinism() {
    // D-1 R-VB-C: same result serialized twice must be byte-identical (map-order guard).
    let res = decompose_inner(request(32)).unwrap();
    let a = serde_json::to_string(&res).unwrap();
    let b = serde_json::to_string(&res).unwrap();
    assert_eq!(
        a, b,
        "double-serialize not byte-identical — canonical serializer is order-unstable"
    );
}
