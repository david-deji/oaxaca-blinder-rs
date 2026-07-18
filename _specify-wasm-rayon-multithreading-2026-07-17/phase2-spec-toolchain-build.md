# Phase 2 Spec — Toolchain + Threaded WASM Build Pipeline

> Issue: 0014-MERIDIAN
> Bureau: hr-apps (Meridian)
> Date: 2026-07-17
> Author: Spec Writer (toolchain-build) — Draft

Domain scope: the threaded wasm32 toolchain, `.cargo/config.toml`, feature gating for the build layer, `scripts/build-wasm.sh` rework, the `--target web` migration, link args, `.github/workflows/ci.yml` wasm-verify changes, and the sha256 baseline / reproducibility procedure. RNG determinism, memory-budget numerics, engine parallelization verdicts, and Meridian worker JS are referenced from `phase2-cross-domain-summary.md`, not respecced here.

---

## 1. Executive Summary

Real Rayon parallelism in `pay-equity-engine`'s wasm build requires atomics + shared memory, which in 2026 is still **nightly-only**: `-Zbuild-std=panic_abort,std` remains unstable with no stable browser-threads path (`phase1-toolchain-build-nightly-reproducibility.md:7,18`). The build layer must therefore (a) introduce a pinned nightly used **only** for the wasm-threads blob while native builds stay on stable `1.90.0` (INV-01; `rust-toolchain.toml:5`), (b) compile with `-C target-feature=+atomics,+bulk-memory,+mutable-globals` + `-Zbuild-std` + shared-memory link args, (c) migrate the wasm-bindgen step from `--target bundler` to `--target web` — a hard requirement for threaded glue that applies **regardless of reproducibility strategy** (`phase1-toolchain-build-dual-artifact-patterns.md:13,18`), and (d) preserve the raw-wasm sha256 reproducibility model (INV-04; `scripts/build-wasm.sh:4-11`) by re-recording a byte-reproducible baseline and adding an empirical double-build check, since build-std reproducibility is achievable-in-practice but not formally guaranteed (`phase1-toolchain-build-nightly-reproducibility.md:23`).

Two reproducibility strategies are both fully specified in §4.6: **Strategy A — dual-artifact** (ship a stable sequential blob + a nightly threaded blob, two baselines) and **Strategy B — single threaded artifact** (ship one nightly threaded blob that runs sequentially when the pool is never initialized, one re-recorded baseline). Both keep native on stable via a `cargo +nightly-DATE` override; the difference is blob count, baseline count, `pkg/` dir count, and loader-branch shape. **Recommendation: Strategy B**, rationale in §4.6.3. The founder ratifies at the buildability gate (In-Scope 2; SC-02).

---

## 2. Requirements

### R1 — Pinned nightly for the wasm-threads build; native untouched

Introduce a pinned nightly toolchain used exclusively for the threaded wasm compile, invoked via `cargo +nightly-<DATE>` override in `scripts/build-wasm.sh` and CI. `rust-toolchain.toml` stays pinned to stable `1.90.0` so every native invocation (CLI, `meridian-mcp`, `oaxaca_blinder` rlib, `cargo fmt/clippy/test`) is unchanged.

- Default pin literal: `nightly-2024-08-02` (the wasm-bindgen-rayon crate-tested pin, `phase1-toolchain-build-nightly-reproducibility.md:19`), subject to the validation procedure in §4.1.
- Nightly carries `rust-src` + the wasm target: `rustup toolchain install nightly-2024-08-02 --component rust-src --target wasm32-unknown-unknown`.

**Acceptance criteria**
- AC-R1.1: `rust-toolchain.toml` `channel` remains `1.90.0`; diff against current shows no change to `channel`/`components` (INV-01).
- AC-R1.2: `cargo build -p oaxaca-cli` and `cargo test --workspace` run on stable with no nightly flags and no `[unstable]` errors.
- AC-R1.3: The threaded blob builds under `cargo +nightly-2024-08-02 build -Zbuild-std=panic_abort,std ...` and links (no unresolved atomics/shared-memory errors).
- AC-R1.4: The chosen nightly date is recorded as a literal in `scripts/build-wasm.sh`, `ci.yml`, and this spec's §4.1 — one source of truth, no `stable`/floating channel.

### R2 — `.cargo/config.toml` carries the threaded flags + build-std

Create `apps/hr-apps/oaxaca-blinder-rs/.cargo/config.toml` holding the static threaded rustflags (atomics/bulk-memory/mutable-globals + shared-memory link args) under `[target.wasm32-unknown-unknown]` and the build-std list under `[unstable]`. The target table applies to the wasm triple only; native targets never read it (INV-01). The dynamic path-remap set (machine-specific `$HOME`/`$PWD`) stays in the script-composed `RUSTFLAGS` because config.toml cannot expand env vars (§4.2 precedence note).

**Acceptance criteria**
- AC-R2.1: File exists with the exact content in §4.2.
- AC-R2.2: A bare `cargo +nightly-2024-08-02 build -p pay-equity-engine --features wasm-threads --target wasm32-unknown-unknown -Zbuild-std=panic_abort,std` (no env RUSTFLAGS) produces a threaded blob (atomics present) — proves config-driven dev ergonomics.
- AC-R2.3: `cargo build -p oaxaca-cli` (stable native) does not error on the `[unstable]` block; at most a benign "unused config key" warning (§4.2).
- AC-R2.4: The link args include `--max-memory` and `-zstack-size`; the `--max-memory` value equals the shared-memory maximum owned by the memory-budget domain (§5 cross-domain contract).

### R3 — Feature gating: `wasm-threads` in the engine; native path is off

Add a `wasm-threads` feature to `engine/Cargo.toml` that pulls `wasm-bindgen-rayon` and re-exports `init_thread_pool`. `wasm-threads` is a superset of `wasm` (never built for native). Rayon is already an unconditional dep of `oaxaca_blinder` (`oaxaca_blinder/Cargo.toml:23`), so no native-facing feature change is needed there; any `oaxaca_blinder`-side gate (e.g. `POLARS_MAX_THREADS`, seeded-RNG plumbing) is owned by the engine-parallel-surface / deterministic-rng domains (cross-domain, §5).

**Acceptance criteria**
- AC-R3.1: `engine/Cargo.toml` defines `wasm-threads = ["wasm", "dep:wasm-bindgen-rayon"]` and adds `wasm-bindgen-rayon = { version = "1.3.0", optional = true }` (§4.3).
- AC-R3.2: `cargo build -p pay-equity-engine` (native, default features) does not compile `wasm-bindgen-rayon` — verify via `cargo tree -p pay-equity-engine -i wasm-bindgen-rayon` returning empty on the default feature set.
- AC-R3.3: `engine/src/lib.rs` re-exports `init_thread_pool` behind `#[cfg(feature = "wasm-threads")]` (§4.3).
- AC-R3.4: The `wasm` (sequential) feature still builds unchanged for Strategy A's sequential blob and for the native `cdylib` smoke path.

### R4 — `build-wasm.sh` threaded build path with link args and `--target web`

Rework `scripts/build-wasm.sh` to build the threaded raw wasm under the pinned nightly with build-std + atomics + shared-memory link args, then run wasm-bindgen with `--target web` (threaded glue is unsupported on `--target bundler`; `phase1-toolchain-build-dual-artifact-patterns.md:18`). Re-record the raw-wasm sha256 baseline. Strategy A additionally retains the existing stable sequential build+baseline pass.

**Acceptance criteria**
- AC-R4.1: The script emits a raw threaded wasm at `target/wasm32-unknown-unknown/release/pay_equity_engine.wasm` and hashes it into the baseline (§4.4 / §4.6).
- AC-R4.2: wasm-bindgen is invoked with `--target web` for the threaded output dir (not `bundler`).
- AC-R4.3: Link args `--max-memory=<mem>` and `-zstack-size=<stack>` are present in the compile RUSTFLAGS (§4.4).
- AC-R4.4: `set -euo pipefail` retained; script fails loudly if the nightly toolchain or `rust-src` is missing.
- AC-R4.5: The composed `RUSTFLAGS` includes the full dynamic remap set (`$HOME/.cargo`, `$PWD`, `$HOME/.rustup`) — the `.rustup` remap is now load-bearing because build-std compiles std from source and embeds its `~/.rustup/toolchains/...` paths (`phase1-toolchain-build-nightly-reproducibility.md:20`).

### R5 — Reproducibility strategy: both implemented as concrete plans, one recommended

Present Strategy A (dual-artifact) and Strategy B (single threaded artifact) as executable plans; recommend B with rationale; leave the ratification to the founder at the buildability gate (SC-02). Include the sha256 baseline procedure and an empirical double-build reproducibility test for the chosen strategy (INV-04; `phase1-toolchain-build-nightly-reproducibility.md:23,29`).

**Acceptance criteria**
- AC-R5.1: §4.6 contains both strategies with exact file lists, build steps, and baseline layout.
- AC-R5.2: §4.6.3 states one recommendation with rationale tied to INV-01/03/04 and the loader-branch cost.
- AC-R5.3: A double-build test (build twice into two target dirs, assert sha256 equal) is specified and wired into CI (§4.7).
- AC-R5.4: The chosen strategy preserves at least one committed, re-buildable, hashed raw-wasm baseline (INV-04).

### R6 — CI `wasm-verify` job updated for the threaded toolchain + baseline(s)

Update the `wasm-verify` job in `ci.yml` to install the pinned nightly with `rust-src` + wasm target, run the threaded build with `--locked`, verify the baseline(s), and run the double-build reproducibility check. The `quality` and `security` jobs stay on stable and are untouched.

**Acceptance criteria**
- AC-R6.1: `wasm-verify` installs `nightly-2024-08-02` with `rust-src` and `wasm32-unknown-unknown` (§4.5).
- AC-R6.2: The build step uses `-Zbuild-std=panic_abort,std --locked` and the atomics/link-arg RUSTFLAGS (§4.5).
- AC-R6.3: The sha256 verify step(s) match the strategy's baseline layout (one baseline for B, two for A).
- AC-R6.4: A reproducibility step builds the raw wasm a second time into a distinct `CARGO_TARGET_DIR` and fails on sha256 divergence.
- AC-R6.5: `quality` job diff shows no toolchain change (still `dtolnay/rust-toolchain@stable` reading `rust-toolchain.toml`).

---

## 3. Technical Architecture

```
rust-toolchain.toml (stable 1.90.0)  ──governs──► native: CLI, meridian-mcp, rlib, fmt/clippy/test   [INV-01 untouched]
nightly-2024-08-02 (+rust-src)       ──override──► wasm-threads compile (build-std + atomics)
.cargo/config.toml [target.wasm32]   ──static flags──► atomics/bulk-memory/mutable-globals + --max-memory + -zstack-size
scripts/build-wasm.sh                ──authoritative──► RUSTFLAGS = static flags + dynamic remap; -Zbuild-std; wasm-bindgen --target web
   └─ produces ─► raw wasm ──sha256──► committed baseline(s)  ◄──verified── ci.yml wasm-verify (nightly job)
engine wasm-threads feature          ──pulls──► wasm-bindgen-rayon 1.3.0 ──exports──► init_thread_pool  (consumed by Meridian worker, cross-domain)
```

Key architectural facts (all cited):
- `--target bundler` cannot drive threaded glue; the worker's import path migrates to `--target web` init under **both** strategies (`phase1-toolchain-build-dual-artifact-patterns.md:13,29`). The Meridian-side `init(url)` wiring is owned by the meridian-integration domain (§5).
- One glue cannot serve both a sequential and a threaded blob — imports/exports differ between atomics and non-atomics builds; dual-artifact means two wasm-bindgen runs into two dirs (`phase1-toolchain-build-dual-artifact-patterns.md:19`).
- `-Zbuild-std` does not change `Cargo.lock` resolution; std is an implicit dep from the pinned nightly's `rust-src`; use `--locked` in CI (`phase1-toolchain-build-nightly-reproducibility.md:21`).
- Reproducibility of build-std output is empirically achievable but formally unguaranteed → an empirical double-build sha256 test is mandatory, mirroring the existing SC-04 model (`phase1-toolchain-build-nightly-reproducibility.md:23,29`).

---

## 4. Implementation Details

### 4.1 Nightly pin + validation procedure

Default pin: `nightly-2024-08-02`. Because no public known-bad-nightly list exists and our dep set (polars 0.44, nalgebra 0.32, clarabel 0.11.1, statrs 0.18) is heavier than the wasm-bindgen-rayon example, validate before locking:

```bash
# 1. install candidate nightly with rust-src + wasm target
rustup toolchain install nightly-2024-08-02 --component rust-src --target wasm32-unknown-unknown

# 2. attempt the threaded build (see 4.4 for the full RUSTFLAGS)
cargo +nightly-2024-08-02 build -p pay-equity-engine \
  --features wasm-threads --target wasm32-unknown-unknown --release \
  -Zbuild-std=panic_abort,std --locked

# 3. if it compiles, run the double-build reproducibility check (4.7).
#    if it FAILS to compile our deps, advance the nightly in ~2-week steps
#    (e.g. nightly-2024-08-16, nightly-2024-09-02) until it builds, then re-run step 3
#    and record the working date as the pin literal in build-wasm.sh + ci.yml.
```

> NEEDS RESEARCH: does `nightly-2024-08-02` compile the full dep set (polars 0.44 + clarabel 0.11.1 + nalgebra 0.32) for `wasm32-unknown-unknown` with `-Zbuild-std`? If not, the earliest nightly ≥ 2024-08-02 that does. Single-agent: run steps 1–2 above against `nightly-2024-08-02`, then the next 2–3 dated nightlies if it fails; report the first date that compiles + links.

### 4.2 `.cargo/config.toml` (NEW)

`apps/hr-apps/oaxaca-blinder-rs/.cargo/config.toml`:

```toml
# WASM-threads build configuration for pay-equity-engine — issue 0014-MERIDIAN.
#
# [target.wasm32-unknown-unknown] applies to the wasm triple ONLY. Native targets
# (oaxaca-cli, meridian-mcp, the rlib) never read this table, so native builds are
# byte-behaviour-equivalent (INV-01). build-std requires the nightly channel; the
# [unstable] block below takes effect only under `cargo +nightly-2024-08-02` and is
# ignored (benign "unused config key" warning) on the stable default toolchain.
#
# SOURCE-OF-TRUTH NOTE: scripts/build-wasm.sh sets RUSTFLAGS explicitly (static flags
# + dynamic path remaps). Setting RUSTFLAGS REPLACES this target table (Cargo does not
# merge env RUSTFLAGS with config target.rustflags). The script therefore re-lists the
# static flags below so the hashed blob is authoritative. This table exists for
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

Precedence caveat (why remap cannot live here): Cargo picks exactly one rustflags source — `RUSTFLAGS` env > `[target.<triple>].rustflags` > `[build].rustflags` (no merge). The dynamic remaps use machine-specific `$HOME`/`$PWD` and config.toml does not expand env vars, so they must be injected via `RUSTFLAGS` in the script; that injection overrides this target table, which is why the script re-lists the static flags. This is intentional and documented in the file header.

`536870912` = 512 MiB (`--max-memory`); `1048576` = 1 MiB (`-zstack-size`). Both are the memory-budget domain's to finalize (§5); the literals above are the buildable defaults from the cross-domain envelope (`phase2-cross-domain-summary.md:13`).

> NEEDS RESEARCH: with `nightly-2024-08-02` lld, does `+atomics` alone emit a shared memory (making `--shared-memory`/`--import-memory` unnecessary), or must `-C link-arg=--shared-memory` and/or `-C link-arg=--import-memory` be added explicitly alongside `--max-memory`? Single-agent: build 4.4 with and without `--shared-memory`; report which set links a valid shared-memory module that wasm-bindgen-rayon accepts.

### 4.3 Feature gating (engine)

`engine/Cargo.toml` — add the optional dep and the feature:

```toml
# [dependencies] additions
wasm-bindgen-rayon = { version = "1.3.0", optional = true }

# [features] additions (existing `wasm` block unchanged)
wasm-threads = ["wasm", "dep:wasm-bindgen-rayon"]
```

`engine/src/lib.rs` — re-export the pool initializer (threaded builds only):

```rust
#[cfg(feature = "wasm-threads")]
pub use wasm_bindgen_rayon::init_thread_pool;
```

Native untouched: `wasm-threads` is never in the native/default feature set; `cargo tree -p pay-equity-engine -i wasm-bindgen-rayon` is empty by default (AC-R3.2). The `oaxaca_blinder/Cargo.toml` `wasm-threads` passthrough named in the charter Deliverable Shape is owned by the deterministic-rng / engine-parallel domains (rayon is already unconditional there — `oaxaca_blinder/Cargo.toml:23`); the toolchain layer requires no change to that crate's manifest.

> NEEDS RESEARCH: is `wasm-bindgen-rayon 1.3.0` API-compatible with the repo-pinned `wasm-bindgen 0.2.106` (ASM-01, valid until 2026-10-17)? Single-agent: confirm 1.3.0's `wasm-bindgen` dep range includes 0.2.106 and that `init_thread_pool` is the current export name; re-verify at build time per ASM-01.

### 4.4 `scripts/build-wasm.sh` — threaded path (Strategy B primary; Strategy A variant in §4.6.1)

Rework (Strategy B, single threaded blob):

```bash
#!/usr/bin/env bash
# build-wasm.sh — Reproducible THREADED WASM build for pay-equity-engine (0014-MERIDIAN).
#
# Reproducibility model (INV-04, extends Track-0 SC-04):
#   * Native stays on stable via rust-toolchain.toml (1.90.0); this script overrides to a
#     pinned NIGHTLY for the wasm-threads compile only.
#   * The RAW cargo-compiled wasm is the byte-reproducible hashed unit. build-std compiles
#     std from source, so the ~/.rustup remap is now load-bearing (std source paths embed).
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
STACK_SIZE="1048576"     # 1 MiB per-thread stack

# Static threaded flags (must mirror .cargo/config.toml — env RUSTFLAGS overrides that table).
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
echo "  Raw wasm (reproducible, hashed):  $RAW_WASM"
echo "  Baseline written:                 $BASELINE"
echo "  Raw sha256:                       $RAW_HASH"
echo "  App package (--target web, not hashed): engine/pkg/"
```

Notes: (1) `+$NIGHTLY` override leaves native on stable (INV-01). (2) `THREAD_FLAGS` is re-listed here because setting `RUSTFLAGS` replaces the config.toml target table (§4.2). (3) `--remove-name-section --remove-producers-section` retained; they affect only the un-hashed `pkg/` blob.

### 4.5 `ci.yml` — `wasm-verify` job (Strategy B primary)

Replace the current `wasm-verify` steps (`.github/workflows/ci.yml:39-82`) with the nightly threaded flow; `quality` (`:14-37`) and `security` (`:84-93`) stay untouched.

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

      - name: Install wasm-bindgen-cli (pinned)
        run: cargo install wasm-bindgen-cli --version 0.2.106 --locked

      - name: Build THREADED WASM (build-std + atomics)
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

      - name: Reproducibility double-build (build-std is not formally guaranteed byte-stable)
        run: |
          RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals \
            -C link-arg=--max-memory=536870912 -C link-arg=-zstack-size=1048576 \
            --remap-path-prefix $HOME/.cargo=/cargo --remap-path-prefix $PWD=/src \
            --remap-path-prefix $HOME/.rustup=/rustup" \
            CARGO_TARGET_DIR=target-repro \
            cargo +nightly-2024-08-02 build -p pay-equity-engine \
              --features wasm-threads --target wasm32-unknown-unknown --release \
              -Zbuild-std=panic_abort,std --locked
          A=$(sha256sum target/wasm32-unknown-unknown/release/pay_equity_engine.wasm | cut -d' ' -f1)
          B=$(sha256sum target-repro/wasm32-unknown-unknown/release/pay_equity_engine.wasm | cut -d' ' -f1)
          if [ "$A" != "$B" ]; then
            echo "build-std output not byte-reproducible across target dirs: $A vs $B"; exit 1
          fi
          echo "Double-build reproducibility verified: $A"
```

> NEEDS RESEARCH: does `Swatinem/rust-cache@v2` cache the `-Zbuild-std` std artifacts correctly, or does the double-build step share cache and mask a nondeterminism? Single-agent: confirm the double-build uses a cold `target-repro` (it does — separate `CARGO_TARGET_DIR`) and that std is recompiled, not copied; if cache leaks across the two dirs, disable cache for the repro step.

### 4.6 Reproducibility strategies (both implementations)

#### 4.6.1 Strategy A — Dual-artifact (sequential stable blob + threaded nightly blob)

Ship two blobs; the loader selects at runtime (the crossOriginIsolated branch is needed anyway — `phase1-toolchain-build-dual-artifact-patterns.md:27`). Files/steps:

- **`build-wasm.sh`**: two passes.
  - Pass 1 (unchanged from current, `scripts/build-wasm.sh:34-44`): stable `1.90.0`, `--features wasm`, no atomics, `--remap` only → raw seq wasm → `engine/pay_equity_engine.wasm.sha256` → `wasm-bindgen --target bundler --out-dir engine/pkg`. **Existing baseline untouched (INV-04 evidence chain intact).**
  - Pass 2 (new): `+nightly-2024-08-02`, `--features wasm-threads`, atomics + build-std + link args → raw threaded wasm → **new** `engine/pay_equity_engine.threaded.wasm.sha256` → `wasm-bindgen --target web --out-dir engine/pkg-threaded`.
- **`.cargo/config.toml`**: the `[target.wasm32-unknown-unknown]` atomics table would contaminate Pass 1's seq blob (changing its baseline). Under Strategy A, **omit the `[target.*] rustflags` from config.toml** and inject threaded flags via `RUSTFLAGS` in Pass 2 only; keep `[unstable] build-std` in config (ignored on the stable Pass 1). This is the one place the strategies' config.toml differs.
- **`ci.yml`**: two build+verify blocks (stable seq job step + nightly threaded job step), two sha256 verifies, double-build on the threaded blob only.
- **Baselines**: `engine/pay_equity_engine.wasm.sha256` (seq, unchanged) + `engine/pay_equity_engine.threaded.wasm.sha256` (new).
- **Meridian** (cross-domain): loads `pkg` (bundler) for seq and `pkg-threaded` (web) for threaded; loader branches on `crossOriginIsolated`.

Cost: two builds (std rebuilt for the threaded pass), two `pkg` dirs, two baselines, two CI blocks, doubled test surface (`phase1-toolchain-build-dual-artifact-patterns.md:22`). Benefit: the stable seq baseline and its Meridian bundler-glue path are **untouched**; the nightly surface is purely additive.

#### 4.6.2 Strategy B — Single threaded artifact (recommended)

Ship one threaded blob; it runs sequentially when `init_thread_pool` is never called (rayon's built-in seq fallback — `phase1-toolchain-build-dual-artifact-patterns.md:28`). This is the `build-wasm.sh` in §4.4 and the `ci.yml` in §4.5. One baseline (`engine/pay_equity_engine.wasm.sha256`, re-recorded from the threaded raw wasm), one `pkg/` (`--target web`), one CI block, one double-build check. `.cargo/config.toml` is clean (§4.2) because the only wasm build is threaded. Meridian loads the single `--target web` blob and conditionally calls `init_thread_pool` behind the crossOriginIsolated check (cross-domain).

#### 4.6.3 Recommendation — Strategy B

Recommend **Strategy B (single threaded artifact)**. Rationale:
1. **Fewest moving parts to keep reproducible**: one raw-wasm baseline, one build path, one glue target — the reproducibility surface CI must guard is halved vs A (INV-04 is easier to hold with one hashed unit).
2. **Native is equally protected under both** — both keep `rust-toolchain.toml` on stable and use `+nightly` override (INV-01 holds either way), so A's "native untouched" edge is not actually an advantage.
3. **The loader must branch on `crossOriginIsolated` under both** (`phase2-cross-domain-summary.md:12`); B's branch only decides *whether to call `init_thread_pool`* on one blob, which is simpler than A's *load-a-different-URL-and-glue* branch.
4. **`.cargo/config.toml` stays idiomatic** (SC-01 wants config-driven rustflags); B lets the atomics table live in config without contaminating a sibling seq build, whereas A must special-case it out (§4.6.1).
5. **INV-03 fallback is satisfied intrinsically**: the same blob degrades to sequential when the pool is never initialized — no separate fallback artifact to keep in sync.

Cost accepted: all consumers run nightly-built code even in sequential mode, and the existing stable baseline is retired. This is safe because (a) the bit-identical parity job (INV-02) gates native / wasm-seq / wasm-threaded byte-equality (`phase2-cross-domain-summary.md:18`), so nightly-built seq output is verified against native, and (b) reversibility is `committable` — the baseline is re-recordable (charter Classification).

Founder ratifies at the buildability gate (SC-02). If the founder prefers to preserve the untouched stable baseline as a defensibility artifact, Strategy A is fully specified in §4.6.1 and requires only the config.toml carve-out + the second CI block.

### 4.7 sha256 baseline procedure

- **Record**: `scripts/build-wasm.sh` writes `sha256sum(raw_wasm)` into the baseline file(s) for the chosen strategy (§4.4 / §4.6.1). Commit the baseline; never commit `pkg/`/`pkg-threaded/` (gitignored, charter anti-goal; `scripts/build-wasm.sh:11`).
- **Verify (CI)**: `wasm-verify` recomputes the raw-wasm hash and fails on mismatch (§4.5), exactly as today (`.github/workflows/ci.yml:70-82`).
- **Empirical reproducibility (CI + local)**: build the raw wasm twice into distinct `CARGO_TARGET_DIR`s and assert equal (§4.5 double-build). This is the acceptance test for the UNVERIFIED build-std byte-reproducibility claim (`phase1-toolchain-build-nightly-reproducibility.md:23`) and satisfies INV-04's "rebuilds it byte-identically."
- **Re-baseline on intentional change**: run `bash scripts/build-wasm.sh`, commit the updated baseline (CI failure message already instructs this — `.github/workflows/ci.yml:78-79`).

---

## 5. Dependencies and Integrations

Cross-domain contracts (owned elsewhere; this spec consumes/produces the named interface):

| Interface | Owner domain (per `phase2-cross-domain-summary.md`) | Contract this spec depends on |
|---|---|---|
| `--max-memory` value + `-zstack-size` + thread-cap formula | memory-budget | Provides the exact shared-memory maximum; §4.2/§4.4 use `536870912`/`1048576` as buildable defaults from the 256–512 MiB envelope (`:13`). The final `--max-memory` must equal the sized maximum. |
| `init_thread_pool` call + `crossOriginIsolated` feature-detect + seq fallback | meridian-integration | This spec exports `init_thread_pool` (§4.3) and produces the `--target web` glue; the worker's `init(url)` + pool init is owned there (`:16`). |
| COOP/COEP headers on audit-forge `/pay-equity/` + Vite dev | meridian-integration | Required for SharedArrayBuffer at runtime; not a build-layer change (`:16`). |
| Seeded per-rep RNG (bit-identical, INV-02) | deterministic-rng | Makes nightly-built seq output byte-equal to native, which underwrites the Strategy-B recommendation (§4.6.3). Unconditional in `oaxaca_blinder`; no toolchain gate (`:14`). |
| Parity job (native / wasm-seq / wasm-threaded 2,4) | verification-benchmark | Consumes the blob(s) this spec builds; the double-build repro job (§4.5) is this spec's contribution to that suite (`:18`). |
| `oaxaca_blinder` parallel-surface verdicts + `POLARS_MAX_THREADS=1` | engine-parallel-surface | Determines what actually parallelizes; the toolchain only enables threads (`:13,15`). |

External / pinned:
- `wasm-bindgen-rayon = 1.3.0` (new optional dep, §4.3); `wasm-bindgen 0.2.106` (repo pin, unchanged); `wasm-bindgen-cli 0.2.106` (unchanged, `scripts/build-wasm.sh:17`).
- Pinned nightly `nightly-2024-08-02` + `rust-src` + `wasm32-unknown-unknown` (new; native stays stable `1.90.0`).
- `dtolnay/rust-toolchain@nightly` action for the CI wasm job (native jobs keep `@stable`).

---

## 6. Risk Assessment

| # | Risk | Severity | Mitigation |
|---|---|---|---|
| 1 | `nightly-2024-08-02` fails to compile polars/clarabel/nalgebra under `-Zbuild-std` | High (blocks build) | §4.1 validation procedure with forward-bump fallback; NEEDS RESEARCH in §4.1 resolves before build dispatch. |
| 2 | build-std output not byte-reproducible across machines (UNVERIFIED, `phase1-...-nightly-reproducibility.md:23`) | High (breaks INV-04) | §4.5 double-build CI gate catches divergence; `.rustup` remap made load-bearing (AC-R4.5); if divergent, escalate to founder — may force Strategy A's untouched-stable-baseline as the defensibility artifact. |
| 3 | env `RUSTFLAGS` silently overrides `.cargo/config.toml` target table, dropping atomics | Medium (silent seq blob) | §4.2 precedence note + script re-lists static flags; AC-R2.2 dev-path test + parity job detect a non-atomics blob. |
| 4 | `[unstable] build-std` in config.toml errors on stable native builds | Medium (breaks native CI) | AC-R2.3 asserts benign warning only; if it hard-errors on `1.90.0`, move build-std out of config.toml onto the command line only (Strategy A already does this) — one-line change. |
| 5 | Nightly toolchain regression changes codegen between the pin and a future re-pin | Medium (baseline churn) | Fixed dated pin (no floating `nightly`); re-baseline is a deliberate committed action (§4.7). |
| 6 | `--shared-memory`/`--import-memory` actually required by lld and missing → link failure | Medium (blocks build) | §4.2 NEEDS RESEARCH resolves before dispatch; add the link-arg if the probe shows it's needed. |
| 7 | `--target web` migration breaks Meridian's Vite bundler-glue import path | Medium | Migration is mandatory under both strategies (`phase1-...-dual-artifact-patterns.md:18`); Meridian domain owns the `init(url)` rewrite; toolchain only guarantees a `--target web` `pkg/`. |
| 8 | Doubled build/CS time (Strategy A) or full-nightly-consumer surface (Strategy B) | Low | Framed in the §4.6.3 recommendation; founder decides the tradeoff at the gate. |

---

## Gaps Requiring Deeper Research

Each is scoped as a single-agent question:

1. **Nightly compile validation** (§4.1) — Does `nightly-2024-08-02` build polars 0.44 + clarabel 0.11.1 + nalgebra 0.32 for `wasm32-unknown-unknown` with `-Zbuild-std=panic_abort,std`? If not, the earliest dated nightly that does. (Local build attempt; deterministic.)
2. **Shared-memory link args** (§4.2) — With the pinned nightly's lld, does `+atomics` auto-emit a shared memory, or are `-C link-arg=--shared-memory` / `--import-memory` required alongside `--max-memory`? (Two local link attempts; compare.)
3. **wasm-bindgen-rayon 1.3.0 ↔ wasm-bindgen 0.2.106 compatibility** (§4.3, ASM-01) — Confirm 1.3.0's dep range admits 0.2.106 and `init_thread_pool` is the current export. (Docs/manifest check; re-verify at build time.)
4. **build-std reproducibility across machines** (§4.5, risk 2) — Is the raw threaded wasm byte-identical across two hosts (not just two dirs on one host) under identical pin+flags+remap? (Two-machine double-build; if unavailable, two-container.)
5. **Cache correctness for double-build** (§4.5) — Does `Swatinem/rust-cache@v2` leak std artifacts across `CARGO_TARGET_DIR`s and mask nondeterminism? (Inspect cache scoping; disable for the repro step if it leaks.)

---

## 8. Spark Notes

- Threads on wasm32-unknown-unknown are nightly-only in 2026: `-Zbuild-std=panic_abort,std` + `-C target-feature=+atomics,+bulk-memory,+mutable-globals`; no stable path (`phase1-...-nightly-reproducibility.md:7,18`).
- Native stays on stable `1.90.0`; the nightly is applied only via `cargo +nightly-2024-08-02` override — INV-01 holds under both strategies.
- `--target bundler` cannot drive threaded glue → migrate the wasm-bindgen step to `--target web`; this is mandatory regardless of strategy (`phase1-...-dual-artifact-patterns.md:18`).
- Default pin `nightly-2024-08-02` (crate-tested) with a forward-bump validation procedure; the exact date is the one build-blocking gap (§4.1).
- Two reproducibility strategies fully specified; **recommend Strategy B (single threaded blob, seq fallback, one re-recorded baseline)** for the smaller reproducible surface, intrinsic INV-03 fallback, and clean config.toml. Founder ratifies at the buildability gate.
- build-std reproducibility is empirically-yes / formally-no → a double-build sha256 CI gate is mandatory (INV-04); the `~/.rustup` remap becomes load-bearing because std compiles from source.
- New: `.cargo/config.toml` (atomics + link args + build-std), `wasm-threads` engine feature pulling `wasm-bindgen-rayon 1.3.0`, `init_thread_pool` re-export. `--max-memory`/`-zstack-size` values are the memory-budget domain's to finalize.
