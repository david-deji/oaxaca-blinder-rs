//! Row accounting for the data matrices (0118-MERIDIAN S1).
//!
//! `OaxacaBuilder::clean_dataframe` drops every row that has a blank in a model column, so the
//! matrices the optimiser works on are SHORTER than the file and their row `i` is no longer file
//! row `i`. Anything that turns a matrix row back into an employee needs the original row
//! ordinal of each matrix row. Those ordinals are produced here, by the same single cleaning
//! pass that produces the matrices, so the two can never disagree.
//!
//! # Ordinals
//!
//! An ordinal is the zero-based position of a row among the PARSED DATA rows of the frame the
//! builder was given. For a CSV that is the position among the data rows after the header, with
//! physically blank lines skipped by the reader — the same list the browser app holds as
//! `store.csvData`.
//!
//! # Labels
//!
//! Groups are labelled `reference` and `target` here, never A/B. The legacy
//! `get_data_matrices` tuple is A = target (every non-reference value), B = reference.
//!
//! # Excluded rows
//!
//! A row is excluded when it has a blank in any column `clean_dataframe` checks. Each excluded
//! row appears once, however many columns are blank on it, with every blank column named and
//! the distinct reasons those columns fall under.

use nalgebra::{DMatrix, DVector};
use serde::Serialize;

/// Why a row was left out of the analysis. Serialises camelCase, matching the casing of the
/// engine's `RowKeySource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExclusionReason {
    /// Blank outcome (the wage).
    Outcome,
    /// Blank in a continuous predictor.
    NumericPredictor,
    /// Blank in a categorical predictor.
    CategoricalPredictor,
    /// Blank sample weight.
    Weights,
    /// Blank Heckman selection outcome.
    SelectionOutcome,
    /// Blank Heckman selection predictor.
    SelectionPredictor,
    /// Blank group value (the gender / comparison column).
    GroupValue,
}

/// One row left out of the analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedRow {
    /// Original row ordinal.
    pub index: usize,
    /// Distinct reasons, in the order `clean_dataframe` checks its columns.
    pub reasons: Vec<ExclusionReason>,
    /// Every blank column on this row, in the same order.
    pub columns: Vec<String>,
}

/// Which original rows were analysed and which were excluded, without the matrices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowAccounting {
    /// Rows in the frame the builder was given.
    pub total_rows: usize,
    /// Ordinals of the analysed reference-group rows, ascending.
    pub reference_rows: Vec<usize>,
    /// Ordinals of the analysed target-group rows, ascending.
    pub target_rows: Vec<usize>,
    /// Every excluded row, ascending by ordinal.
    pub excluded_rows: Vec<ExcludedRow>,
}

/// One group's design matrix and outcome with the original ordinal of each matrix row.
#[derive(Debug, Clone)]
pub struct GroupMatrices {
    pub x: DMatrix<f64>,
    pub y: DVector<f64>,
    /// `rows[i]` is the original ordinal of matrix row `i`. Same length as `y`.
    pub rows: Vec<usize>,
}

/// Matrices plus the row accounting, from one cleaning pass.
#[derive(Debug, Clone)]
pub struct DataMatricesWithRows {
    pub reference: GroupMatrices,
    pub target: GroupMatrices,
    pub predictor_names: Vec<String>,
    pub excluded_rows: Vec<ExcludedRow>,
    pub total_rows: usize,
}
