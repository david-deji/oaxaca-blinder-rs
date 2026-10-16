//! The documentation comments of the library are rendered on docs.rs and the manifest ships in the package. Neither
//! may name an internal tracker id (`NNNN-MERIDIAN`), an invariant code (`INV-02`) or the removed `python.rs`: a
//! reader cannot open them and they say nothing about the code.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn is_internal_id(line: &str) -> bool {
    let bytes = line.as_bytes();
    // `NNNN-MERIDIAN`: four digits, then the project name.
    let tracker = line
        .match_indices("-MERIDIAN")
        .any(|(at, _)| at >= 4 && bytes[at - 4..at].iter().all(|b| b.is_ascii_digit()));
    let invariant = line
        .match_indices("INV-")
        .any(|(at, _)| bytes.get(at + 4).is_some_and(|b| b.is_ascii_digit()));
    tracker || invariant || line.contains("python.rs")
}

#[test]
fn doc_comments_and_the_manifest_name_no_internal_ids() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    let mut offenders = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("read source");
        for (n, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if (t.starts_with("///") || t.starts_with("//!")) && is_internal_id(t) {
                offenders.push(format!("{}:{}: {}", file.display(), n + 1, t));
            }
        }
    }
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("manifest");
    for (n, line) in manifest.lines().enumerate() {
        if line.trim_start().starts_with('#') && is_internal_id(line) {
            offenders.push(format!("Cargo.toml:{}: {}", n + 1, line.trim()));
        }
    }
    assert!(
        offenders.is_empty(),
        "internal ids in published text:\n{}",
        offenders.join("\n")
    );
}
