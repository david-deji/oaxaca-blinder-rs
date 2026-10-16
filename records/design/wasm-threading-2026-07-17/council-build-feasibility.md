# Council — Build-Feasibility & Cost Skeptic

> Lens: Can a /build pipeline actually execute these 7 specs as written? Ranked, evidence-based, every finding cites a file:line I Read against ground truth (specs + live engine repo + Cargo.lock + registry manifests).

## Verdict: SHIP_WITH_FIXES

The spec set is unusually disciplined — line anchors are byte-accurate, the two just-applied CRITICAL fixes are sound against ground truth, ACs are overwhelmingly mechanically checkable, and the three open founder decisions are correctly deferred (they don't block writing or building). But the single most important buildability precondition — that the threaded WASM half compiles at all — rests on a pinned nightly that, verified against the actual locked dependency graph, **cannot compile it**. The build fails *safe* (loud halt at Phase-0 preflight E1, not corrupt numbers), so this is SHIP_WITH_FIXES rather than BLOCK, but E1 is not the "quick preflight" the specs present it as, and the recovery path is under-specified and possibly non-convergent under `--locked`. Fix that before dispatch and the rest of the set is genuinely buildable.

---

## F1 — MAJOR (top finding): the pinned nightly cannot compile the locked WASM dep graph; E1 will fail deterministically and the forward-bump is under-estimated and may not converge under `--locked`

**Where:** `phase4-final-toolchain-build.md:46,136,280` (pin = `nightly-2024-08-02`, framed as "the wasm-bindgen-rayon crate-tested pin" with a "~2-week-step" forward-bump); `phase4.5-reconciliation.md:31` and `phase4-build-readiness-audit.md:25` both treat E1 as an expected-pass exit-0 check.

**Ground truth I verified:**
- `engine/Cargo.toml:11` — `oaxaca_blinder = { path = "../oaxaca_blinder" }` with **no `default-features = false`**, so oaxaca_blinder's default feature set is on for the engine, native and wasm alike.
- `oaxaca_blinder/Cargo.toml:53-55` — `default = ["display"]`, `display = ["dep:comfy-table"]`; and `askama = "0.14.0"` at `Cargo.toml:41` is an **unconditional** (non-optional, non-cfg-gated) dependency.
- Registry manifests (`~/.cargo/registry/src/.../Cargo.toml`): **`comfy-table-7.2.1` is `edition = "2024"`, `rust-version = "1.85"`**; **`askama-0.14.0` is `rust-version = "1.83"`** (with `askama_derive`/`askama_parser` 0.14 the same); `icu_*` 2.x (pulled transitively) are `rust-version = "1.83"`; `home-0.5.12` is `1.88`.
- `Cargo.lock` pins all of these (resolved against the stable `rust-toolchain.toml:4` `channel = "1.90.0"`, a ~2026 Rust).

`nightly-2024-08-02` is roughly a Rust-1.82-era nightly. **Edition 2024 was not stabilized until Rust 1.85 (Feb 2025)** — a 2024-08 toolchain does not know the `2024` edition token at all, so `comfy-table 7.2.1` is an unconditional hard compile failure, not an advisory MSRV warning. `askama 0.14` (MSRV 1.83, unconditional) independently sinks the same build. Both are in the `pay-equity-engine --features wasm-threads --target wasm32-unknown-unknown` graph. The existing *stable* wasm build (ASM-06) compiles these fine on 1.90 — the **only** variable that breaks is the old nightly this spec introduces.

**Why the forward-bump framing is wrong, not just optimistic:**
1. E1 does not "probably pass on `nightly-2024-08-02`, subject to a check." It **will** fail. The pin was copied from wasm-bindgen-rayon's own examples, which drag a minimal dep set — not TM's polars+clarabel+askama+comfy-table+icu graph.
2. The forward-bump must jump to at least a nightly that supports edition 2024 and MSRV 1.85 (comfy-table), realistically ~Feb-2025+; if `home 0.5.12` (MSRV 1.88) is reachable, mid-2025. That is ~13–22 two-week steps from 2024-08-02, **each a full `-Zbuild-std` recompile of ~475 crates for wasm32** — a very different cost profile than the specs imply.
3. **Non-convergence risk the spec never addresses:** a working pin must *simultaneously* satisfy (a) the deps' editions/MSRVs (pushes newer), (b) a functioning `-Zbuild-std` for `wasm32-unknown-unknown` (chronically breaks across nightlies), and (c) `wasm-bindgen-rayon 1.3.0` + `wasm-bindgen 0.2.106` compatibility (`phase4-final-toolchain-build.md:55`). `--locked` (`build-wasm.sh` D5 line 162, and every AC) **forbids** the obvious escape — downgrading comfy-table/askama to edition-2021 versions. The reproducibility pin and the old-nightly requirement pull in opposite directions and no spec reconciles them. If forward-bump exhausts with no nightly satisfying all three, there is **no Plan B in any of the 7 specs**.

**Failure direction:** safe — E1 halts loudly at Phase 0 before any file edit (`phase4-final-toolchain-build.md:290`), so it never ships wrong numbers. That is why this is MAJOR, not a correctness CRITICAL. But it can block the entire threading half indefinitely.

**Recommendation (do before the buildability gate, not at Phase 0):**
- **Trim the wasm subgraph so an older, build-std-friendly nightly works:** set `default-features = false` on engine's `oaxaca_blinder` dep (drops `display`→`comfy-table`), and `cfg`-gate `askama` off the wasm target (`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`). A wasm graph of edition-2021 / low-MSRV crates is far more likely to compile under the wasm-bindgen-rayon-tested nightly *and* keeps `--locked` intact. This is the highest-leverage fix and also removes dead terminal-table/templating weight from the wasm blob.
- **OR** run E1 empirically against the *actual* wasm subgraph now and pin the literal to whatever nightly first satisfies deps+build-std+rayon, explicitly documenting the `--locked`-vs-nightly tension and the vendor/patch fallback if none converges.
- Either way, correct `phase4-final-toolchain-build.md:46,280` and the reconciliation's "E1 = expected exit-0" framing — E1 is the top build risk, not a formality.

---

## F2 — MINOR→MAJOR: the E3 main-thread-relay fallback is under-specified and, if taken, defeats the worker-offload architecture

**Where:** `phase4-final-meridian-integration.md:20,208-215` (D7) and `:270` (AC-M9.1); `phase4.5-reconciliation.md:31` calls E3 "the top unknown."

The happy path (nested `initThreadPool` spawns inside the dedicated worker) is well-supported by W2 [CONSENSUS]. The fallback is the problem. D7 says: if nested spawn is unsupported, run `initThreadPool` **on the main thread** plus a relay forwarding `{id,type,payload}` to/from the compute worker, and claims the postMessage contract stays byte-identical. Two gaps:

1. rayon's pool lives where `initThreadPool` ran. If the pool is on the main thread, the `par_iter`-backed `decompose()` must **execute on the main thread** to use it — so the heavy 50k-row bootstrap runs on the UI thread, blocking it. The whole point of `analysis.worker.js` was to keep compute *off* the main thread. The fallback silently trades the app's non-blocking guarantee for threading, and D7 frames this as a no-cost "byte-identical contract" change. The message *shape* is identical; the *UX* (frozen UI during compute) is not.
2. Whether `wasm-bindgen-rayon`'s `initThreadPool` even functions correctly when called from the main thread (vs. a worker) is **asserted, not verified** — no W-finding backs the fallback. AC-M9.1 checks "`threadCount > 1`" but not that the main thread stays responsive.

This matters precisely because E3 is the named top unknown — the fallback is the branch most likely to actually be exercised, and it's the least-specified. **Recommendation:** either verify main-thread `initThreadPool` behaviour as part of the E3 preflight (so the fallback is real), or make the honest fallback "**sequential** (no pool) with a visible degraded-mode banner" — which INV-03 already guarantees and which doesn't block the UI — rather than "threaded pool on main thread." State the UI-blocking consequence explicitly wherever D7 claims byte-identical parity.

---

## F3 — MINOR: the engine "allocation-pressure self-report" is a named mechanism with no specified detection algorithm; its AC only checks that it's named

**Where:** `phase4-final-memory-budget.md:244-248` (D5.2), `:319` (D8 lists the `analysis.rs` self-report change), `:407-411` (AC-M11); consumed at `phase4-final-meridian-integration.md:204` and tested by `AC-M8.2` (`:266`).

D5 point 2 says "the Rust engine detects its own allocation pressure at rep-batch boundaries (checked allocation) and returns a structured `{code:'OOM'}` value rather than trapping." But D5 point 3 and `:255-257` admit **there is no runtime free-memory query** on wasm. So the spec asserts a detection capability while stating the primitive it would need does not exist. A worker handed D8's line "expose ... allocation-pressure self-report" has no concrete algorithm — it can't query proximity to `M_max` without an unspecified `memory_size(0)`-vs-`M_max` comparison that no build step defines. AC-M11 (`:407`) only checks that the spec *names* the self-report and reinstantiation paths — it verifies naming, not function (failure mode #9, incorrect verification). AC-M8.2 then tests behaviour "with a forced-pressure fixture from verification-benchmark" that is itself not defined here.

Not build-blocking (budget-prevention via `N_max_const`/`--max-memory` is the load-bearing defense and *is* well-specified), but a worker can stall on "implement the self-report," and the AC won't catch a hollow implementation. **Recommendation:** either specify the detection concretely (e.g. engine samples `core::arch::wasm32::memory_size(0)` at rep-batch boundaries, compares to the compiled-in `M_max`, returns the structured value above `headroom_frac`), or downgrade D5.2 to "best-effort, optional" and lean AC-M11 on the trap-fallback + reinstantiation path (D5.3), which *is* implementable.

---

## F4 — MINOR (sequencing/schedule risk): the memory profile is a long serial critical-path gate, built on admittedly-imprecise wasm instrumentation, that everything threading blocks on

**Where:** `phase4-final-memory-budget.md:328-359` (build order), `:110-113` (the `memory_size`-conflates-retention caveat), `:170-177` (D3 `N_max_const`); toolchain Build Step 6 (`phase4-final-toolchain-build.md:295`) and meridian M7 (`phase4-final-meridian-integration.md:226`) both consume the profile's outputs.

The real critical path is serial and long: deterministic-rng seeding refactor → shared memory profile harness (substantial net-new code: tracking `GlobalAlloc` + wasm `memory_size` checkpoints A/B/C/D) → run profile → compute `M_max`/`N_max_const` → *then* toolchain finalizes `--max-memory` + re-records the sha256 baseline, meridian writes `thread-cap.js`, and verification-benchmark's ceiling test can run. Toolchain scaffolding can proceed with the 512 MiB buildable default, but the *final, hashable* threaded build cannot complete until the profile lands. And D1's own caveat (`:110`) concedes the wasm `memory_size`-only instrumentation "conflates allocator retention with live bytes → `Sc` may be overstated → thread cap conservative." A conservative cap is the safe direction, but `N_max_const` is the number the entire thread-count design turns on, derived from an instrument the spec flags as imprecise. This is largely inherent to the problem and handled reasonably (conservative-by-construction, O3/O4 route the measurement questions non-blockingly), but it is the schedule/risk headline for /build: expect the memory domain, not the toolchain domain, to be the long pole — and it is gated behind F1 (the profile's *threaded* Sc measurement needs the E1 nightly working). The *pre-threading* profile runs on the existing stable wasm build, so there's no hard circularity, but plan the two profile passes (pre-refactor stable, post-threading nightly) around F1's resolution.

---

## What holds up (verified, for balance)

- **CRITICAL-1 fix is sound.** `OaxacaBuilder::decompose_quantile()` exists at `builder.rs:720-766`, calls `calculate_rif` per group (`:730,735`) and re-runs `run()` (`:765`), which produces per-predictor detail; the WASM quantile branch at `analysis.rs:166-206` currently calls `QuantileDecompositionBuilder` and returns `Vec::new()` detail (`:203-204`); the mean-branch extraction the fix reuses is real at `analysis.rs:261-269`. The ~12-line rewrite in D5 (`phase4-final-engine-parallel-surface.md:96-116`) type-checks against the real `OaxacaResults`/`two_fold()`/`DetailedComponent` shapes. No divergent RIF/KDE path is introduced. The MM→RIF aggregate switch is correctly surfaced as a founder decision (a/b/c) at the gate, and (b) is correctly rejected as incoherent (detail wouldn't sum to a MM aggregate).
- **CRITICAL-2 fix is sound.** Single-owner assignment is now clean: memory-budget owns the value `N_max_const` (`memory-budget.md:170-177`); meridian M7 writes `frontend/src/wasm/thread-cap.js` (`meridian-integration.md:193,226`); the worker imports it (`:90`). The three-way "engine-parallelization" misattribution is gone. The build step that creates the file now exists.
- **`rand_chacha` de-risks, not adds risk.** `rand_chacha 0.3.1` is *already* in `Cargo.lock` (transitive via `rand 0.8.5`), so the deterministic-rng/memory-budget "add `rand_chacha = "0.3"` direct dep" step resolves under `--locked` with zero new version resolution. (The specs call it a "new dependency," which is true only as a *direct* dep; harmless.)
- **ACs are overwhelmingly mechanical.** Across all 7 specs the acceptance criteria are grep / exit-code / byte-compare / sha256 / Playwright assertions with no "works correctly" subjective language — the auditor's read holds. The two soft spots are AC-M11 (F3) and AC-M9.1 (F2).
- **Founder decisions are correctly deferred, not blocking.** Strategy A/B, H2 margins (O1), and In-Scope-12 MM→RIF (a/b/c) each have fully-specified branches; none blocks spec-writing or the early build steps. That deferral is right.

## Missing coverage (build-feasibility lens)

The reconciliation and the build-readiness audit both verified *line anchors* byte-for-byte but **neither checked whether the pinned toolchain can compile the locked dependency graph** — the one check that would have caught F1. For a spec whose entire threading half hinges on a nightly + `-Zbuild-std`, "does the pin compile the real dep set?" is the load-bearing feasibility question, and it was assumed rather than tested. Recommend adding a standing gate item: whenever a spec pins a non-default toolchain, resolve it against the actual target subgraph's editions/MSRVs before the buildability gate, not at Phase 0.
