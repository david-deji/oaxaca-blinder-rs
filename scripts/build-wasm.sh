#!/usr/bin/env bash
# build-wasm.sh — Dual-artifact WASM build for pay-equity-engine (0014-MERIDIAN, Strategy A).
#
# Reproducibility model (INV-04; extends Track-0 SC-04): two independent artifacts, each with
# its own committed RAW-wasm sha256 baseline. The raw cargo wasm is the hashed unit; it is
# wasm-bindgen-target-independent, so the "sequential defensibility artifact" is the seq RAW
# wasm + its baseline, NOT the glue.
#
#   1. SEQUENTIAL (defensibility baseline): stable (rust-toolchain.toml 1.90.0) + --features wasm.
#        raw+baseline: engine/pay_equity_engine.wasm.sha256      glue: engine/pkg/ (--target web)
#      Stable toolchain, no atomics — the reproducible, audit-defensible artifact. It is NOT
#      contaminated by the threaded flags (those live only in the nightly pass's RUSTFLAGS,
#      never in .cargo/config.toml, so the stable target table cannot pick them up).
#
#   2. THREADED (rayon multithreading via wasm-bindgen-rayon): pinned NIGHTLY + --features
#        wasm-threads + -Zbuild-std + atomics + memory-budget link args.
#        raw+baseline: engine/pay_equity_engine.threaded.wasm.sha256  glue: engine/pkg-threaded/
#      build-std compiles std from source, so the ~/.rustup remap is LOAD-BEARING.
#
# Both glues are --target web (AC-9): the app imports the wasm-bindgen JS glue, which resolves
# *_bg.wasm via new URL(import.meta.url); Vite bundles it (no vite-plugin-wasm). Threaded glue is
# only supported on --target web. wasm-bindgen output is nondeterministic and is NOT hashed (pkg
# dirs gitignored); the raw cargo wasm is the hashed reproducible unit for each artifact.
#
# Native builds stay on stable via rust-toolchain.toml; this script overrides to the pinned
# nightly for the THREADED pass ONLY (INV-01).
#
# NOTE (council MJ-2 / buildability gate): the THREADED baseline is authoritative only when
# generated inside the pinned CI/container toolchain (cross-machine build-std sha256 is fragile);
# a dev-box run refreshes engine/pkg-threaded/ for local testing, CI re-records + verifies.
#
# Usage:  bash scripts/build-wasm.sh   (run from the oaxaca-blinder-rs workspace root)

set -euo pipefail

NIGHTLY="nightly-2025-06-27"            # E1-proven pin (Phase 0); single source of truth
WASM_BINDGEN_VERSION="0.2.106"
MAX_MEMORY="342228992"                  # 326 MiB — memory-budget domain's sized maximum (INV-05)
STACK_SIZE="1048576"                    # 1 MiB per-thread stack (memory-budget)

RAW_WASM="target/wasm32-unknown-unknown/release/pay_equity_engine.wasm"
SEQ_BASELINE="engine/pay_equity_engine.wasm.sha256"
THREADED_BASELINE="engine/pay_equity_engine.threaded.wasm.sha256"

# Host-path remaps: reproducible across machines. Under -Zbuild-std the ~/.rustup remap is
# load-bearing (std source paths embed into the threaded blob).
REMAP="--remap-path-prefix $HOME/.cargo=/cargo --remap-path-prefix $PWD=/src --remap-path-prefix $HOME/.rustup=/rustup"

# Static threaded flags — injected via RUSTFLAGS in the nightly pass ONLY (never in
# .cargo/config.toml), so they cannot contaminate the stable sequential baseline (Strategy A).
# E2 RESOLVED (verified 2026-07-18): the MINIMAL set below links — +atomics makes rustc
# auto-emit --shared-memory/--import-memory to lld, so the wasm-bindgen-rayon recipe needs only
# the target-features plus the memory-budget caps; no explicit --shared-memory/--import-memory.
THREAD_FLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--max-memory=$MAX_MEMORY -C link-arg=-zstack-size=$STACK_SIZE"

# 1. wasm-bindgen-cli at the pinned version (must match the crate's wasm-bindgen dep).
if wasm-bindgen --version 2>/dev/null | grep -q "$WASM_BINDGEN_VERSION"; then
    echo "wasm-bindgen-cli $WASM_BINDGEN_VERSION already installed"
else
    echo "Installing wasm-bindgen-cli $WASM_BINDGEN_VERSION..."
    cargo install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --locked
fi

# ---------------------------------------------------------------------------
# Artifact 1 — SEQUENTIAL (stable, --features wasm). Glue emitted here, before the threaded
# pass overwrites the raw wasm.
# ---------------------------------------------------------------------------
echo "== [seq] building sequential wasm (stable, --features wasm) =="
RUSTFLAGS="$REMAP" cargo build -p pay-equity-engine --features wasm --target wasm32-unknown-unknown --release
SEQ_HASH=$(sha256sum "$RAW_WASM" | cut -d' ' -f1)
echo "$SEQ_HASH  pay_equity_engine.wasm" > "$SEQ_BASELINE"
wasm-bindgen "$RAW_WASM" --out-dir engine/pkg --target web \
    --remove-name-section --remove-producers-section

# ---------------------------------------------------------------------------
# Artifact 2 — THREADED (pinned nightly, build-std, atomics). Overwrites the raw wasm.
# ---------------------------------------------------------------------------
echo "== [threaded] ensuring pinned nightly ($NIGHTLY) + rust-src + wasm target =="
rustup toolchain install "$NIGHTLY" --component rust-src --target wasm32-unknown-unknown

echo "== [threaded] building threaded wasm (build-std + atomics + link args) =="
RUSTFLAGS="$THREAD_FLAGS $REMAP" cargo "+$NIGHTLY" build \
    -p pay-equity-engine --features wasm-threads --target wasm32-unknown-unknown --release \
    -Zbuild-std=panic_abort,std --locked
THREADED_HASH=$(sha256sum "$RAW_WASM" | cut -d' ' -f1)
echo "$THREADED_HASH  pay_equity_engine.threaded.wasm" > "$THREADED_BASELINE"
wasm-bindgen "$RAW_WASM" --out-dir engine/pkg-threaded --target web \
    --remove-name-section --remove-producers-section

# wasm-bindgen does not emit a package.json for the threaded (--target web + rayon snippets)
# build, so the rayon workerHelpers `import('../../..')` (the pkg root) fails to resolve under
# Vite/rollup. Write one whose "main" points at the glue and whose "sideEffects" keeps the
# workerHelpers side-effect code from being tree-shaken.
cat > engine/pkg-threaded/package.json <<'PKGJSON'
{
  "name": "pay-equity-engine-threaded",
  "type": "module",
  "version": "0.1.0",
  "files": [
    "pay_equity_engine_bg.wasm",
    "pay_equity_engine.js",
    "pay_equity_engine.d.ts",
    "snippets/"
  ],
  "main": "pay_equity_engine.js",
  "types": "pay_equity_engine.d.ts",
  "sideEffects": [
    "./snippets/*"
  ]
}
PKGJSON

echo "Dual-artifact WASM build complete."
echo "  [seq]      raw sha256: $SEQ_HASH   -> $SEQ_BASELINE, engine/pkg/ (--target web)"
echo "  [threaded] raw sha256: $THREADED_HASH   -> $THREADED_BASELINE, engine/pkg-threaded/ (--target web)"
