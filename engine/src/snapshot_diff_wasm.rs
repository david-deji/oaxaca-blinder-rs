//! The wasm-bindgen surface for `snapshot_diff` (0018-MERIDIAN phase 4).
//!
//! It lives in its own file rather than in `lib.rs` alongside `decompose`/`optimize` because
//! `snapshot_diff_stamp.rs` hashes this file's source: hashing `lib.rs` would make the freshness
//! digest move on every unrelated engine edit and the stamp would stop meaning "the diff
//! changed".
//!
//! ## Why columnar and not `serde_wasm_bindgen`
//!
//! Every other export in this crate takes and returns a `JsValue` through serde. That is right
//! for a request/response of a few hundred fields and wrong for 1.3M cells a week: serde would
//! allocate a `JsValue` per cell in both directions. Here the boundary carries a fixed number of
//! typed arrays per call — crossings are O(number of arrays), not O(cells).
//!
//! ## Why the values arrive TYPE-TAGGED rather than kind-classified
//!
//! See `snapshot_diff.rs`'s module header, branch 2. The JS side cannot decide which of the
//! `nums` / `texts` streams a cell belongs to without knowing the PRIOR cell's kind, which lives
//! here. So it sends `typeof value` and the value; every kind lookup and every type guard runs
//! in Rust, on the same branch the oracle runs it on.
//!
//! ## Lifetime
//!
//! `SnapshotDiffSession` is a wasm-bindgen struct: it is NOT garbage-collected from JS and needs
//! an explicit `free()`. A 130k-subject session that is never freed leaks tens of MB of linear
//! memory that wasm never returns to the host. `wasmDiff.js`'s `WasmPriorState.dispose()` is the
//! only correct way to end a session, on the success path AND on the abort path.

use wasm_bindgen::prelude::*;

use crate::snapshot_diff::{
    DiffOutput, RowBatch, SeedBatch, Session, ABSENT_LEN, CHANGE_ADDED, CHANGE_CHANGED,
    CHANGE_REMOVED,
};

/// The change wire for one call. One getter per column; JS reads each once and then `free()`s.
#[wasm_bindgen]
pub struct DiffOut {
    inner: DiffOutput,
}

#[wasm_bindgen]
impl DiffOut {
    #[wasm_bindgen(getter)]
    pub fn change_count(&self) -> u32 {
        self.inner.change_kinds.len() as u32
    }
    #[wasm_bindgen(getter)]
    pub fn subjects_concat(&self) -> String {
        self.inner.subjects_concat.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn subject_lens(&self) -> Vec<u32> {
        self.inner.subject_lens.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn change_subject_idx(&self) -> Vec<u32> {
        self.inner.change_subject_idx.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn change_target_ids(&self) -> Vec<u32> {
        self.inner.change_target_ids.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn change_kinds(&self) -> Vec<u8> {
        self.inner.change_kinds.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn change_value_kind_ids(&self) -> Vec<u32> {
        self.inner.change_value_kind_ids.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn old_nums(&self) -> Vec<f64> {
        self.inner.old_nums.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn new_nums(&self) -> Vec<f64> {
        self.inner.new_nums.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn old_texts_concat(&self) -> String {
        self.inner.old_texts_concat.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn old_text_lens(&self) -> Vec<u32> {
        self.inner.old_text_lens.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn new_texts_concat(&self) -> String {
        self.inner.new_texts_concat.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn new_text_lens(&self) -> Vec<u32> {
        self.inner.new_text_lens.clone()
    }
    /// `emptyDiffStats()`'s eight counters, in declaration order.
    #[wasm_bindgen(getter)]
    pub fn stats(&self) -> Vec<u32> {
        self.inner.stats.to_vec()
    }
}

/// The constants the JS decoder needs, exported so the two sides cannot drift on a magic number.
#[wasm_bindgen]
pub fn snapshot_diff_wire_constants() -> Vec<u32> {
    vec![
        CHANGE_ADDED as u32,
        CHANGE_CHANGED as u32,
        CHANGE_REMOVED as u32,
        ABSENT_LEN,
    ]
}

#[wasm_bindgen]
pub struct SnapshotDiffSession {
    inner: Session,
}

#[wasm_bindgen]
impl SnapshotDiffSession {
    #[wasm_bindgen(constructor)]
    pub fn new(subject_target_key: String) -> SnapshotDiffSession {
        SnapshotDiffSession {
            inner: Session::new(subject_target_key),
        }
    }

    /// Append target keys to the intern table. The JS side mints the same ids in the same order,
    /// so a change row's `target_key` resolves from a JS array with no string on the hot path.
    pub fn intern_targets(&mut self, concat: &str, lens: &[u32]) {
        let mut at = 0usize;
        for l in lens {
            let (b, n) = utf16_slice(concat, at, *l);
            self.inner.intern_target(&concat[b..b + n]);
            at = b + n;
        }
    }

    pub fn intern_kinds(&mut self, concat: &str, lens: &[u32]) {
        let mut at = 0usize;
        for l in lens {
            let (b, n) = utf16_slice(concat, at, *l);
            self.inner.intern_kind(&concat[b..b + n]);
            at = b + n;
        }
    }

    /// Pin `valueKinds`. Only truthy kinds reach here — an empty-string kind is left unpinned,
    /// matching `kindOf`'s `!kind` test.
    pub fn set_pinned(&mut self, target_ids: &[u32], kind_ids: &[u32]) {
        for i in 0..target_ids.len() {
            self.inner.set_pinned(target_ids[i], kind_ids[i]);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn seed(
        &mut self,
        subject_keys: &str,
        subject_key_lens: &[u32],
        subject_cell_counts: &[u32],
        cell_target_ids: &[u32],
        cell_kind_ids: &[u32],
        cell_tags: &[u8],
        cell_nums: &[f64],
        cell_texts: &str,
        cell_text_lens: &[u32],
    ) {
        let batch = SeedBatch {
            subject_keys,
            subject_key_lens,
            subject_cell_counts,
            cell_target_ids,
            cell_kind_ids,
            cell_tags,
            cell_nums,
            cell_texts,
            cell_text_lens,
        };
        self.inner.seed(&batch);
    }

    pub fn seal(&mut self) {
        self.inner.seal();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn diff_batch(
        &mut self,
        subject_keys: &str,
        subject_key_lens: &[u32],
        row_cell_counts: &[u32],
        cell_target_ids: &[u32],
        cell_tags: &[u8],
        cell_nums: &[f64],
        cell_texts: &str,
        cell_text_lens: &[u32],
    ) -> Result<DiffOut, JsValue> {
        let batch = RowBatch {
            subject_keys,
            subject_key_lens,
            row_cell_counts,
            cell_target_ids,
            cell_tags,
            cell_nums,
            cell_texts,
            cell_text_lens,
        };
        match self.inner.diff_batch(&batch) {
            Ok(out) => Ok(DiffOut { inner: out }),
            // The JSON carries class + message + details so `wasmDiff.js` can re-throw the REAL
            // `ValueKindError` / `ValueTypeError` imported from the oracle. A bare string would
            // make the caller catch a different constructor than the JS path throws.
            Err(e) => Err(JsValue::from_str(&e.to_json())),
        }
    }

    pub fn diff_vanished(&mut self) -> Result<DiffOut, JsValue> {
        match self.inner.diff_vanished() {
            Ok(out) => Ok(DiffOut { inner: out }),
            Err(e) => Err(JsValue::from_str(&e.to_json())),
        }
    }

    /// The measurable analogue of `PriorState.cellCount` — AC-11's shape assertion runs on it.
    #[wasm_bindgen(getter)]
    pub fn resident_cell_count(&self) -> u32 {
        self.inner.resident_cell_count() as u32
    }

    #[wasm_bindgen(getter)]
    pub fn initial_cell_count(&self) -> u32 {
        self.inner.initial_cell_count() as u32
    }

    /// `PriorState.size` — subjects not yet consumed.
    #[wasm_bindgen(getter)]
    pub fn subject_count(&self) -> u32 {
        self.inner.subject_count() as u32
    }
}

/// Byte range of the next `units` UTF-16 code units starting at byte offset `from`.
fn utf16_slice(s: &str, from: usize, units: u32) -> (usize, usize) {
    let mut remaining = units;
    let mut idx = from;
    if remaining > 0 {
        for c in s[from..].chars() {
            idx += c.len_utf8();
            remaining = remaining.saturating_sub(c.len_utf16() as u32);
            if remaining == 0 {
                break;
            }
        }
    }
    (from, idx - from)
}
