#!/usr/bin/env bash
# build-wasm.sh — Reproducible WASM build for pay-equity-engine.
#
# Reproducibility model (Track-0 item 4, SC-04):
#   * The toolchain is pinned by rust-toolchain.toml (channel 1.90.0).
#   * The RAW cargo-compiled wasm (target/.../pay_equity_engine.wasm) is byte-reproducible
#     under the pin + path remapping. It is the unit we hash and verify in CI.
#   * wasm-bindgen 0.2.106 is NONDETERMINISTIC: it reorders ~654 bytes of the module body
#     per process run (verified 2026-06-06, even with debug sections stripped). The
#     post-processed engine/pkg/*_bg.wasm is therefore NOT hashed. engine/pkg/ is a build
#     convenience for app consumption only (gitignored in this repo).
#
# Usage:  bash scripts/build-wasm.sh   (run from the oaxaca-blinder-rs workspace root)

set -euo pipefail

WASM_BINDGEN_VERSION="0.2.106"
RAW_WASM="target/wasm32-unknown-unknown/release/pay_equity_engine.wasm"
BASELINE="engine/pay_equity_engine.wasm.sha256"

# Strip host-specific absolute paths so the blob is reproducible across machines.
# std library paths are already distribution-remapped to /rustc/<hash>/; the .rustup
# remap is defensive (no-op if std is the only sysroot source).
REMAP="--remap-path-prefix $HOME/.cargo=/cargo --remap-path-prefix $PWD=/src --remap-path-prefix $HOME/.rustup=/rustup"

# 1. wasm-bindgen-cli at the pinned version (must match the crate's wasm-bindgen dep).
if wasm-bindgen --version 2>/dev/null | grep -q "$WASM_BINDGEN_VERSION"; then
    echo "wasm-bindgen-cli $WASM_BINDGEN_VERSION already installed"
else
    echo "Installing wasm-bindgen-cli $WASM_BINDGEN_VERSION..."
    cargo install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --locked
fi

# 2. Build the engine crate (release, wasm target). This RAW artifact is the reproducible unit.
RUSTFLAGS="$REMAP" cargo build -p pay-equity-engine --features wasm --target wasm32-unknown-unknown --release

# 3. Record/refresh the raw-wasm reproducibility baseline (committed; CI verifies this).
RAW_HASH=$(sha256sum "$RAW_WASM" | cut -d' ' -f1)
echo "$RAW_HASH  pay_equity_engine.wasm" > "$BASELINE"

# 4. Generate the JS glue + processed wasm into engine/pkg/ for app consumption.
#    NOT hashed (wasm-bindgen output is nondeterministic). --remove-*-section trims size.
wasm-bindgen "$RAW_WASM" --out-dir engine/pkg --target bundler \
    --remove-name-section --remove-producers-section

echo "WASM build complete."
echo "  Raw wasm (reproducible, hashed):  $RAW_WASM"
echo "  Baseline written:                 $BASELINE"
echo "  Raw sha256:                       $RAW_HASH"
echo "  App package (not hashed):         engine/pkg/"
