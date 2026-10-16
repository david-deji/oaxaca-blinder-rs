//! The command-line reference page of the docs site includes `docs/src/reference/cli-help.txt`, which is the
//! `--help` text of the shipped command line. This test runs the binary and compares its help with that file,
//! so a flag, a default or a description that changes without the page changing fails CI.
//!
//! Regenerate the file after an intended change:
//! `UPDATE_DOCS=1 cargo test -p oaxaca_blinder --test cli_help_doc_test`
//!
//! The file holds the help of the build a crates.io user gets (default features). With `experimental` the
//! help lists more analysis types, so this test is compiled out there; CI runs it in the default-feature step.
#![cfg(not(feature = "experimental"))]

use std::path::PathBuf;
use std::process::Command;

/// The commands whose help the page shows.
const COMMANDS: &[&[&str]] = &[&["--help"], &["run", "--help"], &["report", "--help"]];

fn doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/src/reference/cli-help.txt")
}

fn render() -> String {
    let mut out = String::new();
    for args in COMMANDS {
        let output = Command::new(env!("CARGO_BIN_EXE_oaxaca-cli"))
            .args(*args)
            .env_remove("COLUMNS")
            .env_remove("NO_COLOR")
            .output()
            .expect("run oaxaca-cli");
        assert!(output.status.success(), "{args:?} failed");
        out.push_str(&format!("$ oaxaca-cli {}\n", args.join(" ")));
        out.push_str(&String::from_utf8(output.stdout).expect("utf-8 help"));
        out.push('\n');
    }
    out
}

#[test]
fn the_cli_reference_page_matches_the_shipped_help() {
    let actual = render();
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::write(doc_path(), &actual).expect("write cli-help.txt");
        return;
    }
    let expected = std::fs::read_to_string(doc_path()).expect("docs/src/reference/cli-help.txt");
    assert_eq!(
        expected, actual,
        "docs/src/reference/cli-help.txt is out of date. Regenerate it with \
         `UPDATE_DOCS=1 cargo test -p oaxaca_blinder --test cli_help_doc_test` and review the diff."
    );
}

#[test]
fn the_help_shown_on_the_page_carries_no_internal_ids() {
    let actual = render();
    for needle in ["MERIDIAN", "0120", "0121", "0122"] {
        assert!(
            !actual.contains(needle),
            "the help text names an internal id ({needle}); it is published on the docs site"
        );
    }
}
