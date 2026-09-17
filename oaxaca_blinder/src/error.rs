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

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::PolarsError;

    #[test]
    fn test_oaxaca_error_display() {
        let polars_err = OaxacaError::PolarsError(PolarsError::ColumnNotFound("col".into()));
        assert_eq!(polars_err.to_string(), "Polars error: not found: col");

        let col_err = OaxacaError::ColumnNotFound("gender".to_string());
        assert_eq!(col_err.to_string(), "Column not found: gender");

        let group_err = OaxacaError::InvalidGroupVariable("group".to_string());
        assert_eq!(group_err.to_string(), "Invalid group variable: group");

        let nalgebra_err = OaxacaError::NalgebraError("singular matrix".to_string());
        assert_eq!(nalgebra_err.to_string(), "Nalgebra error: singular matrix");

        let diag_err = OaxacaError::DiagnosticError("vif failed".to_string());
        assert_eq!(diag_err.to_string(), "Diagnostic error: vif failed");

        let data_err = OaxacaError::InsufficientData("not enough rows".to_string());
        assert_eq!(data_err.to_string(), "Insufficient data: not enough rows");

        let empty_level_err = OaxacaError::EmptyLevelInGroup {
            column: "education".to_string(),
            level: "PhD".to_string(),
            missing_from_group: "group_b".to_string(),
        };
        assert_eq!(
            empty_level_err.to_string(),
            "EMPTY_LEVEL_IN_GROUP: column=education, level=PhD, missing_from_group=group_b"
        );
    }

    #[test]
    fn test_from_polars_error() {
        let polars_err = PolarsError::ColumnNotFound("missing".into());
        let oaxaca_err: OaxacaError = polars_err.into();
        assert!(matches!(oaxaca_err, OaxacaError::PolarsError(_)));
        assert_eq!(oaxaca_err.to_string(), "Polars error: not found: missing");
    }
}
