# Contributing

Thank you for looking at `oaxaca-blinder-rs`. The project is maintained by Telos Machina. It reports wage gaps that
people act on, so a change to a number is held to a higher bar than a change to a style.

## Before you start

- Search the existing issues and pull requests.
- For anything larger than a fix, open an issue first and describe the analysis you need. A new method needs an
  external reference to check against (a paper and an R or Stata command); a method with no oracle is accepted only
  as experimental.
- Security problems go through `SECURITY.md`, not a public issue.

## Licence

The crates are licensed `MIT OR Apache-2.0`. By submitting a contribution you agree that it is licensed on the same
terms, with no additional conditions.

## Making a change

1. Fork, then branch from `main`.
2. Make the change, with a test that fails without it.
3. Run the local checks below.
4. Add a line under `[Unreleased]` in `CHANGELOG.md` for anything a user can see: a changed number, a changed
   default, a new option, a removed one.
5. Open a pull request against `main` and fill in the template.

## Local checks

These are what CI runs. All must pass.

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
python3 scripts/check-ci-invariants.py
python3 scripts/test-ci-invariants.py
```

The development guide (`docs/DEVELOPMENT.md`) explains the layout of the workspace and how to run one test.

## What CI enforces

`gate` is the only required check on `main`. It passes only when every other job passes:

- formatting, clippy and the whole test suite;
- `cargo audit` and the dated list of accepted advisories in `.cargo/audit.toml`;
- the library compiling on Rust 1.88, the minimum supported version;
- the raw WASM bytes of the engine equal the committed hashes, and a browser run of the threaded build agrees with
  the native result.

A workflow check refuses a change that would let a job pass without running (a job-level `if`,
`continue-on-error`, an unpinned action). Do not work around it; change the check in the same pull request if the
rule itself is wrong.

## Rules that are easy to trip over

- **WASM hashes.** If your change alters the engine's raw WASM bytes, CI fails the hash comparison. When that is
  intended, take the new hashes from the `wasm-raw-seq` and `wasm-raw-threaded` artifacts of the failing run, confirm
  they equal a local `bash scripts/build-wasm.sh --record --no-publish`, and commit both
  `engine/*.wasm.sha256` files in the same pull request. Do this only while the golden tests and the browser parity
  job stay green: the bytes moved, the numbers did not.
- **Generators and goldens.** Never edit a script under `verification/gen_*` and never edit a golden. Each golden
  records the hash of its generator, and the tests refuse a golden that no longer matches. A golden that has to
  change is a discussion in an issue first.
- **Results do not move silently.** A change to a statistical result needs an entry in `CHANGELOG.md` that says what
  moved and by how much.
- **Dependencies.** Adding one needs a reason in the pull request. `Cargo.lock` is committed and CI builds with
  `--locked`.

## Code review

Reviewers look for numerical correctness against the literature, behaviour on edge cases (singular matrices, empty
groups, missing values), tests that would fail without the change, and no `unsafe`.
