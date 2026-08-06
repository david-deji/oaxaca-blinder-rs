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
# Usage:  bash scripts/build-wasm.sh                (run from the oaxaca-blinder-rs workspace root)
#         bash scripts/build-wasm.sh --no-publish   (build only; leave the app's copies stale)
#
# PUBLISH (0017-P4): after both artifacts are generated the script copies them into the
# consuming Meridian app, by default the sibling checkout ../pay-equity-app/frontend/src/
# (override with MERIDIAN_FRONTEND=/path/to/frontend/src). Every copy is sha256-verified.
# Without this step the app keeps running the PREVIOUS build and no test catches it — the
# frontend suites mock the engine, so a stale blob ships green. See the PUBLISH block below.

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

# ---------------------------------------------------------------------------
# PUBLISH — copy the generated artifacts into the consuming app (0017-P4).
#
# Why this lives in the script rather than in a runbook: until 0017-P4 this step was manual and
# undocumented. The build wrote engine/pkg{,-threaded}/ and stopped, so an engine change and the
# artifact the browser actually runs could drift apart silently — and NOTHING catches it, because
# the frontend test suites mock the engine. During P4 the shipped blobs sat 3 hours stale against
# engine source while every suite stayed green. A manual step between a source change and the
# binary that ships is a stale-artifact defect waiting to happen; automating it is the fix.
#
# File-by-file, never a directory sync: frontend/src/wasm/ also holds analysis.worker.js,
# thread-cap.js and .gitignore, which are FRONTEND-owned and are not build output. An
# rsync --delete or a rm -rf + cp of that directory would delete them. If you ever "simplify"
# this into a directory copy, re-check that list first — it has grown once already.
# ---------------------------------------------------------------------------

PUBLISH=1
for arg in "$@"; do
    case "$arg" in
        --no-publish) PUBLISH=0 ;;
    esac
done

# Derived, never hardcoded: the app is a sibling checkout of this repo. Override with
# MERIDIAN_FRONTEND when the two live elsewhere relative to each other.
FRONTEND_SRC="${MERIDIAN_FRONTEND:-$PWD/../pay-equity-app/frontend/src}"

publish_file() {
    # publish_file <src> <dest> — copy, then prove the bytes match. A cp that half-wrote or
    # landed on a full disk must fail the build, not print a success line.
    local src="$1" dest="$2"
    cp "$src" "$dest"
    local a b
    a=$(sha256sum "$src" | cut -d' ' -f1)
    b=$(sha256sum "$dest" | cut -d' ' -f1)
    if [ "$a" != "$b" ]; then
        echo "  ERROR: published copy does not match source" >&2
        echo "         src  $src  $a" >&2
        echo "         dest $dest $b" >&2
        exit 1
    fi
}

if [ "$PUBLISH" -eq 0 ]; then
    echo ""
    echo "Publish SKIPPED (--no-publish). engine/pkg{,-threaded}/ are fresh; the app's"
    echo "src/wasm{,-threaded}/ are NOT — the browser will run the previous build."
elif [ ! -d "$FRONTEND_SRC/wasm" ] || [ ! -d "$FRONTEND_SRC/wasm-threaded" ]; then
    # Not an error: the engine repo is usable without the app checked out beside it.
    echo ""
    echo "Publish SKIPPED — no consuming app found at:"
    echo "  $FRONTEND_SRC/{wasm,wasm-threaded}"
    echo "Set MERIDIAN_FRONTEND to the app's src/ directory if it lives elsewhere."
else
    echo ""
    echo "== publishing artifacts to $FRONTEND_SRC =="

    # Sequential glue + binary. Explicit list: exactly what wasm-bindgen emits into engine/pkg/.
    for f in package.json \
             pay_equity_engine.js \
             pay_equity_engine.d.ts \
             pay_equity_engine_bg.js \
             pay_equity_engine_bg.wasm \
             pay_equity_engine_bg.wasm.d.ts; do
        publish_file "engine/pkg/$f" "$FRONTEND_SRC/wasm/$f"
    done

    # Threaded glue + binary. No *_bg.js here — the rayon --target web build inlines the glue
    # into pay_equity_engine.js and emits snippets/ instead.
    for f in package.json \
             pay_equity_engine.js \
             pay_equity_engine.d.ts \
             pay_equity_engine_bg.wasm \
             pay_equity_engine_bg.wasm.d.ts; do
        publish_file "engine/pkg-threaded/$f" "$FRONTEND_SRC/wasm-threaded/$f"
    done

    # snippets/ is wholly generated (rayon workerHelpers). Replace rather than merge, so a
    # snippet dropped by a newer wasm-bindgen does not linger and get bundled. Guarded on the
    # literal suffix so this rm can never walk anywhere else.
    SNIPPETS_DEST="$FRONTEND_SRC/wasm-threaded/snippets"
    case "$SNIPPETS_DEST" in
        */wasm-threaded/snippets) rm -rf "$SNIPPETS_DEST" ;;
        *) echo "  ERROR: refusing to remove unexpected path $SNIPPETS_DEST" >&2; exit 1 ;;
    esac
    cp -r engine/pkg-threaded/snippets "$SNIPPETS_DEST"

    echo "  [seq]      -> $FRONTEND_SRC/wasm/            (6 files, sha256-verified)"
    echo "  [threaded] -> $FRONTEND_SRC/wasm-threaded/   (5 files + snippets/, sha256-verified)"
    echo "  Frontend-owned analysis.worker.js, thread-cap.js, .gitignore left untouched."
fi
