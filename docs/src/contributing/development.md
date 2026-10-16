# Development

## Prerequisites

Rust 1.90.0 for development. `rust-toolchain.toml` pins it, because the engine's WASM bytes depend on the compiler
version. The library itself builds on Rust 1.88, its minimum supported version, and CI checks that.

## Set up

```bash
git clone https://github.com/david-deji/oaxaca-blinder-rs.git
cd oaxaca-blinder-rs
cargo build --workspace
```

## Checks

These are what CI runs. All must pass.

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo clippy --locked -p oaxaca_blinder --all-targets -- -D warnings   # default features
cargo test --locked -p oaxaca_blinder                                    # default features
python3 scripts/check-ci-invariants.py
python3 scripts/test-ci-invariants.py
```

The two default-feature lines build the library the way a crates.io user gets it, without the `experimental`
feature.

One test or one crate:

```bash
cargo test -p oaxaca_blinder --test parity_test
cargo test -p pay-equity-engine remedy
```

## Documentation

The docs site is an [mdBook](https://rust-lang.github.io/mdBook/) in `docs/`.

```bash
cargo install mdbook --version 0.4.52 --locked
mdbook build docs --dest-dir "$PWD/target/book"
mdbook serve docs
```

Three kinds of page text are generated and checked by tests:

- `docs/src/reference/cli-help.txt` is the `--help` of the command line.
- `docs/src/reference/mcp-tools.json` is the MCP server's `tools/list`.
- `docs/src/outputs/*.txt` are the printed output of the examples in `oaxaca_blinder/examples/` and of the sample
  command-line run.

After an intended change, regenerate with `UPDATE_DOCS=1`:

```bash
UPDATE_DOCS=1 cargo test -p oaxaca_blinder --test cli_help_doc_test
UPDATE_DOCS=1 cargo test -p oaxaca_blinder --test docs_examples_test
UPDATE_DOCS=1 cargo test -p meridian-mcp --test tools_doc_test
```

and read the diff before committing it. The README snippets are compiled as doctests, so a README that stops
compiling fails `cargo test --doc`.

## Coding guidelines

- **Errors.** Use `OaxacaError` in the library. Do not `panic!` in library code. `anyhow` is for the command line and
  the MCP server.
- **Performance.** Use Rayon for the bootstrap. Prefer Nalgebra operations to hand-written loops.
- **Documentation.** Every public item has a doc comment. For a statistical item, say what it computes and the
  convention it follows.
- **Hygiene.** Run `cargo fmt` and `cargo clippy` before committing.
- **No `unsafe`.**

## Running the MCP server

```bash
cargo run -p meridian-mcp                                   # standard input and output
MCP_API_KEY=local-test-key cargo run -p meridian-mcp -- --port 8084 --transport sse
```
