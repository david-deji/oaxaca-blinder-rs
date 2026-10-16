## What changes and why
<!-- The problem, and what you did about it. Link the issue. -->

Fixes #

## Checks
- [ ] `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` and `cargo test --locked --workspace --all-features` pass locally
- [ ] A test fails without this change
- [ ] `CHANGELOG.md` has a line under `[Unreleased]` for anything a user can see
- [ ] No script under `verification/gen_*` and no golden is edited
- [ ] If the engine's raw WASM bytes changed: both `engine/*.wasm.sha256` are updated from the CI artifacts, and the golden tests and browser parity are green

## Numbers
<!-- Does any reported number change? Say which, by how much, and what you checked it against. Write "none" if not. -->

## Breaking changes
- [ ] This PR changes a public API, a default, a CLI flag or the MCP/JSON contract (describe the migration)
