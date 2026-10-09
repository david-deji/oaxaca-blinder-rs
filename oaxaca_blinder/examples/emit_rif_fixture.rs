//! Writes `tests/fixtures/norm_skewed_rif.csv`: the model columns of `norm_skewed_fixture.csv`
//! plus the engine's own per-group RIF outcome at tau = 0.1, 0.5, 0.9 (`rif_q10`, `rif_q50`,
//! `rif_q90`). Stage 2 of `verification/regen_norm_goldens.sh`; the R generator then treats
//! each column as an ordinary outcome and runs the package oracles on it (0120-MERIDIAN V1d).
//! The normalisation test re-derives these columns from the engine and refuses a stale file.
//!
//! Run: `cargo run -p oaxaca_blinder --example emit_rif_fixture`

use oaxaca_blinder::OaxacaBuilder;
use polars::prelude::*;

fn main() {
    let dir = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));
    let df = LazyCsvReader::new(format!("{dir}/norm_skewed_fixture.csv"))
        .with_has_header(true)
        .finish()
        .expect("fixture readable")
        .collect()
        .expect("fixture parses");
    let mut b = OaxacaBuilder::new(df, "log_salary", "Gender", "Female");
    b.predictors(vec!["Age", "Experience_Years"])
        .categorical_predictors(vec!["Department", "Location"]);

    let mut out: Option<DataFrame> = None;
    for (tag, tau) in [("q10", 0.1), ("q50", 0.5), ("q90", 0.9)] {
        let frame = b.rif_outcome_frame(tau).expect("rif frame");
        let rif = frame
            .column("log_salary")
            .expect("outcome column")
            .as_materialized_series()
            .clone()
            .with_name(format!("rif_{tag}").as_str().into());
        match &mut out {
            None => {
                let mut base = frame
                    .select([
                        "Age",
                        "Experience_Years",
                        "Gender",
                        "Department",
                        "Location",
                    ])
                    .expect("model columns");
                base.with_column(rif).expect("add rif");
                out = Some(base);
            }
            Some(base) => {
                base.with_column(rif).expect("add rif");
            }
        }
    }
    let mut out = out.expect("at least one tau");
    let path = format!("{dir}/norm_skewed_rif.csv");
    let mut file = std::fs::File::create(&path).expect("create csv");
    CsvWriter::new(&mut file)
        .finish(&mut out)
        .expect("write csv");
    println!("wrote {path} ({} rows)", out.height());
}
