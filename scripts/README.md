# Scripts

Tooling that CI and the maintainers run. None of it ships in a crate.

| Script | What it does | Run by |
|---|---|---|
| `build-wasm.sh` | Builds the engine's two raw WASM artifacts (sequential on stable, threaded on the pinned nightly), runs wasm-bindgen, and publishes into the Meridian app checkout. `--record` is the only mode that writes `engine/*.wasm.sha256`; `--verify` builds and compares and writes nothing. | maintainers |
| `wasm-recipe.sh` | The one build recipe (flags, toolchain pins, path remap) that `build-wasm.sh` and every CI job that builds a hashed blob source. Never executed on its own. | `build-wasm.sh`, CI |
| `check-ci-invariants.py` | Fails when a workflow could silence itself: a gating job with `continue-on-error` or `if`, an unpinned action, widened permissions, a `gate` that no longer needs every gating job. | CI (`gate`) |
| `test-ci-invariants.py` | Runs the checker against mutated copies of the workflows; every mutation must fail. | CI (`gate`) |
| `gate-step-audit.py` | Reads a finished run through the jobs API and fails when a successful job has a skipped step. | CI (`gate`) |
| `validate-audit-ignores.py` | Checks every ignore in `.cargo/audit.toml`: dated, unexpired, matched to a locked crate, classified. Also holds minimum locked versions for advisories cargo-audit cannot see. | CI (`security`) |
| `test-audit-validator.py` | One mutated `audit.toml` or `Cargo.lock` per failure class; each must fail. | CI (`security`) |
| `ground.sh` | Deterministic probes of the repository's state (suites, receipts, drift). Always exits 0; a probe that could not run is reported inside the JSON. Writes `records/ground/YYYY-MM-DD-probes.json`. | maintainers' dev loop |
| `verify-live.sh <epic>` | Runs the checks that need the Meridian app checkout and writes `records/ground/receipts/<epic>-live.json`. Refuses a dirty tree. | maintainers' dev loop |
| `test-loop-scripts.py` | Tests of `ground.sh` and `verify-live.sh`. | CI (`quality`) |
| `lib/` | Python and Node helpers behind the two scripts above. | |

The scripts that touch the Meridian app expect it as a sibling checkout (`../pay-equity-app`) and skip with a notice
when it is absent, so this repository stays usable on its own.
