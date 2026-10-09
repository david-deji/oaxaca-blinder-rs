#!/usr/bin/env bash
# wasm-recipe.sh — the ONE build recipe for the engine's raw WASM artifacts (0119-MERIDIAN S1).
#
# Sourced, never executed, from the repository root, by scripts/build-wasm.sh and by every CI job
# that builds a hashed blob. Flags and toolchain pins live here and nowhere else, so a local build
# and a CI build cannot drift apart.
#
# Why the sysroot remap exists: without the `rust-src` component, rustc embeds std source paths as
# /rustc/<commit>/library/...; with it, rustc embeds the on-disk location instead
# (<sysroot>/lib/rustlib/src/rust/library/...). `rust-src` is optional, so the same source produced
# two different blobs depending on whether the component was installed (0119 S1: CI sequential
# c639... vs local e70d..., 33 std paths differ). Mapping the on-disk location back to the
# /rustc/<commit> form makes the blob independent of the component.
#
# Requires: cwd = repository root.

NIGHTLY="nightly-2025-06-27"            # E1-proven pin (Phase 0); single source of truth
WASM_BINDGEN_VERSION="0.2.106"
MAX_MEMORY="342228992"                  # 326 MiB, the memory-budget domain's sized maximum (INV-05)
STACK_SIZE="1048576"                    # 1 MiB per-thread stack (memory-budget)
SEQ_BASELINE="engine/pay_equity_engine.wasm.sha256"
THREADED_BASELINE="engine/pay_equity_engine.threaded.wasm.sha256"

# Static threaded flags: injected via RUSTFLAGS in the nightly pass ONLY (never in
# .cargo/config.toml), so they cannot contaminate the stable sequential baseline (Strategy A).
THREAD_FLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--max-memory=$MAX_MEMORY -C link-arg=-zstack-size=$STACK_SIZE"

# Host-path remaps: reproducible across machines. Under -Zbuild-std the ~/.rustup remap is
# load-bearing (std source paths embed into the threaded blob).
REMAP="--remap-path-prefix $HOME/.cargo=/cargo --remap-path-prefix $PWD=/src --remap-path-prefix $HOME/.rustup=/rustup"

# Sequential pass only: map the optional rust-src location to the path rustc embeds without it.
# Must come AFTER the $HOME/.rustup rule above (rustc applies the last matching rule).
wasm_seq_flags() {
    local sysroot commit
    sysroot=$(rustc --print sysroot) || return 1
    commit=$(rustc -vV | sed -n 's/^commit-hash: //p')
    [ -n "$commit" ] || { echo "wasm-recipe: cannot read rustc commit-hash" >&2; return 1; }
    echo "$REMAP --remap-path-prefix $sysroot/lib/rustlib/src/rust=/rustc/$commit"
}

# wasm_preflight — fail early, with the facts needed to diagnose a different machine.
wasm_preflight() {
    local want_channel active host sysroot
    want_channel=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)
    active=$(rustc --version | cut -d' ' -f2)
    host=$(rustc -vV | sed -n 's/^host: //p')
    sysroot=$(rustc --print sysroot)
    echo "wasm-recipe preflight: host=$host toolchain=$active sysroot=$sysroot"
    local bad=0
    if [ "$active" != "$want_channel" ]; then
        echo "  ERROR: stable pass needs rustc $want_channel (rust-toolchain.toml), active is $active" >&2; bad=1
    fi
    if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
        echo "  ERROR: target wasm32-unknown-unknown missing on toolchain $active" >&2; bad=1
    fi
    local nightly_dir
    nightly_dir=$(rustc +"$NIGHTLY" --print sysroot 2>/dev/null) || nightly_dir=""
    if [ -z "$nightly_dir" ]; then
        echo "  ERROR: toolchain $NIGHTLY is not installed (threaded pass)" >&2; bad=1
    else
        echo "wasm-recipe preflight: threaded toolchain dir=$nightly_dir"
        if [ ! -d "$nightly_dir/lib/rustlib/src/rust/library" ]; then
            echo "  ERROR: component rust-src missing on $NIGHTLY (build-std needs it): $nightly_dir" >&2; bad=1
        fi
        if [ ! -d "$nightly_dir/lib/rustlib/wasm32-unknown-unknown" ]; then
            echo "  ERROR: target wasm32-unknown-unknown missing on $NIGHTLY: $nightly_dir" >&2; bad=1
        fi
    fi
    if ! wasm-bindgen --version 2>/dev/null | grep -q "$WASM_BINDGEN_VERSION"; then
        echo "  ERROR: wasm-bindgen-cli $WASM_BINDGEN_VERSION required, found: $(wasm-bindgen --version 2>&1 | head -1)" >&2; bad=1
    fi
    [ "$bad" -eq 0 ] || return 1
}

# Where cargo leaves the raw blob (honours CARGO_TARGET_DIR, used by the reproducibility double-build).
wasm_raw_path() { echo "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/pay_equity_engine.wasm"; }

# wasm_build_seq <outfile> — sequential raw blob (stable, --features wasm).
wasm_build_seq() {
    local out="$1" flags
    flags=$(wasm_seq_flags) || return 1
    RUSTFLAGS="$flags" cargo build --locked -p pay-equity-engine --features wasm \
        --target wasm32-unknown-unknown --release || return 1
    cp "$(wasm_raw_path)" "$out"
}

# wasm_build_threaded <outfile> — threaded raw blob (pinned nightly, build-std, atomics).
# Overwrites the raw blob in the target dir, so the caller copies the sequential blob out first.
wasm_build_threaded() {
    local out="$1"
    RUSTFLAGS="$THREAD_FLAGS $REMAP" cargo "+$NIGHTLY" build --locked \
        -p pay-equity-engine --features wasm-threads --target wasm32-unknown-unknown --release \
        -Zbuild-std=panic_abort,std || return 1
    cp "$(wasm_raw_path)" "$out"
}
