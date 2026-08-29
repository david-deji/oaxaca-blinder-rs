//! Integration tests for the deterministic seeded RNG (0014-MERIDIAN, stage 1).
//!
//! Covers AC-4 (default determinism + seed sensitivity), AC-5 (entropy round-trip,
//! native-only), AC-7 (discard-count invariant + reproducibility), AC-8 (RunMetadata
//! presence in serialized output), AC-11 (quantile seed round-trip through
//! `decompose_quantile`, the In-Scope 12 / CV-1 seam).
//!
//! Offline: reads only the committed `tests/fixtures/parity_fixture.csv`.

use oaxaca_blinder::{OaxacaBuilder, OaxacaResults, ReferenceCoefficients, DEFAULT_SEED};
use polars::prelude::*;

const FIXTURE: &str = "tests/fixtures/parity_fixture.csv";

fn load() -> DataFrame {
    LazyCsvReader::new(FIXTURE)
        .with_has_header(true)
        .finish()
        .expect("fixture readable")
        .collect()
        .expect("fixture parses")
}

/// Full serialization of the result (point estimates + bootstrap SE/CI + run_metadata).
/// Byte-equality here means the entire seeded pipeline is reproducible, not just point math.
fn serialize(r: &OaxacaResults) -> String {
    serde_json::to_string(r).expect("OaxacaResults serializes")
}

fn run_mean(seed: Option<u64>, reps: usize) -> OaxacaResults {
    let mut b = OaxacaBuilder::new(load(), "log_wage", "gender", "F");
    b.predictors(vec!["education", "experience", "tenure"])
        .reference_coefficients(ReferenceCoefficients::GroupB)
        .bootstrap_reps(reps);
    if let Some(s) = seed {
        b.seed(s);
    }
    b.run().expect("mean run")
}

fn run_quantile(seed: u64, q: f64, reps: usize) -> OaxacaResults {
    let mut b = OaxacaBuilder::new(load(), "log_wage", "gender", "F");
    b.predictors(vec!["education", "experience", "tenure"])
        .reference_coefficients(ReferenceCoefficients::GroupB)
        .bootstrap_reps(reps)
        .seed(seed);
    b.decompose_quantile(q).expect("decompose_quantile")
}

/// AC-4: reproducible-by-default (no `.seed()` -> DEFAULT_SEED), and seed-sensitive.
#[test]
fn ac4_default_determinism_and_seed_sensitivity() {
    let a = run_mean(None, 16);
    let b = run_mean(None, 16);
    assert_eq!(
        serialize(&a),
        serialize(&b),
        "default (no-seed) runs must be byte-identical"
    );
    assert_eq!(
        a.run_metadata.seed, DEFAULT_SEED,
        "no-seed run must resolve to DEFAULT_SEED"
    );

    let s1 = run_mean(Some(1), 16);
    let s2 = run_mean(Some(2), 16);
    assert_ne!(
        serialize(&s1),
        serialize(&s2),
        "different seeds must produce different bootstrap distributions"
    );
    // Same explicit seed reproduces byte-identically.
    assert_eq!(
        serialize(&run_mean(Some(7), 16)),
        serialize(&run_mean(Some(7), 16))
    );
}

/// AC-5: entropy seeding records a non-default seed and stays reproducible after the fact.
#[cfg(not(target_family = "wasm"))]
#[test]
fn ac5_entropy_round_trip() {
    let mut b = OaxacaBuilder::new(load(), "log_wage", "gender", "F");
    b.predictors(vec!["education", "experience", "tenure"])
        .reference_coefficients(ReferenceCoefficients::GroupB)
        .bootstrap_reps(16)
        .seed_from_entropy();
    let entropy_run = b.run().expect("entropy run");
    let drawn = entropy_run.run_metadata.seed;
    assert_ne!(
        drawn, DEFAULT_SEED,
        "entropy seed must (essentially never) equal DEFAULT_SEED"
    );

    // Re-running with the recorded seed reproduces the entropy run byte-identically.
    let replay = run_mean(Some(drawn), 16);
    assert_eq!(
        serialize(&entropy_run),
        serialize(&replay),
        "recorded entropy seed must reproduce the run after the fact"
    );
}

/// AC-7: the discard accounting is a closed invariant and reproducible for a fixed seed.
/// (Cross-thread-count identity is exercised by the verification-benchmark stage under
/// RAYON_NUM_THREADS=1/2/4; the closed-form stream design guarantees it.)
#[test]
fn ac7_discard_invariant_and_reproducible() {
    let r = run_mean(Some(123), 20);
    let m = &r.run_metadata;
    assert_eq!(m.bootstrap_reps_requested, 20);
    assert_eq!(
        m.bootstrap_reps_succeeded + m.bootstrap_reps_discarded,
        m.bootstrap_reps_requested,
        "succeeded + discarded must equal requested"
    );
    // The full metadata (incl. the discard count) is reproducible for a fixed seed.
    let r2 = run_mean(Some(123), 20);
    assert_eq!(
        r.run_metadata.bootstrap_reps_discarded,
        r2.run_metadata.bootstrap_reps_discarded
    );
    assert_eq!(
        r.run_metadata.bootstrap_reps_succeeded,
        r2.run_metadata.bootstrap_reps_succeeded
    );
}

/// AC-8: RunMetadata is present with all six fields and is embedded in serialized output.
#[test]
fn ac8_metadata_presence() {
    let r = run_mean(Some(42), 12);
    let m = &r.run_metadata;
    assert_eq!(m.seed, 42, "seed echoes the effective master seed");
    assert_eq!(m.rng_algorithm, "ChaCha8");
    assert_eq!(m.rand_chacha_version, "0.3.1");
    assert_eq!(m.bootstrap_reps_requested, 12);

    let json = serialize(&r);
    for field in [
        "run_metadata",
        "\"seed\"",
        "rng_algorithm",
        "rand_chacha_version",
        "bootstrap_reps_requested",
        "bootstrap_reps_succeeded",
        "bootstrap_reps_discarded",
    ] {
        assert!(json.contains(field), "serialized output missing {field}");
    }
    assert!(
        json.contains("ChaCha8"),
        "serialized output must carry the algorithm string"
    );
}

/// AC-11: the RIF quantile path (`decompose_quantile`) forwards the seed (council CV-1).
/// Guards the seam between the deterministic-rng seed API and the In-Scope 12 wiring.
#[test]
fn ac11_quantile_seed_round_trip() {
    let a = run_quantile(1, 0.5, 8);
    let b = run_quantile(2, 0.5, 8);
    assert_ne!(
        serialize(&a),
        serialize(&b),
        "decompose_quantile must be seed-sensitive (seed forwarded, not dropped)"
    );

    let x1 = run_quantile(7, 0.5, 8);
    let x2 = run_quantile(7, 0.5, 8);
    assert_eq!(
        serialize(&x1),
        serialize(&x2),
        "same seed reproduces byte-identically"
    );
    assert_eq!(
        x1.run_metadata.seed, 7,
        "forwarded seed must land in RunMetadata (CV-1)"
    );
}
