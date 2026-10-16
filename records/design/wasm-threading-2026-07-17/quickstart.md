# Quickstart — building & verifying the threaded WASM engine (0014-MERIDIAN)

> For the /build implementer. Assumes the 7 phase4-final-*.md specs + buildability-gate-rulings.md are the source of truth. Run the Phase 0 preflights FIRST.

## Phase 0 preflights (before any file edit)
```bash
# E1 — validate the nightly compiles the (trimmed) wasm subgraph under build-std.
rustup toolchain install nightly-2024-08-02 --component rust-src --target wasm32-unknown-unknown
# Trim first (toolchain D8): default-features=false on engine's oaxaca_blinder dep; cfg-gate askama off wasm32.
cargo +nightly-2024-08-02 build -p pay-equity-engine \
  --features wasm-threads --target wasm32-unknown-unknown --release \
  -Zbuild-std=panic_abort,std --locked
# If it fails: forward-bump the nightly in ~2-week steps until the trimmed subgraph builds; record the pin.
# E2 — determine minimal shared-memory link args (--shared-memory/--import-memory vs +atomics alone).
# E3 — nested-worker initThreadPool spawn under headless Chromium (main-thread-relay fallback if it fails).
```

## Build order (inviolable)
1. **determinism** — land + verify the seeded RNG (deterministic-rng spec); `cargo test -p oaxaca_blinder rng::` green; INV-02 within-platform byte-identity holds.
2. **memory profile** — build + run the profile harness at 10k/25k/50k (memory-budget spec); commit the report; compute `N_max_const` BEFORE threading.
3. **threading** — `wasm-threads` feature + `init_thread_pool` re-export; Strategy A dual build (stable seq blob untouched + threaded `--target web` blob); Meridian worker wiring + COOP/COEP.
4. **validation** — the trust goldens (R `oaxaca`/`ddecompose`) + the mode-parity/memory-ceiling/reproducibility CI.

## Strategy A build (ratified)
```bash
# Artifact 1 (untouched): existing stable seq build — do NOT change its baseline.
bash scripts/build-wasm.sh            # stable, --target bundler, existing engine/pay_equity_engine.wasm.sha256
# Artifact 2 (new threaded): nightly + build-std + atomics + --target web, own baseline (generate in the pinned container).
#   writes engine/pay_equity_engine.threaded.wasm.sha256 + engine/pkg-threaded/ + frontend/src/wasm-threaded/
```

## In-Scope 12 (both surfaces → RIF)
- `engine/src/analysis.rs:166-206` and `oaxaca-cli` `main.rs:247` both → `OaxacaBuilder::...decompose_quantile(q)`.
- `decompose_quantile` (`builder.rs:720`): forward `self.seed`; recompute `calculate_rif` per bootstrap replicate; `fixed_rif:false`.
- Golden: `Rscript verification/gen_trust_goldens.R` (needs R + oaxaca 0.1.5 + quantreg + ddecompose); Rust tests read committed JSON (no R at `cargo test` time).

## Verify (all must pass)
```bash
cargo test --workspace                                  # native, incl. rng determinism + trust goldens
cargo test -p pay-equity-engine --test mode_parity_test # within-platform byte-identity
# CI: mode-parity (blocking), memory-ceiling@50k (blocking), reproducibility double-build (two CARGO_TARGET_DIRs),
#     threaded browser tests via custom COOP/COEP server + Playwright (NOT wasm-pack test).
```
