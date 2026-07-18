# Phase 1 — Toolchain: Nightly Pin + build-std Reproducibility

> Unit: u1 | Domain: toolchain-build | Date: 2026-07-17 | Method: perplexity_ask (medium context)

## Executive Summary

Threaded wasm32 builds remain nightly-only in 2026: `-Zbuild-std` is still unstable, no stable
browser-threads target exists or is announced. The wasm-bindgen-rayon maintainers' tested pin is
`nightly-2024-08-02`; ecosystem projects pin their own nightlies (e.g. `nightly-2025-01-16`
in @wasm-tool/rollup-plugin-rust). Byte-reproducibility with build-std is achievable in practice
but not formally guaranteed — the extra requirement vs our current stable model is remapping the
rustup toolchain path that contains `rust-src` (std is compiled from source, so its source paths
enter the artifact). Cargo.lock semantics are unchanged; std sources come deterministically from
the pinned toolchain's rust-src component.

## Findings

1. [CONSENSUS] Nightly + `-Zbuild-std=panic_abort,std` + `RUSTFLAGS="-C target-feature=+atomics,+bulk-memory"` (+`mutable-globals` in wasm-bindgen examples) remains the only path for wasm32-unknown-unknown threads; `-Zbuild-std` is documented unstable with no stabilization signal. (docs.rs wasm-bindgen-rayon README; doc.rust-lang.org cargo unstable; rustwasm raytrace example)
2. [CONSENSUS] Pin an exact nightly in rust-toolchain.toml with `components = ["rust-src"]`, `targets = ["wasm32-unknown-unknown"]`. Crate-tested pin: `nightly-2024-08-02`. No public known-bad-nightly list exists — the fixed-pin recommendation exists precisely because regressions have occurred. (docs.rs README, verbatim-confirmed by citation verifier; npmjs rollup-plugin-rust pins nightly-2025-01-16) [Correction 2026-07-17: a tlsnotary/tlsn issue previously cited as a second pin example was unverifiable on direct fetch and has been removed — the claim stands on the crate README alone.]
3. [SINGLE_SOURCE] Reproducibility requirements beyond our current remap set ($HOME/.cargo, $PWD, $HOME/.rustup): the rustup toolchain dir containing rust-src must be covered (std compiled from source embeds its paths). Our existing `--remap-path-prefix $HOME/.rustup=/rustup` likely already covers it since toolchains live under ~/.rustup/toolchains/ — verify empirically with a double-build diff. (cargo unstable docs + synthesis)
4. [CONSENSUS] `-Zbuild-std` does not alter Cargo.lock dependency resolution; std is an implicit dependency provided deterministically by the pinned toolchain. Use `--locked` in CI. (cargo unstable docs)
5. [REPORTED] wasm32-wasip1-threads is the more "official" threaded wasm target but is WASI, not browser — out of scope per Charter anti-goal. (rustc platform-support docs)
6. [UNVERIFIED] Byte-reproducibility of build-std output across machines under identical pin+flags+remaps: achievable per practitioner reports, no formal guarantee. Spec must include an empirical two-machine (or two-dir) rebuild-diff acceptance test, mirroring the existing SC-04 model.

## Spec Implications

- Pin selection is a Phase 3 question: `nightly-2024-08-02` (crate-tested, older) vs a newer nightly validated by our own build — spec should prescribe a validation procedure rather than blind-adopt.
- CI: `rustup toolchain install nightly-YYYY-MM-DD --component rust-src` step; keep existing stable pin for native jobs (dual-toolchain repo is normal — rust-toolchain.toml can stay stable with `cargo +nightly-...` in build-wasm.sh, or a per-directory override).
- The reproducibility acceptance test (build twice, sha256 equal) becomes part of the wasm-verify CI job regardless of chosen strategy.

## Sources

- https://docs.rs/crate/wasm-bindgen-rayon/latest/source/README.md
- https://doc.rust-lang.org/cargo/reference/unstable.html
- https://rustwasm.github.io/docs/wasm-bindgen/examples/raytrace.html
- https://dev-doc.rust-lang.org/nightly/rustc/platform-support/wasm32-wasip1-threads.html
- https://www.npmjs.com/package/@wasm-tool/rollup-plugin-rust
- https://blog.rust-lang.org/inside-rust/2025/07/21/sunsetting-the-rustwasm-github-org/

## Research Inventory

- Perplexity ask 2026-07-17 (this file's sole retrieval; citations above)
- Session recon 2026-07-17: scripts/build-wasm.sh, rust-toolchain.toml, ci.yml (repo ground truth)
