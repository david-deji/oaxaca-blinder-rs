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
//! The shipped quantile-decomposition path (CLI, WASM, MCP) is RIF-regression
//! (Firpo-Fortin-Lemieux 2009) via [`OaxacaBuilder::decompose_quantile`] — call it once
//! per target quantile:
//!
//! ```ignore
//! use polars::prelude::*;
//! use oaxaca_blinder::OaxacaBuilder;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let df = df!(
//!         "wage" => &[10.0, 12.0, 11.0, 13.0, 15.0, 20.0, 22.0, 21.0, 23.0, 25.0, 9.0, 18.0],
//!         "education" => &[12.0, 16.0, 14.0, 16.0, 18.0, 12.0, 16.0, 14.0, 16.0, 18.0, 10.0, 20.0],
//!         "gender" => &["F", "F", "F", "F", "F", "F", "M", "M", "M", "M", "M", "M"]
//!     )?;
//!
//!     let mut builder = OaxacaBuilder::new(df, "wage", "gender", "F");
//!     builder.predictors(["education"]);
//!
//!     for &q in &[0.25, 0.5, 0.75] {
//!         let results = builder.decompose_quantile(q)?;
//!         results.summary();
//!     }
//!     Ok(())
//! }
//! ```
//!
//! [`crate::quantile_decomposition`] (Machado-Mata simulation) is a statistically distinct
//! alternative kept for direct-API consumers, but it is off the shipped surface and has no
//! external-oracle verification — see that module's doc for details.

mod builder;
mod decomposition;
mod display;
mod error;
mod estimation;
mod inference;
mod math;
mod rng;
mod types;

pub mod akm;
pub mod dfl;
pub mod formula;
pub mod heckman;
pub mod jmp;
pub mod matching;
pub mod quantile_decomposition;

// Native, dev/test-only memory-profiling harness (0014-MERIDIAN, D1). Never
// compiled into the production/wasm path — gated by the `mem-profile`
// feature, which is off by default (INV-01).
#[cfg(feature = "mem-profile")]
pub mod mem_profile;

// The tracking allocator is only installed as the process global allocator
// when the `mem-profile` feature is explicitly requested (e.g. `cargo run
// --example mem_profile_harness --features mem-profile`). A plain `cargo
// build`/`cargo test` (default features) never compiles this item, so the
// native byte-equivalence invariant (INV-01) holds unconditionally.
#[cfg(feature = "mem-profile")]
#[global_allocator]
static MEM_PROFILE_ALLOCATOR: mem_profile::TrackingAllocator = mem_profile::TrackingAllocator;

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
#[allow(deprecated)]
pub use quantile_decomposition::QuantileDecompositionBuilder;
pub use rng::{RunMetadata, DEFAULT_SEED};
pub use types::{ComponentResult, DecompositionDetail, OaxacaResults, TwoFoldResults};

/// Quantile-regression coefficient solver, exposed for the statistical-trust-layer QR
/// validation (0014-MERIDIAN AC-5). Thin, allocation-only wrapper over the internal
/// `math::quantile_regression::solve_qr` — additive public surface, no behavior change
/// (INV-01). `x_rows` are the design rows INCLUDING whatever intercept column the caller
/// wants fitted (the solver adds none); returns the coefficient vector in column order.
pub fn qr_coefficients(x_rows: &[Vec<f64>], y: &[f64], tau: f64) -> Result<Vec<f64>, String> {
    use ndarray::{Array1, Array2};
    let n = x_rows.len();
    let k = x_rows.first().map(|r| r.len()).unwrap_or(0);
    let mut x = Array2::<f64>::zeros((n, k));
    for (i, row) in x_rows.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            x[[i, j]] = v;
        }
    }
    let yv = Array1::from(y.to_vec());
    crate::math::quantile_regression::solve_qr(&x, &yv, tau)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
