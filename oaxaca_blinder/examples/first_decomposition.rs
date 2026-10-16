//! A first decomposition: the pay gap between two groups, split into the part the measured
//! characteristics explain and the part they do not. The data is inline, so the example runs as is.
//!
//! Run: `cargo run -p oaxaca_blinder --example first_decomposition`

use oaxaca_blinder::polars::prelude::*;
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let df = df!(
        "wage" => &[
            45600.0, 34900.0, 55600.0, 44800.0, 46600.0, 41100.0, 47200.0, 61600.0, 56300.0, 44800.0,
            40600.0, 50500.0, 55400.0, 60000.0, 49400.0, 44400.0, 57300.0, 55700.0, 46600.0, 58100.0,
            47000.0, 58200.0, 62400.0, 60500.0, 56600.0, 54500.0, 51800.0, 65700.0, 63400.0, 55000.0,
            53700.0, 59900.0,
        ],
        "education" => &[
            16.0, 12.0, 16.0, 18.0, 16.0, 12.0, 18.0, 20.0, 18.0, 14.0, 14.0, 18.0, 18.0, 18.0, 16.0,
            12.0, 14.0, 16.0, 14.0, 16.0, 16.0, 17.0, 16.0, 18.0, 16.0, 12.0, 13.0, 20.0, 20.0, 19.0,
            17.0, 17.0,
        ],
        "experience" => &[
            5.0, 3.0, 19.0, 7.0, 14.0, 18.0, 4.0, 19.0, 13.0, 2.0, 10.0, 4.0, 22.0, 19.0, 4.0, 19.0,
            20.0, 17.0, 10.0, 13.0, 5.0, 13.0, 24.0, 13.0, 21.0, 18.0, 12.0, 12.0, 17.0, 4.0, 10.0,
            17.0,
        ],
        "gender" => &[
            "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F",
            "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M",
        ]
    )?;

    // "F" is the reference group. The default bootstrap (100 replicates, fixed seed) is used,
    // so a second run prints the same numbers.
    let results = OaxacaBuilder::new(df, "wage", "gender", "F")
        .predictors(["education", "experience"])
        .reference_coefficients(ReferenceCoefficients::Pooled)
        .run()?;

    println!("Gap (M minus F): {:.0}", results.total_gap());
    for part in results.two_fold().aggregate() {
        println!(
            "{:<12} {:>8.0}  (95% CI {:.0} to {:.0})",
            part.name(),
            part.estimate(),
            part.ci_lower(),
            part.ci_upper()
        );
    }
    Ok(())
}
