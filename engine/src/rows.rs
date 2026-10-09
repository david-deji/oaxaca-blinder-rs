//! Row accounting shared by the engine entry points (0118-MERIDIAN S3, S5).
//!
//! `OaxacaBuilder::get_data_matrices_with_rows` returns, for each group, the matrices and the
//! original row ordinal of every matrix row. Every place that turns a matrix row into an
//! employee (an `Adjustment.index`, a `row_key`, a wage lookup in the raw frame, a pooled-design
//! slot) reads that ordinal list. None of them recomputes a position from the raw group column,
//! because the matrices have already lost every row with a blank model cell.

use crate::row_key::{RowKeySource, RowKeyTable};
use crate::types::ExcludedRow;
use oaxaca_blinder::ExcludedRow as BuilderExcludedRow;
use polars::prelude::{CsvReader, DataFrame, SerReader};
use std::borrow::Cow;
use std::io::Cursor;

/// The wire text of a builder error from the data-matrix path.
///
/// The two 0118-MERIDIAN S2 refusals (`TOO_MANY_GROUP_VALUES`, `REFERENCE_GROUP_ABSENT`) carry
/// their own stable code prefix, and the app matches on it. Every engine entry point returns
/// them bare, whichever builder call trips them first, so the text a caller sees does not
/// depend on the entry point. Every other builder error keeps the long-standing
/// `Oaxaca Error: ` prefix this path has always added.
pub(crate) fn matrices_error(e: oaxaca_blinder::OaxacaError) -> String {
    use oaxaca_blinder::OaxacaError::{ReferenceGroupAbsent, TooManyGroupValues};
    match e {
        TooManyGroupValues { .. } | ReferenceGroupAbsent { .. } => e.to_string(),
        other => format!("Oaxaca Error: {}", other),
    }
}

/// Where an entry point gets its row-key table from when it reports excluded rows.
///
/// `optimize`, `verify_adjustments` and `check_defensibility` build the table up front because
/// they emit keys on every adjustment. `decompose` emits none, so it hands over the RAW parse
/// (before the Float64 cast, which changes a cell's text rendering and so its key) and the
/// table is built only if some row was actually excluded, keeping a clean run exactly as cheap
/// as before.
pub(crate) enum KeySupply<'a> {
    Ready(&'a RowKeyTable),
    FromRaw(&'a DataFrame),
}

impl KeySupply<'_> {
    pub(crate) fn excluded(&self, rows: &[BuilderExcludedRow]) -> Vec<ExcludedRow> {
        match self {
            KeySupply::Ready(table) => excluded_with_keys(rows, Some(table)),
            KeySupply::FromRaw(raw) => {
                if rows.is_empty() {
                    return Vec::new();
                }
                // A table that cannot be built costs the keys, never the analysis.
                let table = RowKeyTable::build(raw).ok();
                excluded_with_keys(rows, table.as_ref())
            }
        }
    }
}

/// Byte ranges of the physically blank lines in `csv`: lines that hold nothing but their line
/// terminator, found outside any quoted field.
///
/// The browser app reads the file with `skipEmptyLines: true`, so such a line is not a row there;
/// polars' CSV reader turns each one into an all-blank ROW. Left alone, every employee after a
/// blank line would be one ordinal higher in the engine than in the app's `csvData`, and an
/// `Adjustment.index` would name the neighbour. A line holding only spaces is NOT blank here, for
/// the same reason it is not blank to the app: it is a (mostly empty) data row in both.
fn blank_line_ranges(csv: &[u8]) -> Vec<(usize, usize)> {
    let n = csv.len();
    let mut ranges = Vec::new();
    let mut in_quotes = false;
    let mut at_line_start = true;
    let mut i = 0;
    while i < n {
        if at_line_start && !in_quotes {
            if csv[i] == b'\n' {
                ranges.push((i, i + 1));
                i += 1;
                continue;
            }
            if csv[i] == b'\r' && i + 1 < n && csv[i + 1] == b'\n' {
                ranges.push((i, i + 2));
                i += 2;
                continue;
            }
        }
        at_line_start = false;
        match csv[i] {
            // An escaped quote (`""`) toggles twice and leaves the state unchanged.
            b'"' => in_quotes = !in_quotes,
            b'\n' if !in_quotes => at_line_start = true,
            _ => {}
        }
        i += 1;
    }
    ranges
}

/// `csv` without its physically blank lines (0118-MERIDIAN S1). Borrowed, with no copy, when
/// there are none.
pub(crate) fn without_blank_lines(csv: &[u8]) -> Cow<'_, [u8]> {
    let ranges = blank_line_ranges(csv);
    if ranges.is_empty() {
        return Cow::Borrowed(csv);
    }
    let mut out = Vec::with_capacity(csv.len());
    let mut from = 0;
    for (start, end) in ranges {
        out.extend_from_slice(&csv[from..start]);
        from = end;
    }
    out.extend_from_slice(&csv[from..]);
    Cow::Owned(out)
}

/// Parses the CSV bytes an entry point was handed. Every entry point reads through here so a
/// row ordinal means the same thing on all of them: the zero-based position among the data rows
/// the browser app holds as `csvData`, blank lines skipped.
pub(crate) fn read_csv(csv: &[u8]) -> Result<DataFrame, String> {
    let df = match without_blank_lines(csv) {
        Cow::Borrowed(bytes) => CsvReader::new(Cursor::new(bytes)).finish(),
        Cow::Owned(bytes) => CsvReader::new(Cursor::new(bytes)).finish(),
    };
    df.map_err(|e| e.to_string())
}

/// Converts the builder's excluded rows to the wire type, adding each row's stable key when the
/// CSV carries an employee-number column.
///
/// `row_key` is populated only for the `column` key source. A content-hash key is a digest of
/// the row's own cells, which for an excluded row includes the very blank that excluded it, so
/// it is not an identity a consumer can use to find the employee.
pub(crate) fn excluded_with_keys(
    rows: &[BuilderExcludedRow],
    keys: Option<&RowKeyTable>,
) -> Vec<ExcludedRow> {
    rows.iter()
        .map(|r| ExcludedRow {
            index: r.index,
            row_key: keys
                .filter(|k| k.source() == RowKeySource::Column)
                .and_then(|k| k.key_at(r.index)),
            reasons: r.reasons.clone(),
            columns: r.columns.clone(),
        })
        .collect()
}

/// Internal alignment assertion (0118-MERIDIAN S3). The row ordinals and the matrices come from
/// one cleaning pass, so the counts cannot disagree; if they ever do, the pairing of employee
/// to dollars is unsafe and the run must fail instead of emitting a wrong figure.
///
/// `matrix_rows` is the design matrix's row count, `outcome_len` the outcome vector's length,
/// `ordinals` the length of the original-row-ordinal list for the same group.
pub(crate) fn check_alignment(
    group: &str,
    matrix_rows: usize,
    outcome_len: usize,
    ordinals: usize,
) -> Result<(), String> {
    if matrix_rows != outcome_len || matrix_rows != ordinals {
        return Err(format!(
            "Internal row alignment error: the {} group has {} design rows, {} outcome values \
             and {} original row ordinals. Refusing to pair employees with figures.",
            group, matrix_rows, outcome_len, ordinals
        ));
    }
    Ok(())
}

/// `true` at every ordinal that is an analysed row of either group.
pub(crate) fn analysed_mask(total_rows: usize, reference: &[usize], target: &[usize]) -> Vec<bool> {
    let mut mask = vec![false; total_rows];
    for &i in reference.iter().chain(target.iter()) {
        if i < total_rows {
            mask[i] = true;
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_accepts_equal_counts() {
        assert!(check_alignment("target", 5, 5, 5).is_ok());
        assert!(check_alignment("reference", 0, 0, 0).is_ok());
    }

    #[test]
    fn alignment_rejects_every_kind_of_disagreement() {
        // Fewer ordinals than matrix rows: the shape the old raw-position mapping produced in
        // reverse (more raw rows than matrix rows). Each leg must trip on its own.
        let ordinals_short = check_alignment("target", 5, 5, 4).unwrap_err();
        assert!(
            ordinals_short.contains("target group has 5 design rows, 5 outcome values and 4"),
            "{ordinals_short}"
        );
        assert!(check_alignment("target", 5, 5, 6).is_err());
        assert!(check_alignment("reference", 5, 4, 5).is_err());
        assert!(check_alignment("reference", 4, 5, 5).is_err());
    }

    #[test]
    fn blank_lines_are_removed_and_nothing_else() {
        let csv = b"a,b\n1,2\n\n3,4\n\n\n5,6\n\n";
        assert_eq!(&*without_blank_lines(csv), b"a,b\n1,2\n3,4\n5,6\n");
        // Leading blank lines before the header go too (the app's reader skips them).
        assert_eq!(&*without_blank_lines(b"\n\na,b\n1,2\n"), b"a,b\n1,2\n");
        // CRLF files.
        assert_eq!(
            &*without_blank_lines(b"a,b\r\n1,2\r\n\r\n3,4\r\n"),
            b"a,b\r\n1,2\r\n3,4\r\n"
        );
    }

    #[test]
    fn a_file_without_blank_lines_is_borrowed_not_copied() {
        let csv = b"a,b\n1,2\n3,4\n";
        assert!(matches!(without_blank_lines(csv), Cow::Borrowed(_)));
        assert!(matches!(without_blank_lines(b""), Cow::Borrowed(_)));
    }

    #[test]
    fn whitespace_only_and_all_blank_field_lines_are_data_rows_not_blank_lines() {
        // The app keeps both as rows, so the engine must too.
        let csv = b"a,b\n1,2\n  \n,\n3,4\n";
        assert!(matches!(without_blank_lines(csv), Cow::Borrowed(_)));
        let df = read_csv(csv).unwrap();
        assert_eq!(df.height(), 4);
    }

    #[test]
    fn a_blank_line_inside_a_quoted_field_is_part_of_the_field() {
        let csv = b"name,b\n\"line one\n\nline three\",2\n\n\"x\"\"\n\ny\",3\n";
        let cleaned = without_blank_lines(csv);
        assert_eq!(
            &*cleaned,
            b"name,b\n\"line one\n\nline three\",2\n\"x\"\"\n\ny\",3\n".as_slice()
        );
        assert_eq!(read_csv(csv).unwrap().height(), 2);
    }

    #[test]
    fn read_csv_gives_the_app_row_count_with_blank_lines_anywhere() {
        // polars alone would return 5 rows here (a blank line is an all-null row).
        let csv = b"a,b\n1,2\n\n3,4\n5,6\n\n";
        assert_eq!(read_csv(csv).unwrap().height(), 3);
    }

    #[test]
    fn analysed_mask_marks_exactly_the_listed_rows() {
        let mask = analysed_mask(6, &[0, 4], &[2, 5]);
        assert_eq!(mask, vec![true, false, true, false, true, true]);
        // An ordinal past the end is ignored rather than panicking.
        assert_eq!(analysed_mask(2, &[9], &[1]), vec![false, true]);
    }
}
