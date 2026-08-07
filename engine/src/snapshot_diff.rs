//! Week-over-week field-level diff — the Rust half of 0018-MERIDIAN phase 4.
//!
//! This module is the implementation; `frontend/src/services/temporal/diffSnapshots.js` in the
//! Meridian app is the ORACLE. The two must produce byte-identical output over the same input,
//! in the same order, including the text of every error they raise. Everything odd-looking in
//! here is odd because the oracle is odd, and the oracle is the contract.
//!
//! ## The four branches that must be reproduced exactly
//!
//! Reading `diffSnapshots.js:296-356`, an incoming cell takes ONE of four paths and the paths do
//! measurably different work. Getting this wrong is invisible under a small corpus because the
//! divergent branches only fire on inputs a naive fixture set never builds:
//!
//! | # | Condition | Kind used | Type guard run? |
//! |---|---|---|---|
//! | 1 | no prior cell | the PINNED kind (`priorState.kindOf`) — throws `ValueKindError` if unpinned | yes, on the new value |
//! | 2 | prior cell, `priorCell.value === nextValue` | none looked up | **no** — `continue` at `:322` fires BEFORE any `splitValue` |
//! | 3 | prior cell, values differ | `priorCell.value_kind` — the CELL's kind, never the pinned one | yes, OLD value first (`:324`), then new |
//! | 4 | prior cell absent from the row | `priorCell.value_kind` | yes, on the old value |
//!
//! Branch 2 is the one that kills a "classify every incoming cell by its pinned kind on the JS
//! side" encoder: a prior cell may carry a target key that is no longer pinned, or a `value_kind`
//! that disagrees with the pinned one, and as long as the value did not change the oracle emits
//! nothing and raises nothing. An encoder that resolved the pinned kind per cell would throw
//! where the oracle is silent. That is why the JS side of this port sends VALUES WITH THEIR JS
//! TYPE TAG and nothing else, and every kind lookup, every equality test and every type guard
//! happens here, where the prior cell's own kind is known.
//!
//! ## Equality is `==` on `f64`, never `to_bits()`
//!
//! `diffSnapshots.js:26-38`: the diff answers "did the employer change this cell" against
//! SQLite's notion of distinctness. `(-0.0f64) == 0.0f64` is `true`, matching JS `===`.
//! `f64::to_bits()` would separate them, emit a `('changed', -0, 0)` row, and the ingest's
//! `CHECK (change_kind <> 'changed' OR old_text IS NOT new_text OR old_num IS NOT new_num)`
//! would abort an entire week over a float sign. Strings compare as UTF-8 bytes — no trim, no
//! case folding, no normalisation; any of those would make two genuinely different values
//! compare equal and SUPPRESS a real change, which errors nowhere.
//!
//! ## No rayon
//!
//! The diff is a sequential, memory-bound stream, and the whole parity argument rests on ONE
//! emission order. Threading it would let the sequential and threaded artifacts diverge in the
//! only property this phase asserts. There is no `rayon`, no `par_iter`, no `par_bridge` in this
//! file or in `snapshot_diff_wasm.rs`, and `snapshot_diff_tests.rs` asserts that by reading the
//! source text so it fails under `cargo test` rather than depending on someone running a grep.

use std::collections::HashMap;
use std::mem;

// ── wire tags: the JS `typeof` of an incoming value ──────────────────────────────────────────
//
// The JS encoder sends `typeof value`, not a kind. Tags 0 and 1 consume from the `nums` / `texts`
// cursors; every other tag consumes nothing.
pub const TAG_NUMBER: u8 = 0;
pub const TAG_STRING: u8 = 1;
pub const TAG_UNDEFINED: u8 = 2;
pub const TAG_NULL: u8 = 3;
pub const TAG_FALSE: u8 = 4;
pub const TAG_TRUE: u8 = 5;
pub const TAG_OBJECT: u8 = 6;
pub const TAG_BIGINT: u8 = 7;
pub const TAG_SYMBOL: u8 = 8;
pub const TAG_FUNCTION: u8 = 9;

/// `change_kind` codes, decoded back to `'added' | 'changed' | 'removed'` on the JS side.
pub const CHANGE_ADDED: u8 = 0;
pub const CHANGE_CHANGED: u8 = 1;
pub const CHANGE_REMOVED: u8 = 2;

/// A text length of `u32::MAX` means the side is ABSENT (decodes to `null`), which is how
/// absence-is-not-empty-string survives the columnar encoding. The numeric counterpart is `NaN`,
/// sound only because normalize.js guarantees `number|money` values are always finite
/// (`diffSnapshots.js:36-38`).
pub const ABSENT_LEN: u32 = u32::MAX;

/// A JS value, as faithfully as a typed arena can hold one.
///
/// `Opaque` covers `object` / `bigint` / `symbol` / `function`. Each gets a fresh id at decode
/// time, so two opaque values NEVER compare equal. JS `===` on two references to the same object
/// is `true`, and this does not model that — unreachable in production (normalize.js's values
/// contract is string-or-finite-number) and documented rather than silently assumed.
#[derive(Clone, Copy, Debug)]
pub enum Val {
    Num(f64),
    /// Byte range into the arena that owns this value.
    Text { start: u32, len: u32 },
    Undefined,
    Null,
    Bool(bool),
    Opaque { tag: u8, id: u32 },
}

impl Val {
    /// The JS `typeof` of this value — it lands verbatim in `ValueTypeError`'s message.
    pub fn type_of(&self) -> &'static str {
        match self {
            Val::Num(_) => "number",
            Val::Text { .. } => "string",
            Val::Undefined => "undefined",
            Val::Null => "object",
            Val::Bool(_) => "boolean",
            Val::Opaque { tag, .. } => match *tag {
                TAG_BIGINT => "bigint",
                TAG_SYMBOL => "symbol",
                TAG_FUNCTION => "function",
                _ => "object",
            },
        }
    }
}

/// JS `===`. Distinct arenas because a prior cell's text lives in the session arena and an
/// incoming cell's text lives in the batch buffer.
fn js_strict_eq(a: &Val, a_arena: &str, b: &Val, b_arena: &str) -> bool {
    match (a, b) {
        // `==` and not `to_bits()`: see the module header. NaN != NaN falls out correctly.
        (Val::Num(x), Val::Num(y)) => x == y,
        (Val::Text { start: s1, len: l1 }, Val::Text { start: s2, len: l2 }) => {
            let x = &a_arena[*s1 as usize..(*s1 + *l1) as usize];
            let y = &b_arena[*s2 as usize..(*s2 + *l2) as usize];
            x == y
        }
        (Val::Undefined, Val::Undefined) => true,
        (Val::Null, Val::Null) => true,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        (Val::Opaque { id: x, .. }, Val::Opaque { id: y, .. }) => x == y,
        _ => false,
    }
}

// ── errors ───────────────────────────────────────────────────────────────────────────────────

/// The two error classes the diff itself can raise. `SubjectKeyError` is NOT here: it is raised
/// by `readSubjectKey` on the JS side, against the row object, before anything crosses.
///
/// The message templates below are byte-copies of `diffSnapshots.js`. They are duplicated rather
/// than shared because the oracle exports neither `readSubjectKey` nor its message strings, and
/// P4-AC-1 forbids editing it to add an `export`. `wasmDiffParity.spec.js` asserts the templates
/// in this file and in `wasmDiff.js` byte-match the ones in the oracle, so the duplication is
/// checked by a test rather than by a promise.
#[derive(Debug, Clone)]
pub enum DiffError {
    /// `diffSnapshots.js:168-172` — a target key with no declared `value_kind`.
    ValueKind { target_key: String },
    /// `diffSnapshots.js:99-103` — a `number`/`money` cell whose value is not a finite JS number.
    ValueTypeNumeric {
        value_kind: String,
        value_type: &'static str,
    },
    /// `diffSnapshots.js:108-113` — a text-kind cell whose value is not a string.
    ValueTypeText {
        value_kind: String,
        value_type: &'static str,
    },
}

impl DiffError {
    pub fn class(&self) -> &'static str {
        match self {
            DiffError::ValueKind { .. } => "ValueKindError",
            _ => "ValueTypeError",
        }
    }

    pub fn message(&self) -> String {
        match self {
            DiffError::ValueKind { target_key } => format!(
                "diffSnapshots: no value_kind declared for target_key {}. \
The kind is pinned from the profile at ingest time and is never derived from the value.",
                json_quote(target_key)
            ),
            DiffError::ValueTypeNumeric {
                value_kind,
                value_type,
            } => format!(
                "diffSnapshots: value_kind {} requires a finite JS number, got {}. \
Binding it would affinity-convert on the way into value_num (spec §11).",
                json_quote(value_kind),
                value_type
            ),
            DiffError::ValueTypeText {
                value_kind,
                value_type,
            } => format!(
                "diffSnapshots: value_kind {} requires a string, got {}. \
Binding a JS number to a TEXT column is §11's named silent failure: 52000 goes in, \
'52000' comes out.",
                json_quote(value_kind),
                value_type
            ),
        }
    }

    /// The `details` object the JS error class re-attaches via `Object.assign`.
    pub fn details_json(&self) -> String {
        match self {
            DiffError::ValueKind { target_key } => {
                format!("{{\"targetKey\":{}}}", json_quote(target_key))
            }
            DiffError::ValueTypeNumeric {
                value_kind,
                value_type,
            }
            | DiffError::ValueTypeText {
                value_kind,
                value_type,
            } => format!(
                "{{\"valueKind\":{},\"valueType\":{}}}",
                json_quote(value_kind),
                json_quote(value_type)
            ),
        }
    }

    /// One JSON string carrying class + message + details. The JS binding parses it and
    /// re-throws the real `ValueKindError` / `ValueTypeError` imported from the oracle, so the
    /// caller catches the same constructor it would catch from the JS implementation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"class\":{},\"message\":{},\"details\":{}}}",
            json_quote(self.class()),
            json_quote(&self.message()),
            self.details_json()
        )
    }
}

/// `JSON.stringify` of a string, for the error templates. Rust `&str` is always well-formed
/// UTF-8, so the lone-surrogate branch of ES2019 well-formed stringify is unreachable here.
fn json_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── interning ────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct KindDef {
    pub name: String,
    /// `diffSnapshots.js:78-80` — the exact predicate, computed once at intern time.
    pub is_numeric: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub target_id: u32,
    pub kind_id: u32,
    pub val: Val,
}

/// One subject's slot. Kept in SEED ORDER with a tombstone rather than removed, because
/// `diffVanishedSubjects` must emit what remains in the prior map's insertion order and JS
/// `Map.delete` preserves that order for the survivors. `swap_remove` destroys it and
/// `shift_remove` is O(n) at 130k subjects; a bool preserves it at O(1).
struct SubjectSlot {
    key: String,
    taken: bool,
    cells: Vec<Cell>,
}

// ── the columnar wire formats ────────────────────────────────────────────────────────────────
//
// All `*_lens` are in UTF-16 CODE UNITS, not bytes. That is deliberate: the JS encoder gets a
// UTF-16 length from `String.prototype.length` in O(1), whereas a UTF-8 byte length would cost a
// TextEncoder pass per cell. Rust walks the concatenated string once per batch converting units
// to byte offsets, which is the same order of work wasm-bindgen already does validating it.
//
// A lone surrogate is 1 UTF-16 unit on the JS side and arrives as U+FFFD, which is also 1 UTF-16
// unit — so the counts stay aligned even for the one input JS can express and UTF-8 cannot.

/// The seed wire: the prior week, pushed in chunks so the JS side never has to hold a second copy.
pub struct SeedBatch<'a> {
    pub subject_keys: &'a str,
    pub subject_key_lens: &'a [u32],
    /// One per subject; how many cells of the flat arrays belong to it.
    pub subject_cell_counts: &'a [u32],
    pub cell_target_ids: &'a [u32],
    /// Seeded cells carry their OWN kind (`priorCell.value_kind`), which may differ from the
    /// pinned kind or not be pinned at all. Branch 3/4 of the module header depends on it.
    pub cell_kind_ids: &'a [u32],
    pub cell_tags: &'a [u8],
    pub cell_nums: &'a [f64],
    pub cell_texts: &'a str,
    pub cell_text_lens: &'a [u32],
}

/// The incoming-batch wire. No kind ids: an incoming cell has no kind of its own, and which kind
/// applies is a decision this module makes per branch.
pub struct RowBatch<'a> {
    pub subject_keys: &'a str,
    pub subject_key_lens: &'a [u32],
    pub row_cell_counts: &'a [u32],
    pub cell_target_ids: &'a [u32],
    pub cell_tags: &'a [u8],
    pub cell_nums: &'a [f64],
    pub cell_texts: &'a str,
    pub cell_text_lens: &'a [u32],
}

/// The change wire, decoded on the JS side into the oracle's eight-key literal.
#[derive(Debug, Default)]
pub struct DiffOutput {
    pub subjects_concat: String,
    pub subject_lens: Vec<u32>,
    pub change_subject_idx: Vec<u32>,
    pub change_target_ids: Vec<u32>,
    pub change_kinds: Vec<u8>,
    pub change_value_kind_ids: Vec<u32>,
    pub old_nums: Vec<f64>,
    pub new_nums: Vec<f64>,
    pub old_texts_concat: String,
    pub old_text_lens: Vec<u32>,
    pub new_texts_concat: String,
    pub new_text_lens: Vec<u32>,
    /// `emptyDiffStats()`'s eight counters, in declaration order:
    /// rows_in, cells_changed, cells_added, cells_updated, cells_removed,
    /// subjects_seen, subjects_added, subjects_removed.
    pub stats: [u32; 8],
}

pub const STAT_ROWS_IN: usize = 0;
pub const STAT_CELLS_CHANGED: usize = 1;
pub const STAT_CELLS_ADDED: usize = 2;
pub const STAT_CELLS_UPDATED: usize = 3;
pub const STAT_CELLS_REMOVED: usize = 4;
pub const STAT_SUBJECTS_SEEN: usize = 5;
pub const STAT_SUBJECTS_ADDED: usize = 6;
pub const STAT_SUBJECTS_REMOVED: usize = 7;

impl DiffOutput {
    pub fn change_count(&self) -> usize {
        self.change_kinds.len()
    }
}

/// One resolved side of a change, before it is pushed into the output arena.
enum SideRef<'a> {
    Num(f64),
    Text(&'a str),
}

/// `splitValue(kind, value, true)` — `diffSnapshots.js:95-116`, with `present` always true
/// because every call site in the oracle passes `true`.
fn split_value<'a>(
    kinds: &[KindDef],
    kind_id: u32,
    val: &Val,
    arena: &'a str,
) -> Result<SideRef<'a>, DiffError> {
    let kind = &kinds[kind_id as usize];
    if kind.is_numeric {
        match val {
            Val::Num(n) if n.is_finite() => Ok(SideRef::Num(*n)),
            other => Err(DiffError::ValueTypeNumeric {
                value_kind: kind.name.clone(),
                value_type: other.type_of(),
            }),
        }
    } else {
        match val {
            Val::Text { start, len } => Ok(SideRef::Text(
                &arena[*start as usize..(*start + *len) as usize],
            )),
            other => Err(DiffError::ValueTypeText {
                value_kind: kind.name.clone(),
                value_type: other.type_of(),
            }),
        }
    }
}

fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16() as u32).sum()
}

/// Walks a concatenated string, converting successive UTF-16 unit counts to byte ranges.
struct Utf16Cursor<'a> {
    s: &'a str,
    byte: usize,
}

impl<'a> Utf16Cursor<'a> {
    fn new(s: &'a str) -> Self {
        Utf16Cursor { s, byte: 0 }
    }
    fn next(&mut self, units: u32) -> (u32, u32) {
        let start = self.byte;
        let mut remaining = units;
        let mut idx = self.byte;
        if remaining > 0 {
            for c in self.s[self.byte..].chars() {
                let u = c.len_utf16() as u32;
                idx += c.len_utf8();
                remaining = remaining.saturating_sub(u);
                if remaining == 0 {
                    break;
                }
            }
        }
        self.byte = idx;
        (start as u32, (idx - start) as u32)
    }
}

fn push_side(
    texts: &mut String,
    lens: &mut Vec<u32>,
    nums: &mut Vec<f64>,
    side: Option<SideRef<'_>>,
) {
    match side {
        None => {
            nums.push(f64::NAN);
            lens.push(ABSENT_LEN);
        }
        Some(SideRef::Num(n)) => {
            nums.push(n);
            lens.push(ABSENT_LEN);
        }
        Some(SideRef::Text(s)) => {
            nums.push(f64::NAN);
            lens.push(utf16_len(s));
            texts.push_str(s);
        }
    }
}

// ── the session ──────────────────────────────────────────────────────────────────────────────

/// The prior week, resident in wasm linear memory, consumed as the incoming week streams past.
///
/// `resident_cell_count` is the measurable analogue of `PriorState.cellCount` and is what
/// phase 3's AC-11 shape assertion is re-run against: monotonically non-increasing, peaking at
/// exactly the seeded count, reaching 0 after `diff_vanished`.
///
/// Named cost, so it is not discovered later: taking a subject frees its CELL vector (the actual
/// floor, ~1.3M cells) but keeps the key `String` and a bool (~40 B × 130k ≈ 5 MB), and the text
/// arena is never compacted. That is the price of order-preserving removal at O(1).
pub struct Session {
    subject_target_key: String,
    target_keys: Vec<String>,
    target_ids: HashMap<String, u32>,
    kinds: Vec<KindDef>,
    kind_ids: HashMap<String, u32>,
    /// `target_id -> pinned kind id`. `None` means the key is not in `valueKinds`, which is
    /// `kindOf`'s throw condition — and NOT an error unless a branch actually needs it.
    pinned_kind: Vec<Option<u32>>,
    subjects: Vec<SubjectSlot>,
    subject_ids: HashMap<String, u32>,
    arena: String,
    // Epoch-stamped scratch: no clearing pass, and no poisoned state when a batch returns Err
    // partway through.
    prior_idx: Vec<u32>,
    prior_epoch: Vec<u64>,
    row_epoch: Vec<u64>,
    epoch: u64,
    opaque_counter: u32,
    initial_cell_count: usize,
    resident_cell_count: usize,
    sealed: bool,
}

impl Session {
    pub fn new(subject_target_key: String) -> Self {
        Session {
            subject_target_key,
            target_keys: Vec::new(),
            target_ids: HashMap::new(),
            kinds: Vec::new(),
            kind_ids: HashMap::new(),
            pinned_kind: Vec::new(),
            subjects: Vec::new(),
            subject_ids: HashMap::new(),
            arena: String::new(),
            prior_idx: Vec::new(),
            prior_epoch: Vec::new(),
            row_epoch: Vec::new(),
            epoch: 0,
            opaque_counter: 0,
            initial_cell_count: 0,
            resident_cell_count: 0,
            sealed: false,
        }
    }

    pub fn subject_target_key(&self) -> &str {
        &self.subject_target_key
    }

    pub fn resident_cell_count(&self) -> usize {
        self.resident_cell_count
    }

    pub fn initial_cell_count(&self) -> usize {
        self.initial_cell_count
    }

    /// Subjects not yet consumed — the analogue of `PriorState.size`.
    pub fn subject_count(&self) -> usize {
        self.subjects.iter().filter(|s| !s.taken).count()
    }

    pub fn target_count(&self) -> usize {
        self.target_keys.len()
    }

    pub fn kind_count(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    /// Intern one target key, returning its id. Ids are assigned in call order and the JS side
    /// mints exactly the same sequence, which is what lets a change row's `target_key` be resolved
    /// back to a string from a JS-side array with no string ever crossing on the hot path.
    pub fn intern_target(&mut self, name: &str) -> u32 {
        if let Some(id) = self.target_ids.get(name) {
            return *id;
        }
        let id = self.target_keys.len() as u32;
        self.target_keys.push(name.to_string());
        self.target_ids.insert(name.to_string(), id);
        self.pinned_kind.push(None);
        self.prior_idx.push(0);
        self.prior_epoch.push(0);
        self.row_epoch.push(0);
        id
    }

    pub fn intern_kind(&mut self, name: &str) -> u32 {
        if let Some(id) = self.kind_ids.get(name) {
            return *id;
        }
        let id = self.kinds.len() as u32;
        self.kinds.push(KindDef {
            name: name.to_string(),
            is_numeric: name == "number" || name == "money",
        });
        self.kind_ids.insert(name.to_string(), id);
        id
    }

    pub fn target_name(&self, id: u32) -> &str {
        &self.target_keys[id as usize]
    }

    pub fn kind_name(&self, id: u32) -> &str {
        &self.kinds[id as usize].name
    }

    /// Pin `valueKinds`. Only truthy kinds are pinned on the JS side, matching `kindOf`'s `!kind`
    /// test — an empty-string kind is as unpinned as a missing one.
    pub fn set_pinned(&mut self, target_id: u32, kind_id: u32) {
        self.pinned_kind[target_id as usize] = Some(kind_id);
    }

    fn decode_val(
        &mut self,
        tag: u8,
        nums: &[f64],
        num_i: &mut usize,
        texts: &str,
        cursor: &mut Utf16Cursor<'_>,
        text_lens: &[u32],
        text_i: &mut usize,
    ) -> Val {
        match tag {
            TAG_NUMBER => {
                let v = nums[*num_i];
                *num_i += 1;
                Val::Num(v)
            }
            TAG_STRING => {
                let (start, len) = cursor.next(text_lens[*text_i]);
                *text_i += 1;
                let s = &texts[start as usize..(start + len) as usize];
                let arena_start = self.arena.len() as u32;
                self.arena.push_str(s);
                Val::Text {
                    start: arena_start,
                    len,
                }
            }
            TAG_UNDEFINED => Val::Undefined,
            TAG_NULL => Val::Null,
            TAG_FALSE => Val::Bool(false),
            TAG_TRUE => Val::Bool(true),
            other => {
                self.opaque_counter += 1;
                Val::Opaque {
                    tag: other,
                    id: self.opaque_counter,
                }
            }
        }
    }

    /// Seed one chunk of the prior week. Re-seeding an existing (subject, target) OVERWRITES it
    /// without bumping the cell count — `PriorState.set`'s `if (!cells.has(targetKey))` guard.
    pub fn seed(&mut self, batch: &SeedBatch<'_>) {
        let mut sk_cursor = Utf16Cursor::new(batch.subject_keys);
        let mut tx_cursor = Utf16Cursor::new(batch.cell_texts);
        let mut num_i = 0usize;
        let mut text_i = 0usize;
        let mut cell_i = 0usize;

        for s in 0..batch.subject_cell_counts.len() {
            let (kb, kl) = sk_cursor.next(batch.subject_key_lens[s]);
            let subject_key =
                batch.subject_keys[kb as usize..(kb + kl) as usize].to_string();

            let existing = self.subject_ids.get(&subject_key).copied();
            let slot = match existing {
                Some(i) => i,
                None => {
                    let i = self.subjects.len() as u32;
                    self.subjects.push(SubjectSlot {
                        key: subject_key.clone(),
                        taken: false,
                        cells: Vec::new(),
                    });
                    self.subject_ids.insert(subject_key, i);
                    i
                }
            };

            // Stamp the slot's existing cells so a repeated target overwrites rather than appends.
            self.epoch += 1;
            let epoch = self.epoch;
            for (idx, c) in self.subjects[slot as usize].cells.iter().enumerate() {
                self.prior_epoch[c.target_id as usize] = epoch;
                self.prior_idx[c.target_id as usize] = idx as u32;
            }

            let count = batch.subject_cell_counts[s] as usize;
            for _ in 0..count {
                let target_id = batch.cell_target_ids[cell_i];
                let kind_id = batch.cell_kind_ids[cell_i];
                let tag = batch.cell_tags[cell_i];
                cell_i += 1;
                let val = self.decode_val(
                    tag,
                    batch.cell_nums,
                    &mut num_i,
                    batch.cell_texts,
                    &mut tx_cursor,
                    batch.cell_text_lens,
                    &mut text_i,
                );
                let cell = Cell {
                    target_id,
                    kind_id,
                    val,
                };
                if self.prior_epoch[target_id as usize] == epoch {
                    let at = self.prior_idx[target_id as usize] as usize;
                    self.subjects[slot as usize].cells[at] = cell;
                } else {
                    let at = self.subjects[slot as usize].cells.len() as u32;
                    self.subjects[slot as usize].cells.push(cell);
                    self.prior_epoch[target_id as usize] = epoch;
                    self.prior_idx[target_id as usize] = at;
                    self.resident_cell_count += 1;
                }
            }
        }
    }

    /// `PriorState.seal()` — freeze the initial cell count.
    pub fn seal(&mut self) {
        self.initial_cell_count = self.resident_cell_count;
        self.sealed = true;
    }

    /// `diffSnapshots(priorState, nextRows)` for one batch.
    ///
    /// On `Err` the session keeps whatever mutation had already happened, exactly as the oracle
    /// does: `take()` has already deleted the subjects processed so far and the partially built
    /// `changes` array is discarded with the exception. The epoch-stamped scratch means there is
    /// no half-cleared index left behind.
    pub fn diff_batch(&mut self, batch: &RowBatch<'_>) -> Result<DiffOutput, DiffError> {
        let mut out = DiffOutput::default();
        let mut sk_cursor = Utf16Cursor::new(batch.subject_keys);
        let mut tx_cursor = Utf16Cursor::new(batch.cell_texts);
        let mut num_i = 0usize;
        let mut text_i = 0usize;
        let mut cell_i = 0usize;

        for r in 0..batch.row_cell_counts.len() {
            let (kb, kl) = sk_cursor.next(batch.subject_key_lens[r]);
            let subject_key = &batch.subject_keys[kb as usize..(kb + kl) as usize];

            out.stats[STAT_ROWS_IN] += 1;
            out.stats[STAT_SUBJECTS_SEEN] += 1;

            // `PriorState.take()` — DELETES what it returns, which is what bounds the memory and
            // what makes a duplicate subject_key inside one week diff as all-`added` (the wanted
            // loud failure at the PRIMARY KEY, `diffSnapshots.js:21-24`).
            let slot = self.subject_ids.get(subject_key).copied();
            let prior: Option<Vec<Cell>> = match slot {
                Some(i) if !self.subjects[i as usize].taken => {
                    let i = i as usize;
                    self.subjects[i].taken = true;
                    let cells = mem::take(&mut self.subjects[i].cells);
                    self.resident_cell_count -= cells.len();
                    Some(cells)
                }
                _ => None,
            };
            if prior.is_none() {
                out.stats[STAT_SUBJECTS_ADDED] += 1;
            }

            self.epoch += 1;
            let epoch = self.epoch;
            if let Some(p) = &prior {
                for (idx, c) in p.iter().enumerate() {
                    self.prior_epoch[c.target_id as usize] = epoch;
                    self.prior_idx[c.target_id as usize] = idx as u32;
                }
            }

            // The subject's index in this call's subject table, allocated lazily so a row that
            // emits nothing costs nothing.
            let mut subject_idx: Option<u32> = None;

            let count = batch.row_cell_counts[r] as usize;
            for _ in 0..count {
                let target_id = batch.cell_target_ids[cell_i];
                let tag = batch.cell_tags[cell_i];
                cell_i += 1;

                // `hasOwn(row, targetKey)` for the removed pass below.
                self.row_epoch[target_id as usize] = epoch;

                // Decode WITHOUT touching the session arena: an incoming value only enters the
                // arena if it becomes a prior cell, which it never does here.
                let val: Val = match tag {
                    TAG_NUMBER => {
                        let v = batch.cell_nums[num_i];
                        num_i += 1;
                        Val::Num(v)
                    }
                    TAG_STRING => {
                        let (start, len) = tx_cursor.next(batch.cell_text_lens[text_i]);
                        text_i += 1;
                        Val::Text { start, len }
                    }
                    TAG_UNDEFINED => Val::Undefined,
                    TAG_NULL => Val::Null,
                    TAG_FALSE => Val::Bool(false),
                    TAG_TRUE => Val::Bool(true),
                    other => {
                        self.opaque_counter += 1;
                        Val::Opaque {
                            tag: other,
                            id: self.opaque_counter,
                        }
                    }
                };

                let prior_cell: Option<&Cell> = match &prior {
                    Some(p) if self.prior_epoch[target_id as usize] == epoch => {
                        Some(&p[self.prior_idx[target_id as usize] as usize])
                    }
                    _ => None,
                };

                match prior_cell {
                    // Branch 1 — no prior cell. The PINNED kind, and it is the only branch that
                    // can raise ValueKindError.
                    None => {
                        let kind_id = match self.pinned_kind[target_id as usize] {
                            Some(k) => k,
                            None => {
                                return Err(DiffError::ValueKind {
                                    target_key: self.target_keys[target_id as usize].clone(),
                                })
                            }
                        };
                        let side =
                            split_value(&self.kinds, kind_id, &val, batch.cell_texts)?;
                        let sidx = *subject_idx.get_or_insert_with(|| {
                            let i = out.subject_lens.len() as u32;
                            out.subject_lens.push(utf16_len(subject_key));
                            out.subjects_concat.push_str(subject_key);
                            i
                        });
                        out.change_subject_idx.push(sidx);
                        out.change_target_ids.push(target_id);
                        out.change_kinds.push(CHANGE_ADDED);
                        out.change_value_kind_ids.push(kind_id);
                        push_side(
                            &mut out.old_texts_concat,
                            &mut out.old_text_lens,
                            &mut out.old_nums,
                            None,
                        );
                        push_side(
                            &mut out.new_texts_concat,
                            &mut out.new_text_lens,
                            &mut out.new_nums,
                            Some(side),
                        );
                        out.stats[STAT_CELLS_ADDED] += 1;
                        out.stats[STAT_CELLS_CHANGED] += 1;
                    }
                    Some(pc) => {
                        // Branch 2 — unchanged. NO kind lookup, NO type guard. `:322`.
                        if js_strict_eq(&pc.val, &self.arena, &val, batch.cell_texts) {
                            continue;
                        }
                        // Branch 3 — the kind travels WITH THE CELL (`:319-321`), and the OLD
                        // value is split first so its `typeof` is the one that reaches the
                        // message when both sides are bad.
                        let kind_id = pc.kind_id;
                        let old = split_value(&self.kinds, kind_id, &pc.val, &self.arena)?;
                        let new = split_value(&self.kinds, kind_id, &val, batch.cell_texts)?;
                        let sidx = *subject_idx.get_or_insert_with(|| {
                            let i = out.subject_lens.len() as u32;
                            out.subject_lens.push(utf16_len(subject_key));
                            out.subjects_concat.push_str(subject_key);
                            i
                        });
                        out.change_subject_idx.push(sidx);
                        out.change_target_ids.push(target_id);
                        out.change_kinds.push(CHANGE_CHANGED);
                        out.change_value_kind_ids.push(kind_id);
                        push_side(
                            &mut out.old_texts_concat,
                            &mut out.old_text_lens,
                            &mut out.old_nums,
                            Some(old),
                        );
                        push_side(
                            &mut out.new_texts_concat,
                            &mut out.new_text_lens,
                            &mut out.new_nums,
                            Some(new),
                        );
                        out.stats[STAT_CELLS_UPDATED] += 1;
                        out.stats[STAT_CELLS_CHANGED] += 1;
                    }
                }
            }

            // Branch 4 — prior cells absent from the row, in the prior map's insertion order.
            if let Some(p) = &prior {
                for c in p.iter() {
                    if self.row_epoch[c.target_id as usize] == epoch {
                        continue;
                    }
                    let old = split_value(&self.kinds, c.kind_id, &c.val, &self.arena)?;
                    let sidx = *subject_idx.get_or_insert_with(|| {
                        let i = out.subject_lens.len() as u32;
                        out.subject_lens.push(utf16_len(subject_key));
                        out.subjects_concat.push_str(subject_key);
                        i
                    });
                    out.change_subject_idx.push(sidx);
                    out.change_target_ids.push(c.target_id);
                    out.change_kinds.push(CHANGE_REMOVED);
                    out.change_value_kind_ids.push(c.kind_id);
                    push_side(
                        &mut out.old_texts_concat,
                        &mut out.old_text_lens,
                        &mut out.old_nums,
                        Some(old),
                    );
                    push_side(
                        &mut out.new_texts_concat,
                        &mut out.new_text_lens,
                        &mut out.new_nums,
                        None,
                    );
                    out.stats[STAT_CELLS_REMOVED] += 1;
                    out.stats[STAT_CELLS_CHANGED] += 1;
                }
            }
        }

        Ok(out)
    }

    /// `diffVanishedSubjects(priorState)` — the subjects never seen in the incoming week, in
    /// seed order, followed by the drain that takes `resident_cell_count` to 0.
    pub fn diff_vanished(&mut self) -> Result<DiffOutput, DiffError> {
        let mut out = DiffOutput::default();

        for i in 0..self.subjects.len() {
            if self.subjects[i].taken {
                continue;
            }
            self.subjects[i].taken = true;
            let cells = mem::take(&mut self.subjects[i].cells);
            self.resident_cell_count -= cells.len();

            out.stats[STAT_SUBJECTS_REMOVED] += 1;
            let sidx = out.subject_lens.len() as u32;
            out.subject_lens.push(utf16_len(&self.subjects[i].key));
            out.subjects_concat.push_str(&self.subjects[i].key);

            for c in cells.iter() {
                let old = split_value(&self.kinds, c.kind_id, &c.val, &self.arena)?;
                out.change_subject_idx.push(sidx);
                out.change_target_ids.push(c.target_id);
                out.change_kinds.push(CHANGE_REMOVED);
                out.change_value_kind_ids.push(c.kind_id);
                push_side(
                    &mut out.old_texts_concat,
                    &mut out.old_text_lens,
                    &mut out.old_nums,
                    Some(old),
                );
                push_side(
                    &mut out.new_texts_concat,
                    &mut out.new_text_lens,
                    &mut out.new_nums,
                    None,
                );
                out.stats[STAT_CELLS_REMOVED] += 1;
                out.stats[STAT_CELLS_CHANGED] += 1;
            }
        }

        Ok(out)
    }
}

// ── an ergonomic builder for the flat wires ──────────────────────────────────────────────────
//
// The hot path is slices so the wasm boundary carries typed arrays and nothing else. Native
// tests need to write fixtures without hand-packing six parallel arrays, so this builder does
// the packing and is the single place that knows the layout. It is NOT on the wasm path.

/// An ergonomic JS value for fixtures.
#[derive(Debug, Clone)]
pub enum JsVal {
    Num(f64),
    Str(String),
    Undefined,
    Null,
    Bool(bool),
    Object,
}

impl JsVal {
    fn tag(&self) -> u8 {
        match self {
            JsVal::Num(_) => TAG_NUMBER,
            JsVal::Str(_) => TAG_STRING,
            JsVal::Undefined => TAG_UNDEFINED,
            JsVal::Null => TAG_NULL,
            JsVal::Bool(false) => TAG_FALSE,
            JsVal::Bool(true) => TAG_TRUE,
            JsVal::Object => TAG_OBJECT,
        }
    }
}

#[derive(Default)]
pub struct WireBuilder {
    pub subject_keys: String,
    pub subject_key_lens: Vec<u32>,
    pub counts: Vec<u32>,
    pub cell_target_ids: Vec<u32>,
    pub cell_kind_ids: Vec<u32>,
    pub cell_tags: Vec<u8>,
    pub cell_nums: Vec<f64>,
    pub cell_texts: String,
    pub cell_text_lens: Vec<u32>,
}

impl WireBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_subject(&mut self, key: &str) {
        self.subject_keys.push_str(key);
        self.subject_key_lens.push(utf16_len(key));
        self.counts.push(0);
    }

    pub fn push_cell(&mut self, target_id: u32, kind_id: u32, val: &JsVal) {
        let last = self.counts.len() - 1;
        self.counts[last] += 1;
        self.cell_target_ids.push(target_id);
        self.cell_kind_ids.push(kind_id);
        self.cell_tags.push(val.tag());
        match val {
            JsVal::Num(n) => self.cell_nums.push(*n),
            JsVal::Str(s) => {
                self.cell_texts.push_str(s);
                self.cell_text_lens.push(utf16_len(s));
            }
            _ => {}
        }
    }

    pub fn as_seed(&self) -> SeedBatch<'_> {
        SeedBatch {
            subject_keys: &self.subject_keys,
            subject_key_lens: &self.subject_key_lens,
            subject_cell_counts: &self.counts,
            cell_target_ids: &self.cell_target_ids,
            cell_kind_ids: &self.cell_kind_ids,
            cell_tags: &self.cell_tags,
            cell_nums: &self.cell_nums,
            cell_texts: &self.cell_texts,
            cell_text_lens: &self.cell_text_lens,
        }
    }

    pub fn as_rows(&self) -> RowBatch<'_> {
        RowBatch {
            subject_keys: &self.subject_keys,
            subject_key_lens: &self.subject_key_lens,
            row_cell_counts: &self.counts,
            cell_target_ids: &self.cell_target_ids,
            cell_tags: &self.cell_tags,
            cell_nums: &self.cell_nums,
            cell_texts: &self.cell_texts,
            cell_text_lens: &self.cell_text_lens,
        }
    }
}

/// A decoded change row, for native assertions. The wasm path decodes the same fields in JS.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedChange {
    pub subject_key: String,
    pub target_key: String,
    pub change_kind: String,
    pub value_kind: String,
    pub old_text: Option<String>,
    pub old_num: Option<f64>,
    pub new_text: Option<String>,
    pub new_num: Option<f64>,
}

/// Decode a `DiffOutput` the way `wasmDiff.js` does, so the native corpus asserts the same shape
/// the parity suite asserts.
pub fn decode_changes(session: &Session, out: &DiffOutput) -> Vec<DecodedChange> {
    let mut subjects: Vec<&str> = Vec::with_capacity(out.subject_lens.len());
    {
        let mut cur = Utf16Cursor::new(&out.subjects_concat);
        for l in &out.subject_lens {
            let (b, n) = cur.next(*l);
            subjects.push(&out.subjects_concat[b as usize..(b + n) as usize]);
        }
    }
    let mut old_cur = Utf16Cursor::new(&out.old_texts_concat);
    let mut new_cur = Utf16Cursor::new(&out.new_texts_concat);
    let mut rows = Vec::with_capacity(out.change_kinds.len());
    for i in 0..out.change_kinds.len() {
        let old_text = if out.old_text_lens[i] == ABSENT_LEN {
            None
        } else {
            let (b, n) = old_cur.next(out.old_text_lens[i]);
            Some(out.old_texts_concat[b as usize..(b + n) as usize].to_string())
        };
        let new_text = if out.new_text_lens[i] == ABSENT_LEN {
            None
        } else {
            let (b, n) = new_cur.next(out.new_text_lens[i]);
            Some(out.new_texts_concat[b as usize..(b + n) as usize].to_string())
        };
        rows.push(DecodedChange {
            subject_key: subjects[out.change_subject_idx[i] as usize].to_string(),
            target_key: session.target_name(out.change_target_ids[i]).to_string(),
            change_kind: match out.change_kinds[i] {
                CHANGE_ADDED => "added",
                CHANGE_CHANGED => "changed",
                _ => "removed",
            }
            .to_string(),
            value_kind: session.kind_name(out.change_value_kind_ids[i]).to_string(),
            old_text,
            old_num: if out.old_nums[i].is_nan() {
                None
            } else {
                Some(out.old_nums[i])
            },
            new_text,
            new_num: if out.new_nums[i].is_nan() {
                None
            } else {
                Some(out.new_nums[i])
            },
        });
    }
    rows
}

// Phase-4 F2 negative control (2026-08-06): this line was appended, the freshness assertion
// went red without a republish, and `bash scripts/build-wasm.sh` turned it green again.
