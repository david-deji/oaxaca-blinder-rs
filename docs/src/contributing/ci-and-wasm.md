# CI and the WASM build

## The gate

`gate` is the only required check on `main`. It passes only when every other job passes:

| Job | What it checks |
|---|---|
| `quality` | formatting, clippy and the whole test suite, with all features and with default features; the loop scripts; the release scripts |
| `security` | `cargo audit`, and the dated list of accepted advisories in `.cargo/audit.toml` |
| `msrv` | the library compiles on Rust 1.88, the minimum supported version |
| `portability` | the command line and the MCP server type-check on macOS and Windows, the systems the release builds for |
| `memory-ceiling` | peak memory of a 50,000-row run stays under the stated ceiling |
| `docs` | the docs site builds, the API docs build without warnings, the doctests pass |
| `wasm-seq`, `wasm-threaded` | the engine's raw WASM bytes equal the committed hashes |
| `wasm-repro` | two fresh builds of the threaded blob are byte-identical |
| `native-baseline`, `browser-parity` | a browser run of the threaded build agrees with the native result |

A skipped or cancelled job counts as a failure. A workflow check refuses a change that would let a job pass without
running: a job-level `if`, `continue-on-error`, an unpinned action, wider permissions. Do not work around it. Change
the check in the same pull request if the rule itself is wrong.

## Why the WASM bytes are hashed

The engine ships to a web application as WebAssembly. Two artifacts are built: a sequential one on the pinned stable
compiler, and a threaded one (Rayon on web workers) on a pinned nightly with a rebuilt standard library. The raw
`cargo` output of each is the hashed unit, and its SHA-256 is committed in
`engine/pay_equity_engine.wasm.sha256` and `engine/pay_equity_engine.threaded.wasm.sha256`. A result is then tied to
exact bytes: the same source and toolchains give the same file.

`scripts/wasm-recipe.sh` holds the one build recipe: flags, toolchain pins and path remapping. Both the local script
and every CI job source it.

## When your change moves the bytes

Any change to the engine, or to the library code it compiles, can change the raw bytes. CI then fails the hash
comparison. When that is intended:

1. Take the new hashes from the `wasm-raw-seq` and `wasm-raw-threaded` artifacts of the failing run.
2. Confirm they equal a local `bash scripts/build-wasm.sh --record --no-publish`.
3. Commit both `engine/*.wasm.sha256` files in the same pull request.

Do this only while every golden test and the browser parity job stay green: the bytes moved, the numbers did not.

`bash scripts/build-wasm.sh --verify` builds both blobs and compares them with the committed hashes without writing
anything.

## Rules that are easy to trip over

- **Generators and goldens.** Never edit a script under `verification/gen_*` and never edit a golden. Each golden
  records the hash of its generator and the tests refuse a golden that no longer matches. A golden that has to change
  is a discussion in an issue first.
- **Results do not move silently.** A change to a statistical result needs a line in `CHANGELOG.md` that says what
  moved and by how much.
- **Dependencies.** Adding one needs a reason in the pull request. `Cargo.lock` is committed and CI builds with
  `--locked`.

## Releasing

A release is a tag push, and only a maintainer pushes one. `.github/workflows/release.yml` runs on a tag that looks
like `v0.3.0` and nothing else. Its jobs run in this order, and a failure stops everything after it:

1. `check-tag`: the tag is `v` plus the version in `oaxaca_blinder/Cargo.toml`; the tagged commit is on `main` and its
   `gate` check passed; `CHANGELOG.md` has a section for the version.
2. `verify` and `binaries`, in parallel: formatting, clippy, the tests and a dry-run publish; and `oaxaca-cli` and
   `meridian-mcp` built in release mode for Linux (x86-64), macOS (Apple silicon) and Windows (x86-64), each archived
   with both licence files and a SHA-256 file.
3. `publish-crates`: `cargo publish --workspace --locked`. This cannot be undone: crates.io never reuses a version.
   Only `oaxaca_blinder` publishes; the engine and the MCP server are `publish = false`.
4. `github-release` and `book`: the release page gets the changelog section as its text, the archives and two SBOMs;
   the documentation is built and pushed to the `gh-pages` branch under `v0.3/` and `latest/`.

Before the first tag, two things outside the repository must be set: the repository secret `CARGO_REGISTRY_TOKEN`, and
GitHub Pages serving the `gh-pages` branch (the workflow creates the branch on the first release).

To prepare a release: move the entries under `[Unreleased]` in `CHANGELOG.md` to a new `## [X.Y.Z] - date` heading,
set the version in the manifests, re-record the engine's WASM hashes if the engine's version changed (the version is
stamped into every result), and merge through `gate`. `python3 scripts/release-notes.py vX.Y.Z` prints what the
release page will say, and `python3 scripts/test-release-scripts.py` checks the scripts the workflow calls.

The workflow guard (`scripts/check-ci-invariants.py`) holds the release graph: every job waits for `check-tag`,
`publish-crates` waits for the checks and the builds, and the registry token is readable by `publish-crates` only.
