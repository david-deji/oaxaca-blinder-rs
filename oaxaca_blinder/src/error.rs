use polars::prelude::PolarsError;
use std::fmt;

/// Error type for the `oaxaca_blinder` library.
#[derive(Debug)]
pub enum OaxacaError {
    /// Wraps a `PolarsError`.
    PolarsError(PolarsError),
    /// Occurs when a specified column name does not exist in the DataFrame.
    ColumnNotFound(String),
    /// Occurs when the grouping variable does not contain exactly two unique, non-null groups.
    InvalidGroupVariable(String),
    /// Occurs when there is an issue with linear algebra operations, such as a singular matrix.
    NalgebraError(String),
    /// Occurs when there is an issue with a diagnostic calculation.
    DiagnosticError(String),
    /// Occurs when there is not enough data for an operation.
    InsufficientData(String),
    /// Occurs when a categorical predictor level is present in the full dataset
    /// but entirely absent from one of the two comparison groups (0014-close
    /// round-1, D1). Left undetected this collapses that group's design matrix
    /// to a singular `X'X` and reaches `math/ols.rs`'s Cholesky check as an
    /// opaque "multicollinearity" message naming neither column, level, nor
    /// group; this variant names all three before estimation is attempted.
    EmptyLevelInGroup {
        column: String,
        level: String,
        missing_from_group: String,
    },
}

impl From<PolarsError> for OaxacaError {
    fn from(err: PolarsError) -> Self {
        OaxacaError::PolarsError(err)
    }
}

impl fmt::Display for OaxacaError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OaxacaError::PolarsError(e) => write!(f, "Polars error: {}", e),
            OaxacaError::ColumnNotFound(s) => write!(f, "Column not found: {}", s),
            OaxacaError::InvalidGroupVariable(s) => write!(f, "Invalid group variable: {}", s),
            OaxacaError::NalgebraError(s) => write!(f, "Nalgebra error: {}", s),
            OaxacaError::DiagnosticError(s) => write!(f, "Diagnostic error: {}", s),
            OaxacaError::InsufficientData(s) => write!(f, "Insufficient data: {}", s),
            OaxacaError::EmptyLevelInGroup {
                column,
                level,
                missing_from_group,
            } => write!(
                f,
                "EMPTY_LEVEL_IN_GROUP: column={}, level={}, missing_from_group={}",
                column, level, missing_from_group
            ),
        }
    }
}

impl std::error::Error for OaxacaError {}
