//! The MCP reference page of the docs site links `docs/src/reference/mcp-tools.json`, the schemas the server
//! lists. This test starts the real binary on standard input and output, asks for `tools/list` the way an MCP
//! client does, and compares the answer with that file, so a tool or argument that changes without the page
//! changing fails CI. It also proves the server answers a request that carries the standard `jsonrpc` key.
//!
//! Regenerate the file after an intended change:
//! `UPDATE_DOCS=1 cargo test -p meridian-mcp --test tools_doc_test`

use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/src/reference/mcp-tools.json")
}

/// Sends the lines to the server's standard input, closes it, and returns each line it printed as JSON.
fn talk(lines: &[&str]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_meridian-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start meridian-mcp");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        for line in lines {
            writeln!(stdin, "{line}").expect("write request");
        }
    }
    let output = child.wait_with_output().expect("server output");
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}")))
        .collect()
}

#[test]
fn a_request_with_the_standard_jsonrpc_key_is_answered() {
    let replies = talk(&[r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#]);
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert!(
        replies[0].get("error").is_none(),
        "the server refused a standard JSON-RPC request: {}",
        replies[0]
    );
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "meridian-mcp");
}

#[test]
fn the_server_reports_the_version_it_was_built_as() {
    let replies = talk(&[r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#]);
    assert_eq!(
        replies[0]["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION"),
        "serverInfo.version must be the crate version, not a literal"
    );
}

#[test]
fn the_mcp_reference_page_matches_the_listed_tools() {
    let replies = talk(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);
    assert_eq!(replies.len(), 1, "{replies:?}");
    let tools = replies[0]["result"].clone();
    assert!(tools["tools"].as_array().is_some_and(|t| !t.is_empty()));

    if std::env::var_os("UPDATE_DOCS").is_some() {
        let mut text = serde_json::to_string_pretty(&tools).expect("serialise");
        text.push('\n');
        std::fs::write(doc_path(), text).expect("write mcp-tools.json");
        return;
    }
    let committed: Value = serde_json::from_str(
        &std::fs::read_to_string(doc_path()).expect("docs/src/reference/mcp-tools.json"),
    )
    .expect("mcp-tools.json is JSON");
    assert_eq!(
        committed, tools,
        "docs/src/reference/mcp-tools.json is out of date. Regenerate it with \
         `UPDATE_DOCS=1 cargo test -p meridian-mcp --test tools_doc_test` and review the diff."
    );
}

#[test]
fn every_documented_tool_is_listed_and_every_listed_tool_is_documented() {
    let page = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/src/reference/mcp.md"),
    )
    .expect("mcp.md");
    let replies = talk(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);
    let listed: Vec<String> = replies[0]["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name").to_string())
        .collect();
    for name in &listed {
        assert!(
            page.contains(&format!("### `{name}`")),
            "tool {name} has no section in docs/src/reference/mcp.md"
        );
    }
    // And the page documents no tool the server does not list.
    for line in page.lines().filter(|l| l.starts_with("### `")) {
        let name = line.trim_start_matches("### `").trim_end_matches('`');
        assert!(
            listed.iter().any(|l| l == name),
            "mcp.md documents {name}, which the server does not list"
        );
    }
}
