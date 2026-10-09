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
# Usage (run from the oaxaca-blinder-rs workspace root):
#   bash scripts/build-wasm.sh              development: build both artifacts, run wasm-bindgen, publish into the
#                                           app with a manifest. Writes NO baseline.
#   bash scripts/build-wasm.sh --no-publish same, but leave the app's copies alone
#   bash scripts/build-wasm.sh --record     the ONLY mode that writes engine/*.sha256 (then commit them)
#   bash scripts/build-wasm.sh --verify     build both raw blobs, compare with `git show HEAD:engine/*.sha256`;
#                                           never writes a baseline, never publishes, exit 1 on a mismatch.
#                                           Writes target/wasm-verify.json for scripts/ground.sh and verify-live.sh.
#
# MANIFEST (0119 S7): wasm-bindgen output is NOT deterministic (two runs over one raw blob differ in ~120
# bytes of the _bg.wasm, glue JS identical; measured 2026-10-09 and recorded in issue 0119), so the shipped
# blob cannot be compared with a committed hash. Publishing therefore writes engine-manifest.json beside
# each shipped blob: the raw sha256 (reproducible, equals the baseline), the shipped _bg.wasm sha256, the
# engine commit it was built from and whether the tree was dirty. The app's freshness gate checks the blob
# against that manifest, and the manifest against the engine source, with no mtimes involved.
#
# PUBLISH (0017-P4): after both artifacts are generated the script copies them into the
# consuming Meridian app, by default the sibling checkout ../pay-equity-app/frontend/src/
# (override with MERIDIAN_FRONTEND=/path/to/frontend/src). Every copy is sha256-verified.
# Without this step the app keeps running the PREVIOUS build and no test catches it — the
# frontend suites mock the engine, so a stale blob ships green. See the PUBLISH block below.

set -euo pipefail

MODE=publish   # publish | record | verify
PUBLISH=1
for arg in "$@"; do
    case "$arg" in
        --no-publish) PUBLISH=0 ;;
        --record)     MODE=record ;;
        --verify)     MODE=verify ;;
        *) echo "build-wasm.sh: unknown argument '$arg' (use --verify, --record, --no-publish)" >&2; exit 2 ;;
    esac
done
if [ "$MODE" = verify ] && [ "$#" -gt 1 ]; then
    echo "build-wasm.sh: --verify never writes and never publishes; it takes no other flag" >&2; exit 2
fi
if [ "$MODE" = verify ]; then PUBLISH=0; fi

# Flags, pins and the build functions live in scripts/wasm-recipe.sh, shared with CI (0119 S1).
# shellcheck source=scripts/wasm-recipe.sh
source "$(dirname "${BASH_SOURCE[0]}")/wasm-recipe.sh"

# Paths whose content feeds the shipped blob. Derived from engine/Cargo.toml (path dependencies), the same
# rule the app's freshness gate uses, so a crate added later is covered by adding it to the manifest.
# engine/*.sha256 is excluded: a baseline is the OUTPUT of a build, not an input to it.
wasm_source_paths() {
    local p rel
    printf '%s\n' engine Cargo.toml Cargo.lock rust-toolchain.toml .cargo/config.toml scripts/wasm-recipe.sh
    for p in $(sed -n 's/.*path *= *"\([^"]*\)".*/\1/p' engine/Cargo.toml); do
        rel=$(realpath -m --relative-to=. "engine/$p")
        case "$rel" in ..*) ;; *) echo "$rel" ;; esac
    done
}
wasm_source_dirty() {   # prints the dirty paths, one per line (empty when the build inputs are clean)
    local paths
    mapfile -t paths < <(wasm_source_paths)
    git status --porcelain --untracked-files=normal -- "${paths[@]}" ':(exclude)engine/*.sha256'
}
head_baseline() {       # head_baseline <file> -> the sha256 recorded in HEAD, or empty
    git show "HEAD:$1" 2>/dev/null | cut -d' ' -f1
}

# ---------------------------------------------------------------------------
# --verify: build, compare with the baseline in HEAD, write nothing that ships. No installs either:
# a missing component is the preflight's job to name, not this mode's job to repair.
# ---------------------------------------------------------------------------
if [ "$MODE" = verify ]; then
    wasm_preflight all --no-bindgen
    echo "== [verify] building sequential wasm =="
    wasm_build_seq target/seq.raw.wasm
    echo "== [verify] building threaded wasm =="
    wasm_build_threaded target/threaded.raw.wasm
    SEQ_BUILT=$(sha256sum target/seq.raw.wasm | cut -d' ' -f1)
    THR_BUILT=$(sha256sum target/threaded.raw.wasm | cut -d' ' -f1)
    SEQ_HEAD=$(head_baseline "$SEQ_BASELINE")
    THR_HEAD=$(head_baseline "$THREADED_BASELINE")
    DIRTY=$(wasm_source_dirty | wc -l | tr -d ' ')
    FAIL=0
    SEQ_OK=false; THR_OK=false
    [ -n "$SEQ_HEAD" ] && [ "$SEQ_BUILT" = "$SEQ_HEAD" ] && SEQ_OK=true
    [ -n "$THR_HEAD" ] && [ "$THR_BUILT" = "$THR_HEAD" ] && THR_OK=true
    echo "verify seq      built $SEQ_BUILT"; echo "                head  ${SEQ_HEAD:-<none in HEAD>}  -> $([ "$SEQ_OK" = true ] && echo MATCH || echo MISMATCH)"
    echo "verify threaded built $THR_BUILT"; echo "                head  ${THR_HEAD:-<none in HEAD>}  -> $([ "$THR_OK" = true ] && echo MATCH || echo MISMATCH)"
    [ "$DIRTY" -eq 0 ] || echo "note: $DIRTY build-input path(s) have uncommitted changes; the comparison is against HEAD's baseline"
    [ "$SEQ_OK" = true ] && [ "$THR_OK" = true ] || FAIL=1
    mkdir -p target
    printf '{"ran_at":"%s","commit":"%s","dirty_paths":%s,"sequential":{"built":"%s","head_baseline":"%s","match":%s},"threaded":{"built":"%s","head_baseline":"%s","match":%s},"match":%s}\n' \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(git rev-parse HEAD)" "$DIRTY" \
        "$SEQ_BUILT" "$SEQ_HEAD" "$SEQ_OK" "$THR_BUILT" "$THR_HEAD" "$THR_OK" \
        "$([ "$FAIL" -eq 0 ] && echo true || echo false)" > target/wasm-verify.json
    [ "$FAIL" -eq 0 ] || { echo "verify FAILED: a raw blob does not equal HEAD's baseline. Intentional? run scripts/build-wasm.sh --record and commit engine/*.sha256." >&2; exit 1; }
    echo "verify OK: both raw blobs equal HEAD's baselines"
    exit 0
fi

# 1. wasm-bindgen-cli at the pinned version (must match the crate's wasm-bindgen dep).
if wasm-bindgen --version 2>/dev/null | grep -q "$WASM_BINDGEN_VERSION"; then
    echo "wasm-bindgen-cli $WASM_BINDGEN_VERSION already installed"
else
    echo "Installing wasm-bindgen-cli $WASM_BINDGEN_VERSION..."
    cargo install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --locked
fi

# 2. Pinned nightly with rust-src + the wasm target (threaded pass), then the preflight: it fails,
# printing host triple and toolchain dirs, when anything the recipe needs is missing.
echo "== ensuring pinned nightly ($NIGHTLY) + rust-src + wasm target =="
rustup toolchain install "$NIGHTLY" --component rust-src --target wasm32-unknown-unknown
wasm_preflight

ENGINE_COMMIT=$(git rev-parse HEAD)
ENGINE_DIRTY=false
[ -z "$(wasm_source_dirty)" ] || ENGINE_DIRTY=true
RUSTC_DESC=$(rustc --version)

# write_manifest <artifact> <outdir> <raw sha256> — engine-manifest.json, copied beside the shipped blob.
write_manifest() {
    local artifact="$1" outdir="$2" raw="$3" bg
    bg=$(sha256sum "$outdir/pay_equity_engine_bg.wasm" | cut -d' ' -f1)
    printf '{\n  "schema_version": 1,\n  "artifact": "%s",\n  "engine_commit": "%s",\n  "engine_dirty": %s,\n  "raw_sha256": "%s",\n  "bg_wasm_sha256": "%s",\n  "wasm_bindgen": "%s",\n  "rustc": "%s"\n}\n' \
        "$artifact" "$ENGINE_COMMIT" "$ENGINE_DIRTY" "$raw" "$bg" "$WASM_BINDGEN_VERSION" "$RUSTC_DESC" > "$outdir/engine-manifest.json"
}

# ---------------------------------------------------------------------------
# Artifact 1 — SEQUENTIAL (stable, --features wasm). Glue emitted here, before the threaded
# pass overwrites the raw wasm.
# ---------------------------------------------------------------------------
echo "== [seq] building sequential wasm (stable, --features wasm) =="
wasm_build_seq target/seq.raw.wasm
SEQ_HASH=$(sha256sum target/seq.raw.wasm | cut -d' ' -f1)
# engine/pkg/ is gitignored build output; a leftover pay_equity_engine_bg.js from an older bundler-target
# build is not produced by --target web and must never be published (0119 S7).
rm -f engine/pkg/pay_equity_engine_bg.js
wasm-bindgen target/seq.raw.wasm --out-dir engine/pkg --out-name pay_equity_engine --target web \
    --remove-name-section --remove-producers-section
# --target web emits no package.json either; write the one the app has always shipped, so a fresh checkout
# can publish (it used to exist only as a stale file in the gitignored engine/pkg/).
cat > engine/pkg/package.json <<'PKGJSON'
{
  "name": "pay-equity-engine",
  "type": "module",
  "collaborators": [
    "OpenPay Team <contact@openpay.ai>"
  ],
  "version": "0.1.0",
  "files": [
    "pay_equity_engine_bg.wasm",
    "pay_equity_engine.js",
    "pay_equity_engine.d.ts"
  ],
  "main": "pay_equity_engine.js",
  "types": "pay_equity_engine.d.ts",
  "sideEffects": [
    "./snippets/*"
  ]
}
PKGJSON
truncate -s -1 engine/pkg/package.json   # byte-identical to the file the app already ships (no trailing newline)
write_manifest sequential engine/pkg "$SEQ_HASH"

# ---------------------------------------------------------------------------
# Artifact 2 — THREADED (pinned nightly, build-std, atomics). Overwrites the raw wasm.
# ---------------------------------------------------------------------------
echo "== [threaded] building threaded wasm (build-std + atomics + link args) =="
wasm_build_threaded target/threaded.raw.wasm
THREADED_HASH=$(sha256sum target/threaded.raw.wasm | cut -d' ' -f1)
wasm-bindgen target/threaded.raw.wasm --out-dir engine/pkg-threaded --out-name pay_equity_engine --target web \
    --remove-name-section --remove-producers-section
write_manifest threaded engine/pkg-threaded "$THREADED_HASH"

# Baselines: written by --record and by nothing else (0119 S7). A development build that silently
# rewrote them made "the committed baseline" mean "whatever the last build produced".
if [ "$MODE" = record ]; then
    echo "$SEQ_HASH  pay_equity_engine.wasm" > "$SEQ_BASELINE"
    echo "$THREADED_HASH  pay_equity_engine.threaded.wasm" > "$THREADED_BASELINE"
    echo "== [record] baselines written: $SEQ_BASELINE $THREADED_BASELINE -- commit them =="
else
    for pair in "seq:$SEQ_BASELINE:$SEQ_HASH" "threaded:$THREADED_BASELINE:$THREADED_HASH"; do
        IFS=: read -r name file built <<<"$pair"
        recorded=$(head_baseline "$file")
        if [ "$built" = "$recorded" ]; then echo "  [$name] raw sha256 equals HEAD's baseline"
        else echo "  [$name] raw sha256 differs from HEAD's baseline (${recorded:-none}); run --record and commit if intentional"; fi
    done
fi

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

echo "Dual-artifact WASM build complete (mode: $MODE, engine commit $ENGINE_COMMIT, dirty: $ENGINE_DIRTY)."
echo "  [seq]      raw sha256: $SEQ_HASH   glue: engine/pkg/ (--target web)"
echo "  [threaded] raw sha256: $THREADED_HASH   glue: engine/pkg-threaded/ (--target web)"

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

    # Sequential glue + binary + manifest. Explicit list: exactly what this script produces in engine/pkg/.
    for f in package.json \
             pay_equity_engine.js \
             pay_equity_engine.d.ts \
             pay_equity_engine_bg.wasm \
             pay_equity_engine_bg.wasm.d.ts \
             engine-manifest.json; do
        publish_file "engine/pkg/$f" "$FRONTEND_SRC/wasm/$f"
    done

    # Threaded glue + binary + manifest. No *_bg.js here — the rayon --target web build inlines the
    # glue into pay_equity_engine.js and emits snippets/ instead.
    for f in package.json \
             pay_equity_engine.js \
             pay_equity_engine.d.ts \
             pay_equity_engine_bg.wasm \
             pay_equity_engine_bg.wasm.d.ts \
             engine-manifest.json; do
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

    echo "  [seq]      -> $FRONTEND_SRC/wasm/            (6 files incl. engine-manifest.json, sha256-verified)"
    echo "  [threaded] -> $FRONTEND_SRC/wasm-threaded/   (6 files incl. engine-manifest.json + snippets/, sha256-verified)"
    echo "  Frontend-owned analysis.worker.js, thread-cap.js, .gitignore left untouched."
fi
