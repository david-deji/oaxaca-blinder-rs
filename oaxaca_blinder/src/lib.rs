#![cfg_attr(docsrs, feature(doc_cfg))]
// The crate documentation is the crate README, so the first example is compiled as a doctest and cannot
// drift from the code. The repository README is compiled the same way, as a doctest of a private item.
#![doc = include_str!("../README.md")]

#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
#[allow(dead_code)]
struct RepositoryReadme;

/// The reserved name of the constant column every design matrix starts with, and therefore the
/// `name` of the intercept's entry in every per-variable vector the engine returns
/// (`detailed_explained`, `detailed_unexplained`, `model_coefficients`, per-employee
/// `contributions`). The one definition of the token: a consumer compares
/// against this, never against the string `intercept`, which a real HR column can be called.
pub const INTERCEPT_NAME: &str = "__ob_intercept__";

/// What the printed tables (`OaxacaResults::summary`, the command line, the HTML report) call the intercept row
/// instead of [`INTERCEPT_NAME`]. Data (the results' own names, JSON output) always carries `INTERCEPT_NAME`.
pub const INTERCEPT_LABEL: &str = "(intercept)";

mod builder;
mod decomposition;
mod display;
mod error;
mod estimation;
mod inference;
mod math;
mod rng;
mod rows;
mod types;

// Experimental methods (0121-MERIDIAN L6): no external oracle, results may change in any
// release. Off by default; enable the `experimental` feature to use them.
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub mod akm;
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub mod dfl;
pub mod formula;
pub mod heckman;
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub mod jmp;
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
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

#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub use akm::{AkmBuilder, AkmResult};
pub use builder::{OaxacaBuilder, DEFAULT_BOOTSTRAP_REPS};
pub use decomposition::{BudgetAdjustment, ReferenceCoefficients};
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub use dfl::run_dfl;
pub use error::OaxacaError;
pub use heckman::heckman_two_step;
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub use jmp::decompose_changes;
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub use matching::engine::MatchingEngine;
pub use math::normalization::{
    FactorShares, LevelShare, NormalizationConvention, NormalizationRecord, ShareMap,
};
pub use math::weights::{weighted_quantile, WeightsKind};
/// The Polars version this crate builds its `DataFrame`s with. Use `oaxaca_blinder::polars` to build the input, so
/// the `DataFrame` you pass in is always the type the crate expects, whatever Polars version you would pick yourself.
pub use polars;
#[allow(deprecated)]
pub use quantile_decomposition::QuantileDecompositionBuilder;
pub use rng::{RunMetadata, DEFAULT_SEED};
pub use rows::{DataMatricesWithRows, ExcludedRow, ExclusionReason, GroupMatrices, RowAccounting};
pub use types::{ComponentResult, DecompositionDetail, OaxacaResults, TwoFoldResults};

/// Quantile-regression coefficient solver, exposed so its coefficients can be checked against R `quantreg`.
/// A thin wrapper over the internal `math::quantile_regression::solve_qr`. `x_rows` are the design rows INCLUDING whatever intercept column the caller
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
