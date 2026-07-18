//! INV-02 (quantile path) harness: emit a fixed-seed RIF quantile decomposition serialization to
//! stdout so an external script can byte-compare across RAYON_NUM_THREADS values. The
//! per-replicate RIF bootstrap (ruling 4) runs on the rayon pool; if the seeded RNG or the
//! per-rep RIF recompute were schedule-dependent, the serialized SEs/CIs would differ across
//! thread counts. Within-platform byte-identity is the quantile-path threading-safety property
//! (0014-MERIDIAN stage 3, mirrors emit_seeded_run for the mean path).
//!
//! Run: `RAYON_NUM_THREADS=N cargo run -p oaxaca_blinder --example emit_seeded_quantile`

use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;

fn main() {
    let path = format!(
        "{}/tests/fixtures/parity_fixture.csv",
        env!("CARGO_MANIFEST_DIR")
    );
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
    let r = b.decompose_quantile(0.5).expect("decompose_quantile");
    print!("{}", serde_json::to_string(&r).expect("serialize"));
}
