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
    /// The group column carries more than one value besides the reference group
    /// (0118-MERIDIAN S2). The decomposition compares exactly two groups; a third
    /// value used to be silently dropped from the estimation frames while the
    /// optimiser still counted its rows as target employees. Checked once, on the
    /// RAW frame, so a third value that only appears on a row blank elsewhere is
    /// still refused. Values are compared untrimmed and case-sensitively: `"F "`,
    /// `" "` and `"f"` are values.
    TooManyGroupValues {
        group_column: String,
        reference_group: String,
        /// Every distinct non-null value other than the reference, ascending.
        other_values: Vec<String>,
    },
    /// The reference group value does not occur in the group column at all
    /// (0118-MERIDIAN S2).
    ReferenceGroupAbsent {
        group_column: String,
        reference_group: String,
    },
    /// A `reference_coefficients` name that is not one of the accepted schemes, or none at all
    /// (0120-MERIDIAN S4). The shipped surfaces used to fall back to `Pooled` for anything
    /// unrecognised, which made the least-verified scheme the silent default.
    UnknownReferenceCoefficients {
        /// What the caller sent; `None` when the field was absent.
        given: Option<String>,
    },
    /// A normalisation request the data cannot satisfy (0120-MERIDIAN S1).
    NormalizationError(String),
    /// A weights column was named without saying what its weights mean (0120-MERIDIAN S9). No
    /// single convention makes both "uniform fractional weights are a no-op" and "w = 2 is the
    /// row twice" true, so the caller states `frequency` or `relative`.
    WeightsKindRequired { column: String },
    /// One weight the stated kind cannot take (0120-MERIDIAN S9): not finite, negative, or a
    /// fractional value under `frequency`. `row` is the 0-based position among the data rows of
    /// the frame given to the builder.
    InvalidWeight {
        column: String,
        row: usize,
        value: f64,
        reason: String,
    },
}

impl From<PolarsError> for OaxacaError {
    fn from(err: PolarsError) -> Self {
        OaxacaError::PolarsError(err)
    }
}

/// How many distinct group values [`OaxacaError::TooManyGroupValues`] prints before "and N more".
pub const MAX_LISTED_GROUP_VALUES: usize = 5;

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
            OaxacaError::TooManyGroupValues {
                group_column,
                reference_group,
                other_values,
            } => {
                // The variant keeps every value; the message names at most
                // `MAX_LISTED_GROUP_VALUES` of them. A wrong group column (a name or an ID)
                // would otherwise put every distinct cell into an error shown in the UI and
                // written to logs. `distinct_other_values` is the full count.
                let shown = &other_values[..other_values.len().min(MAX_LISTED_GROUP_VALUES)];
                write!(
                    f,
                    "TOO_MANY_GROUP_VALUES: column={}, reference_group={:?}, \
                     distinct_other_values={}, other_values={:?}",
                    group_column,
                    reference_group,
                    other_values.len(),
                    shown
                )?;
                if other_values.len() > shown.len() {
                    write!(f, " and {} more", other_values.len() - shown.len())?;
                }
                Ok(())
            }
            OaxacaError::ReferenceGroupAbsent {
                group_column,
                reference_group,
            } => write!(
                f,
                "REFERENCE_GROUP_ABSENT: column={}, reference_group={:?}",
                group_column, reference_group
            ),
            OaxacaError::UnknownReferenceCoefficients { given } => {
                let valid = crate::decomposition::ReferenceCoefficients::ACCEPTED_NAMES.join(", ");
                match given {
                    Some(g) => write!(
                        f,
                        "UNKNOWN_REFERENCE_COEFFICIENTS: got {:?}; reference_coefficients must be exactly one of: {}",
                        g, valid
                    ),
                    None => write!(
                        f,
                        "UNKNOWN_REFERENCE_COEFFICIENTS: reference_coefficients is required and was absent; it must be exactly one of: {}",
                        valid
                    ),
                }
            }
            OaxacaError::NormalizationError(s) => write!(f, "Normalization error: {}", s),
            OaxacaError::WeightsKindRequired { column } => write!(
                f,
                "WEIGHTS_KIND_REQUIRED: column={}; weights_kind must be stated, exactly one of: {}",
                column,
                crate::math::weights::WeightsKind::ACCEPTED_NAMES.join(", ")
            ),
            OaxacaError::InvalidWeight {
                column,
                row,
                value,
                reason,
            } => write!(
                f,
                "INVALID_WEIGHT: column={}, row={}, value={}: {}",
                column, row, value, reason
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
        // Prefix is ours; the rest is polars wording, which changes across versions.
        let msg = polars_err.to_string();
        assert!(msg.starts_with("Polars error: "), "{msg}");
        assert!(msg.contains("col"), "{msg}");

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
    fn too_many_group_values_message_is_capped_but_the_variant_keeps_every_value() {
        let few = OaxacaError::TooManyGroupValues {
            group_column: "Gender".to_string(),
            reference_group: "Male".to_string(),
            other_values: vec!["Female".to_string(), "X".to_string()],
        };
        assert_eq!(
            few.to_string(),
            "TOO_MANY_GROUP_VALUES: column=Gender, reference_group=\"Male\", \
             distinct_other_values=2, other_values=[\"Female\", \"X\"]"
        );

        // Exactly at the cap: nothing is cut.
        let at_cap = OaxacaError::TooManyGroupValues {
            group_column: "Name".to_string(),
            reference_group: "Ann".to_string(),
            other_values: (1..=5).map(|i| format!("n{i}")).collect(),
        };
        assert_eq!(
            at_cap.to_string(),
            "TOO_MANY_GROUP_VALUES: column=Name, reference_group=\"Ann\", \
             distinct_other_values=5, other_values=[\"n1\", \"n2\", \"n3\", \"n4\", \"n5\"]"
        );

        // One over: the first five, then the remainder as a count.
        let many: Vec<String> = (1..=300).map(|i| format!("n{i:03}")).collect();
        let big = OaxacaError::TooManyGroupValues {
            group_column: "Name".to_string(),
            reference_group: "Ann".to_string(),
            other_values: many.clone(),
        };
        let msg = big.to_string();
        assert_eq!(
            msg,
            "TOO_MANY_GROUP_VALUES: column=Name, reference_group=\"Ann\", \
             distinct_other_values=300, \
             other_values=[\"n001\", \"n002\", \"n003\", \"n004\", \"n005\"] and 295 more"
        );
        assert!(!msg.contains("n006"), "{msg}");
        assert!(
            msg.len() < 300,
            "message must stay short: {} bytes",
            msg.len()
        );
        // The variant itself still carries all of them.
        match big {
            OaxacaError::TooManyGroupValues { other_values, .. } => {
                assert_eq!(other_values, many)
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_from_polars_error() {
        let polars_err = PolarsError::ColumnNotFound("missing".into());
        let oaxaca_err: OaxacaError = polars_err.into();
        assert!(matches!(oaxaca_err, OaxacaError::PolarsError(_)));
        // Prefix is ours; the rest is polars wording, which changes across versions.
        let msg = oaxaca_err.to_string();
        assert!(msg.starts_with("Polars error: "), "{msg}");
        assert!(msg.contains("missing"), "{msg}");
    }
}
