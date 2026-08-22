//! The 0018-MERIDIAN phase-4 diff corpus, run natively against `Session` with no wasm in the
//! loop (P4-AC-3).
//!
//! Every `case_NN_*` name here is mirrored by a case of the same name in the app's
//! `frontend/src/services/temporal/__tests__/wasmDiffParity.spec.js`, and that spec reads THIS
//! file's source to assert the two lists match. A case added on one side and forgotten on the
//! other is a red test, not a silent coverage hole — which matters because the corpus is the
//! only thing standing between a branch divergence and production.
//!
//! Case 12 (`SubjectKeyError`) has no Rust half by construction: `readSubjectKey` runs against
//! the JS row OBJECT, before anything crosses the boundary, so the Rust session never sees a row
//! without a subject key. It is asserted on the JS side only, and named here so the gap is
//! deliberate rather than missing.
//!
//! This file is NOT hashed by `snapshot_diff_stamp.rs`. A test edit is not an engine change and
//! must not force a republish to keep the freshness assertion green.

use crate::snapshot_diff::{
    decode_changes, DecodedChange, DiffError, JsVal, Session, WireBuilder, STAT_CELLS_ADDED,
    STAT_CELLS_CHANGED, STAT_CELLS_REMOVED, STAT_CELLS_UPDATED, STAT_ROWS_IN, STAT_SUBJECTS_ADDED,
    STAT_SUBJECTS_REMOVED, STAT_SUBJECTS_SEEN,
};

fn n(v: f64) -> JsVal {
    JsVal::Num(v)
}
fn t(v: &str) -> JsVal {
    JsVal::Str(v.to_string())
}

struct Harness {
    s: Session,
}

// 0037-MERIDIAN: named rather than repeated inline at the two sites clippy's type_complexity
// flagged. The shape is the wire format's own — a subject key plus its cells, each carrying its
// value kind — so it deserves a name more than it deserves an `#[allow]`.
type SeedSubject<'a> = (&'a str, Vec<(&'a str, &'a str, JsVal)>);

impl Harness {
    fn new(subject_target_key: &str, value_kinds: &[(&str, &str)]) -> Self {
        let mut s = Session::new(subject_target_key.to_string());
        for (target, kind) in value_kinds {
            let ti = s.intern_target(target);
            let ki = s.intern_kind(kind);
            s.set_pinned(ti, ki);
        }
        Harness { s }
    }

    /// `(subject_key, [(target_key, value_kind, value)])` — the kind travels WITH THE CELL.
    fn seed(&mut self, subjects: &[SeedSubject<'_>]) {
        let mut b = WireBuilder::new();
        for (key, cells) in subjects {
            b.push_subject(key);
            for (target, kind, val) in cells {
                let ti = self.s.intern_target(target);
                let ki = self.s.intern_kind(kind);
                b.push_cell(ti, ki, val);
            }
        }
        self.s.seed(&b.as_seed());
        self.s.seal();
    }

    /// `(subject_key, [(target_key, value)])` in `Object.keys(row)` order. Incoming cells carry
    /// no kind: which kind applies is a per-branch decision inside `diff_batch`.
    fn diff(
        &mut self,
        rows: &[(&str, Vec<(&str, JsVal)>)],
    ) -> Result<(Vec<DecodedChange>, [u32; 8]), DiffError> {
        let mut b = WireBuilder::new();
        for (key, cells) in rows {
            b.push_subject(key);
            for (target, val) in cells {
                let ti = self.s.intern_target(target);
                b.push_cell(ti, 0, val);
            }
        }
        let out = self.s.diff_batch(&b.as_rows())?;
        let changes = decode_changes(&self.s, &out);
        Ok((changes, out.stats))
    }

    fn vanished(&mut self) -> (Vec<DecodedChange>, [u32; 8]) {
        let out = self.s.diff_vanished().expect("vanished must not throw here");
        let changes = decode_changes(&self.s, &out);
        (changes, out.stats)
    }
}

fn changed(
    subject: &str,
    target: &str,
    kind: &str,
    old_text: Option<&str>,
    old_num: Option<f64>,
    new_text: Option<&str>,
    new_num: Option<f64>,
) -> DecodedChange {
    DecodedChange {
        subject_key: subject.to_string(),
        target_key: target.to_string(),
        change_kind: "changed".to_string(),
        value_kind: kind.to_string(),
        old_text: old_text.map(|s| s.to_string()),
        old_num,
        new_text: new_text.map(|s| s.to_string()),
        new_num,
    }
}

fn added(
    subject: &str,
    target: &str,
    kind: &str,
    new_text: Option<&str>,
    new_num: Option<f64>,
) -> DecodedChange {
    DecodedChange {
        subject_key: subject.to_string(),
        target_key: target.to_string(),
        change_kind: "added".to_string(),
        value_kind: kind.to_string(),
        old_text: None,
        old_num: None,
        new_text: new_text.map(|s| s.to_string()),
        new_num,
    }
}

fn removed(
    subject: &str,
    target: &str,
    kind: &str,
    old_text: Option<&str>,
    old_num: Option<f64>,
) -> DecodedChange {
    DecodedChange {
        subject_key: subject.to_string(),
        target_key: target.to_string(),
        change_kind: "removed".to_string(),
        value_kind: kind.to_string(),
        old_text: old_text.map(|s| s.to_string()),
        old_num,
        new_text: None,
        new_num: None,
    }
}

const KINDS: &[(&str, &str)] = &[
    ("employee_id", "text"),
    ("salary", "money"),
    ("hours", "number"),
    ("sex", "text"),
];

#[test]
fn case_01_changed_money_cell() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("salary", "money", n(52000.0)),
        ],
    )]);
    let (changes, stats) = h
        .diff(&[("E1", vec![("employee_id", t("E1")), ("salary", n(53500.0))])])
        .unwrap();
    assert_eq!(
        changes,
        vec![changed(
            "E1",
            "salary",
            "money",
            None,
            Some(52000.0),
            None,
            Some(53500.0)
        )]
    );
    assert_eq!(stats[STAT_CELLS_UPDATED], 1);
    assert_eq!(stats[STAT_CELLS_CHANGED], 1);
    assert_eq!(stats[STAT_ROWS_IN], 1);
    assert_eq!(stats[STAT_SUBJECTS_SEEN], 1);
    assert_eq!(stats[STAT_SUBJECTS_ADDED], 0);
}

#[test]
fn case_02_added_cell() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[("E1", vec![("employee_id", "text", t("E1"))])]);
    let (changes, stats) = h
        .diff(&[("E1", vec![("employee_id", t("E1")), ("hours", n(37.5))])])
        .unwrap();
    assert_eq!(
        changes,
        vec![added("E1", "hours", "number", None, Some(37.5))]
    );
    assert_eq!(stats[STAT_CELLS_ADDED], 1);
    assert_eq!(stats[STAT_CELLS_CHANGED], 1);
}

#[test]
fn case_03_removed_cell() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("sex", "text", t("F")),
        ],
    )]);
    let (changes, stats) = h
        .diff(&[("E1", vec![("employee_id", t("E1"))])])
        .unwrap();
    assert_eq!(
        changes,
        vec![removed("E1", "sex", "text", Some("F"), None)]
    );
    assert_eq!(stats[STAT_CELLS_REMOVED], 1);
}

#[test]
fn case_04_subject_vanished_entirely() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[
        ("E1", vec![("employee_id", "text", t("E1"))]),
        (
            "E2",
            vec![
                ("employee_id", "text", t("E2")),
                ("salary", "money", n(41000.0)),
            ],
        ),
    ]);
    let (changes, _) = h.diff(&[("E1", vec![("employee_id", t("E1"))])]).unwrap();
    assert!(changes.is_empty());
    let (tail, stats) = h.vanished();
    assert_eq!(
        tail,
        vec![
            removed("E2", "employee_id", "text", Some("E2"), None),
            removed("E2", "salary", "money", None, Some(41000.0)),
        ]
    );
    assert_eq!(stats[STAT_SUBJECTS_REMOVED], 1);
    assert_eq!(stats[STAT_CELLS_REMOVED], 2);
    assert_eq!(h.s.resident_cell_count(), 0);
}

#[test]
fn case_05_subject_added_entirely() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[("E1", vec![("employee_id", "text", t("E1"))])]);
    let (changes, stats) = h
        .diff(&[
            ("E1", vec![("employee_id", t("E1"))]),
            ("E9", vec![("employee_id", t("E9")), ("salary", n(60000.0))]),
        ])
        .unwrap();
    assert_eq!(
        changes,
        vec![
            added("E9", "employee_id", "text", Some("E9"), None),
            added("E9", "salary", "money", None, Some(60000.0)),
        ]
    );
    assert_eq!(stats[STAT_SUBJECTS_ADDED], 1);
    assert_eq!(stats[STAT_SUBJECTS_SEEN], 2);
}

#[test]
fn case_06_noop_row_emits_nothing() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("salary", "money", n(52000.0)),
            ("sex", "text", t("F")),
        ],
    )]);
    let (changes, stats) = h
        .diff(&[(
            "E1",
            vec![
                ("employee_id", t("E1")),
                ("salary", n(52000.0)),
                ("sex", t("F")),
            ],
        )])
        .unwrap();
    assert!(changes.is_empty());
    assert_eq!(stats[STAT_CELLS_CHANGED], 0);
    assert_eq!(stats[STAT_ROWS_IN], 1);
}

#[test]
fn case_07_zero_and_empty_string_are_values_not_absence() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[("E1", vec![("employee_id", "text", t("E1"))])]);
    // Added with the falsy-but-legitimate values.
    let (changes, _) = h
        .diff(&[(
            "E1",
            vec![("employee_id", t("E1")), ("salary", n(0.0)), ("sex", t(""))],
        )])
        .unwrap();
    assert_eq!(
        changes,
        vec![
            added("E1", "salary", "money", None, Some(0.0)),
            added("E1", "sex", "text", Some(""), None),
        ]
    );

    // And they are equal to themselves rather than being read as absent.
    let mut h2 = Harness::new("employee_id", KINDS);
    h2.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("salary", "money", n(0.0)),
            ("sex", "text", t("")),
        ],
    )]);
    let (changes2, _) = h2
        .diff(&[(
            "E1",
            vec![("employee_id", t("E1")), ("salary", n(0.0)), ("sex", t(""))],
        )])
        .unwrap();
    assert!(changes2.is_empty());
}

#[test]
fn case_08_negative_zero_equals_zero_and_emits_nothing() {
    // The single most attractive wrong optimisation in the phase: `f64::to_bits()` would separate
    // -0.0 from 0.0, emit a ('changed', -0, 0) row, and the ingest's no-op CHECK would abort a
    // whole week over a float sign. `==` is the JS `===` behaviour and is correct.
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("salary", "money", n(-0.0)),
        ],
    )]);
    let (changes, stats) = h
        .diff(&[("E1", vec![("employee_id", t("E1")), ("salary", n(0.0))])])
        .unwrap();
    assert!(changes.is_empty());
    assert_eq!(stats[STAT_CELLS_CHANGED], 0);
    // 0037-MERIDIAN: these two lines are executable documentation of the trap named at the top of
    // this test, not assertions about the code under test (that is the two asserts above). The
    // first was written as `assert!((-0.0f64) == 0.0f64)`, a constant expression the compiler
    // folds away — clippy's assertions_on_constants, correct and the third vacuous assertion this
    // sweep found. `black_box` keeps both as real runtime checks so the contrast they exist to
    // draw actually runs, instead of deleting the half that documents the wrong optimisation.
    let neg = std::hint::black_box(-0.0f64);
    let pos = std::hint::black_box(0.0f64);
    assert!(neg == pos, "`==` must read -0.0 and 0.0 as equal — the JS `===` behaviour");
    assert!(
        neg.to_bits() != pos.to_bits(),
        "`to_bits()` separates them — the wrong optimisation this test exists to forbid"
    );
}

#[test]
fn case_09_duplicate_subject_key_diffs_as_all_added() {
    // `take()` DELETES, so the second appearance finds no prior entry, diffs as all-`added`, and
    // collides on PRIMARY KEY (snapshot_id, subject_key, target_key) inside the ingest
    // transaction — the wanted loud failure (`diffSnapshots.js:21-24`).
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("salary", "money", n(52000.0)),
        ],
    )]);
    let (changes, stats) = h
        .diff(&[
            ("E1", vec![("employee_id", t("E1")), ("salary", n(53000.0))]),
            ("E1", vec![("employee_id", t("E1")), ("salary", n(54000.0))]),
        ])
        .unwrap();
    assert_eq!(
        changes,
        vec![
            changed("E1", "salary", "money", None, Some(52000.0), None, Some(53000.0)),
            added("E1", "employee_id", "text", Some("E1"), None),
            added("E1", "salary", "money", None, Some(54000.0)),
        ]
    );
    assert_eq!(stats[STAT_SUBJECTS_ADDED], 1);
    assert_eq!(stats[STAT_SUBJECTS_SEEN], 2);
}

#[test]
fn case_10_prior_kind_differs_from_pinned_and_types_mismatch() {
    // pinned salary = text; the prior CELL says money. The kind travels with the cell, so the
    // changed path splits with `money` and the incoming string fails the numeric guard.
    let mut h = Harness::new("employee_id", &[("employee_id", "text"), ("salary", "text")]);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("salary", "money", n(52000.0)),
        ],
    )]);
    let err = h
        .diff(&[("E1", vec![("employee_id", t("E1")), ("salary", t("53000"))])])
        .unwrap_err();
    assert_eq!(err.class(), "ValueTypeError");
    assert_eq!(
        err.message(),
        "diffSnapshots: value_kind \"money\" requires a finite JS number, got string. \
Binding it would affinity-convert on the way into value_num (spec §11)."
    );
    assert_eq!(
        err.details_json(),
        "{\"valueKind\":\"money\",\"valueType\":\"string\"}"
    );
}

#[test]
fn case_11_target_key_absent_from_value_kinds_on_an_incoming_row() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[("E1", vec![("employee_id", "text", t("E1"))])]);
    let err = h
        .diff(&[("E1", vec![("employee_id", t("E1")), ("bonus", n(500.0))])])
        .unwrap_err();
    assert_eq!(err.class(), "ValueKindError");
    assert_eq!(
        err.message(),
        "diffSnapshots: no value_kind declared for target_key \"bonus\". \
The kind is pinned from the profile at ingest time and is never derived from the value."
    );
    assert_eq!(err.details_json(), "{\"targetKey\":\"bonus\"}");
}

// case_12_subject_key_error has no Rust half — see the module header. Asserted in
// wasmDiffParity.spec.js only.

#[test]
fn case_13_prior_cell_with_unpinned_target_key_removed_cleanly() {
    // The oracle never calls `kindOf` on the removed path (`:341-352`), so a prior cell whose
    // target key is no longer pinned must survive removal rather than raise ValueKindError.
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("legacy_code", "text", t("ZX9")),
        ],
    )]);
    let (changes, stats) = h.diff(&[("E1", vec![("employee_id", t("E1"))])]).unwrap();
    assert_eq!(
        changes,
        vec![removed("E1", "legacy_code", "text", Some("ZX9"), None)]
    );
    assert_eq!(stats[STAT_CELLS_REMOVED], 1);
}

#[test]
fn case_14_astral_and_replacement_character_text() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E\u{1F600}1",
        vec![
            ("employee_id", "text", t("E\u{1F600}1")),
            ("sex", "text", t("\u{FFFD}")),
        ],
    )]);
    let (changes, _) = h
        .diff(&[(
            "E\u{1F600}1",
            vec![
                ("employee_id", t("E\u{1F600}1")),
                ("sex", t("\u{1F3D4}\u{FFFD}")),
            ],
        )])
        .unwrap();
    assert_eq!(
        changes,
        vec![changed(
            "E\u{1F600}1",
            "sex",
            "text",
            Some("\u{FFFD}"),
            None,
            Some("\u{1F3D4}\u{FFFD}"),
            None
        )]
    );
}

// ── the three cases the spec's §3.4 encoder would have broken (review finding 1) ─────────────
//
// Each of these is legal per §3.3 decision 4 and each throws under an encoder that resolves the
// PINNED kind per incoming cell. They are the reason the wire carries `typeof value` instead.

#[test]
fn case_15_unpinned_target_unchanged_emits_nothing_and_does_not_throw() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("legacy_code", "text", t("ZX9")),
        ],
    )]);
    let (changes, stats) = h
        .diff(&[(
            "E1",
            vec![("employee_id", t("E1")), ("legacy_code", t("ZX9"))],
        )])
        .unwrap();
    assert!(changes.is_empty());
    assert_eq!(stats[STAT_CELLS_CHANGED], 0);
}

#[test]
fn case_16_unpinned_target_changed_uses_the_prior_cells_kind() {
    let mut h = Harness::new("employee_id", KINDS);
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("legacy_code", "text", t("ZX9")),
        ],
    )]);
    let (changes, stats) = h
        .diff(&[(
            "E1",
            vec![("employee_id", t("E1")), ("legacy_code", t("QQ0"))],
        )])
        .unwrap();
    assert_eq!(
        changes,
        vec![changed(
            "E1",
            "legacy_code",
            "text",
            Some("ZX9"),
            None,
            Some("QQ0"),
            None
        )]
    );
    assert_eq!(stats[STAT_CELLS_UPDATED], 1);
}

#[test]
fn case_17_unchanged_value_with_mismatched_pinned_kind_emits_nothing() {
    // pinned note = money; the prior CELL says text and holds a string. The value is unchanged,
    // so `:322` fires before any `splitValue` and nothing is raised.
    let mut h = Harness::new(
        "employee_id",
        &[("employee_id", "text"), ("note", "money")],
    );
    h.seed(&[(
        "E1",
        vec![
            ("employee_id", "text", t("E1")),
            ("note", "text", t("hello")),
        ],
    )]);
    let (changes, stats) = h
        .diff(&[("E1", vec![("employee_id", t("E1")), ("note", t("hello"))])])
        .unwrap();
    assert!(changes.is_empty());
    assert_eq!(stats[STAT_CELLS_CHANGED], 0);
}

// ── shape and hygiene ────────────────────────────────────────────────────────────────────────

#[test]
fn ac11_memory_shape_survives_the_port() {
    // The same assertion phase 3 makes on `PriorState.cellCount`
    // (`diffSnapshots.spec.js:282-317`), re-run against the Rust session: monotonically
    // non-increasing, peaking at exactly the seeded count, reaching 0.
    const SUBJECTS: usize = 400;
    const CELLS: usize = 5;
    let mut h = Harness::new(
        "employee_id",
        &[
            ("employee_id", "text"),
            ("salary", "money"),
            ("hours", "number"),
            ("sex", "text"),
            ("hired_on", "text"),
        ],
    );
    let keys: Vec<String> = (0..SUBJECTS).map(|i| format!("E{}", i)).collect();
    let seed: Vec<SeedSubject<'_>> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| {
            (
                k.as_str(),
                vec![
                    ("employee_id", "text", t(k)),
                    ("salary", "money", n(40000.0 + i as f64)),
                    ("hours", "number", n(37.5)),
                    ("sex", "text", t(if i % 2 == 1 { "F" } else { "M" })),
                    ("hired_on", "text", t("2020-01-05")),
                ],
            )
        })
        .collect();
    h.seed(&seed);

    let initial = h.s.resident_cell_count();
    assert_eq!(initial, SUBJECTS * CELLS);
    assert_eq!(h.s.initial_cell_count(), initial);

    let mut peak = initial;
    let mut last = initial;
    let mut total_changed = 0u32;
    for chunk in keys.chunks(50) {
        let rows: Vec<(&str, Vec<(&str, JsVal)>)> = chunk
            .iter()
            .map(|k| {
                let i: usize = k[1..].parse().unwrap();
                (
                    k.as_str(),
                    vec![
                        ("employee_id", t(k)),
                        ("salary", n(41000.0 + i as f64)),
                        ("hours", n(37.5)),
                        ("sex", t(if i % 2 == 1 { "F" } else { "M" })),
                        ("hired_on", t("2020-01-05")),
                    ],
                )
            })
            .collect();
        let (_, stats) = h.diff(&rows).unwrap();
        total_changed += stats[STAT_CELLS_CHANGED];
        let resident = h.s.resident_cell_count();
        assert!(resident <= last, "resident cells must never grow");
        peak = peak.max(resident);
        last = resident;
    }
    assert_eq!(peak, initial, "never both weeks — the peak IS the prior week");
    assert_eq!(h.s.resident_cell_count(), 0);
    assert_eq!(total_changed, SUBJECTS as u32);
    let (tail, _) = h.vanished();
    assert!(tail.is_empty());
}

#[test]
fn no_rayon_reaches_the_diff() {
    // P4-AC-15, asserted by reading the source so it runs under `cargo test` rather than
    // depending on someone remembering to run a grep. Threading the diff would let the
    // sequential and threaded artifacts diverge in emission order — the one property the whole
    // parity argument rests on.
    for (name, src) in [
        ("snapshot_diff.rs", include_str!("snapshot_diff.rs")),
        ("snapshot_diff_wasm.rs", include_str!("snapshot_diff_wasm.rs")),
    ] {
        // Strip `//` comment lines: the prose above explains WHY there is no rayon and names it.
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for needle in ["rayon", "par_iter", "par_bridge"] {
            assert!(
                !code.contains(needle),
                "{} must not reference {}",
                name,
                needle
            );
        }
    }
}

#[test]
fn the_stamp_moves_when_the_diff_source_moves() {
    // Not a tautology check: it pins the digest RECIPE so the JS side can reproduce it. If this
    // changes shape, `wasmDiffParity.spec.js`'s recomputation must change with it.
    let hex = crate::snapshot_diff_stamp::diff_source_sha256();
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));

    use sha2::{Digest, Sha256};
    let core = crate::snapshot_diff_stamp::DIFF_SOURCE_CORE;
    let wasm = crate::snapshot_diff_stamp::DIFF_SOURCE_WASM;
    let mut hasher = Sha256::new();
    hasher.update((core.len() as u64).to_le_bytes());
    hasher.update(core.as_bytes());
    hasher.update((wasm.len() as u64).to_le_bytes());
    hasher.update(wasm.as_bytes());
    let expect: String = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    assert_eq!(hex, expect);

    // Length-prefixing is what makes moving a line between the two files change the digest.
    let mut swapped = Sha256::new();
    swapped.update((wasm.len() as u64).to_le_bytes());
    swapped.update(wasm.as_bytes());
    swapped.update((core.len() as u64).to_le_bytes());
    swapped.update(core.as_bytes());
    let other: String = swapped.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    assert_ne!(hex, other);
}
