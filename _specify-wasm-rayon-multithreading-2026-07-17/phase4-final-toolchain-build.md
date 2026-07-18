# Phase 4 Final Spec — Toolchain + Threaded WASM Build Pipeline

> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Phase 4 refined-spec writer (toolchain-build)
> Status: FINAL — build handoff. /build reads this as source of truth for the toolchain domain.
> Strategy: **B (single threaded artifact, rayon sequential fallback)** — Strategy A fallback noted in § Design D7.

---

## Summary

Real Rayon parallelism in `pay-equity-engine`'s wasm build requires atomics + shared memory, which in 2026 is still **nightly-only**: `-Zbuild-std=panic_abort,std` remains unstable with no stable browser-threads path (ASM-02, valid until 2026-10-17). The toolchain domain delivers five buildable changes:

1. A pinned nightly (`nightly-2024-08-02`, the wasm-bindgen-rayon crate-tested pin) used **only** for the wasm-threads compile via `cargo +nightly-2024-08-02` override; `rust-toolchain.toml` stays on stable `1.90.0` so every native invocation (CLI, `meridian-mcp`, `oaxaca_blinder` rlib, `cargo fmt/clippy/test`) is byte-behaviour-unchanged (INV-01; `rust-toolchain.toml:5`).
2. A new `apps/hr-apps/oaxaca-blinder-rs/.cargo/config.toml` carrying the static threaded rustflags (`+atomics,+bulk-memory,+mutable-globals` + `--max-memory` + `-zstack-size`) under `[target.wasm32-unknown-unknown]` and `build-std` under `[unstable]`. The wasm target table is invisible to native targets (INV-01).
3. A `wasm-threads` feature in `engine/Cargo.toml` that pulls `wasm-bindgen-rayon 1.3.0` (optional) and re-exports `init_thread_pool`; the native/default feature set never compiles it.
4. A reworked `scripts/build-wasm.sh` that compiles the threaded raw wasm under the pinned nightly with build-std + link args, re-records the raw-wasm sha256 baseline (INV-04), and migrates the wasm-bindgen step from `--target bundler` to `--target web` (threaded glue is unsupported on bundler — a hard requirement regardless of reproducibility strategy).
5. A `ci.yml` `wasm-verify` job updated for the nightly toolchain, the committed baseline, and an **honest double-build reproducibility check** that recompiles std in **two distinct non-default `CARGO_TARGET_DIR`s** so Swatinem/rust-cache@v2 cannot mask nondeterminism (Correction W8).

This domain sits at the **threading** stage of the mandated build order (determinism → memory profile → threading → validation). It does not reorder that sequence: the final `--max-memory`/`-zstack-size` literals are owned by the memory-budget domain (In-Scope 5/11) and are consumed here as a cross-domain contract; the toolchain scaffolding is buildable ahead of that with the documented buildable-default literals, then finalized when memory-budget delivers the sized maximum.

---

## In-Scope (this domain)

- Pinned nightly toolchain (`nightly-2024-08-02` + `rust-src` + `wasm32-unknown-unknown`), `cargo +nightly` override, forward-bump procedure.
- `.cargo/config.toml` (NEW) — wasm target rustflags + `[unstable] build-std`.
- `wasm-threads` feature gating in `engine/Cargo.toml` + `init_thread_pool` re-export in `engine/src/lib.rs`.
- `scripts/build-wasm.sh` rework — threaded build-std compile, `--target web` migration, link args, baseline re-record.
- `.github/workflows/ci.yml` `wasm-verify` job — nightly install, `--locked`, baseline verify, double-build repro (two non-default target dirs).
- Reproducibility model (SC-04/INV-04) — single re-recorded raw-wasm baseline under Strategy B; sha256 double-build gate.
- `rust-toolchain.toml` — asserted unchanged (INV-01 guard).

**Owned elsewhere, consumed here (cross-domain, do not respec):** the `--max-memory`/`-zstack-size`/thread-cap values (memory-budget); the `init(url)` worker wiring + `crossOriginIsolated` feature-detect + COOP/COEP headers (meridian-integration); seeded per-rep RNG (deterministic-rng); the parity/benchmark suite and the threaded browser CI harness (verification-benchmark); the `oaxaca_blinder` parallel-surface verdicts (engine-parallel-surface).

---

## Design & Decisions

### D1 — Pinned nightly, native untouched (INV-01)

`rust-toolchain.toml:5` pins `channel = "1.90.0"` with `components = ["rustfmt", "clippy"]` and `targets = ["wasm32-unknown-unknown"]` (`rust-toolchain.toml:6-7`). This file is **not modified**. The threaded wasm compile is invoked exclusively via `cargo +nightly-2024-08-02` override in `scripts/build-wasm.sh` and CI, so native builds keep reading the stable pin.

- Default pin literal: **`nightly-2024-08-02`** — the wasm-bindgen-rayon crate-tested pin (`phase1-toolchain-build-nightly-reproducibility.md:19`), subject to the E1 preflight (below).
- Nightly install carries `rust-src` (build-std compiles std from source) + the wasm target:
  `rustup toolchain install nightly-2024-08-02 --component rust-src --target wasm32-unknown-unknown`.
- One source of truth for the date: the literal appears in `scripts/build-wasm.sh`, `.github/workflows/ci.yml`, and this spec — no floating `nightly` channel (baseline churn risk).

### D2 — `wasm-bindgen-rayon 1.3.0` compatibility is LOCKED (W1, ASM-01)

Settled in Phase 3, not a build-time research question:

- `wasm-bindgen-rayon 1.3.0` declares `wasm-bindgen = "0.2"` (caret → `>=0.2.0,<0.3.0`), which **admits the repo pin `wasm-bindgen 0.2.106`** (`engine/Cargo.toml:14`). No bindgen bump is forced (W1 [CONSENSUS]).
- Re-export mechanism is a plain function re-export — **`pub use wasm_bindgen_rayon::init_thread_pool;`** — no macro (W1).
- Re-exporting emits a JS-visible async `initThreadPool(numThreads) → Promise`; the Meridian worker calls `await init(); await initThreadPool(cap);` (worker wiring owned by meridian-integration).
- ASM-01 re-verify (valid until 2026-10-17) is folded into the E1 preflight compile — if 1.3.0 fails to resolve against 0.2.106 at build time, that surfaces as a link/resolve error in E1, not a silent pass.

### D3 — `.cargo/config.toml` (NEW) — static flags + build-std

`apps/hr-apps/oaxaca-blinder-rs/.cargo/config.toml`. The `[target.wasm32-unknown-unknown]` table applies to the wasm triple **only**; native targets never read it (INV-01). `[unstable] build-std` takes effect only under the nightly override and is a benign "unused config key" warning on stable native (risk-guarded by AC-8).

```toml
# WASM-threads build configuration for pay-equity-engine — issue 0014-MERIDIAN.
#
# [target.wasm32-unknown-unknown] applies to the wasm triple ONLY. Native targets
# (oaxaca-cli, meridian-mcp, the rlib) never read this table (INV-01). build-std
# requires the nightly channel; the [unstable] block takes effect only under
# `cargo +nightly-2024-08-02` and is ignored (benign "unused config key" warning)
# on the stable default toolchain.
#
# SOURCE-OF-TRUTH NOTE: scripts/build-wasm.sh sets RUSTFLAGS explicitly (static flags
# + dynamic path remaps). Setting env RUSTFLAGS REPLACES this target table (Cargo does
# NOT merge env RUSTFLAGS with config target.rustflags). The script therefore re-lists
# the static flags so the hashed blob is authoritative; this table exists for
# convenience `cargo +nightly build --target wasm32-unknown-unknown` dev runs.

[unstable]
build-std = ["panic_abort", "std"]

[target.wasm32-unknown-unknown]
rustflags = [
  "-C", "target-feature=+atomics,+bulk-memory,+mutable-globals",
  "-C", "link-arg=--max-memory=536870912",
  "-C", "link-arg=-zstack-size=1048576",
]
```

- `536870912` = 512 MiB (`--max-memory`); `1048576` = 1 MiB (`-zstack-size`) — **buildable defaults** from the 256–512 MiB cross-domain envelope. The memory-budget domain finalizes both; the final `--max-memory` must equal the sized shared-memory maximum (cross-domain contract).
- Precedence caveat (why the dynamic remap set cannot live here): Cargo picks exactly one rustflags source — env `RUSTFLAGS` > `[target.<triple>].rustflags` > `[build].rustflags`, no merge. The machine-specific `$HOME`/`$PWD`/`$HOME/.rustup` remaps use env expansion that config.toml cannot perform, so they are injected via `RUSTFLAGS` in the script; that injection overrides this target table, which is why the script re-lists the static flags. Intentional and documented in the file header.

### D4 — Feature gating (engine)

`engine/Cargo.toml` — the existing `wasm` feature block is at `engine/Cargo.toml:38-47`; `wasm-bindgen` optional dep at `:14`. Add the optional dep and the superset feature:

```toml
# [dependencies] addition
wasm-bindgen-rayon = { version = "1.3.0", optional = true }

# [features] addition (existing `wasm` block at :38-47 unchanged)
wasm-threads = ["wasm", "dep:wasm-bindgen-rayon"]
```

`engine/src/lib.rs` — the wasm wrappers begin at `engine/src/lib.rs:7-12` (the `#[cfg(feature = "wasm")]` import block) and `:18-22` (`init_panic_hook`). Add the pool re-export in the same wasm-cfg region, gated on the superset feature so native and sequential-wasm builds never see it:

```rust
#[cfg(feature = "wasm-threads")]
pub use wasm_bindgen_rayon::init_thread_pool;
```

- `wasm-threads` is never in the native/default feature set (`engine/Cargo.toml:34` `default = []`), so `cargo tree -p pay-equity-engine -i wasm-bindgen-rayon` is empty by default (AC-6).
- Rayon is already an **unconditional** dep of `oaxaca_blinder` (`oaxaca_blinder/Cargo.toml:23` — `rayon = "1.11.0"`), so no native-facing feature change is needed there; the toolchain layer requires no change to that crate's manifest. The `oaxaca_blinder`-side seeded-RNG plumbing is owned by deterministic-rng.
- **L4 deletion:** every `POLARS_MAX_THREADS=1` instruction from the Phase-2 draft is dropped. polars 0.44 never reads that env var on wasm (`POOL` is the `polars_utils::wasm::Pool` stub; `polars-utils-0.44.2 src/wasm.rs`); it routes `join`/`scope`/`spawn` onto **our** global rayon registry — cooperative work-stealing on one pool, not oversubscription. The only residual parallel-surface audit (polars-internal parallel float reductions on the hot path) is narrow (our stats run in nalgebra after `take`) and is owned by engine-parallel-surface — not a toolchain change.

### D5 — `scripts/build-wasm.sh` rework (Strategy B)

Current script is `scripts/build-wasm.sh:1-51` (stable `1.90.0`, `--features wasm`, no atomics, `wasm-bindgen --target bundler` at `:43`, baseline write at `:38-39`, remap at `:24`). The reworked Strategy-B script:

```bash
#!/usr/bin/env bash
# build-wasm.sh — Reproducible THREADED WASM build for pay-equity-engine (0014-MERIDIAN).
#
# Reproducibility model (INV-04, extends Track-0 SC-04):
#   * Native stays on stable via rust-toolchain.toml (1.90.0); this script overrides to a
#     pinned NIGHTLY for the wasm-threads compile only (INV-01).
#   * The RAW cargo-compiled wasm is the byte-reproducible hashed unit. build-std compiles
#     std from source, so the ~/.rustup remap is now LOAD-BEARING (std source paths embed).
#   * Threaded glue requires wasm-bindgen --target web (bundler is unsupported for threads).
#   * wasm-bindgen output stays NONDETERMINISTIC and is NOT hashed (pkg/ gitignored).
#
# Usage:  bash scripts/build-wasm.sh   (run from the oaxaca-blinder-rs workspace root)

set -euo pipefail

NIGHTLY="nightly-2024-08-02"
WASM_BINDGEN_VERSION="0.2.106"
RAW_WASM="target/wasm32-unknown-unknown/release/pay_equity_engine.wasm"
BASELINE="engine/pay_equity_engine.wasm.sha256"
MAX_MEMORY="536870912"   # 512 MiB — memory-budget domain owns the final value
STACK_SIZE="1048576"     # 1 MiB per-thread stack — memory-budget domain owns the final value

# Static threaded flags (must mirror .cargo/config.toml — env RUSTFLAGS REPLACES that table).
THREAD_FLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--max-memory=$MAX_MEMORY -C link-arg=-zstack-size=$STACK_SIZE"
# Dynamic path remaps (machine-specific; the .rustup remap is load-bearing under build-std).
REMAP="--remap-path-prefix $HOME/.cargo=/cargo --remap-path-prefix $PWD=/src --remap-path-prefix $HOME/.rustup=/rustup"

# 1. Ensure the pinned nightly + rust-src + wasm target are present.
rustup toolchain install "$NIGHTLY" --component rust-src --target wasm32-unknown-unknown

# 2. wasm-bindgen-cli at the pinned version.
if wasm-bindgen --version 2>/dev/null | grep -q "$WASM_BINDGEN_VERSION"; then
    echo "wasm-bindgen-cli $WASM_BINDGEN_VERSION already installed"
else
    echo "Installing wasm-bindgen-cli $WASM_BINDGEN_VERSION..."
    cargo install wasm-bindgen-cli --version "$WASM_BINDGEN_VERSION" --locked
fi

# 3. Build the threaded raw wasm (the reproducible, hashed unit).
RUSTFLAGS="$THREAD_FLAGS $REMAP" cargo "+$NIGHTLY" build \
    -p pay-equity-engine --features wasm-threads --target wasm32-unknown-unknown --release \
    -Zbuild-std=panic_abort,std --locked

# 4. Record/refresh the raw-wasm reproducibility baseline (committed; CI verifies).
RAW_HASH=$(sha256sum "$RAW_WASM" | cut -d' ' -f1)
echo "$RAW_HASH  pay_equity_engine.wasm" > "$BASELINE"

# 5. Generate JS glue + processed wasm into engine/pkg/ for app consumption.
#    --target web (threaded glue is unsupported on --target bundler).
wasm-bindgen "$RAW_WASM" --out-dir engine/pkg --target web \
    --remove-name-section --remove-producers-section

echo "Threaded WASM build complete."
echo "  Raw wasm (reproducible, hashed):        $RAW_WASM"
echo "  Baseline written:                       $BASELINE"
echo "  Raw sha256:                             $RAW_HASH"
echo "  App package (--target web, not hashed): engine/pkg/"
```

Deltas from `scripts/build-wasm.sh:1-51`: (1) `+$NIGHTLY` override added (native stays stable, INV-01); (2) `--features wasm` → `--features wasm-threads`; (3) `-Zbuild-std=panic_abort,std --locked` added; (4) `THREAD_FLAGS` prepended to `RUSTFLAGS` (re-listed because env RUSTFLAGS replaces the config.toml target table, D3); (5) `--target bundler` (`:43`) → `--target web`; (6) the `~/.rustup` remap (`:24`) is retained and now load-bearing because build-std compiles std from source. `--remove-name-section --remove-producers-section` are kept — they affect only the un-hashed `pkg/` blob.

### D6 — CI `wasm-verify` job (Strategy B; Correction W8 double-build shape)

Replace the current `wasm-verify` steps (`.github/workflows/ci.yml:39-82`) with the nightly threaded flow. The `quality` job (`:14-37`) and `security` job (`:84-93`) stay untouched — `quality` still uses `dtolnay/rust-toolchain@stable` reading `rust-toolchain.toml` (`:20-22`), proving native is unchanged (AC-13).

**Correction W8 (the fold-in):** Swatinem/rust-cache@v2 caches only `~/.cargo` + `./target` (dependency artifacts under the default target dir); its key includes RUSTFLAGS + hashes of Cargo.lock/Cargo.toml/rust-toolchain(.toml)/.cargo/config.toml (W8 [CONSENSUS]). A custom `CARGO_TARGET_DIR` is therefore **never restored from cache**. The honest double-build must build into **two distinct non-default target dirs** (`target-repro-a`, `target-repro-b`) — neither is `./target`, so both recompile std from source under `-Zbuild-std` and the sha256 comparison cannot be masked. (Equivalent alternative: disable rust-cache for the whole job. Two non-default dirs is preferred — it keeps cache speed on the baseline-verify build while keeping the repro check honest.)

```yaml
  wasm-verify:
    name: WASM Build + Verify (threaded)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup pinned nightly (rust-src + wasm target)
        uses: dtolnay/rust-toolchain@nightly
        with:
          toolchain: nightly-2024-08-02
          components: rust-src
          targets: wasm32-unknown-unknown

      - name: Rust Cache
        uses: Swatinem/rust-cache@v2
        with:
          shared-key: "rust-wasm-threaded"
          # Caches ./target only. The repro double-build below uses non-default
          # CARGO_TARGET_DIRs (target-repro-a / -b), which this action never restores,
          # so both repro builds recompile std from source — no cache masking (W8).

      - name: Install wasm-bindgen-cli (pinned)
        run: cargo install wasm-bindgen-cli --version 0.2.106 --locked

      - name: Build THREADED WASM (build-std + atomics) into ./target
        run: |
          RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals \
            -C link-arg=--max-memory=536870912 -C link-arg=-zstack-size=1048576 \
            --remap-path-prefix $HOME/.cargo=/cargo --remap-path-prefix $PWD=/src \
            --remap-path-prefix $HOME/.rustup=/rustup" \
            cargo +nightly-2024-08-02 build -p pay-equity-engine \
              --features wasm-threads --target wasm32-unknown-unknown --release \
              -Zbuild-std=panic_abort,std --locked
          # Smoke the binding step (output NOT hashed): --target web for threaded glue.
          wasm-bindgen \
            target/wasm32-unknown-unknown/release/pay_equity_engine.wasm \
            --out-dir engine/pkg --target web \
            --remove-name-section --remove-producers-section

      - name: Verify raw WASM sha256 (reproducibility baseline)
        run: |
          EXPECTED=$(cut -d' ' -f1 engine/pay_equity_engine.wasm.sha256)
          ACTUAL=$(sha256sum target/wasm32-unknown-unknown/release/pay_equity_engine.wasm | cut -d' ' -f1)
          if [ "$EXPECTED" != "$ACTUAL" ]; then
            echo "Raw WASM sha256 mismatch! Expected $EXPECTED Actual $ACTUAL"
            echo "If intentional, run scripts/build-wasm.sh and commit the updated baseline."
            exit 1
          fi
          echo "Raw WASM sha256 verified: $ACTUAL"

      - name: Reproducibility double-build (two non-default target dirs — no cache masking)
        run: |
          FLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals \
            -C link-arg=--max-memory=536870912 -C link-arg=-zstack-size=1048576 \
            --remap-path-prefix $HOME/.cargo=/cargo --remap-path-prefix $PWD=/src \
            --remap-path-prefix $HOME/.rustup=/rustup"
          # Build A into target-repro-a (non-default -> uncached -> std recompiled from source).
          RUSTFLAGS="$FLAGS" CARGO_TARGET_DIR=target-repro-a \
            cargo +nightly-2024-08-02 build -p pay-equity-engine \
              --features wasm-threads --target wasm32-unknown-unknown --release \
              -Zbuild-std=panic_abort,std --locked
          # Build B into target-repro-b (non-default -> uncached -> std recompiled from source).
          RUSTFLAGS="$FLAGS" CARGO_TARGET_DIR=target-repro-b \
            cargo +nightly-2024-08-02 build -p pay-equity-engine \
              --features wasm-threads --target wasm32-unknown-unknown --release \
              -Zbuild-std=panic_abort,std --locked
          A=$(sha256sum target-repro-a/wasm32-unknown-unknown/release/pay_equity_engine.wasm | cut -d' ' -f1)
          B=$(sha256sum target-repro-b/wasm32-unknown-unknown/release/pay_equity_engine.wasm | cut -d' ' -f1)
          if [ "$A" != "$B" ]; then
            echo "build-std output not byte-reproducible across fresh recompiles: $A vs $B"; exit 1
          fi
          echo "Double-build reproducibility verified (both std recompiles): $A"
```

### D7 — Reproducibility strategy: B recommended, A fallback

**RATIFIED 2026-07-18: Strategy A (dual artifact)** — see the A paragraph below; it is now the build path. `buildability-gate-rulings.md` ruling 1. The Strategy-B analysis is retained only for the rationale record.

**Strategy B (single threaded artifact) — NOT CHOSEN (rationale record).** Ship one nightly-built threaded blob; it degrades to sequential execution when `init_thread_pool` is never called (rayon's built-in seq fallback — `phase1-toolchain-build-dual-artifact-patterns.md:28`), satisfying INV-03 intrinsically with no second artifact to keep in sync. One re-recorded baseline (`engine/pay_equity_engine.wasm.sha256`), one `pkg/` (`--target web`), one CI block, one double-build check, clean `.cargo/config.toml`. Rationale: the reproducible surface CI must guard is halved vs A (INV-04 easier to hold with one hashed unit); native is equally protected under both (INV-01 via `+nightly` override either way); the loader branches on `crossOriginIsolated` under both, and B's branch only decides *whether to call `init_thread_pool`* on one blob (simpler than A's load-a-different-URL branch). Cost accepted: all consumers run nightly-built code even in sequential mode, and the existing stable baseline is retired — safe because the bit-identical parity job (INV-02, verification-benchmark) gates nightly-built seq output against native, and re-baselining is `committable` (Charter Classification).

**Strategy A (dual-artifact) — RATIFIED, the build path.** Preserve the untouched stable sequential baseline as a defensibility artifact: keep the current `scripts/build-wasm.sh:34-44` stable/`--features wasm`/`--target bundler` pass and its existing `engine/pay_equity_engine.wasm.sha256` untouched, then add a second pass (`+nightly` [E1-confirmed pin], `--features wasm-threads`, atomics + build-std + link args) writing a new `engine/pay_equity_engine.threaded.wasm.sha256` and `wasm-bindgen --target web --out-dir engine/pkg-threaded`; in `.cargo/config.toml`, omit the `[target.*] rustflags` (inject threaded flags via `RUSTFLAGS` in the nightly pass only, so they cannot contaminate the seq baseline) while keeping `[unstable] build-std` (ignored on stable); `ci.yml` gains a second build+verify block and runs the double-build on the threaded blob only. **The threaded baseline is generated inside the pinned CI/container, not the dev box** (council MJ-2: cross-machine build-std sha256 is fragile). Cost: two builds, two `pkg` dirs, two baselines, two CI blocks — accepted for the defensibility asset. Meridian conditionally dynamic-`import()`s the threaded glue in the isolated branch, sequential glue otherwise (meridian D5).

### D8 — E1/E2 as /build Phase 0 preflight (NOT specify research)

These are build-time verification tasks the /build orchestrator runs before dispatching workers — deterministic local build attempts, not open research. Each has a forward-bump fallback so the build cannot stall on a bad nightly.

- **E1 — nightly compile validation + ASM-01 re-verify.** Run:
  ```bash
  rustup toolchain install nightly-2024-08-02 --component rust-src --target wasm32-unknown-unknown
  cargo +nightly-2024-08-02 build -p pay-equity-engine \
    --features wasm-threads --target wasm32-unknown-unknown --release \
    -Zbuild-std=panic_abort,std --locked
  ```
  Confirms `nightly-2024-08-02` compiles the full dep set (polars 0.44 + clarabel 0.11.1 + nalgebra 0.32 + statrs 0.18) for wasm32 under build-std, and that `wasm-bindgen-rayon 1.3.0` resolves against `wasm-bindgen 0.2.106` (ASM-01).

  **Council MJ-4 (MAJOR — E1 will likely fail as written).** `nightly-2024-08-02` is ~13 months older than the `1.90.0` stable pin; the *locked* dep graph very likely will NOT compile on it (newer edition/MSRV requirements in the resolved deps). Two mitigations, in order:
  1. **Trim the wasm subgraph FIRST** so the tested-old nightly can build it: set `default-features = false` on `engine`'s `oaxaca_blinder` dependency for the wasm build (drops `comfy-table`/`display`), and cfg-gate any `askama`/CLI-only deps off `wasm32`. A leaner, lower-MSRV wasm subgraph is far more likely to build on the wasm-bindgen-rayon-tested nightly with `--locked` intact. Verify the trimmed subgraph still exports the 5 WASM entry points.
  2. **If still failing, forward-bump** the nightly in ~2-week steps (`nightly-2024-08-16`, `nightly-2024-09-02`, …) until the trimmed wasm subgraph builds under `-Zbuild-std(wasm32)` + links wasm-bindgen-rayon; record the working date as the pin literal in `scripts/build-wasm.sh`, `ci.yml`, and D1 (one source of truth). **Note:** a forward-bump changes the raw-wasm bytes, so the committed baseline (AC-12/AC-16) is PROVISIONAL until E1 fixes the pin — regenerate the baseline against the final nightly + trimmed subgraph, inside the pinned CI/container env (see D7 / buildability-gate Strategy note), not the dev box.
- **E2 — shared-memory link args.** Build E1 twice: once with `THREAD_FLAGS` as in D3, once additionally with `-C link-arg=--shared-memory` (and, if needed, `-C link-arg=--import-memory`). Determine whether `+atomics` alone makes lld emit a shared memory that wasm-bindgen-rayon accepts, or whether the explicit `--shared-memory`/`--import-memory` link args are required alongside `--max-memory`. Record the minimal linking set and, if `--shared-memory` is required, add it to `THREAD_FLAGS` in D3/D5/D6 before the build proceeds.
- **E3 (adjacent, owned by verification-benchmark/meridian, noted for sequencing):** theoretical cross-origin isolation of nested rayon pool workers is [CONSENSUS] resolved (W2), but proof that the pool actually *spawns* under our COOP/COEP + `--target web` glue is a runtime preflight in the browser harness — it depends on this domain's `pkg/` output but is not a toolchain change.

---

## Build Steps (ordered, buildable)

Ordering respects the mandated build order (determinism → memory profile → threading → validation). Steps 1–5 are the toolchain (threading-enablement) scaffolding and are buildable ahead of the final memory literals using the D3 buildable defaults; step 6 finalizes those literals when the memory-budget domain delivers.

1. **/build Phase 0 preflight** — run **E1** (nightly compile validation + ASM-01) and **E2** (shared-memory link args). Fix the pin literal (E1 forward-bump) and the minimal link set (E2) before any file edit. Do not proceed to step 2 until E1 compiles+links.
2. **Assert INV-01 guard** — confirm `rust-toolchain.toml:5` still reads `channel = "1.90.0"`; make no change to that file.
3. **Create `.cargo/config.toml`** — D3 content (with E2's confirmed link set).
4. **Feature-gate the engine** — edit `engine/Cargo.toml` (add `wasm-bindgen-rayon = { version = "1.3.0", optional = true }` and `wasm-threads = ["wasm", "dep:wasm-bindgen-rayon"]`) and `engine/src/lib.rs` (add `#[cfg(feature = "wasm-threads")] pub use wasm_bindgen_rayon::init_thread_pool;` in the wasm-cfg region near `:7-22`). Delete any `POLARS_MAX_THREADS=1` carried from the Phase-2 draft (L4).
5. **Rework `scripts/build-wasm.sh`** — D5 content; run it locally to produce the threaded raw wasm and re-record `engine/pay_equity_engine.wasm.sha256`. Commit the baseline; never commit `engine/pkg/` (gitignored).
6. **Finalize memory literals** — when memory-budget delivers the sized `--max-memory`/`-zstack-size`, replace the `536870912`/`1048576` defaults in `.cargo/config.toml` (D3), `scripts/build-wasm.sh` (`MAX_MEMORY`/`STACK_SIZE`), and `ci.yml` (both RUSTFLAGS blocks), then re-run step 5 to re-record the baseline against the final flags.
7. **Update `ci.yml` `wasm-verify`** — replace `:39-82` with the D6 job; leave `quality` (`:14-37`) and `security` (`:84-93`) untouched.
8. **Push; confirm CI green** — the nightly build compiles+links, the baseline sha256 matches, and the double-build (two non-default target dirs) asserts byte-equality.

---

## Acceptance Criteria

Each is a test, exit code, byte-compare, or file check.

- **AC-1 (INV-01, native untouched):** `git diff rust-toolchain.toml` is empty; `channel` remains `1.90.0` (`rust-toolchain.toml:5`).
- **AC-2 (native on stable):** `cargo build -p oaxaca-cli` and `cargo test --workspace` complete on the stable default toolchain with no `-Z`/nightly flags and no `[unstable]` errors (exit 0).
- **AC-3 (threaded blob links):** `cargo +nightly-2024-08-02 build -p pay-equity-engine --features wasm-threads --target wasm32-unknown-unknown --release -Zbuild-std=panic_abort,std --locked` exits 0 (E1).
- **AC-4 (pin is a single literal):** `grep -R "nightly-2024-08-02"` returns exactly the three sanctioned sites (`scripts/build-wasm.sh`, `.github/workflows/ci.yml`, this spec); no floating `nightly` channel appears in the script or CI.
- **AC-5 (config.toml exists, content-exact):** `apps/hr-apps/oaxaca-blinder-rs/.cargo/config.toml` exists with the D3 `[unstable]` and `[target.wasm32-unknown-unknown]` blocks; a bare `cargo +nightly-2024-08-02 build -p pay-equity-engine --features wasm-threads --target wasm32-unknown-unknown -Zbuild-std=panic_abort,std` (no env RUSTFLAGS) produces a wasm with atomics present.
- **AC-6 (native does not pull the rayon crate):** `cargo tree -p pay-equity-engine -i wasm-bindgen-rayon` returns empty on the default feature set.
- **AC-7 (feature wiring):** `engine/Cargo.toml` contains `wasm-threads = ["wasm", "dep:wasm-bindgen-rayon"]` and the optional `wasm-bindgen-rayon = { version = "1.3.0", optional = true }`; `engine/src/lib.rs` contains `#[cfg(feature = "wasm-threads")] pub use wasm_bindgen_rayon::init_thread_pool;`.
- **AC-8 (build-std in config is benign on stable):** `cargo build -p oaxaca-cli` on stable emits at most an "unused config key" warning for `[unstable] build-std` and exits 0; if it hard-errors, move `build-std` to the command line only (risk-guard, one-line change).
- **AC-9 (`--target web` migration):** `scripts/build-wasm.sh` and the `ci.yml` build step invoke `wasm-bindgen ... --target web` (not `bundler`); `grep -c "target bundler" scripts/build-wasm.sh` returns 0.
- **AC-10 (link args present):** the compile `RUSTFLAGS` in `scripts/build-wasm.sh` and both `ci.yml` build blocks contain `--max-memory=<mem>` and `-zstack-size=<stack>`; `<mem>` equals the memory-budget domain's sized maximum (cross-domain contract) after step 6.
- **AC-11 (`.rustup` remap load-bearing):** the composed `RUSTFLAGS` includes all three remaps (`$HOME/.cargo`, `$PWD`, `$HOME/.rustup`) in the script and CI.
- **AC-12 (baseline verify):** CI `wasm-verify` recomputes `sha256sum` of the raw threaded wasm and fails (exit 1) on mismatch with `engine/pay_equity_engine.wasm.sha256`.
- **AC-13 (double-build honesty, W8):** the CI double-build sets `CARGO_TARGET_DIR=target-repro-a` and `target-repro-b` (both non-default), asserts `sha256(a) == sha256(b)`, and fails (exit 1) on divergence; neither dir is `./target`, so both recompile std under `-Zbuild-std` (verifiable: `target-repro-a`/`target-repro-b` are absent from Swatinem cache scope, which is `./target` only).
- **AC-14 (native CI unchanged):** `git diff` of `ci.yml` shows no change to `quality` (`:14-37`) or `security` (`:84-93`); `quality` still uses `dtolnay/rust-toolchain@stable`.
- **AC-15 (no POLARS_MAX_THREADS):** `grep -R "POLARS_MAX_THREADS" .cargo scripts .github engine` returns 0 hits in this domain's files (L4).
- **AC-16 (INV-04 baseline committed + rebuildable):** `engine/pay_equity_engine.wasm.sha256` is committed and a fresh `bash scripts/build-wasm.sh` reproduces the same hash (double-build equality, AC-13).

---

## Open Items

Route to the buildability gate (SC-02) or to /build Phase 0 preflight — none block writing the spec; all have a defined resolution path.

1. **Reproducibility strategy — RATIFIED Strategy A** (2026-07-18, `buildability-gate-rulings.md` ruling 1). Dual artifact: untouched stable seq baseline + threaded `--target web` blob with its own container-generated baseline. No longer open.
2. **E1 pin confirmation (/build Phase 0).** `nightly-2024-08-02` compile validation with forward-bump; resolves before worker dispatch. If bumped, update the three sanctioned literals (AC-4).
3. **E2 shared-memory link set (/build Phase 0).** Confirm whether `--shared-memory`/`--import-memory` are required alongside `--max-memory`; add to `THREAD_FLAGS` if so, before the build proceeds.
4. **Memory literals finalization (cross-domain, memory-budget).** `--max-memory`/`-zstack-size` defaults (512 MiB / 1 MiB) are replaced by the sized values at build step 6; the baseline is re-recorded against the final flags.
5. **Cross-machine reproducibility (verification-benchmark).** The double-build proves byte-stability across two fresh recompiles on one host; two-host/two-container equality is owned by the verification-benchmark suite. If divergent across hosts, escalate — may motivate Strategy A's untouched-stable baseline as the defensibility artifact.

---

## Sources

- `phase2-spec-toolchain-build.md` — starting draft (this domain).
- `phase3-web-w1-w2-w3-w4-w5-w6-w7-w8-w9-w10-findings.md` — W1 (wasm-bindgen-rayon 1.3.0 ↔ 0.2.106, re-export), W8 (rust-cache double-build), W2/W3 (COI + Vite, cross-domain), L4 correction cross-ref.
- `phase3-local-l1-l2-l3-l4-l5-l6-findings.md` — L4 (POLARS_MAX_THREADS no-op on wasm; `polars-utils-0.44.2 src/wasm.rs` Pool stub), L2 (rand_chacha), L5 (clarabel single-threaded).
- `phase4-writer-brief.md` — the six Phase-3 corrections + locked decisions (Strategy B, W1 re-export, E1/E2-as-preflight).
- `spec-charter.md` — In-Scope 1/2, SC-01/02/04, INV-01/03/04, ASM-01/02, anti-goals (WASI, pkg/ commit, reproducibility weakening).
- Verified code anchors (this session, Read): `rust-toolchain.toml:5-7`; `engine/Cargo.toml:14,34,38-47`; `engine/src/lib.rs:7-12,18-22`; `oaxaca_blinder/Cargo.toml:23`; `scripts/build-wasm.sh:1-51` (bundler at `:43`, remap at `:24`, baseline at `:38-39`); `.github/workflows/ci.yml:14-37,20-22,39-82,70-82,84-93`.
- Phase-1 carry-forward (cited via phase2 draft): `phase1-toolchain-build-nightly-reproducibility.md:7,18,19,20,21,23,29`; `phase1-toolchain-build-dual-artifact-patterns.md:13,18,19,22,27,28,29`.
- External (W1/W8 Perplexity-returned): https://github.com/RReverser/wasm-bindgen-rayon ; https://docs.rs/crate/wasm-bindgen-rayon/latest/source/README.md ; https://github.com/Swatinem/rust-cache ; https://github.com/marketplace/actions/rust-cache
