# 0122-MERIDIAN engine PR #107: contract and vacuity review

Head 8d39342 against base 31ff136. Lens: wire changes and their CHANGELOG/API entries, MCP strict enums and
INVALID_BUDGET on every surface, default byte compatibility (frontier, budget 0), and for each new test a wrong
implementation that still passes. Cap: the 39 files of the PR diff. Probes were run in a throw-away test file
(`engine/tests/zz_review_probe.rs`, deleted; `git status` shows no change from this review).

## Findings

### C-01 HIGH: under `Pooled`, a dollar to a compared employee can LOWER the group's gap, so "best reachable" is not the best and an attainable target is called unreachable
Evidence:
- `engine/src/support.rs` `GapWeights::pooled`: a compared row's weight is `d~_i / (d~'d~)`, the residual of the group indicator on the model columns. It is negative for a compared row whose characteristics sit far beyond the reference group's. The CHANGELOG, `docs/DIAGNOSTICS.md` and the `optimize_inner` header all say every dollar to a compared employee moves the gap the same way. The plan review (D-plan-review.md:47) assumed "monotone along the diff-descending order" and no oracle case tests it.
- A random search over 20,000 small rosters found compared-row weights down to -4.1. A 15-row roster (10 reference rows, 5 compared rows of which two sit at x = 35 and 38; reference x 0..10) reproduces it in the engine, target `Pooled`, strategy Greedy, no cap:
  - `original_unexplained_gap` -2792.0; paying every shortfall (13,960.1) ends at 111.6 = `best_reachable_gap`.
  - Sweeping the cap, `new_unexplained_gap` peaks at 311.8 at a cap of 8,655.3, then FALLS to 111.6 as the rest is paid.
  - `target_gap = 200`: engine returns `target_gap_reachable: false`, `shortfall_to_target` 88.4, pays all 13,960.1, and ends at 111.6. A cap of 8,655 would have passed 300.
- Consequences: `best_reachable_gap` is not the "largest value in the optimiser's sign" (its stated reason for the rename); D1's "says when X cannot be reached and the lowest reachable gap" is false for this roster; closure rises while the gap falls; the Pooled Greedy walk (`found.unwrap_or(need_target)`) returns the first crossing, not the least budget.
- All oracle rosters (F, K, E, tiny) have only non-negative weights, so V1/V2 cannot see it.
Fix: either (a) compute `best_reachable_gap` as the maximum along the strategy's own path (Greedy: running maximum over the sorted prefixes; Equitable is linear in the share so `max(before, full)` suffices) and let the walk find the first crossing of that path; or (b) detect `min(weights.target) < 0` under `Pooled`, return a named flag (`gap_monotone: false`) and say so in the field docs. Either way correct the "same way" sentences. Add this roster as an oracle case in `gen_remedy_goldens.R` (R: refit at each cap, `uniroot` on the first crossing) and a rules test that sweeps caps and asserts `best_reachable_gap >= max over caps of new_unexplained_gap`.

### C-02 MEDIUM: `INVALID_BUDGET` is not applied to the frontier's `max_budget` (or `steps`)
Evidence (probe, `calculate_efficient_frontier_inner`, 3 steps): `max_budget` NaN returns Ok with budgets `[0, NaN, NaN, NaN]`; `+inf` returns `[0, inf, inf, inf]` (both serialise as `null`); `-5` and `-inf` return one baseline point with a stderr warning; `steps: 0` returns one point. `validate_remedy_request` runs only in `optimize_inner`, and the frontier calls it with a hard-coded budget 0. The WASM export `calculate_efficient_frontier` takes `max_budget` straight from JS; the MCP schema does not expose it. The CHANGELOG states a negative or non-finite budget is refused by name; the frontier is another place a budget is typed.
Fix: in `calculate_efficient_frontier_inner`, refuse non-finite or negative `max_budget` with `INVALID_BUDGET` and `steps == 0` with a named error before any work; add the five values above to `money_the_rule_cannot_honour_is_refused_by_name` (frontier variant).

### C-03 MEDIUM: nothing proves the MCP frontier (or `simulate_remediation`) passes its settings on
Evidence: `every_enumerated_argument_of_every_remedy_tool_is_exact` checks that valid values run and invalid ones are refused; `the_remedy_tools_list_every_field_they_accept` checks the schema's property names. No MCP test checks that a value reaches the engine request. Mutant that passes every MCP test: delete `min_gap_pct: frontier_params.min_gap_pct,` or `adjust_both_groups: ...` from the `EfficientFrontierRequest` literal in `handle_tool_call`; or parse `strategy` and then discard it. The engine tests build `EfficientFrontierRequest` directly, so they cannot see it either. This is the same "accepted and ignored" defect the frontier test file's header says it guards against, at the MCP layer.
Fix: one MCP test per frontier setting, alone: call `generate_efficient_frontier` with the setting and assert the last point's `budget` (the remedy's full cost) or a middle point's `group_coefficient` differs from the default curve, and equals what `simulate_remediation` with the same setting reports as `total_cost` for the last budget.

### C-04 MEDIUM: the frontier's `confidence_level` now also moves the pay line for `LowerBound` / `UpperBound`, undocumented and untested
Evidence: `calculate_efficient_frontier_inner` passes `confidence_level: req.confidence_level` into the optimiser request. Probe, `range_target: LowerBound`, 2 steps: level absent gives a last budget of 14,669.10; level 0.80 gives 16,566.02. API.md, the MCP schema and the CHANGELOG describe `confidence_level` on the frontier only as the significance threshold. Every frontier test passes `confidence_level: None`; all 51 R remedy cases and 11 schedules are at level 0.95. Mutant that passes everything: drop that line (the frontier reverts to its pre-0122 `None`).
Fix: say it in the three places ("also sets the interval the bounds are read from"), and add an oracle case at 0.80 with `LowerBound` and a frontier test at a non-default level against the optimiser at the same level.

### C-05 LOW: "`required_budget` is the same figure in both entry points" is overstated in the CHANGELOG, API.md and the MCP text
Evidence: probe, same roster, `min_gap_pct = 0.04`, no schedule: `optimize` `required_budget` 16,884.44, `check_defensibility` 19,606.99. `check_defensibility` takes neither `min_gap_pct` nor `range_target`, so it counts all midpoint shortfalls. CHANGELOG ("in both entry points") and API.md ("the same figure `simulate_remediation` returns") say so flatly; only DIAGNOSTICS carries the "on the default basis" caveat. The V7 test (`optimize_and_check_defensibility_report_one_set_of_figures...`) runs 8 scenarios, all at threshold 0 and Midpoint.
Fix: put the DIAGNOSTICS caveat in the CHANGELOG and API.md sentence; add a scenario at `min_gap_pct 0.04` asserting the documented inequality.

### C-06 LOW: a silent sign flip on four unchanged field names has no marker on the wire
Evidence: `check_defensibility` `original_gap`, `new_gap`, `original_unexplained_gap`, `new_unexplained_gap` change sign with no field, version or `_space` token saying which convention a result uses (`row_key_space` exists for the key change). A cached result, a saved scenario, or an app/engine blob mismatch reads as plausible numbers with the wrong sign. The CHANGELOG entry is the only notice.
Fix: add one literal such as `gap_sign: "compared_minus_line"` to both results (always present), or confirm in the app plan that the blob sha check makes a mismatch impossible and record it in the issue.

### C-07 LOW: the one-cent position tolerance is pinned on one side only
Evidence: the R schedules `F_sched_to_upper` (delta 0, `Inside`) and `F_sched_over_upper` (delta 0.02, `Above`) are the only edge cases (`gen_remedy_goldens.R:349-350`). Any `DEFENSIBLE_TOLERANCE` in (0, 0.02), e.g. 0.019 or 0.0005, passes both; the claimed plant (5 cents) is caught, a 1.9 cent one is not. No lower-edge case exists.
Fix: add schedules at upper + 0.005 (`Inside`), upper + 0.015 (`Above`), lower - 0.005 (`Inside`), lower - 0.015 (`Below`).

### C-08 LOW: the D5 gate scans seven files and one pattern family
Evidence: `no_remedy_text_cites_a_pay_equity_act_article` reads `types.rs`, `analysis.rs`, `defensibility.rs`, `support.rs`, MCP `main.rs`, `docs/API.md`, `docs/DIAGNOSTICS.md`. A statute citation in `CHANGELOG.md`, the three READMEs, `ARCHITECTURE.md`, `CLAUDE.md`, `engine/src/lib.rs`, or the deprecation text in `oaxaca_blinder/src/types.rs` passes. Citations written "s. 70", "section 73" or "L.R.Q." pass the article pattern.
Fix: glob every `*.md` outside `ground/`, `audit/`, `target/` and every doc comment under `engine/src`, `meridian-mcp/src`, `oaxaca_blinder/src/types.rs`; add `section`/`s.`/`L.R.Q.` followed by a digit.

### C-09 LOW: library `OaxacaResults::optimize_budget` accepts a NaN budget
Evidence: `budget.min(total_needed)` ignores NaN, so a NaN budget funds the full need, the defect `INVALID_BUDGET` was added to remove. Negative budgets pay nothing. Deprecated, `python.rs` "is not compiled", no shipped surface. Fix: return an empty Vec for a non-finite budget, or note it in the deprecation text.

## Checked, no finding
- CHANGELOG/API coverage of wire fields: every new `OptimizationResult` field, the three per-row fields, `position_counts`, `group_test`, the five frontier request fields, the four sign flips, `required_budget` basis, the frontier axis and the Equitable change are each in the CHANGELOG; DIAGNOSTICS has the field table. Gap: C-04, C-05.
- Default byte compatibility: `optimize` with budget 0 and defaults pays the same rows (`allocation_need * 1.00001` equals the old `total_need * 1.00001` when no reference raises; Equitable ratio is 1). The null-free golden keeps every line except `optimize/noisy/forensic_both`, and `frontier/noisy/default` passes `max_budget` explicitly; the new default axis (full cost, last point snapped to `max_budget`) is a documented break, so the default frontier is not byte-identical (last budget may differ by an ulp; the golden compares at 1e-9, not bytes).
- MCP enums: all three remedy tools, `Reference`/`Pooled`, `Greedy`/`Equitable`, `Midpoint`/`LowerBound`/`UpperBound`, exact and case-sensitive; the 37-call count is asserted; a `_ =>` arm, a lower-case acceptor and an empty-string acceptor all fail it. Not covered: whitespace-padded values (`"Reference "`), non-string JSON types (serde error, not `UNKNOWN_*`).
- Mutants: I could not confirm the "33 plants" count; no plant log is committed (the issue Log row and `ground/` hold none). I checked the claimed plants are detectable by the test text for V8 (0.02 delta), V12 (midpoint run inside the test), V13, F-17 (`nobody_below_the_line...`), the V10 per-setting variants, tie order, and the 1.1x axis. The V7 test compares the engine to itself through the shared `GapWeights`; a wrong weight formula passes it and is caught only by the R cases (V3/V4), which is acceptable but means V7 alone is not a gate for weights.
- Oracle coverage thin spots (not defects): no R case with Equitable plus threshold or range target, none with forensic mode, none with a level other than 0.95 (see C-04).
