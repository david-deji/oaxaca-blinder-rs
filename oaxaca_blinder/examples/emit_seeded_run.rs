//! AC-6 / INV-02 harness: emit a fixed-seed decomposition serialization to stdout so an
//! external script can byte-compare across RAYON_NUM_THREADS values. The bootstrap (reps=64)
//! runs on the rayon pool; if the seeded RNG were schedule-dependent, the serialized SEs would
//! differ across thread counts. Within-platform byte-identity is the threading-safety property.
//!
//! Run: `RAYON_NUM_THREADS=N cargo run -p oaxaca_blinder --example emit_seeded_run`

use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;

fn main() {
    let path = format!("{}/tests/fixtures/parity_fixture.csv", env!("CARGO_MANIFEST_DIR"));
    let df = LazyCsvReader::new(path)
        .with_has_header(true)
        .finish()
        .expect("fixture readable")
        .collect()
        .expect("fixture parses");
    let mut b = OaxacaBuilder::new(df, "log_wage", "gender", "F");
    b.predictors(vec!["education", "experience", "tenure"])
        .reference_coefficients(ReferenceCoefficients::GroupB)
        .bootstrap_reps(64)
        .seed(0x00AB_CDEF);
    let r = b.run().expect("run");
    print!("{}", serde_json::to_string(&r).expect("serialize"));
}
