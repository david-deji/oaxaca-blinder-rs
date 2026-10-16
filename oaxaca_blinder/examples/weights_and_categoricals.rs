//! Categorical drivers and sample weights. `sector` is a categorical predictor, so each of its levels gets a
//! row in the detailed result, and `headcount` says how many employees each row stands for.
//!
//! Run: `cargo run -p oaxaca_blinder --example weights_and_categoricals`

use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients, WeightsKind, INTERCEPT_NAME};
use polars::prelude::*;

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
        ],
        "sector" => &[
            "Sales", "Tech", "Admin", "Admin", "Admin", "Sales", "Admin", "Admin", "Admin", "Tech", "Sales", "Tech", "Admin", "Tech", "Tech", "Admin",
            "Sales", "Sales", "Admin", "Tech", "Admin", "Admin", "Admin", "Sales", "Sales", "Tech", "Tech", "Tech", "Sales", "Admin", "Sales", "Sales",
        ],
        "headcount" => &[
            2.0, 1.0, 3.0, 1.0, 2.0, 1.0, 1.0, 2.0, 1.0, 1.0, 2.0, 1.0, 3.0, 1.0, 2.0, 1.0,
            1.0, 2.0, 1.0, 1.0, 2.0, 1.0, 3.0, 1.0, 2.0, 1.0, 1.0, 2.0, 1.0, 1.0, 2.0, 1.0,
        ]
    )?;

    let results = OaxacaBuilder::new(df, "wage", "gender", "F")
        .predictors(["education", "experience"])
        .categorical_predictors(["sector"])
        // Each level of `sector` is shown as a deviation from the share-weighted average of all
        // its levels, so the rows do not depend on which level sorts first.
        .normalize_all_categoricals()
        // `headcount` is a whole-number count: a row with 2 is two employees.
        .weights("headcount")
        .weights_kind(WeightsKind::Frequency)
        .reference_coefficients(ReferenceCoefficients::Pooled)
        .run()?;

    println!("Gap (M minus F): {:.0}", results.total_gap());
    println!("Explained, by driver");
    // The intercept has its own reserved name and is not a driver.
    for driver in results
        .two_fold()
        .detailed_explained()
        .iter()
        .filter(|d| d.name() != INTERCEPT_NAME)
    {
        println!("  {:<22} {:>8.0}", driver.name(), driver.estimate());
    }
    Ok(())
}
