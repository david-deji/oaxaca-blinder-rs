//! A Rust implementation of the Oaxaca-Blinder decomposition method.
//!
//! This library provides tools to decompose the mean difference in an outcome
//! variable between two groups into an "explained" part (due to differences
//! in observable characteristics) and an "unexplained" part (due to differences
//! in the returns to those characteristics).
//!
//! Currently, the library supports numerical predictors and calculates standard
//! errors using bootstrapping.
//!
//! # Example
//!
//! ```ignore
//! use polars::prelude::*;
//! use oaxaca_blinder::OaxacaBuilder;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let df = df!(
//!         "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0],
//!         "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0],
//!         "gender" => &["F", "F", "F", "F", "F", "M", "M", "M", "M", "M"]
//!     )?;
//!
//!     let results = OaxacaBuilder::new(df, "wage", "gender", "F")
//!         .predictors(&["education"])
//!         .run()?;
//!
//!     results.summary();
//!     Ok(())
//! }
//! ```
//!
//! ### Quantile Regression Decomposition
//!
//! ```ignore
//! use polars::prelude::*;
//! use oaxaca_blinder::QuantileDecompositionBuilder;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let df = df!(
//!         "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0, 9.0, 18.0],
//!         "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 10.0, 20.0],
//!         "gender" => &["F", "F", "F", "F", "F", "F", "M", "M", "M", "M", "M", "M"]
//!     )?;
//!
//!     let results = QuantileDecompositionBuilder::new(df, "wage", "gender", "F")
//!         .predictors(&["education"])
//!         .quantiles(&[0.25, 0.5, 0.75])
//!         .run()?;
//!
//!     results.summary();
//!     Ok(())
//! }
//! ```

mod builder;
pub mod decomposition;
mod display;
mod error;
mod estimation;
mod inference;
mod math;
mod types;

pub mod akm;
pub mod dfl;
pub mod formula;
pub mod heckman;
pub mod jmp;
pub mod matching;
pub mod quantile_decomposition;

// #[cfg(feature = "python")]
// pub mod python;

pub use akm::{AkmBuilder, AkmResult};
pub use builder::OaxacaBuilder;
pub use decomposition::{BudgetAdjustment, ReferenceCoefficients};
pub use dfl::run_dfl;
pub use error::OaxacaError;
pub use heckman::heckman_two_step;
pub use jmp::decompose_changes;
pub use matching::engine::MatchingEngine;
pub use quantile_decomposition::QuantileDecompositionBuilder;
pub use types::{ComponentResult, DecompositionDetail, OaxacaResults, TwoFoldResults};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
