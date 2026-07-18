# Phase 3 — WEB findings (W1–W10)

> Orchestrator-direct Perplexity (perplexity-only-web-research.md compliant), 2026-07-17.
> Tags per research-standards.md 5-tag taxonomy. Every claim cites the URL Perplexity returned.
> W10 is the founder scope-expansion unit (In-Scope 12 full per-predictor math) — deep-reasoning pass.

## W1 — wasm-bindgen-rayon 1.3.0 mechanics ✅ [CONSENSUS]

- **wasm-bindgen dep range**: declared `wasm-bindgen = "0.2"` (caret → `>=0.2.0,<0.3.0`). **Admits 0.2.106.** (docs.rs/crate/wasm-bindgen-rayon README; github.com/RReverser/wasm-bindgen-rayon). Repo pin 0.2.106 is compatible — no bindgen bump forced by the rayon crate.
- **Re-export mechanism**: plain function re-export — `pub use wasm_bindgen_rayon::init_thread_pool;` in the consuming crate. **No macro.** Confirmed across README + web.dev + third-party examples.
- **JS-visible export**: re-exporting `init_thread_pool` makes wasm-bindgen emit an **async `initThreadPool(numThreads) → Promise`** in the generated JS. Rust side: `pub fn init_thread_pool(num_threads: usize) -> Promise` (GoogleChromeLabs/wasm-bindgen-rayon src/lib.rs). Usage: `await init(); await initThreadPool(navigator.hardwareConcurrency);`.
- **Spec consequence**: engine lib.rs adds the `pub use` line under the wasm+threads cfg; Meridian worker calls `await initThreadPool(cap)` after `await init()`. Both confirmed as drafted. Note the crate README repeats the bundler recommendation "Vite has all necessary plugins built-in" — feeds W3.

## W2 — worker cross-origin-isolation semantics ✅ [CONSENSUS] — resolves meridian RK2 + Phase-1 COOP-inheritance leftover

- A dedicated worker created by a cross-origin-isolated document **is itself cross-origin isolated**: `self.crossOriginIsolated === true` inside the worker (MDN WorkerGlobalScope.crossOriginIsolated; MDN Window.crossOriginIsolated).
- **COOP is a document/browsing-context header** — worker scripts do NOT need their own COOP header; they inherit the owner's agent-cluster isolation state. **COEP loading rules still apply**: the worker script response must be loadable under the page's COEP (same-origin scripts are fine; cross-origin need CORP/CORS). Our worker is same-origin (served by audit-forge) → satisfied by the page-level COOP/COEP alone (web.dev/articles/coop-coep, cross-origin-isolation-guide).
- **Classic vs module worker**: no difference for the isolation property. **Nested dedicated workers** (rayon's pool workers spawned from inside our analysis worker): inherit the isolated worker-global state the same way; `crossOriginIsolated === true` holds, subject to the same COEP-loadability rule.
- **SharedArrayBuffer** is usable and postMessage-transferable inside these isolated dedicated workers.
- **Spec consequence**: setting COOP `same-origin` + COEP `require-corp` on the audit-forge `/pay-equity/` document response (webui/__init__.py:125 beside `_apply_meridian_csp`) is **sufficient** — no per-worker header work. Confirms the Phase-2 meridian-integration draft. Caveat carried to E3: theoretical isolation ≠ proof the nested pool actually *spawns* — E3 (build-preflight experiment) still required for the spawn mechanics, but the COI/SAB permission question is now [CONSENSUS] resolved.

## W3 — Vite production config for --target web + wasm-bindgen-rayon ✅ [CONSENSUS] — resolves meridian RK3 + conditional-import question

- **(a) `worker.format: 'es'` is REQUIRED** (Vite default is `'iife'`). Rayon spawns nested ES-module workers that use `import`/`import.meta.url`; IIFE workers break code-splitting + dynamic import inside workers ("UMD and IIFE output formats are not supported for code-splitting builds" — vitejs/vite#18585, #17483; SO 78331515). This is the single highest-value config fact.
- **(b) Asset URLs**: use bundler mode (do NOT enable rayon's `no-bundler` feature). The glue locates `.wasm` via `new URL("pkg_bg.wasm", import.meta.url)`; Vite emits hashed assets and rewrites those references in prod, and bundles rayon's internal `workerHelpers.js` as ES-module worker chunks. Pitfalls: manual string-concat URLs (Vite won't emit/hash them) and wrapping glue in non-module scripts (breaks import.meta.url).
- **(c) `vite-plugin-wasm`**: **not needed** for `--target web` when you import the wasm-bindgen JS glue (not the raw `.wasm`); rayon README says Vite has the needed plugins built-in. Adding it is unnecessary and potentially harmful if it changes `.wasm` handling. Verdict: omit it.
- **(d) Conditional dynamic `import()` of the threaded glue works in Vite prod** provided `worker.format:'es'`; it produces a separate code-split chunk. Static-import-guarded-at-runtime also works and is simpler for the bundler. **Both viable** — under Strategy B (single threaded artifact w/ rayon seq fallback) neither is strictly needed; under Strategy A (dual artifact) dynamic import is the clean split. Sources: github.com/RReverser/wasm-bindgen-rayon; vite.dev/config/worker-options.

## W4 — WASM OOM error taxonomy ✅ [CONSENSUS] with a hard limitation — feeds worker OOM tagging

- Two layers: **(1) wasm traps** (allocator panic → `unreachable`) surface to JS as `WebAssembly.RuntimeError`, message engine-defined and NOT standardized (`"unreachable"`, `"memory access out of bounds"`, sometimes `"out of memory"`). **(2) JS-side memory ops** (`WebAssembly.Memory.prototype.grow`, `new WebAssembly.Memory({shared:true})`) that fail throw **`RangeError`**, e.g. Chrome `"RangeError: WebAssembly.Memory(): could not allocate memory"` (w3.org/TR/wasm-js-api-2; MDN RuntimeError; SO 52406217).
- **Growing a shared memory beyond its declared maximum → `RangeError`** (WebAssembly/spec#879, #584), same as OOM — the two causes are indistinguishable from the error object.
- **Hard limitation [CONSENSUS]**: **an in-execution OOM cannot be reliably distinguished from other traps** by inspecting the error — same constructor, non-standard message, engine may even hard-crash with no JS exception.
- **Spec consequence**: the worker's OOM tagging must NOT rely on parsing `RuntimeError` messages. Two robust routes for the spec: (a) **proactive** — enforce the computed thread-cap/`--max-memory` budget so OOM is prevented, not caught; (b) **application-level protocol** — have the Rust engine detect its own allocation-pressure (e.g. checked allocation at rep-batch boundaries) and return a structured `{error:"OOM", ...}` value rather than trapping, so the worker gets a clean tag. Draft must be reframed away from "catch RuntimeError and classify" toward budget-prevention + structured self-report. **New finding — record as a Phase-4 correction to verification-benchmark + meridian drafts.**

## W5 — R oaxaca 0.1.5 bootstrap index hook ✅ [CONSENSUS] — decides golden-script shape

- `oaxaca()` exposes only scalar `R` (replicate count). **No external resample-index argument, no custom sampler, no pre-resampled-dataset input** (CRAN oaxaca.pdf; rdocumentation 0.1.5; vignette oaxaca.Rtex).
- Uses its **own internal `sample()`-style loop, not the `boot` package**; resamples **within each group** (group-wise, not joint pool); reproducibility **only via `set.seed()` before the call**.
- **Spec consequence — DECISION**: the shared-resample-index cross-implementation strategy (Rust and R consume the *same* index matrix) is **NOT achievable through R `oaxaca` directly**. Golden-script shape must be the **manual-loop form**: write an explicit R script that (i) reads a fixed index matrix (or regenerates the identical resampling from a recorded seed+algorithm), (ii) hand-computes the OB decomposition per replicate via `lm()`. This was already the specified fallback (phase2 statistical-trust-layer §) — now confirmed load-bearing, not optional. The mean-path golden is a bespoke R script, not a call to `oaxaca(R=...)`.

## W6 — MM/quantile golden routine ⚠️ [SINGLE_SOURCE → superseded by W10]

- `Counterfactual` R package (Chernozhukov-Fernández-Val-Melly): main fn `counterfactual(formula, data, ..., decomposition=FALSE, quantiles=c(1:9)/10, method="qr", reps=100, ...)`, **on CRAN as of 2025-26**, accepts `seed=` + `reps=` bootstrap control (cran.r-project.org/package=Counterfactual; RJ-2017-033; rdocumentation Counterfactual 1.2).
- **Limitation**: Perplexity could NOT confirm `Counterfactual` emits **per-covariate detailed** contributions (it does counterfactual distributions/quantile effects + aggregate decomposition). For the **detailed** quantile golden, W10 supersedes this → use `ddecompose` (see W10 §5).
- **Spec consequence**: `Counterfactual` remains a viable golden for the **aggregate** quantile decomposition (matches current engine output at quantile_decomposition.rs:267-271). For In-Scope 12's **per-predictor** golden, route to `ddecompose` per W10.

## W7 — wasm-pack test COI support ✅ [CONSENSUS] — decides CI browser harness

- **`wasm-pack test --chrome --headless` / wasm-bindgen-test-runner do NOT serve the test page with COOP/COEP headers** — no built-in support, no `WASM_BINDGEN_TEST_*` env var for it; `crossOriginIsolated` is false and SharedArrayBuffer/threads unavailable in that harness (rustwasm.github.io/docs/wasm-pack/commands/test; wasm-bindgen-test/browsers.html; wasm-bindgen#2151, wasm-pack#1355).
- **Threaded-wasm projects use a custom harness**: own static server emitting COOP `same-origin` + COEP `require-corp`, driven by headless Chrome via **Playwright/Puppeteer/WebDriver**; assert `self.crossOriginIsolated === true` before SAB tests. (Chrome `--enable-features=SharedArrayBuffer` is a non-standard local-only shortcut — not for realistic CI.)
- **Spec consequence — DECISION**: the browser parity/threaded CI job is **NOT `wasm-pack test`**. Spec must specify a **tiny COOP/COEP static server (Python http.server subclass or Node/Express, ~30 lines) + Playwright headless Chromium** harness for the threaded/SAB browser tests. Non-threaded unit tests may still use `wasm-pack test` (Node). This confirms and hardens the phase2 verification-benchmark draft's "COOP/COEP-capable test server + headless Chromium" requirement (was UNVERIFIED → now [CONSENSUS]).

## W8 — rust-cache and the reproducibility double-build ✅ [CONSENSUS] — de-risks SC-04 double-build test

- Swatinem/rust-cache@v2 caches **`~/.cargo`** (registry, git deps, installed bins) + **`./target` (dependency artifacts only)**; key = job_id + rustc release/host/hash + RUSTFLAGS + hash of all Cargo.lock/Cargo.toml + rust-toolchain(.toml) + .cargo/config.toml (github.com/Swatinem/rust-cache README; marketplace).
- **Custom `CARGO_TARGET_DIR` is NOT cached** — the action only knows `./target`. So a double-build into **two separate CARGO_TARGET_DIRs will not have the second build's artifacts restored from cache → no masking of a real recompilation.** This is exactly the isolation the SC-04 sha256 double-build test needs.
- `-Zbuild-std` std artifacts live under the target dir and are treated as dependency artifacts (kept, subject to prune rules) — but again only under `./target`.
- **Spec consequence — DECISION**: the CI reproducibility double-build must set **two distinct `CARGO_TARGET_DIR` values** (e.g. `target-a`, `target-b`), neither being `./target`, OR disable rust-cache for that job. Either guarantees both builds recompile from source (incl. build-std) and the sha256 comparison is honest. Confirms SC-04/INV-04 buildable as drafted; adds the concrete CARGO_TARGET_DIR requirement.

## W9 — R/Stata seed reproducibility (citation hygiene) ✅ [CONSENSUS] — primary-source defensibility

- **R `boot`**: `boot()` uses the global RNG state (`.Random.seed`); base R `set.seed()` + `RNGkind()` fully determine it. The CRAN boot manual documents parallel-bootstrap reproducibility conditions (seed-at-worker-spawn, `ncpus` unchanged, `mc.reset.stream()`), and warns loading the `parallel` namespace can change the seed — set seed before `boot()`. (r-universe.dev/manuals/boot.html; UCLA R FAQ.)
- **Stata**: `[R] set seed` fixes the global RNG state; `[R] bootstrap` depends on it; documented practice is `set seed` before `bootstrap` (modern Stata adds `rngstate()`/`set rngstate` to save/restore exact state).
- **Spec consequence**: the spec's defensibility narrative — "deterministic seeded bootstrap matches the standard reproducibility guarantee of R/Stata" — is now backed by primary-doc-level citations. Supports INV-02's framing: TM's master-seed + per-rep-stream design is the *same class* of guarantee, made stronger (bit-identical across thread counts, which R/Stata parallel bootstrap does NOT guarantee across `ncpus` changes — a point the spec can legitimately claim as an improvement).

## W10 — Per-predictor quantile decomposition methodology ✅ [CONSENSUS] — FOUNDER SCOPE-EXPANSION UNIT (In-Scope 12 full math)

Deep-reasoning pass, primary-source grounded (FFL 2009 Econometrica; FFL 2018 Econometrics 6(2):28; Fortin-Lemieux-Firpo 2011 Handbook ch.1; Rios-Avila Stata Journal 2020; `ddecompose` CRAN).

### The math (buildable formula set)
1. **RIF of the τ-quantile**: `RIF(y; q_τ, F) = q_τ + (τ − 1{y ≤ q_τ}) / f_Y(q_τ)`, where `q_τ` is the sample τ-quantile and `f_Y(q_τ)` is the density at that quantile (kernel estimate). `E[RIF] = q_τ` by construction.
2. **RIF-OLS per group**: regress the per-observation `RIF_i` on `X` by OLS within each group g → coefficient vector `β_{g,τ}`. The unconditional quantile is approximated by `q̂_{τ,g} ≈ X̄_g' β_{g,τ}`.
3. **Detailed decomposition = standard Oaxaca-Blinder algebra on the RIF-OLS coefficients** — formally identical to the mean path:
   - Per-predictor **composition/endowments**: `C_{k,τ} = (X̄_{A,k} − X̄_{B,k}) · β_{B,τ,k}`
   - Per-predictor **wage structure/coefficients**: `S_{k,τ} = X̄_{A,k} · (β_{A,τ,k} − β_{B,τ,k})`
   - Aggregates are the sums over k — and equal the current engine's aggregate-only output, so **the new detail is additive: existing aggregates are unchanged** (satisfies the Charter anti-goal carve-out INV wording).
   This is exactly the mean-path Oaxaca detail with RIF as the dependent variable — the engine already has OLS + the OB detail machinery for the mean path; the quantile detail reuses it with RIF as `y`.

### Defensibility of the one-stage (no-reweighting) form
- The **simple one-stage RIF-OLS detailed decomposition is defensible and publishable as a first-order approximation** (it IS the original FFL 2009 procedure). Toepfer 2017 and the JRC working paper find the reweighting correction typically small (specification error small when linearity approx holds).
- **Documented limitations** the spec must state: (i) local linear approximation of `E[RIF|X]` — nonlinearity → specification error; (ii) no double-robustness without reweighting; (iii) sensitivity to the `f_Y(q_τ)` density estimate; (iv) base-category dependence for categoricals (see below).
- **Two-stage refinement** (optional, future): DFL reweighting adds a counterfactual sample → four-term decomposition (pure structure + specification error + pure composition + reweighting error), doubly robust. **Recommendation for THIS spec: ship the one-stage RIF-OLS detail** (matches FFL 2009, reuses existing OLS/OB machinery, additive to current aggregates); note two-stage as a documented v2 extension. This bounds the build.

### Machado-Mata path (the current engine's simulation route)
- **MM does NOT admit a clean additive per-predictor detail.** Attributing MM counterfactual differences to individual covariates requires sequential one-covariate-at-a-time changes → **path-dependent, order-sensitive, non-additive** (Fortin-Lemieux-Firpo 2011 Handbook: "sequential DFL-type reweighting… sensitive to order used"). This path-dependence is precisely why FFL invented the RIF route.
- **Spec consequence — DECISION for In-Scope 12**: implement per-predictor detail via **RIF-regression (RIF-OLS + OB algebra), NOT via extending the MM simulation.** The MM/aggregate path stays for the aggregate quantile effect; the detailed path is a new RIF-OLS computation. This is the defensible, additive, path-independent route and reuses the mean-path OB detail code.

### Categoricals (Education_Level 3-level per L6, plus Gender/Department/etc.)
- Base-category (omitted-dummy) dependence of detailed OB **carries over identically** to RIF detail at quantiles. **Gardeazabal-Ugidos / Yun normalization applies the same way** (mechanically, on the RIF-OLS dummy coefficients). Since the engine already implements Gardeazabal-Ugidos category-invariance for the mean path, the same normalization extends to the quantile detail. Spec must require the detail path to route categoricals through the existing normalization.

### Golden generator (supersedes W6 for the detail path) — EXACT SIGNATURE (Phase-3.5 verified)
- **R `ddecompose::ob_decompose()`** implements the FFL RIF decomposition with **per-covariate** detailed contributions at quantiles, path-independent, reweighting optional. Verified golden call (from CRAN RDocumentation, directly fetched Phase-3.5):
  ```r
  ob_decompose(
    formula = log(wage) ~ education + experience + <categoricals>,
    data = df, group = female,
    rifreg_statistic = "quantiles", rifreg_probs = c(0.10, 0.50, 0.90),
    reweighting = FALSE,        # ONE-STAGE RIF-OLS = FFL-2009 form, matches our chosen engine route
    bootstrap = TRUE, bootstrap_iterations = <N>
  )
  ```
  `reweighting = FALSE` gives the one-stage detail our engine implements; `reweighting = TRUE` adds the four-term doubly-robust decomposition (documented v2). Detailed per-covariate effects are in the summary; reproducible via `set.seed()`. Companion `rifreg` package computes the RIF variables (cran.r-project.org/web/packages/ddecompose).
- **Stata `oaxaca_rif` / `rifreg`** (Rios-Avila, Stata Journal 2020) — RIF as outcome into `oaxaca`'s detailed machinery; per-covariate quantile detail; `set seed` for reproducibility. Categoricals: "just add dummies" (statalist 1541825) — same base-category caveat.
- **Spec consequence — DECISION**: the per-predictor quantile golden is an **R `ddecompose` script** (primary) with optional Stata `oaxaca_rif` cross-check. Fixed `set.seed()`; one-stage (no reweighting) to match the engine's chosen route; validates the new RIF-OLS detail against an independent reference implementation. This satisfies the "new math requires its own trust-layer validation" clause the Charter added for In-Scope 12.

### In-Scope 12 buildability verdict
Buildable and bounded. Route: RIF-OLS (density est + per-group OLS on RIF) → reuse existing mean-path OB detail algebra → route categoricals through existing Gardeazabal-Ugidos normalization → validate against R `ddecompose` golden. One-stage form (no reweighting) is the defensible first-order deliverable; two-stage reweighting is a documented v2 extension explicitly out of scope for this spec. No MM-simulation extension needed for detail.

## Sources (full URLs, per unit — Perplexity-returned citations)

- W1: https://github.com/RReverser/wasm-bindgen-rayon , https://docs.rs/crate/wasm-bindgen-rayon/latest/source/README.md , https://github.com/GoogleChromeLabs/wasm-bindgen-rayon/blob/main/src/lib.rs , https://web.dev/articles/webassembly-threads
- W2: https://developer.mozilla.org/en-US/docs/Web/API/Window/crossOriginIsolated , https://developer.mozilla.org/en-US/docs/Web/API/WorkerGlobalScope/crossOriginIsolated , https://web.dev/articles/coop-coep , https://web.dev/articles/cross-origin-isolation-guide
- W3: https://vite.dev/config/worker-options , https://github.com/vitejs/vite/issues/18585 , https://github.com/vitejs/vite/issues/17483 , https://stackoverflow.com/questions/78331515 , https://github.com/RReverser/wasm-bindgen-rayon , https://www.npmjs.com/package/vite-plugin-wasm
- W4: https://www.w3.org/TR/wasm-js-api-2/ , https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/RuntimeError , https://github.com/WebAssembly/spec/issues/879 , https://stackoverflow.com/questions/52406217
- W5: https://cran.r-project.org/web/packages/oaxaca/oaxaca.pdf , https://cran.r-project.org/web/packages/oaxaca/vignettes/oaxaca.Rtex , https://www.rdocumentation.org/packages/oaxaca/versions/0.1.5/topics/oaxaca
- W6: https://cran.r-project.org/package=Counterfactual , https://journal.r-project.org/articles/RJ-2017-033/ , https://www.rdocumentation.org/packages/Counterfactual/versions/1.2/topics/counterfactual
- W7: https://rustwasm.github.io/docs/wasm-pack/commands/test.html , https://rustwasm.github.io/docs/wasm-bindgen/wasm-bindgen-test/browsers.html , https://github.com/rustwasm/wasm-bindgen/issues/2151 , https://github.com/rustwasm/wasm-pack/issues/1355
- W8: https://github.com/Swatinem/rust-cache , https://github.com/marketplace/actions/rust-cache , https://latchkey.dev/learn/ci-how-to/how-to-cache-rust-target-github-actions
- W9: https://r-universe.dev/manuals/boot.html , https://stats.oarc.ucla.edu/r/faq/how-can-i-generate-bootstrap-statistics-in-r/
- W10: https://boris-portal.unibe.ch/server/api/core/bitstreams/ee526cf2-b8dd-4391-afdb-596568e029d6/content , https://cran.r-project.org/web/packages/ddecompose/readme/README.html , http://fmwww.bc.edu/repec/bocode/o/oaxaca_rif.sthlp , https://publications.jrc.ec.europa.eu/repository/bitstream/JRC125900/JRC125900_01.pdf , https://eml.berkeley.edu/~cle/secnf/fortinlemieux.pdf , https://economics.ubc.ca/wp-content/uploads/sites/38/2013/05/pdf_paper_nicole-fortin-decomposition-methods.pdf , https://www.econstor.eu/bitstream/10419/168422/1/Toepfer-2017-Detailed-RIF-Decomposition-with-Selection.pdf

## Phase 4 corrections queued (from Phase 3)
1. **W4** — reframe worker OOM handling: budget-prevention + structured engine self-report, NOT RuntimeError message parsing. (verification-benchmark + meridian drafts)
2. **W5** — mean-path golden is a bespoke manual-loop R script (fixed index/seed + `lm()`), NOT `oaxaca(R=...)`. (statistical-trust-layer draft)
3. **W7** — threaded browser CI harness is a custom COOP/COEP server + Playwright, NOT `wasm-pack test`. (verification-benchmark draft)
4. **W8** — reproducibility double-build sets two non-default CARGO_TARGET_DIRs (or disables rust-cache) for that job. (toolchain-build + verification drafts)
5. **L4** — drop every `POLARS_MAX_THREADS=1` mention; reframe one-parallel-layer as "polars wasm stub routes join/scope onto our global rayon pool — cooperative, not oversubscribed; audit only polars-internal parallel float reductions on the hot path." (memory-budget + engine-parallel-surface + verification drafts)
6. **W10** — In-Scope 12 detail via one-stage RIF-OLS (reuse mean-path OB detail + G-U normalization), golden = R `ddecompose`; MM stays aggregate-only; two-stage reweighting = documented v2. (new engine-parallel-surface + statistical-trust-layer content)
