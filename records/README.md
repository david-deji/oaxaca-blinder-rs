# Records

Working records of how this repository was built and checked. They are history, not documentation: nothing here
describes how to use the library, the CLI or the MCP server. For that, read the docs site and the crate READMEs.

| Path | What it holds |
|---|---|
| `LOOP-LEDGER.md` | One row per development cycle: what was picked, what it cost, what was left open. Written by the maintainers' dev loop. |
| `ground/` | Probe reports taken before each cycle (`*-probes.json`, `*-ground.md`), cycle reviews, `picks.jsonl` and `cost.jsonl` (the loop's own bookkeeping). |
| `ground/receipts/` | One JSON receipt per finished cycle: which checks ran, against which commit, and what they found. Counts, flags and hashes only. |
| `design/` | Specification and planning material for larger changes (the threaded WASM build). |
| `audits/` | Point-in-time audits of the workspace, one directory per audit date. |
| `research/` | Background notes on the methods. Not reviewed against primary sources; do not cite them. |

## Who writes here

The maintainers' scripts write `ground/` and `LOOP-LEDGER.md`: `scripts/ground.sh` writes the dated probes file and
`scripts/verify-live.sh <epic>` writes the receipt. Nobody edits a receipt by hand; a receipt describes one commit.

## Rules

- Receipts and probes are committed to a public repository. They carry counts, flags and hashes. They never carry
  absolute paths, file names from a private checkout, or `git status` output. The scripts scrub them.
- Old records are not rewritten. A correction is a new record that points at the old one.
- Files here may mention paths that have since moved (`ground/` was the repository root's folder before 0.3.0; it is
  now `records/ground/`). The oracle scripts in `verification/` name the old path in a comment and are not edited,
  because their hashes are recorded in the fixtures they generate.
