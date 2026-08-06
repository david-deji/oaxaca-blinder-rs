//! Stable row identity for adjustment rows — 0017-MERIDIAN P4.
//!
//! `Adjustment.index` is a POSITIONAL offset: the 0-based row ordinal of the parsed CSV
//! DataFrame (`analysis.rs` `optimize_inner` assigns `index: pot.orig_idx`, where `orig_idx`
//! comes from enumerating the group-column mask). It is correct as an offset — seven sites in
//! this crate use it to index a `Vec`, a polars `ChunkedArray`, or a matrix-row map — but it is
//! WRONG as an identity across a save boundary. Insert one row near the top of a corrected CSV
//! and every subsequent index shifts by one, silently re-attaching the consultant's persisted
//! overrides and CNESST justification narratives to different employees.
//!
//! This module mints a STRING key per DataFrame row, at the parse boundary, so identity stops
//! being a function of position. `index` is kept and unchanged; the key is purely additive.
//!
//! # Derivation (founder ruling, 0017-MERIDIAN P4)
//!
//! 1. **Employee-number column, when the CSV carries one.** The first column, in file order,
//!    whose normalized header is in `KEY_COLUMN_NAMES` AND whose values are all present,
//!    non-blank, at most `MAX_COLUMN_KEY_LEN` chars, and unique across every parsed row.
//!    Key = `c:<trimmed value>`.
//! 2. **Content-derived fallback, when no column qualifies.** SHA-256 over the row's full cell
//!    set — every column, name and value, ordered by column NAME (not by file position, so a
//!    column reorder cannot change the key) — truncated to `CONTENT_HASH_HEX_LEN` hex chars.
//!    Key = `h:<hex>#1`, minted ONLY when that digest occurs exactly once in the file.
//!
//! The fallback is NOT positional in any part: two rows swapping places keep their keys, and no
//! component of the key is a function of row order.
//!
//! # Why byte-identical rows get NO key
//!
//! Duplicate rows are ordinary in compiled Meridian CSVs (two job classes with the same
//! predominance, rate and points compile to identical records), so a content hash is not total.
//! An earlier revision closed that gap with a 1-based occurrence ordinal (`h:<hex>#2` for the
//! second row sharing a digest). That ordinal was assigned in row order, which put the P4 defect
//! back inside the key: prepend a third row with the same cell set to a corrected CSV and the
//! previously-`#2` row becomes `#3`, so an annotation keyed `h:<hex>#2` resolves cleanly — to a
//! different physical row — and `unresolved_row_keys` stays zero. Silent re-attachment with no
//! signal is precisely what this module exists to remove, so the ordinal is gone.
//!
//! There is no honest replacement. Two rows that are cell-for-cell indistinguishable carry
//! nothing outside their position that could tell them apart, so no stable key exists for either
//! one. `key_at` therefore returns `None` for every row whose digest is not unique in the file,
//! and `unkeyable_rows()` counts them so a caller can say so out loud. A `None` key is a visible
//! deferral to the positional `index` path; an order-dependent key is an invisible mis-attachment
//! on a figure that goes to the CNESST. The first is the correct failure direction — the same
//! fail-closed posture `resolve` takes on an unknown key.
//!
//! The `#1` suffix is retained on the keys that ARE minted: it keeps the wire format stable, and
//! every already-persisted key for an unambiguous row keeps resolving. Only keys that were never
//! trustworthy — the `#2`, `#3`, … of a duplicate cluster — stop resolving, and those surface as
//! counted orphans rather than as silent hits on the wrong employee.
//!
//! # Why the key is built BEFORE the Float64 cast
//!
//! Every entry point casts the outcome and predictor columns to `Float64` right after parsing.
//! Casting changes a cell's string rendering (`90000` -> `90000.0`), so a key derived after the
//! cast would depend on WHICH columns the operator selected as predictors — the same CSV
//! analyzed with a different predictor set would mint different keys and orphan every
//! annotation. Callers must build the table immediately after `CsvReader::finish()`.
//!
//! # Determinism
//!
//! Every map here is a `BTreeMap`/`BTreeSet`, never a `std` `HashMap` — the same rule P1
//! established in `defensibility.rs` (D14). Nothing in this module accumulates floats, but the
//! key table feeds row-keyed maps downstream and hash iteration order must never reach them.
//!
//! # Scope of stability
//!
//! Stable WITHIN a project (the ruling): the same CSV bytes always mint the same keys, and an
//! edit elsewhere in the file does not move an untouched row's key. Not globally unique and not
//! stable across different projects — two projects with the same employee numbers mint the same
//! keys, which is fine because a key is only ever resolved against its own project's CSV.

use polars::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Literal key-space discriminator emitted on every `OptimizationResult`. A reader that does
/// not recognise this value must REFUSE the keys rather than reinterpret them — the same
/// posture P2's `keySpace: 'adjustmentIndex'` guard takes on the persisted ledger block.
pub const ROW_KEY_SPACE: &str = "rowKeyV1";

/// A cell longer than this is not an employee number; the column is disqualified rather than
/// minting multi-kilobyte keys.
const MAX_COLUMN_KEY_LEN: usize = 128;

/// Hex chars kept from the SHA-256 digest. 24 hex = 96 bits: at 10,000 rows the birthday
/// collision probability is ~6e-19. Truncating further (16 hex / 64 bits) is unsafe here
/// because a digest collision between two DIFFERENT employees would read as a duplicate-content
/// cluster and silently un-key both of them, degrading identity to position for rows that in
/// fact had a perfectly good key.
const CONTENT_HASH_HEX_LEN: usize = 24;

/// Normalized headers accepted as an employee-number column. An explicit allow-list rather than
/// a regex: a regex like `/id$/i` would match `Location_Valid_Id` or a categorical predictor and
/// silently promote it to identity. Normalization strips accents and non-alphanumerics and
/// lowercases, so `No. Employé`, `employee_id` and `EmployeeID` all land here.
const KEY_COLUMN_NAMES: &[&str] = &[
    // Generic
    "id",
    "rowid",
    "recordid",
    "uid",
    "uuid",
    // EN employee
    "employeeid",
    "employeeno",
    "employeenum",
    "employeenumber",
    "empid",
    "empno",
    "empnum",
    "empnumber",
    "personid",
    "personnelid",
    "personnelno",
    "personnelnumber",
    "staffid",
    "staffnumber",
    // FR employee (accents stripped by `normalize_header`)
    "matricule",
    "nomatricule",
    "nummatricule",
    "matriculeemploye",
    "employeide",
    "idemploye",
    "noemploye",
    "numemploye",
    "numeroemploye",
    "numerodemploye",
    "nodemploye",
    // Job-class grain (compiled Meridian CSVs)
    "jobclassid",
    "classid",
    "idclasse",
    "codeclasse",
    "noclasse",
    "numeroclasse",
];

/// Which rule produced the keys in a `RowKeyTable`. Serializes to JS as `"column"` /
/// `"contentHash"`.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RowKeySource {
    /// Derived from an employee-number column (`row_key_column` names it).
    Column,
    /// Derived from the row's full cell set. No qualifying column existed.
    ContentHash,
}

/// Row ordinal -> stable key, plus the reverse index used to resolve an inbound key.
///
/// `keys[i]` is `None` when no stable key exists for that row — see the module docs on
/// byte-identical rows. It is dense over every parsed row either way, so `keys.len()` is the
/// DataFrame height and an ordinal is always in range or out of range, never ambiguous.
pub struct RowKeyTable {
    keys: Vec<Option<String>>,
    by_key: BTreeMap<String, usize>,
    source: RowKeySource,
    column: Option<String>,
}

impl RowKeyTable {
    /// Build the table from a freshly parsed DataFrame. Call immediately after
    /// `CsvReader::finish()`, before any `cast` — see the module docs.
    pub fn build(df: &DataFrame) -> Result<Self, String> {
        let n = df.height();

        // One string view per column, in file order. Built from the materialized series so the
        // rendering is the parsed value, not a re-parse of the raw bytes.
        let mut views: Vec<(String, Series)> = Vec::with_capacity(df.width());
        for col in df.get_columns() {
            let name = col.name().to_string();
            let as_str = col
                .as_materialized_series()
                .cast(&DataType::String)
                .map_err(|e| format!("row key: cannot render column '{}' as text: {}", name, e))?;
            views.push((name, as_str));
        }

        // Rule 1 — employee-number column, first qualifying in file order.
        for (name, series) in &views {
            if !KEY_COLUMN_NAMES.contains(&normalize_header(name).as_str()) {
                continue;
            }
            let Ok(ca) = series.str() else { continue };
            if let Some(keys) = qualify_column(ca, n) {
                // Rule 1 qualifies only on a fully unique, fully present column, so every row
                // is keyed. Widening to `Option` here is a shape change, not a policy change.
                let keys = keys.into_iter().map(Some).collect();
                return Ok(Self::finish(keys, RowKeySource::Column, Some(name.clone())));
            }
        }

        // Rule 2 — content-derived fallback.
        let keys = content_keys(&views, n)?;
        Ok(Self::finish(keys, RowKeySource::ContentHash, None))
    }

    fn finish(keys: Vec<Option<String>>, source: RowKeySource, column: Option<String>) -> Self {
        // BTreeMap, not HashMap (D14 precedent). First-wins: both derivations guarantee
        // uniqueness among the keys they DO mint, so the entry API here is a belt-and-braces
        // no-op rather than a policy. Unkeyed rows contribute no reverse entry, which is what
        // makes an inbound key for a duplicate-content row fail closed in `resolve`.
        let mut by_key: BTreeMap<String, usize> = BTreeMap::new();
        for (idx, key) in keys.iter().enumerate() {
            if let Some(key) = key {
                by_key.entry(key.clone()).or_insert(idx);
            }
        }
        Self {
            keys,
            by_key,
            source,
            column,
        }
    }

    /// The stable key for a DataFrame row ordinal.
    ///
    /// `None` in two cases, both meaning "do not key this row": the ordinal is out of range, or
    /// no stable key exists for it because its cell set is not unique in the file (module docs,
    /// § Why byte-identical rows get NO key). A caller emits the `None` verbatim — the wire
    /// contract already carries `row_key: Option<String>` — and falls back to `index` for that
    /// row, which is a visible deferral rather than a silent mis-attachment.
    pub fn key_at(&self, idx: usize) -> Option<String> {
        self.keys.get(idx).cloned().flatten()
    }

    /// Resolve an inbound proposed adjustment to a DataFrame row ordinal.
    ///
    /// - No `row_key` (every pre-P4 caller): trust `index`. Behaviour is byte-identical to the
    ///   pre-P4 engine, which is what keeps existing payloads and existing clients working.
    /// - `row_key` present and found: return ITS row, ignoring `index`. The key wins because
    ///   the key is the identity and the index is only an offset.
    /// - `row_key` present and NOT found: return `None`. The caller skips the adjustment and
    ///   counts it. Falling back to `index` here would silently apply a consultant's override
    ///   to whichever employee now occupies that position — the defect this module exists to
    ///   remove — so an unresolved key must fail closed, never fail over.
    pub fn resolve(&self, index: usize, row_key: Option<&str>) -> Option<usize> {
        match row_key {
            Some(k) if !k.is_empty() => self.by_key.get(k).copied(),
            _ => Some(index),
        }
    }

    pub fn source(&self) -> RowKeySource {
        self.source
    }

    pub fn column(&self) -> Option<String> {
        self.column.clone()
    }

    /// How many parsed rows got no stable key. Non-zero only on the content-hash path, and only
    /// when the CSV contains cell-for-cell duplicate rows; those rows fall back to the positional
    /// `index` and a caller should say so rather than presenting the project as fully keyed.
    pub fn unkeyable_rows(&self) -> usize {
        self.keys.iter().filter(|k| k.is_none()).count()
    }

    /// Total parsed rows, keyed or not.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Lowercase, strip accents, drop every non-alphanumeric character.
fn normalize_header(name: &str) -> String {
    name.chars()
        .map(deaccent)
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn deaccent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        other => other,
    }
}

/// `Some(keys)` when the column qualifies as an employee-number column: every row present,
/// non-blank after trimming, within the length cap, and unique. `None` on the first violation.
/// True when every value in the column is exactly its own row position, under either 0- or
/// 1-basing — i.e. the column is a row counter (`1,2,3…N` or `0,1,2…N-1`).
///
/// Such a column is position wearing an identity column's clothes. It is refused as the identity
/// key (`qualify_column`) AND excluded from the content digest (`content_keys`), because either
/// use would make a key move when a row is inserted above it. Both call sites share this one
/// definition so they can never drift apart and re-open the gap on one side only.
///
/// `n <= 1` is never a counter: a single row trivially matches both bases and refusing it would
/// throw away a perfectly good one-row identity.
fn is_row_counter(ca: &StringChunked, n: usize) -> bool {
    if n <= 1 {
        return false;
    }
    let mut zero_based = true;
    let mut one_based = true;
    for i in 0..n {
        match ca.get(i).map(|v| v.trim().parse::<usize>()) {
            Some(Ok(v)) => {
                if v != i {
                    zero_based = false;
                }
                if v != i + 1 {
                    one_based = false;
                }
            }
            // A null or non-integer cell means it is not a clean counter.
            _ => return false,
        }
        if !zero_based && !one_based {
            return false;
        }
    }
    zero_based || one_based
}

fn qualify_column(ca: &StringChunked, n: usize) -> Option<Vec<String>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut keys = Vec::with_capacity(n);
    for i in 0..n {
        let raw = ca.get(i)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_COLUMN_KEY_LEN {
            return None;
        }
        if !seen.insert(trimmed.to_string()) {
            return None;
        }
        keys.push(format!("c:{}", trimmed));
    }
    // ROW-COUNTER REJECTION (0017-P4 review, CONFIRMED HIGH).
    //
    // Presence + non-blank + length + uniqueness are all satisfied by a plain row counter — a
    // `No.` / `#` / `Ligne` column holding 1,2,3,…N. Such a column is positional identity wearing
    // an identity column's clothes: it passes every test above, so it was being promoted to the
    // stable key, and then a corrected CSV with one row inserted near the top renumbers every
    // subsequent row. Every override and every signed justification would move one employee down
    // — which is the exact defect this whole module exists to remove, reintroduced through the
    // rule meant to fix it, and reintroduced specifically for the commonest shape of exported CSV.
    //
    // A genuine employee-number column that happens to run 1..N in file order is refused too.
    // That is the correct direction to fail: refusing falls through to the whole-row content hash,
    // which is stable under insertion and reordering. Accepting a counter cannot be detected later
    // and mis-attributes silently. Cheap and reversible versus silent and load-bearing.
    if is_row_counter(ca, n) {
        return None;
    }
    Some(keys)
}

/// SHA-256 over the row's full cell set, columns ordered by NAME.
///
/// Hashing the FULL row, not an "identity-bearing" subset, is deliberate: a whole-row digest
/// makes any edit mint a NEW key, so a stale annotation surfaces as an explicit orphan the
/// ledger keeps and flags, rather than silently attaching to a different employee. A subset
/// hash is more stable but collides across two employees who share the subset — the wrong
/// failure direction for a figure that goes to the CNESST.
///
/// Two passes, not one: the digests are computed for every row first, then a row is keyed only
/// if its digest is unique across the whole file. A single streaming pass could only disambiguate
/// duplicates by the order it met them, which is the positional dependency this module removes.
fn content_keys(views: &[(String, Series)], n: usize) -> Result<Vec<Option<String>>, String> {
    // Name-ordered column traversal: a column reorder in a corrected CSV must not move keys.
    // BTreeMap for the ordering, per D14.
    let mut order: BTreeMap<&str, usize> = BTreeMap::new();
    for (pos, (name, _)) in views.iter().enumerate() {
        order.entry(name.as_str()).or_insert(pos);
    }

    let mut chunked: Vec<(&str, &StringChunked)> = Vec::with_capacity(order.len());
    for (name, pos) in &order {
        let ca = views[*pos]
            .1
            .str()
            .map_err(|e| format!("row key: column '{}' is not text after cast: {}", name, e))?;
        chunked.push((name, ca));
    }

    // ROW-COUNTER EXCLUSION (0017-P4, found by the consequence test of the qualify_column fix).
    //
    // Refusing a row counter as the IDENTITY column is necessary but not sufficient: this hash is
    // over the full row, so a counter column's VALUE still enters every digest. A corrected CSV
    // with one row inserted near the top renumbers that column, which changes the cell set of
    // every row below the insertion, which mints a brand-new key for each of them — so every
    // annotation below the insertion orphans at once. The counter was refused as identity and
    // then smuggled its positional dependence into the fallback that exists precisely to be free
    // of it.
    //
    // A column whose values are exactly the row ordinal carries no information about the row
    // beyond where it sits, so dropping it from the digest loses no discriminating power: two
    // rows distinguished ONLY by their counter are, by definition, cell-for-cell identical
    // otherwise, and the pass-2 uniqueness rule already refuses to key those. Fail-closed either
    // way, and this way an untouched row keeps its key.
    let counter_columns: BTreeSet<&str> = chunked
        .iter()
        .filter(|(_, ca)| is_row_counter(ca, n))
        .map(|(name, _)| *name)
        .collect();

    // Pass 1 — digest every row, and count how many rows share each digest.
    let mut digests: Vec<String> = Vec::with_capacity(n);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    for i in 0..n {
        let mut hasher = Sha256::new();
        for (name, ca) in &chunked {
            if counter_columns.contains(*name) {
                continue;
            }
            hasher.update(name.as_bytes());
            hasher.update([0x1F]); // unit separator: name | value
            match ca.get(i) {
                // Tag present-vs-null so an empty cell and a null cell cannot collide.
                Some(v) => {
                    hasher.update([0x01]);
                    hasher.update(v.as_bytes());
                }
                None => hasher.update([0x00]),
            }
            hasher.update([0x1E]); // record separator: end of field
        }
        let digest = hasher.finalize();
        let mut hex = String::with_capacity(CONTENT_HASH_HEX_LEN);
        for byte in digest.iter().take(CONTENT_HASH_HEX_LEN / 2) {
            hex.push_str(&format!("{:02x}", byte));
        }
        *counts.entry(hex.clone()).or_insert(0) += 1;
        digests.push(hex);
    }

    // Pass 2 — key only the rows whose cell set is unique in this file. A row sharing its digest
    // with any other row has no stable identity to mint (module docs), so it is left unkeyed
    // rather than disambiguated by the order the rows happen to appear in.
    let keys = digests
        .into_iter()
        .map(|hex| match counts.get(&hex) {
            Some(1) => Some(format!("h:{}#1", hex)),
            _ => None,
        })
        .collect();

    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse(csv: &str) -> DataFrame {
        CsvReader::new(Cursor::new(csv.as_bytes().to_vec()))
            .finish()
            .unwrap()
    }

    const WITH_ID: &str = "employee_id,wage,gender,education\n\
                           E-101,50000,Male,12\n\
                           E-102,40000,Female,12\n\
                           E-103,60000,Male,14\n";

    const NO_ID: &str = "wage,gender,education\n\
                         50000,Male,12\n\
                         40000,Female,12\n\
                         60000,Male,14\n";

    // ── Row-counter rejection (0017-P4 review, CONFIRMED HIGH) ────────────────────────────
    //
    // A `No.` column holding 1,2,3…N passes presence + non-blank + length + uniqueness, so it was
    // being promoted to stable identity. It is positional identity in disguise: insert a row and
    // the renumbering moves every annotation one employee down. These pin the refusal AND the
    // consequence — the same file with a row inserted must keep the original rows' keys.

    #[test]
    fn a_one_based_row_counter_column_is_refused() {
        let df = parse("employee_id,wage,gender\n1,50000,Male\n2,40000,Female\n3,60000,Male\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(
            t.source(),
            RowKeySource::ContentHash,
            "1,2,3 is a row counter, not an identity column"
        );
    }

    #[test]
    fn a_zero_based_row_counter_column_is_refused() {
        let df = parse("id,wage,gender\n0,50000,Male\n1,40000,Female\n2,60000,Male\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(t.source(), RowKeySource::ContentHash);
    }

    #[test]
    fn a_non_sequential_numeric_id_column_is_still_accepted() {
        // The refusal must be narrow: real employee numbers are numeric but not row-positional.
        let df = parse("No. Employe,wage,gender\n4102,50000,Male\n4110,40000,Female\n4137,60000,Male\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(t.source(), RowKeySource::Column);
        assert_eq!(t.key_at(0).unwrap(), "c:4102");
    }

    #[test]
    fn refusing_a_row_counter_keeps_keys_stable_across_an_inserted_row() {
        // The whole point. With the counter refused, both files fall to the content hash, so the
        // two original rows keep their keys even though their positions and their counter values
        // both shifted. Had the counter been accepted, row "2" would be a different employee.
        let before = parse("employee_id,wage,gender\n1,50000,Male\n2,40000,Female\n");
        let after = parse("employee_id,wage,gender\n1,99000,Other\n2,50000,Male\n3,40000,Female\n");
        let tb = RowKeyTable::build(&before).unwrap();
        let ta = RowKeyTable::build(&after).unwrap();
        assert_eq!(tb.key_at(0), ta.key_at(1), "the 50000/Male row keeps its key");
        assert_eq!(tb.key_at(1), ta.key_at(2), "the 40000/Female row keeps its key");
    }

    #[test]
    fn employee_number_column_is_preferred() {
        let t = RowKeyTable::build(&parse(WITH_ID)).unwrap();
        assert_eq!(t.source(), RowKeySource::Column);
        assert_eq!(t.column().as_deref(), Some("employee_id"));
        assert_eq!(t.key_at(0).unwrap(), "c:E-101");
        assert_eq!(t.key_at(2).unwrap(), "c:E-103");
    }

    #[test]
    fn accented_and_spaced_french_headers_are_detected() {
        let df = parse("No. Employé,wage,gender\n7,50000,Male\n8,40000,Female\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(t.source(), RowKeySource::Column);
        assert_eq!(t.column().as_deref(), Some("No. Employé"));
        assert_eq!(t.key_at(0).unwrap(), "c:7");
    }

    #[test]
    fn duplicate_values_disqualify_the_column() {
        let df = parse("employee_id,wage,gender\nE-1,50000,Male\nE-1,40000,Female\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(t.source(), RowKeySource::ContentHash);
        assert!(t.column().is_none());
    }

    #[test]
    fn blank_values_disqualify_the_column() {
        let df = parse("employee_id,wage,gender\nE-1,50000,Male\n,40000,Female\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(t.source(), RowKeySource::ContentHash);
    }

    #[test]
    fn a_categorical_column_is_never_promoted_to_identity() {
        // `Location` is unique here but is not an employee-number header; the allow-list must
        // refuse it rather than a regex accidentally matching.
        let df = parse("Location,wage,gender\nAustin,50000,Male\nBoston,40000,Female\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(t.source(), RowKeySource::ContentHash);
    }

    #[test]
    fn fallback_keys_are_not_positional() {
        // Same three rows, first and last swapped. A positional key would move; a content key
        // does not.
        let a = RowKeyTable::build(&parse(NO_ID)).unwrap();
        let swapped = "wage,gender,education\n\
                       60000,Male,14\n\
                       40000,Female,12\n\
                       50000,Male,12\n";
        let b = RowKeyTable::build(&parse(swapped)).unwrap();
        assert_eq!(a.source(), RowKeySource::ContentHash);
        assert_eq!(a.key_at(0).unwrap(), b.key_at(2).unwrap());
        assert_eq!(a.key_at(2).unwrap(), b.key_at(0).unwrap());
    }

    #[test]
    fn fallback_keys_survive_a_row_inserted_at_the_top() {
        // The P4 defect scenario. Every `index` shifts by one; every key must not.
        let before = RowKeyTable::build(&parse(NO_ID)).unwrap();
        let after_csv = "wage,gender,education\n\
                         99000,Male,20\n\
                         50000,Male,12\n\
                         40000,Female,12\n\
                         60000,Male,14\n";
        let after = RowKeyTable::build(&parse(after_csv)).unwrap();
        for i in 0..3 {
            assert_eq!(before.key_at(i).unwrap(), after.key_at(i + 1).unwrap());
        }
    }

    #[test]
    fn column_keys_survive_a_row_inserted_at_the_top() {
        let before = RowKeyTable::build(&parse(WITH_ID)).unwrap();
        let after_csv = "employee_id,wage,gender,education\n\
                         E-999,99000,Male,20\n\
                         E-101,50000,Male,12\n\
                         E-102,40000,Female,12\n\
                         E-103,60000,Male,14\n";
        let after = RowKeyTable::build(&parse(after_csv)).unwrap();
        for i in 0..3 {
            assert_eq!(before.key_at(i).unwrap(), after.key_at(i + 1).unwrap());
        }
    }

    #[test]
    fn fallback_keys_are_column_order_independent() {
        let a = RowKeyTable::build(&parse(NO_ID)).unwrap();
        let reordered = "education,wage,gender\n\
                         12,50000,Male\n\
                         12,40000,Female\n\
                         14,60000,Male\n";
        let b = RowKeyTable::build(&parse(reordered)).unwrap();
        assert_eq!(a.key_at(0).unwrap(), b.key_at(0).unwrap());
        assert_eq!(a.key_at(1).unwrap(), b.key_at(1).unwrap());
    }

    /// Byte-identical rows carry nothing but their position to tell them apart, so no stable key
    /// exists for any of them. The table says so instead of minting an order-dependent one.
    #[test]
    fn byte_identical_rows_get_no_key_at_all() {
        let df = parse("wage,gender,education\n50000,Male,12\n50000,Male,12\n50000,Male,12\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(t.source(), RowKeySource::ContentHash);
        assert_eq!(t.len(), 3);
        for i in 0..3 {
            assert!(t.key_at(i).is_none(), "row {} should be unkeyed", i);
        }
        assert_eq!(t.unkeyable_rows(), 3);
    }

    /// A duplicate cluster does not un-key the rest of the file: the unique rows keep their keys,
    /// and the count of unkeyable rows is exactly the cluster.
    #[test]
    fn a_duplicate_cluster_does_not_un_key_the_unique_rows() {
        let df = parse(
            "wage,gender,education\n\
             50000,Male,12\n\
             50000,Male,12\n\
             40000,Female,12\n\
             60000,Male,14\n",
        );
        let t = RowKeyTable::build(&df).unwrap();
        assert!(t.key_at(0).is_none());
        assert!(t.key_at(1).is_none());
        assert!(t.key_at(2).unwrap().starts_with("h:"));
        assert!(t.key_at(3).unwrap().starts_with("h:"));
        assert_eq!(t.unkeyable_rows(), 2);
        assert_ne!(t.key_at(2).unwrap(), t.key_at(3).unwrap());
    }

    /// The MEDIUM finding, measured. Under the old occurrence-ordinal derivation, prepending a
    /// third row with the same cell set renumbered the duplicate cluster, so an annotation keyed
    /// to the old `#2` resolved cleanly onto a DIFFERENT physical row and nothing warned. No key
    /// may now change which row it denotes when a byte-identical row is inserted ahead of it.
    #[test]
    fn inserting_a_byte_identical_row_retargets_no_key() {
        let before_csv = "wage,gender,education\n\
                          50000,Male,12\n\
                          50000,Male,12\n\
                          41000,Female,12\n";
        let after_csv = "wage,gender,education\n\
                         50000,Male,12\n\
                         50000,Male,12\n\
                         50000,Male,12\n\
                         41000,Female,12\n";
        let before = RowKeyTable::build(&parse(before_csv)).unwrap();
        let after = RowKeyTable::build(&parse(after_csv)).unwrap();

        // The duplicate cluster is unkeyed on both sides, so there is no key whose meaning the
        // insert could have moved.
        for i in 0..2 {
            assert!(before.key_at(i).is_none());
        }
        for i in 0..3 {
            assert!(after.key_at(i).is_none());
        }

        // Every key the OLD file minted still denotes the same row's content in the NEW file.
        // The untouched Female row shifted from index 2 to index 3 and kept its key.
        let female = before.key_at(2).unwrap();
        assert_eq!(after.key_at(3).unwrap(), female);
        assert_eq!(after.resolve(0, Some(&female)), Some(3));

        // And the key that the old derivation would have issued for the second duplicate
        // (`...#2`) resolves nowhere, rather than landing on a row that is not the one it was
        // written against. Fail closed, exactly like an unknown key.
        let stale_occurrence_key = format!("{}#2", female.trim_end_matches("#1"));
        assert_eq!(after.resolve(1, Some(&stale_occurrence_key)), None);
    }

    /// Migration guarantee: a key minted for an unambiguous row keeps the `#1` wire shape, so
    /// every already-persisted annotation on a unique row still resolves after this change.
    #[test]
    fn unique_content_keys_keep_the_occurrence_one_wire_shape() {
        let t = RowKeyTable::build(&parse(NO_ID)).unwrap();
        assert_eq!(t.unkeyable_rows(), 0);
        for i in 0..3 {
            let k = t.key_at(i).unwrap();
            assert!(k.starts_with("h:"), "expected content key, got {}", k);
            assert!(k.ends_with("#1"), "expected #1 suffix, got {}", k);
        }
        let unique: BTreeSet<String> = (0..3).map(|i| t.key_at(i).unwrap()).collect();
        assert_eq!(unique.len(), 3);
    }

    /// An unkeyed row must not be reachable by key from either direction: it contributes no
    /// reverse-index entry, so no inbound key can resolve onto it.
    #[test]
    fn an_unkeyed_row_is_not_reachable_by_any_key() {
        let df = parse(
            "wage,gender,education\n\
             50000,Male,12\n\
             50000,Male,12\n\
             41000,Female,12\n",
        );
        let t = RowKeyTable::build(&df).unwrap();
        let keyed: BTreeSet<usize> = (0..t.len())
            .filter_map(|i| t.key_at(i).map(|k| t.resolve(usize::MAX, Some(&k)).unwrap()))
            .collect();
        assert_eq!(keyed, BTreeSet::from([2]));
        // The legacy no-key path still returns the caller's index untouched for those rows.
        assert_eq!(t.resolve(1, None), Some(1));
    }

    #[test]
    fn build_is_deterministic_across_runs() {
        let df = parse(NO_ID);
        let a = RowKeyTable::build(&df).unwrap();
        let b = RowKeyTable::build(&df).unwrap();
        for i in 0..df.height() {
            assert_eq!(a.key_at(i).unwrap(), b.key_at(i).unwrap());
        }
    }

    #[test]
    fn resolve_without_a_key_is_the_legacy_index_path() {
        let t = RowKeyTable::build(&parse(WITH_ID)).unwrap();
        assert_eq!(t.resolve(2, None), Some(2));
        assert_eq!(t.resolve(2, Some("")), Some(2));
        // Out of range is still returned verbatim: bounds are the caller's contract, unchanged.
        assert_eq!(t.resolve(99, None), Some(99));
    }

    #[test]
    fn resolve_prefers_the_key_over_a_stale_index() {
        let t = RowKeyTable::build(&parse(WITH_ID)).unwrap();
        assert_eq!(t.resolve(0, Some("c:E-103")), Some(2));
    }

    #[test]
    fn resolve_fails_closed_on_an_unknown_key() {
        let t = RowKeyTable::build(&parse(WITH_ID)).unwrap();
        assert_eq!(t.resolve(0, Some("c:E-404")), None);
    }

    #[test]
    fn keys_are_dense_over_every_parsed_row() {
        let t = RowKeyTable::build(&parse(NO_ID)).unwrap();
        assert_eq!(t.len(), 3);
        assert!(t.key_at(3).is_none());
    }

    #[test]
    fn a_null_cell_and_an_empty_cell_do_not_collide() {
        // Row 1 has an empty education cell; row 2 has the literal text "". polars renders the
        // quoted empty field as an empty string and the bare one as null, so the tagging in
        // `content_keys` must keep them apart.
        let df = parse("wage,gender,education\n50000,Male,\n50000,Male,\"\"\n");
        let t = RowKeyTable::build(&df).unwrap();
        assert_eq!(t.len(), 2);
        // A digest collision now un-keys BOTH rows rather than separating them with an
        // occurrence ordinal, so "did not collide" is exactly "both rows are keyed". Assert that
        // first: it names the failure instead of surfacing as an unwrap panic on a None key.
        assert_eq!(t.unkeyable_rows(), 0, "the null/empty tagging collided");
        assert_ne!(t.key_at(0).unwrap(), t.key_at(1).unwrap());
    }
}
