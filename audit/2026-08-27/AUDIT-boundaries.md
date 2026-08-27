# Boundary Audit — oaxaca-blinder-rs (WASM / CLI / MCP)

Scope: silent divergence between surfaces (CLI, WASM, MCP) that are supposed to compute the
same thing, and boundaries that drop data without erroring. Findings are appended as read;
this file is written incrementally, not batched at the end.

Files read, in order: `engine/src/lib.rs`, `engine/src/types.rs`, `engine/src/row_key.rs`,
`meridian-mcp/src/main.rs`, `oaxaca_blinder/src/main.rs`,
`oaxaca_blinder/tests/cli_wasm_parity_test.rs`, `engine/tests/mode_parity_test.rs`,
`engine/tests/row_key_integration_test.rs`.

---

## 1. WASM-exported functions

Source: `engine/src/lib.rs` (only file read so far for this question).

| Fn | lib.rs line | Request type | Result type | Exercised end-to-end through WASM boundary by a test? |
|---|---|---|---|---|
| `init_panic_hook` | 50 | none | `()` | No test found in this file. |
| `decompose` | 56-65 | `DecompositionRequest` (via `serde_wasm_bindgen::from_value`) | `Result<JsValue, JsValue>` wrapping decompose's inner result | No `wasm_bindgen_test` for `decompose` in `lib.rs`'s own `#[cfg(test)]` module — only `calculate_efficient_frontier` is tested there (which internally calls `decompose_inner`/optimize-adjacent logic, not `decompose` itself through the boundary). Need to check `oaxaca_blinder/tests/cli_wasm_parity_test.rs` for whether it covers this via wasm-bindgen-test — pending read. |
| `optimize` | 68-73 | `OptimizationRequest` | `Result<JsValue, JsValue>` | No test in this file. |
| `verify_adjustments` | 76-81 | `VerificationRequest` | `Result<JsValue, JsValue>` | No test in this file. |
| `calculate_efficient_frontier` | 84-90 | `EfficientFrontierRequest` | `Result<JsValue, JsValue>` | YES — `test_calculate_efficient_frontier_valid_structure` (lib.rs:135-164) and `test_calculate_efficient_frontier_invalid_js_value` (lib.rs:166-178) both call the exported `calculate_efficient_frontier(js_val)` wasm_bindgen fn directly with a `serde_wasm_bindgen`-encoded JsValue — this is a real boundary-level test (module is `#[cfg(test)] #[cfg(feature = "wasm")]` + `wasm_bindgen_test_configure!(run_in_browser)`, lib.rs:126-134). |
| `check_defensibility` | 93-99 | `VerificationRequest` | `Result<JsValue, JsValue>` | No test in this file. |
| `init_thread_pool` | 107 (re-export of `wasm_bindgen_rayon::init_thread_pool`) | n/a (numThreads: u32, per module comment) | `Promise` | Gated behind `wasm-threads` feature; no test in this file. |
| `validate_access_code` | 111-116 | `(code: String, registry_url: String)` | `Result<JsValue, JsValue>` | Gated behind `partner-access` feature (off by default — lib.rs:118-121 states this path is "dead in the default/TM build"). No test in this file. |

## 2 (partial). MCP surface — `meridian-mcp/src/main.rs`

MCP exposes 5 `tools/call` names (main.rs:710-797), matching the 5 non-gated WASM exports
1:1 by underlying `_inner` function:

| MCP tool name | Calls | main.rs line |
|---|---|---|
| `forensic_decomposition` | `decompose_inner` | 710-721 |
| `simulate_remediation` | `optimize_inner` | 722-756 |
| `verify_adjustments` | `verify_inner` | 757-768 |
| `check_defensibility` | `check_defensibility_inner` | 769-780 |
| `generate_efficient_frontier` | `calculate_efficient_frontier_inner` | 781-796 |

Two MCP-only asymmetries found, neither marked deliberate by a comment:

**(a) `generate_efficient_frontier` silently drops caller control over `steps`/`max_budget`.**
`EfficientFrontierRequest` (types.rs:200-206) has `steps: Option<usize>` and
`max_budget: Option<f64>` as caller-settable fields, and the WASM `calculate_efficient_frontier`
(lib.rs:84-90) forwards whatever the caller sent. The MCP handler ignores the caller entirely
and hardcodes both:
```
main.rs:786-790
let req = EfficientFrontierRequest {
    decomposition_params: mcp_params.into(),
    steps: Some(50),
    max_budget: None,
};
```
The MCP tool's own advertised JSON Schema (`tools/list`, main.rs:635-648) does not list `steps`
or `max_budget` as accepted properties, so this isn't a client-supplied-but-ignored situation —
the schema itself never offers the knob. No comment states this is intentional. Net effect: the
MCP surface can never reproduce a WASM/CLI-driven efficient-frontier run configured with a
non-default step count or budget cap — same inputs, same tool, structurally different output
resolution, with no error to signal it.

**(b) MCP tool schemas omit `row_key` from the `adjustments` item schema.**
`McpProposedAdjustment` (main.rs:128-137) has a real `row_key: Option<String>` field wired
through to `ProposedAdjustment` (`#[serde(default)]`, so it's accepted if sent). But the
`tools/list` schemas for `verify_adjustments` (main.rs:592-601) and `check_defensibility`
(main.rs:618-629) list only `index`/`value` (and `predictor_overrides` for the latter) as
`adjustments[].properties` — `row_key` is absent from both advertised schemas. A schema-honest
MCP client has no way to discover it can echo back the stable row key (0017-MERIDIAN P4) and
will fall back to the position-based `index` path every time, even though the field is silently
accepted server-side if sent. Looks like schema drift (row_key added post-hoc, tool schemas not
updated) rather than a deliberate MCP-only restriction — no comment marks it as intended.

**(c) MCP silently clamps `bootstrap_reps` to 10000 — a number-changing coercion, not an error.**
Every MCP handler that accepts `bootstrap_reps` clamps it before constructing the request:
```
main.rs:712-714 (forensic_decomposition)
if let Some(reps) = mcp_params.bootstrap_reps {
    mcp_params.bootstrap_reps = Some(reps.min(10000));
}
```
Same pattern at main.rs:759-761 (`verify_adjustments`), main.rs:771-773 (`check_defensibility`),
main.rs:783-785 (`generate_efficient_frontier`). A caller who asks for e.g. 50,000 bootstrap
reps for a tighter confidence interval gets silently downgraded to 10,000 — no error, no
warning field in the response, no signal at all. This is MCP-only: nothing in `lib.rs`'s WASM
wrappers or (pending read) the CLI applies this cap. Flagged fully under Q6 below; flagged here
because it is also a surface asymmetry — the same `bootstrap_reps: 50000` request produces a
different actual rep count depending on which of the three surfaces receives it, with the MCP
result differing from CLI/WASM AND with no field in `OptimizationResult`/`DecompositionResult`
that reports the effective rep count actually used vs requested (need to confirm against
`RunMetadata` — not yet read in full; `types.rs:67` shows `run_metadata: RunMetadata` exists,
which per lib.rs:59 carries `seed`/RNG algorithm/rep accounting — worth checking whether
`RunMetadata` surfaces "requested vs actual reps" or only actual; if only actual, a caller
cannot tell from the response alone that clamping happened at all).

## 2. Capability-by-surface table

Source: CLI = `oaxaca_blinder/src/main.rs` (full file read). WASM = `engine/src/lib.rs`.
MCP = `meridian-mcp/src/main.rs`.

**Structural finding first, because it explains most of the table below:** the CLI binary
(`oaxaca_blinder/src/main.rs`) has NO dependency on `pay_equity_engine` (the `engine` crate)
at all. Its only analysis imports are `oaxaca_blinder::{OaxacaBuilder, AkmBuilder,
MatchingEngine, ComponentResult, ReferenceCoefficients}` (main.rs:2, 303, 334, 363). Every
WASM export and every MCP tool, by contrast, calls into `pay_equity_engine::analysis::*`
(`decompose_inner`/`optimize_inner`/`verify_inner`/`calculate_efficient_frontier_inner`) or
`pay_equity_engine::defensibility::check_defensibility_inner`. So the CLI and the WASM/MCP
pair sit on different sides of the `oaxaca_blinder` <-> `engine` crate boundary — this is an
architecture split, not a per-function oversight, but nothing in the 8 files read states it as
a deliberate design decision in so many words (closest is the crate-purpose split in the
top-level `CLAUDE.md`, which is a doc file, not a code comment, and doesn't say "CLI must not
gain optimize/verify").

| Capability | CLI | WASM | MCP | Asymmetry | Deliberate? |
|---|---|---|---|---|---|
| Mean/two-fold decomposition | Yes — `run_mean_analysis` (main.rs:175) | Yes — `decompose` (lib.rs:56) | Yes — `forensic_decomposition` (main.rs:710) | none | n/a |
| Quantile (RIF) decomposition | Yes — `run_quantile_analysis` (main.rs:234), explicit comment "SAME OaxacaBuilder::decompose_quantile path as WASM/MCP" (main.rs:235-239, AC-13) | Yes — via `DecompositionRequest.quantile` (types.rs:29) | Yes — same field, forwarded (main.rs:86) | CLI can batch multiple `--quantiles` in one call (writes `.qX.XX` suffixed files, main.rs:282-296); WASM/MCP take one `quantile` per call | Deliberate parity claim exists in a comment (AC-13) for the single-quantile numeric path; the CLI's multi-quantile batching is a CLI-only convenience, not called out as intentionally CLI-only vs. accidental gap on the other side |
| Three-fold decomposition | **No `--three-fold` flag in `RunArgs`** (scanned all fields, main.rs:43-128) | Yes — `DecompositionRequest.three_fold: Option<bool>` (types.rs:28) | Yes — `three_fold` in `McpDecompositionParams`/schema (main.rs:85, 554) | CLI cannot run three-fold at all | **No comment found marking this as deliberate** — looks accidental/undocumented |
| Budget-constrained optimization (remediation) | **No** — no call into `engine::analysis::optimize_inner` anywhere in main.rs | Yes — `optimize` (lib.rs:68) | Yes — `simulate_remediation` (main.rs:722) | CLI cannot propose wage adjustments at all | Looks architecturally deliberate (crate-boundary split) but not stated as intentional by any comment in the 8 files read — labeled INFERRED |
| Verify proposed adjustments | **No** | Yes — `verify_adjustments` (lib.rs:76) | Yes — `verify_adjustments` (main.rs:757) | CLI cannot verify | Same as above — INFERRED architectural, not stated |
| Defensibility check | **No** | Yes — `check_defensibility` (lib.rs:93) | Yes — `check_defensibility` (main.rs:769) | CLI cannot check defensibility | Same — INFERRED architectural, not stated |
| Efficient frontier | **No** | Yes — `calculate_efficient_frontier` (lib.rs:84), caller controls `steps`/`max_budget` | Yes — `generate_efficient_frontier` (main.rs:781), but **hardcodes** `steps: Some(50), max_budget: None` regardless of caller input (main.rs:788-789) | CLI: absent entirely. MCP: present but caller cannot configure steps/budget (see finding 2(a) above) — a 3-way asymmetry on one capability | CLI absence: INFERRED architectural. MCP steps/budget hardcoding: no comment found — looks accidental |
| AKM (Abowd-Kramarz-Margolis) fixed effects | Yes — `run_akm_analysis` (main.rs:302), `--worker-id`/`--firm-id` | **No** — no AKM request/result type in `types.rs`, no WASM export | **No** — no MCP tool | CLI-only | No comment found marking this as deliberate; reads as a capability the browser/MCP surfaces simply never got |
| Propensity-score / Mahalanobis / Euclidean matching | Yes — `run_matching_analysis` (main.rs:333), `--matching-method` | **No** | **No** | CLI-only | Same — undocumented |
| Heckman two-step selection correction | Yes — `--selection-outcome`/`--selection-predictors` (main.rs:97-103, 206-215) | **No** — `DecompositionRequest` has no selection fields | **No** | CLI-only | Same — undocumented |
| Formula-based model spec (`wage ~ education + C(sector)`) | Yes — `--formula` (main.rs:89-91, `OaxacaBuilder::from_formula`) | **No** — `DecompositionRequest` requires explicit `predictors`/`categorical_predictors` | **No** | CLI-only | Not stated; plausibly a UX convenience layered only on the CLI, but not documented as such |
| Static HTML report generation | Yes — `Report` subcommand (main.rs:376-412, askama template) | No (out of scope — output-format feature, not an analysis) | No | CLI-only | Plausible-deliberate (a file-output feature naturally belongs to a CLI), not documented |
| Markdown/JSON export of results | Yes — `--output-json`/`--output-markdown` | JSON only, via `serde_wasm_bindgen::to_value` return | JSON only, via `serde_json::to_string` in `content[].text` | Output-format difference, not an analysis-capability gap | n/a |
| Row-key stable identity (0017-MERIDIAN P4) | **Not visibly used** — `run_analysis`/CLI never constructs a `ProposedAdjustment` or reads `Adjustment.row_key` (CLI has no optimize/verify path at all, so row_key is moot for it) | Yes — full `RowKeyTable` plumbing via `optimize`/`verify_adjustments` | Yes, but **schema-undocumented** — `McpProposedAdjustment.row_key` exists and is wired (main.rs:132-137) but `tools/list` schemas never mention it (main.rs:592-601, 618-629) | CLI: moot (no optimize path to begin with). MCP: functional but discoverability gap | MCP gap: no comment found — looks like schema drift, not a deliberate restriction |

**Summary of the sharpest asymmetries:**
1. CLI vs. (WASM+MCP): the whole `optimize`/`verify_adjustments`/`check_defensibility`/
   `calculate_efficient_frontier` capability set is present on WASM+MCP and entirely absent
   from the CLI. This is the single largest capability split in the codebase and is architectural
   (crate boundary), not a per-endpoint oversight — but no comment in these 8 files says so in
   as many words.
2. CLI vs. (WASM+MCP), reversed direction: AKM, matching, Heckman selection, formula-based
   specs, three-fold decomposition are CLI-only and absent from WASM/MCP, again with no comment
   marking this as deliberate scope.
3. Within (WASM vs. MCP), the two "twin" surfaces that should be interchangeable have two
   real divergences: MCP hardcodes efficient-frontier `steps`/`max_budget` (silently ignoring
   caller intent), and MCP's advertised schema hides the `row_key` capability it actually
   supports. Neither is marked deliberate by a comment.

---

Preliminary note: only `calculate_efficient_frontier` has an in-crate wasm_bindgen_test that
exercises the actual exported function signature (JsValue in, JsValue out) rather than the
`_inner` Rust function. `decompose`, `optimize`, `verify_adjustments`, and `check_defensibility`
have no such test in `lib.rs`. Checking `oaxaca_blinder/tests/cli_wasm_parity_test.rs` next —
if it does not use `wasm_bindgen_test` (i.e. it's a native test), it cannot exercise the actual
WASM boundary (serde_wasm_bindgen serialization) at all, only the shared `_inner` functions
compiled natively. This is a candidate silent-divergence risk: "parity" tests that call
`_inner` on both "sides" don't touch the WASM (de)serialization layer where the u64-seed bug
(lib.rs:59-63) and the typed-array bug (types.rs, to be read) actually live.

---

All 8 requested files have now been read. Composing final numbered answers below.

---

## FINAL ANSWERS

### 1. Every function exported to WASM (`engine/src/lib.rs`)

| Fn | Line | Request type | Result type | Tested end-to-end through the WASM boundary? |
|---|---|---|---|---|
| `init_panic_hook` | 50 | none | `()` | No |
| `decompose` | 56 | `DecompositionRequest` | `Result<JsValue, JsValue>` (`DecompositionResult`) | **No** |
| `optimize` | 68 | `OptimizationRequest` | `Result<JsValue, JsValue>` (`OptimizationResult`) | **No** |
| `verify_adjustments` | 76 | `VerificationRequest` | `Result<JsValue, JsValue>` (`DecompositionResult`) | **No** |
| `calculate_efficient_frontier` | 84 | `EfficientFrontierRequest` | `Result<JsValue, JsValue>` (`Vec<FrontierPoint>`-shaped) | **Yes, but shape-only** — `test_calculate_efficient_frontier_valid_structure` (lib.rs:135) and `test_calculate_efficient_frontier_invalid_js_value` (lib.rs:166) call the real exported fn with a `serde_wasm_bindgen`-encoded `JsValue`, inside a `wasm_bindgen_test_configure!(run_in_browser)` module (lib.rs:126-134). The success-path assertion is `assert!(val.is_object())` (lib.rs:160) — no field or number is checked. |
| `check_defensibility` | 93 | `VerificationRequest` | `Result<JsValue, JsValue>` | **No** |
| `init_thread_pool` (re-export) | 107 | `numThreads: u32` (per comment) | `Promise` | No; gated behind `wasm-threads` feature |
| `validate_access_code` | 111 | `(code: String, registry_url: String)` | `Result<JsValue, JsValue>` | No; gated behind `partner-access`, and per lib.rs:118-121 comment, "dead in the default/TM build" |

**Bottom line:** of the five compute-bearing exports, only `calculate_efficient_frontier` is exercised through the actual `wasm_bindgen` call boundary by any test in this repo, and that test checks only that the return value is a JS object — it asserts nothing about any number in it. `decompose`, `optimize`, `verify_adjustments`, and `check_defensibility` have zero WASM-boundary test coverage in the files read; every test that touches them (row_key integration test, mode parity test) calls the native `_inner` Rust function directly, never `serde_wasm_bindgen::from_value`/`to_value`, never the `#[wasm_bindgen]`-wrapped fn.

### 2. Capability-by-surface table

See the detailed table and discussion logged above (§ "2. Capability-by-surface table"). Summary:

- **CLI has no `pay_equity_engine` dependency at all** (`oaxaca_blinder/src/main.rs` imports only from `oaxaca_blinder`). It therefore has **zero access to optimize / verify_adjustments / check_defensibility / calculate_efficient_frontier** — the entire budget-optimization and legal-defensibility layer that both WASM and MCP expose. This is the largest single asymmetry in the codebase.
- **WASM and MCP have zero access to AKM, propensity/Mahalanobis/Euclidean matching, Heckman selection correction, formula-based model specs, and three-fold decomposition** — all CLI-only, all reachable only through `oaxaca_blinder`'s builders directly, none reachable via `DecompositionRequest`/`OptimizationRequest`.
- Within WASM vs. MCP (the two surfaces meant to be twins over the same `engine` crate): **MCP hardcodes `steps: Some(50), max_budget: None`** for `generate_efficient_frontier` regardless of caller input (main.rs:786-790), while WASM forwards the caller's `steps`/`max_budget` untouched — same tool, different resolution, no error. **MCP's advertised JSON schemas never mention `row_key`** on `adjustments[]` (main.rs:592-601, 618-629) even though the field is wired and functional (main.rs:132-137) — a schema-honest MCP client cannot discover the 0017-MERIDIAN P4 stable-identity feature that WASM callers get for free.
- **None of these asymmetries carries an explicit "this is intentional" comment** in the 8 files read. The CLI/engine split reads as architectural (crate boundary) but is never stated as a deliberate scope decision; the MCP-specific gaps (hardcoded frontier params, missing row_key in schema) read as accidental drift — most plausibly the schema wasn't updated when `row_key` (0017-MERIDIAN P4) and the frontier request's `steps`/`max_budget` fields were added or wired.

### 3. Do the "parity" tests actually catch a numeric divergence?

**`cli_wasm_parity_test.rs`** — despite its name and its docstring claim ("CLI==library proves CLI==WASM transitively without needing a browser," file header lines 6-9), this test:
- Compares: CLI subprocess `--output-json` output vs. a **direct in-process call to the same native `OaxacaBuilder::decompose_quantile` function**, both on the native (non-WASM) target.
- Tolerance: `TOL = 1e-9` for floats (`assert_close`, lines 25-36), exact equality for `n_a`/`n_b`/component count/names.
- Input: `tests/data/wage.csv`, quantile 0.5, `bootstrap_reps=2`, `--ref-coeffs group-b`.
- **It never invokes `serde_wasm_bindgen`, never invokes anything in `engine/src/lib.rs`, never invokes the WASM target, and never invokes MCP.** The transitive "CLI==WASM" claim rests entirely on a *code comment* in `oaxaca_blinder/src/main.rs:234-239` ("the CLI uses the SAME `OaxacaBuilder::decompose_quantile` path as the WASM/MCP surface") — a claim this test cannot verify because it doesn't touch the WASM/MCP side of that claim at all.
- **Would it catch a numeric divergence?** Yes, but only a divergence between the CLI binary's own argument-to-`OaxacaBuilder`-call plumbing and a direct call to the same function — i.e., a CLI-side wiring bug. It would **not** catch a divergence introduced anywhere in `engine::analysis::decompose_inner`'s wrapping logic, in `serde_wasm_bindgen` (de)serialization, or in the MCP JSON-RPC path, because none of those code paths execute during this test.

**`mode_parity_test.rs`** — also despite its name, this is **not** a CLI/WASM/MCP surface-parity test at all. "Mode" here means **thread-pool size** (1 vs 2 vs 4 threads), not deployment surface:
- Compares: `decompose_inner(request)` (native, in-process, `pay_equity_engine::analysis::decompose_inner` called directly — line 19, 49) serialized via `serde_json::to_string`, run inside `rayon::ThreadPoolBuilder` pools of 1/2/4 threads.
- Tolerance: **exact string (byte) equality** (`assert_eq!(s1, s2)`, lines 58, 62) — the tightest tolerance possible, but only meaningful for detecting non-determinism, not correctness.
- Input: `parity_fixture.csv`, `three_fold: Some(true)`, predictors `education`/`experience`/`tenure`, `bootstrap_reps: 64` (first test) / `32` (second, `ac1_serializer_double_serialize_determinism`).
- **Would it catch a numeric divergence?** Only one caused by thread-count-dependent non-determinism in the bootstrap reduction. The file's own header states the opposite case explicitly: *"a math bug would pass here (all modes wrong-identically) and fail the trust goldens"* (lines 8-9) — i.e., by the test author's own account, a systematic math/numeric bug that produces the same wrong answer at every thread count is **explicitly out of scope** for this test and is claimed to be caught by "trust goldens" that are **not among the 8 files read** (not found in this file, and no path to them was given). The same header also states the real CLI/WASM/native cross-surface tolerance-parity check "runs in the Playwright/COI CI job" — also outside the 8 files read.

**Plain statement, per the task's instruction:** neither test would catch a divergence in the *numbers* at the place that matters most for this audit — the WASM (de)serialization boundary. `cli_wasm_parity_test.rs` checks real numbers but only across two native call paths that share one function; `mode_parity_test.rs` checks byte-exact output but only across native thread-pool configurations, and explicitly disclaims covering math correctness. The one test that does cross the actual `wasm_bindgen`/`serde_wasm_bindgen` boundary (`lib.rs`'s `calculate_efficient_frontier` `wasm_bindgen_test`) checks **shape only** (`val.is_object()`) — it would not catch a numeric divergence of any kind. **No test in the 8 files read compares a number computed through the real WASM boundary against a number computed through the CLI or MCP boundary on the same input.** The WASM<->native numeric-tolerance comparison is claimed (by comment) to exist in a Playwright CI job not in scope for this audit — that claim is unverified here.

### 4. `row_key.rs` — unresolved-key behavior

The resolution function, quoted in full (`engine/src/row_key.rs:241-246`):
```rust
pub fn resolve(&self, index: usize, row_key: Option<&str>) -> Option<usize> {
    match row_key {
        Some(k) if !k.is_empty() => self.by_key.get(k).copied(),
        _ => Some(index),
    }
}
```

- **When a key does not resolve** (a non-empty `row_key` is supplied but not found in `by_key`): the function returns `None`. Per the doc comment immediately above it (row_key.rs:236-240): *"`row_key` present and NOT found: return `None`. The caller skips the adjustment and counts it. Falling back to `index` here would silently apply a consultant's override to whichever employee now occupies that position — the defect this module exists to remove — so an unresolved key must fail closed, never fail over."*
- **Does the caller learn about it?** Yes, structurally: `OptimizationResult.unresolved_row_keys: Option<usize>` and `DecompositionResult.unresolved_row_keys: Option<usize>` (types.rs:165-169, 68-71) carry the count, and `engine/tests/row_key_integration_test.rs::an_unknown_key_fails_closed_and_is_counted` (lines 362-385) proves it end to end: an orphaned key produces `orphaned.unresolved_row_keys == Some(1)` and `orphaned.total_gap == baseline.total_gap` — i.e. the adjustment was skipped, not applied, and the skip is counted and visible on the response.
- **Can an unresolved key silently become a wrong-row attribution?** By design, no — `resolve()` fails closed (`None`) rather than falling back to `index` when a *supplied, non-empty* key isn't found; `an_unknown_key_fails_closed_and_is_counted` and `defensibility_resolves_by_key_and_counts_orphans` (row_key_integration_test.rs:388-416) both pin this directly, including the case where the supplied `index` is deliberately wrong ("index: 0, // wrong on purpose") to prove the key, not the index, governs when a key is present.
- **One caveat, not covered by a comment as a risk:** the guard is on `Some(k) if !k.is_empty()`. A `row_key` of `Some("")` (present but empty) does **not** go through key resolution at all — it falls into the `_ =>` arm and trusts `index` directly, silently, with **no entry in `unresolved_row_keys`** (confirmed by `resolve_without_a_key_is_the_legacy_index_path`, row_key.rs:746-751, which asserts `t.resolve(2, Some(""))  == Some(2)`, treated identically to `t.resolve(2, None)`). This is presumably intentional (empty string == "no key sent"), but it means a client bug that sends `row_key: ""` instead of omitting the field entirely gets silent positional trust with zero signal, rather than being counted as unresolved.
- **A second caveat, out of scope of the 8 files given:** when `row_key` is `None`/empty, the returned `Some(index)` is **not bounds-checked** against the table — row_key.rs's own test comment states it plainly: *"Out of range is still returned verbatim: bounds are the caller's contract, unchanged"* (row_key.rs:750-751, `resolve_without_a_key_is_the_legacy_index_path`). Whether the actual consumer of this return value (per row_key.rs's module doc, "seven sites in this crate index a Vec / ChunkedArray / matrix-row map with it" — i.e. `analysis.rs`, not among the 8 files read) bounds-checks before indexing is **not verified by this audit** — flagged as the top follow-up if a stronger guarantee is needed.

### 5. Does the serde/WASM typed-array fix cover every entry point, and would a test catch a regression?

**The bug** (documented at `engine/src/types.rs:7-20`): `DecompositionRequest` is `#[serde(flatten)]`-ed into `VerificationRequest` and `EfficientFrontierRequest`. Flatten deserializes through `Content` (deserialize_any -> buffer -> replay). For a JS `Uint8Array`, serde-wasm-bindgen's `deserialize_any` calls `visit_bytes` -> `Content::Bytes`, and replaying `Content::Bytes` into a plain `Vec<u8>` calls `deserialize_seq`, which errors ("invalid type: byte array, expected a sequence"). Per the comment: *"the browser's `verify_adjustments`, `check_defensibility` and `calculate_efficient_frontier` were all dead against a Uint8Array payload while `decompose` — the one entry point with no flatten above it — worked."*

**The fix**: `#[serde(with = "serde_bytes")]` on `DecompositionRequest.csv_data` (types.rs:21-22) — `serde_bytes`'s `Vec<u8>` visitor implements `visit_bytes`, `visit_byte_buf` AND `visit_seq`, so both a typed array and a plain JS `Array` of numbers deserialize correctly regardless of flatten.

**Coverage — yes, every WASM entry point that accepts `csv_data` is covered, by construction, not by re-annotation per endpoint:**
- `decompose` uses `DecompositionRequest` directly → fixed.
- `verify_adjustments` and `check_defensibility` both use `VerificationRequest`, which `#[serde(flatten)]`s `DecompositionRequest` (types.rs:187-188) → fixed via the same annotation.
- `calculate_efficient_frontier` uses `EfficientFrontierRequest`, which also flattens `DecompositionRequest` (types.rs:202-203) → fixed via the same annotation.
- `optimize` uses `OptimizationRequest`, which is **not** flattened over `DecompositionRequest` (has its own independent `csv_data` field) — so it was never actually broken by this specific bug, but it independently carries the identical `#[serde(with = "serde_bytes")]` annotation (types.rs:91) with a comment explaining it was added defensively "for symmetry... so that flattening it later cannot reintroduce the bug silently" (types.rs:88-90).
- `init_panic_hook`, `init_thread_pool`, and `validate_access_code` carry no `csv_data`/struct-flatten payload — the bug class does not apply to them (`validate_access_code` takes plain `String` wasm-bindgen params, not a `JsValue`-deserialized struct).

So: **structurally, every entry point is covered.** All five data-carrying request types funnel through one of two independently-annotated `csv_data` fields (the flattened one and the non-flattened one), and the fix is on the field, not per-endpoint — so there is no entry point left holding the old bare-`Vec<u8>` shape.

**Would a test catch a regression?** Partially. The only test that exercises the actual `serde_wasm_bindgen` boundary against a **flattened** struct is `lib.rs`'s `test_calculate_efficient_frontier_valid_structure` (lib.rs:135-164): it builds an `EfficientFrontierRequest` with real `csv_data: Vec<u8>` bytes, serializes the whole request with `serde_wasm_bindgen::to_value(&req)` — which, given the `serde_bytes` annotation, serializes `csv_data` via `serialize_bytes`, which `serde_wasm_bindgen` turns into a genuine JS `Uint8Array` — and then feeds that `JsValue` into the real exported `calculate_efficient_frontier(js_val)` function, which internally does `serde_wasm_bindgen::from_value::<EfficientFrontierRequest>`. If the flatten+typed-array bug regressed, this deserialization would throw and the test's `Err(e) => panic!(...)` branch (lib.rs:162) would fire. **So yes, this test would catch a regression — but only for `calculate_efficient_frontier`.** `decompose`, `verify_adjustments`, `check_defensibility`, and `optimize` have no WASM-boundary test at all (per Q1's table), so a regression specific to one of those four (e.g., a typo re-introducing bare `Vec<u8>` on only `OptimizationRequest.csv_data`, or a change to `VerificationRequest`'s flatten that somehow bypassed `DecompositionRequest`'s annotation) would not be caught by any test in the files read. One caveat on the existing test itself: the `Uint8Array` it produces comes from Rust's own `serde_wasm_bindgen::to_value` serializer, not from an externally-authored JS `new Uint8Array(...)` payload — a reasonable proxy for a browser client's typed array, but not literally the same construction path a real JS caller would use.

### 6. Boundaries that silently drop, truncate, or coerce data (changing a returned number, not producing an error)

**(a) MCP bootstrap_reps silent downgrade — confirmed, MCP-only.** Every MCP handler that accepts `bootstrap_reps` clamps it before building the request, with no error and no field reporting that clamping occurred:
```
meridian-mcp/src/main.rs:712-714
if let Some(reps) = mcp_params.bootstrap_reps {
    mcp_params.bootstrap_reps = Some(reps.min(10000));
}
```
(repeated at main.rs:759-761, 771-773, 783-785). A caller requesting `bootstrap_reps: 50000` for a tighter confidence interval silently gets 10,000 replications instead — changing every `std_err`/`p_value`/`ci_lower`/`ci_upper` in the response — with nothing in `DecompositionResult`/`RunMetadata` (per types.rs) distinguishing "you asked for 50000, we ran 10000" from "you asked for 10000." This exists only on the MCP surface; nothing in `lib.rs`'s WASM wrappers or `oaxaca_blinder/src/main.rs`'s CLI arg handling applies a comparable cap.

**(b) MCP's hand-rolled string→enum mapping silently defaults on any unrecognized value — the sharpest finding in this audit, and MCP-only.** `OptimizationTarget`, `AllocationStrategy`, and `RangeTarget` are plain `#[derive(Deserialize)]` enums (types.rs:75-78, 80-84, 109-114) with no custom serde attributes — on the **WASM boundary**, `serde_wasm_bindgen::from_value` deserializing one of these enums from an unrecognized string is a hard **error** (standard serde enum-deserialization behavior: unknown variant -> `Err`). On the **MCP boundary**, the JSON string is instead run through a hand-written match with a silent default arm:
```
meridian-mcp/src/main.rs:733-736
target: p.target.map(|s| match s.as_str() {
    "Pooled" => OptimizationTarget::Pooled,
    _ => OptimizationTarget::Reference,
}),
```
and identically at main.rs:737-740 (`strategy` → defaults to `AllocationStrategy::Greedy`) and main.rs:745-749 (`range_target` → defaults to `RangeTarget::Midpoint`). A caller who sends a typo'd or malformed value — wrong case (`"pooled"`), a stale value from an older API version, or a client bug — gets **no error at all**; the request silently runs with a semantically different target/strategy/range than requested, changing every adjustment amount, `total_cost`, `new_gap`, and `fair_wage_lower_bound`/`fair_wage_upper_bound` in the response. This is a genuine cross-surface divergence too: the identical malformed input is rejected by WASM and silently reinterpreted by MCP.

**(c) MCP's hardcoded efficient-frontier `steps`/`max_budget`** (already detailed under Q2, finding 2(a)) also qualifies here: it doesn't drop caller data via a coercion path inside a single request, but it does silently substitute `steps: 50, max_budget: None` for whatever the caller might have intended to send (the MCP schema doesn't even expose the fields), changing the entire shape and resolution of the returned frontier curve with no error.

**(d) `RowKeyTable::resolve()` empty-string key** (already detailed under Q4) — `Some("")` is silently treated identically to `None`, trusting the raw `index` with no resolution attempt and no entry in `unresolved_row_keys`. Low-severity (most plausibly intentional pre-P4-compat behavior) but technically a silent coercion of "a key was sent" into "no key was sent."

**Not flagged as a violation (contrast case):** `RunMetadata.seed` (a `u64`) is deliberately serialized as a **String**, not a plain JS Number, specifically *because* `serde_wasm_bindgen`'s default integer serializer throws on a `u64` above `2^53` and the code's default seed exceeds it (lib.rs:59-63). This is the correct, lossless choice, documented in place, and the opposite of a silent-coercion bug — included here only to distinguish it from the findings above.

**Not verified — out of scope of the 8 files read:** whether `analysis.rs` (not in the file list given) does its own `.unwrap_or_default()`, silent cast, or unchecked index against the `usize` values `row_key.rs` returns (particularly the out-of-range-index-passthrough noted in Q4) is unconfirmed. If a stronger guarantee is wanted, `analysis.rs`'s seven `Adjustment.index`/row-key consumer sites (per row_key.rs's own module doc) are the next place to look.

---

## Files NOT read as part of this audit (named per the task's instruction to name what was not reached)

Explicitly out of scope per the given file list, but referenced by the files that were read and load-bearing for full verification of some claims above: `engine/src/analysis.rs` (the actual `decompose_inner`/`optimize_inner`/`verify_inner`/`calculate_efficient_frontier_inner` implementations and the seven sites that consume `row_key`/`index`), `engine/src/defensibility.rs` (`check_defensibility_inner`), `oaxaca_blinder`'s `OaxacaBuilder`/`decompose_quantile` internals, the Playwright/COI CI job referenced by `mode_parity_test.rs`'s header comment as covering "native↔wasm tolerance-parity" and "browser seq/t2/t4 modes," and whatever file holds the "trust goldens" the same header refers to. This audit does not confirm or deny the claims those out-of-scope files make about themselves — only that the 8 files actually read do not contain that verification.

