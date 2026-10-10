# 0122-MERIDIAN engine half: adversarial plan review, 2026-10-09

> Engine `main` 31ff136. Spec: `ground/2026-10-09-D-reground.md` § 3 T1-T14/T18, § 4 S1-S6, § 5 V1-V13 and V16, with D1-D6 (D5 carve-out) and the issue's amendments. Read-only: no engine edit, no git state change. The one new artefact outside this file is `target/regroundD/perf3/src/bin/probe4.rs` (binary `target/regroundD/target/debug/probe4`), built with `CARGO_PROFILE_DEV_DEBUG=0` and a target dir inside the repo.
> Lenses: (1) spec conformance against D1-D6 and the code at main; (2) vacuity of each V gate; (3) one meaning and one independent oracle per new result field.
> Severity: HIGH = the built engine would be wrong or a gate would stay green on a wrong engine; MEDIUM = spec contradiction or hole that forces rework or hides a case; LOW = hygiene.

## 0. Verdict

The plan is sound on its centre (no solver; fixed-line cost n_T(g_T - u0) holds for the Reference line). It is not buildable as written. Six defects change what the engine returns:

1. T1 has the wrong sign in the optimiser's own convention (F-01).
2. T1's closed form is exact only for the Reference line with no reference raises. Under Pooled it misses; under `adjust_both` the gap is not monotone in the budget (F-02, F-03).
3. T1 clamps to a budget of 0, which the engine reads as "no cap", so a target that is already met pays everything (F-04).
4. T4 contradicts itself, and T5/T7 leave two `new_unexplained_gap` meanings for one schedule (F-05, F-06).
5. D4's counts cannot be produced from `check_defensibility` as it returns rows today (F-08).
6. V12 cannot go red on its own plant, and V16 lists one moved golden where at least five move (F-20, F-19).

Probe evidence (probe4, fixture F noisy, run today):

| Probe | Result |
|---|---|
| A. Pooled, Greedy, B = n_T(g_T - u0), g_T half way to the floor | cost 25 318.27 as asked; optimiser says -547.2604; VERIFY(Pooled) refit says -544.5253; miss 2.7352 $ |
| A. Pooled, Equitable, same B | VERIFY(Pooled) -547.1693; miss 0.0911 $; the two strategies end 2.644 $ apart, V1 demands identical |
| A. Reference, Greedy, same construction | VERIFY(GroupB) -541.6271 = g_T, miss 0.000000 |
| T1 literal `n_T*(u0 - g_T)` | -25 515.03 (the correct budget is +25 515.03) |
| Effective budget 0 through today's code | pays 51 030.07 = the full need |
| B. 15 of 31 needed rows proposed to `check_defensibility` | 15 rows returned, 0 "below"; 16 underpaid employees were never scored |
| C. `adjust_both` + LowerBound | target rows land on the lower bound (max error 0.000000); reference rows land on the midpoint (0.000000), 316.73 $ above their lower bound |
| E. same schedule through both entry points | optimise gap -1770.167 -> -494.415, unexplained -1179.503 -> +96.249; defensibility gap +1770.167 -> +494.415, unexplained +1179.503 -> -96.249 (four fields flip, not three) |
| Rust 1.90.0 `iter().sum::<f64>()` over nothing | `-0` (sign bit set) |

## 1. Lens 1: conformance of S1-S6 and T1-T14/T18

### F-01 HIGH: T1 states the budget with the wrong sign

- Spec: T1 "B_T = n_T(g_0 - g_T) in the optimiser's sign"; V1 "engine cost = n_T(g_0 - g_T)".
- Code: the optimiser's unexplained gap is `-net_residual_sum_b / n_target` = mean(actual - fair), negative when underpaid (`engine/src/analysis.rs:1021-1025`). In that sign a payment raises the gap, `new = u0 + cost_T/n_T` (`:1027-1031`), so the budget to reach g_T is `n_T (g_T - u0)`. Literal T1 gives -25 515.03 on fixture F (probe A). The re-ground's `bruteforce_lp.py` and § 0 use the other sign (g = mean(fair - wage), "optimiser prints the negative of this", `bruteforce_lp.py:9`), which is where the formula came from.
- Same defect in the field name: `min_reachable_gap` (T2) is the highest value of the optimiser-sign gap, not the lowest.
- Fix: write T1, T2 and V1 in one stated sign, the sign of `original_unexplained_gap`: `B_T = n_T (g_T - u0)`, clipped to [0, need_eligible]. State that `target_gap` on the wire is on that sign (negative = compared group still below the line); the app converts what the employer types. Rename `min_reachable_gap` to `best_reachable_gap` (= u0 + need/n_T, the largest value) or document it as the closest to zero from below. V1's oracle (`bruteforce_lp.py`) must be sign-translated in its harness, not reused as is.

### F-02 HIGH: the target-gap rule is exact only on a fixed line; Pooled breaks it and V1's tie claim

- Spec: T1/V1 apply `B_T = n_T(...)` to every `target`; "Greedy and Equitable return identical cost and identical `new_unexplained_gap`".
- Code and probe: under Pooled each dollar to employee i moves the group coefficient by c_i = d~_i / (d~'d~), not 1/n_T (re-ground § 0a row 9: c_i in [0.056, 0.179] vs 0.125). Probe A: paying exactly B_T under Pooled misses g_T by 2.7352 $ (Greedy) and 0.0911 $ (Equitable) on fixture F, whose leverage spread is mild; the two strategies differ by 2.644 $, so the V1 tie fails. `min_reachable_gap = u0 + need/n_T` is also wrong under Pooled (full payment gives formula C, `gamma0 + d~'diff / d~'d~`). Refusing the combination is not available: the screen offers Pooled (D6) and D1 binds.
- Fix: derive the budget along the chosen strategy's own allocation, exactly, using T5's formulas.
  - Reference (adjust_both off): closed form as written.
  - Pooled Greedy: the gap is piecewise linear and monotone in the budget along the diff-descending order; walk the breakpoints (one pass, no solver).
  - Pooled Equitable: pay = lambda * diff_i, so gap(lambda) = gamma0 + lambda * d~'diff / d~'d~ is linear; solve lambda in closed form.
  - `best_reachable_gap` under Pooled = formula C at full payment of every eligible shortfall.
  - State that Greedy and Equitable tie on cost only on the fixed line.
  - V1 gains Pooled rows (both strategies) held to R `lm` on the written CSV, and the 2.64 $ spread above is the precondition showing the gate can tell the two apart.

### F-03 HIGH: target_gap with adjust_both_groups has no defined behaviour

- Spec: T1 and T5 are written separately; nothing says what a target does when reference raises are on.
- Code: raising reference employees moves the line and lowers the optimiser-sign gap by `x_T'(X_R'X_R)^-1 X_R' a_R` (T5 formula B). Under the mixed diff-descending order (`analysis.rs:860-864` sorts both groups together) a reference payment lowers the gap, a compared payment raises it: the gap is not monotone in the budget, so "the budget that reaches g_T" and "the lowest reachable gap" are not defined.
- Fix: when `target_gap` is set, derive the budget from the compared group only and refuse `adjust_both_groups = true` with a named error (`TARGET_GAP_WITH_REFERENCE_RAISES`); the app disables the toggle with a sentence (S7). Needs David's nod because the toggle goes grey while a group target is typed (see § 5, item 1).

### F-04 HIGH: T1 clamps to 0, and 0 means "no cap" (target already met pays everything); NaN funds everything

- Spec: T1 "effective budget = min(user cap, B_T, sum cap_eligible)"; T3 keeps "budget == 0 means no cap".
- Code: `analysis.rs:848-854` reads `req.budget > 0.0`, else funds `total_need * 1.00001`. A target at or above the current gap gives B_T <= 0, hence an effective budget of 0, which falls into the no-cap branch. Probe: a computed budget of 0 pays 51 030.07 (the whole need). Separately, `(effective_budget / total_need).min(1.0)` (`:950`) returns 1.0 for a NaN numerator (`f64::min` returns the non-NaN operand), so a NaN `target_gap` funds everything under Equitable and pays nothing under Greedy (`remaining_budget > 0.0` false). T3 validates `budget` only.
- Fix: carry the rule as a three-state value (`Uncapped`, `Capped(x)` with x possibly 0, `AlreadyMet`) instead of an f64 that doubles as the sentinel; `AlreadyMet` pays 0 and sets `target_gap_reachable = true`. Add `INVALID_TARGET_GAP` for non-finite `target_gap` and `INVALID_MIN_GAP_PCT` for non-finite or negative `min_gap_pct`. V1 gains "target at or above the current gap pays 0", "unreachable pays the full eligible need (the same figure as no target; D1 and V15 say no amount changes)", and NaN/inf `target_gap` named errors.

### F-05 HIGH: T4 contradicts itself, and V7 cannot hold under a threshold or a range target

- Spec: T4 "`total_cost` and `required_budget` stay the sums" and, in the same cell, "`check_defensibility.required_budget` and `optimize.required_budget` report the same compared-group need". V7 "`required_budget` equal under adjust_both".
- Code: optimise sums eligible need over both groups (`analysis.rs:842-846`, target rows at `:735`, reference rows at `:786`); defensibility sums target rows only, midpoint only, with no threshold (`defensibility.rs:407-414`). `DefensibilityRequest` has no `min_gap_pct` or `range_target`, so the two cannot agree whenever either is set. If the optimise figure "stays the sum", the equality is false by design (140 601 vs 80 696 on the probe roster).
- Fix: decide once. `required_budget` = compared-group need (D2's denominator, D3's "80 696 $ pour le groupe comparé") in both entry points; `total_cost` stays cost_T + cost_R (money spent). Serialise `cost_target`, `cost_reference`, `need_target`, `need_reference` (T4 only says "tracked"; D3's two cost lines and S7 need them on the wire, and the rows cannot recompute a bound-based need, F12). State that V7's `required_budget` equality is on the default basis (threshold 0, Midpoint), or add the two fields to `DefensibilityRequest`.

### F-06 HIGH: one field name, two arithmetics for the same schedule (T5 vs T7)

- Spec: T5 gives optimise the refit gap (formula B or C); T7 changes only the sign of `check_defensibility`.
- Code: defensibility computes `new_unexplained_gap` as mean(fair - new_wage) on the line fitted to the ORIGINAL data (`defensibility.rs:486-495`, `beta_fair` fitted at `:253-280`); it never refits. With reference rows in the schedule, or target Pooled, the two entry points disagree on the same schedule by the line shift (-87 vs 1 026 on the kink fixture; for Pooled the fixed-line mean differs from the coefficient by sum (c_i - 1/n_T) a_i).
- Fix: one shared function `post_schedule_gap(target, schedule)` (formulas B and C) used by `optimize_inner` and `check_defensibility_on`. V7 gains rows with reference raises and with target Pooled, asserting optimise = defensibility = R `lm` on the adjusted CSV.

### F-07 MEDIUM: the sign flip covers four fields, and more tests than the spec names assert the old sign

- Spec: T7 / V7 name `original_gap`, `original_unexplained_gap`, `new_unexplained_gap` (as REM-5 does).
- Code and probe E: `new_gap` flips too (`defensibility.rs:468`; probe E: +1770 -> +494 against optimise -1770 -> -494). A partial flip that leaves `new_gap` in the old sign satisfies V7 as written.
- Tests asserting the old sign: `engine/tests/rows_verify_defensibility_test.rs:214` (the one the spec names), `:267-279` (`original_gap - new_gap == paid/n_T`, positive orientation), `:335` (`moved = original - new`), and the three `defensibility/*` goldens (F-19).
- Fix: V7 asserts all four fields on three fixtures with |gap| well above tolerance; list the test sites in S6.

### F-08 HIGH: D4's counts cannot be produced from what `check_defensibility` returns

- Spec: D4 "30 personnes dans la fourchette, 0 sous, 9 au-dessus (déjà au-dessus avant les ajustements)"; "reference rows never counted". T8 gives a per-row `range_position` and `source`; S3 "reference rows excluded from any share".
- Code and probe B: the result holds one row per PROPOSED adjustment (`defensibility.rs:316` loops `merged`). A schedule that lists only paid rows returns only those; probe B proposed 15 of 31 needed rows and got 0 "below" while 16 underpaid employees were unscored. Nothing in T8/T9 yields counts, so "0 sous" would be computed by the app over a subset. "Already above before" needs the position of `current_wage`, and T8 has one position (after).
- Fix: the engine returns `position_counts { below, inside, above, above_before }` over ALL analysed compared rows (unproposed rows taken at adjustment 0), plus per-row `range_position_before`; reference rows are in no count. D4's sentence reads those four numbers. V8 asserts them on a partial schedule.

### F-09 MEDIUM: `range_position` has no boundary rule

- Spec: T8 "`range_position: Below | Inside | Above`; `is_defensible` unchanged".
- Code: `is_defensible = new_wage >= lower - DEFENSIBLE_TOLERANCE` (`defensibility.rs:346`). A remedy paid to the UpperBound (or LowerBound) puts every paid row exactly on an edge; without a symmetric tolerance, float dust makes them Above or Below.
- Fix: Below iff `new_wage < lower - tol` (the exact complement of `is_defensible`), Above iff `new_wage > upper + tol`, otherwise Inside; assert `Below <=> !is_defensible` on every row in V8.

### F-10 MEDIUM: reference raises ignore `range_target`

- Code and probe C: the compared loop pays to `target_wage` (midpoint, lower or upper, `analysis.rs:730`); the reference loop always uses `fair - actual` (`:786`). With LowerBound + `adjust_both`, reference rows are raised to the midpoint, 316.73 $ above their own lower bound, while compared rows stop at the bound. T4/T5 and `need_reference` inherit this undefined choice.
- Fix: apply the same line to both groups (reference employees raised to their own bound), so one `range_target` setting reads the same everywhere; add a V3/V4 row with LowerBound + `adjust_both`. Decision flagged in § 5, item 2.

### F-11 MEDIUM: T10 contradicts D6 on the frontier axis

- Spec: D6 "budget axis ends at that remedy's need"; T10 "`max_budget` default = 1.1 x that remedy's need" (today `analysis.rs:1143-1144`).
- Effect: the extra 10 % is a flat tail (the app project JSON shows t constant after full closure). D1-D6 are binding.
- Fix: default `max_budget` = the remedy's need exactly (last step lands on the need). This moves `frontier/*` goldens (F-19). "That remedy's need" must say whether reference need is included under `adjust_both` (the sweep pays both groups, `analysis.rs:1270-1279`): include it, since the axis is money spent.

### F-12 MEDIUM: the frontier sweep is nested, so Equitable cannot reuse it

- Code: `calculate_efficient_frontier_inner` builds one payment list from a single optimise call and consumes it in descending order across steps (`analysis.rs:1270-1330`). T10 only threads `strategy` into `opt_req`; at budget 0 Equitable pays every full shortfall, so the list is identical to Greedy's and the frontier ignores the strategy while accepting it.
- Fix: Equitable per step y_i += diff_i * min(1, B_step / need) (closed form, O(n) per step), recomputed per point; Greedy keeps the nested sweep. Tie order (equal diffs) must follow `optimize_inner`'s order (compared rows first, matrix order), not the frontier's index-sorted list. V10 asserts Equitable alone at half budget (F-22).

### F-13 MEDIUM: T12 breaks the CI clippy gate

- Spec: T12 `#[deprecated]` on `OaxacaResults::optimize_budget`.
- Code: callers at `oaxaca_blinder/tests/optimize_budget_test.rs:40,51,67` and `oaxaca_blinder/src/python.rs:268`; CI runs `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` (`.github/workflows/ci.yml:52`), so each call is a deprecation error. README examples at `README.md:201,219` and `oaxaca_blinder/README.md:169` teach the deprecated call.
- Fix: `#[allow(deprecated)]` on the test module and the python wrapper, README examples marked as library-only. Note the library's `target_gap` is the RAW total gap (`oaxaca_blinder/src/types.rs:112-122`, `current_gap = self.total_gap`), not the unexplained gap T1 targets (F-14).

### F-14 MEDIUM: `target_gap` has two documented meanings in the repo

- `engine/src/types.rs:287` has no doc; `oaxaca_blinder/src/types.rs:96-122` defines it as the desired final raw `total_gap`; T1 targets `original_unexplained_gap`. The screen text « ramener l'écart de groupe » does not say which.
- Fix: doc comment on `OptimizationRequest.target_gap`, MCP schema description and `docs/API.md` all say "the compared group's mean gap to the pay line, same sign and scale as `original_unexplained_gap`"; V1 asserts `new_unexplained_gap` hits g_T and that `new_gap` (raw) is not the figure targeted.

### F-15 MEDIUM: WASM baselines are not in the engine half

- `.github/workflows/ci.yml:129-191` compares the raw WASM hash to `engine/pay_equity_engine.wasm.sha256` (and the threaded one); any change to engine source changes the bytes. Prior engine PRs re-recorded them in the same PR (`6f3113e`, `6777a17`). T20 puts the rebuild in the app half (S11), after `gate` would already be red.
- Fix: add to S6: re-record both baselines and the `engine/pkg*` manifests with `scripts/build-wasm.sh --record` in the Track E PR; S11 keeps the publish and receipt.

### F-16 LOW: T10 request fields need `Serialize` on three enums

- `EfficientFrontierRequest` derives `Serialize` (`engine/src/types.rs:471-472`) and the WASM test serialises it (`engine/src/lib.rs:168`); `AllocationStrategy` (`:268`) and `OptimizationTarget` derive `Deserialize` only. Adding `strategy` / `target` fields does not compile until the enums derive `Serialize` (and `Clone`, for the shared request builders).

### F-17 LOW: engine empty sums serialise as `-0`

- Rust 1.90 `Iterator::sum::<f64>()` over nothing is `-0.0` (verified with rustc above). `required_budget: total_need` (`analysis.rs:842-846`, `:1040`) and every new summed field (`need_*`, `cost_*`, `unfunded_amount`, `need_remaining`) inherit it, and serde_json emits `-0.0`, which `Intl` renders « -0,00 $ ». T16 fixes the display only (app). V14 never exercises the engine.
- Fix: normalise at construction (`x + 0.0` turns -0.0 into +0.0), and add an engine test on a roster with nobody underpaid asserting no numeric field has the sign bit set when its magnitude is 0.

### F-18 LOW: D14 reproducibility and D5 carve-out have no engine-side check

- New defensibility sums (need_R, cost split, formula B terms) must reduce in `BTreeMap` order like the existing three (`defensibility.rs:7-12`); `optimize_defensibility_determinism_test.rs:216-217` compares bit patterns of a fixed field list and must be extended to the new fields.
- D5: grep of `engine/src`, `meridian-mcp/src`, `docs/API.md` finds no Act article today. Nothing keeps it so: add a test or `scripts/` check that the remedy tools' descriptions and the doc comments added by T13 contain no `art. N`, "Pay Equity Act" or "Loi sur l'équité".

### F-19 HIGH: V16 states one moved golden; at least five change, and the comparator must learn the new keys

- Spec: V16 "every golden identical at 1e-9 except `optimize/noisy/forensic_both`".
- Code: `compare` fails on any object key difference (`null_free_regression_test.rs:362-366`). So: (a) every optimise and defensibility case gains the T2/T8 keys and per-row `source` / `range_position`: the comparator needs an allow-list like `ADDED_BY_0118` (`:90-95`) and a per-row `take`, as `extrapolated` already gets (`:599-603`); (b) `defensibility/exact`, `/noisy`, `/noisy+dept` flip sign on `original_gap`, `new_gap`, `original_unexplained_gap`, `new_unexplained_gap` (F-07); the golden is "never regenerated" (`:20-24`), so the comparator should assert equality with the NEGATED golden for those four fields; (c) `optimize/noisy/forensic_both` moves as listed; (d) `optimize/noisy/pooled_target` already skips `new_unexplained_gap` (`POOLED_TARGET_MOVED`, `:435-443`) but `check_pooled_target_case` never checks it, so T5 formula C lands unguarded there (V4 is its only guard); (e) the two range-target cases are scoped by field list (`:618-690`), fine, but after T6 their `original_unexplained_gap` is on the midpoint: the in-test invariant `new_unexplained == original_unexplained + total/n_T` still holds, add `need_to_target_line`; (f) `frontier/*` moves if F-11 is applied.
- Fix: V16 lists exactly these, each with its oracle; the "plant" for the sign cases is leaving one of the four fields unflipped.

## 2. Lens 2: vacuity of V1-V13 and V16

For each gate: a wrong implementation that stays green, then the fix.

| Gate | Wrong implementation that passes | Fix |
|---|---|---|
| V1 | (a) Reference-only closed form with Pooled and `adjust_both` ignored (F-02, F-03); (b) a target already met pays the full need (F-04); (c) unreachable pays 0 instead of the full need (D1/V15: "no amount changes"); (d) literal-sign B_T if the test oracle is written the same way (F-01); (e) a shared code path for both strategies satisfies "identical cost" | Add Pooled (both strategies, vs R `lm` on the written CSV), already-met, unreachable-equals-no-target, NaN/inf, sign. Add the identity `best_reachable_gap == overshoot_mean` for threshold 0, Midpoint, Reference, no `adjust_both`: an independent closed form from the CSV |
| V2 | min_reachable built from `required_budget` summed over both groups; midpoint need used under LowerBound/UpperBound; the planted `-mean(max(0, wage - fair))` equals the correct value at threshold 0, so only 2 % and 5 % discriminate | Add `adjust_both` on, LowerBound and UpperBound (bounds from R `predict.lm`), Pooled (formula C). Assert planted != expected at each threshold used, so the gate is shown able to fail at each |
| V3 | Formula B for Midpoint only (F-10: reference rows go to the midpoint under a range target); fixtures where cost_R is near 0 | Precondition assertion cost_R > 0 and a minimum share; add LowerBound + `adjust_both`, `forensic_mode` + `adjust_both`, a threshold |
| V4 | Fixed-line formula differs by 1 $ on F (the plant's 600.84 vs 599.84); a formula-B-for-Pooled slip is as small | Precondition: |formula C - fixed line| above 0.1 $ on each fixture, else use the kink/employers fixture; R oracle stays |
| V5 | Closure computed from rows and the engine's own `source` flags: a mis-tagged group is consistent with itself; closure = cost/required_budget when both groups share a `source` bug; "equals Equitable's `coverage_ratio`" cites a local variable (`analysis.rs:949`), not an output, and with `adjust_both` the denominator is need_T + need_R | Group membership and need from the input CSV in Python/R (`lm`), not from the result; drop the coverage_ratio line or state its adjust_both form; keep monotonicity over the six caps |
| V6 | "Hand count from the ledger export" is the engine's own rows: a wrong `unfunded_count` is consistent with its export. The field is undefined for Equitable (everyone is partially funded) and for threshold rows under "still below their line" | Count from the CSV with the diff/actual rule; define unfunded as eligible rows with paid < diff - 1e-9; add Equitable at a cap, LowerBound, `adjust_both` |
| V7 | Flip three fields, leave `new_gap` (F-07); equality on three adjust_both-off fixtures hides F-06 and F-05 | Four fields, plus reference-raise and Pooled schedules, plus a threshold/LowerBound run for `required_budget` |
| V8 | Count only proposed rows (F-08); strict `> upper` without tolerance on a schedule paid to a bound (F-09); "generous: 0 inside, n above" holds only if every compared row is underpaid, a fixture property | Partial-schedule case, boundary case (paid to UpperBound exactly and 1 cent over), `Below <=> !is_defensible` on every row, precondition on the generous fixture |
| V9 | `group_test` on a predictor-override schedule: `check_defensibility` rewrites predictor cells before matrices (`defensibility.rs:150-175`); an oracle on the original CSV is wrong there | Add an override case, R `lm` on the overridden data; both Reference and Pooled `target` (the test is always pooled) |
| V10 | Request fields threaded but Equitable still follows the nested Greedy sweep (F-12); LowerBound or Pooled alone make the curve "differ", masking an ignored `strategy`; tie order | One row per field alone (strategy alone at half budget, threshold alone, `adjust_both` alone, `range_target` alone) each compared to `optimize` + `verify_inner(Pooled)` per budget; a duplicated-diff fixture for ties |
| V11 | NaN and +inf cannot travel as MCP JSON (serde_json rejects them before the engine); there is no engine CLI (`fn main` exists only in `meridian-mcp/src/main.rs` and `oaxaca_blinder/src/main.rs`, neither calls `optimize_inner`); `target_gap` and `min_gap_pct` unvalidated | Test the three through `optimize_inner` directly and WASM; over MCP test -1 only; add `target_gap` NaN/inf, `min_gap_pct` NaN/negative, `budget` -0.0 (must equal 0) |
| V12 | Plant "pay to the midpoint": `new_wage >= lower - 0.01` is TRUE for a midpoint payment (midpoint > lower), so the gate cannot go red. Paying to the upper bound or overpaying also passes | Assert `|new_wage - lower| <= 0.01` for every fully funded paid row and `new_wage <= lower + 0.01` under a cap; with the midpoint plant the first form is red. `test_lower_bound_optimization` and `test_auto_budget_lower_bound` also keep their `checked_any` fallback: replace with `adjustments.len() > 0` and the equality |
| V13 | Fixes `strategy` and `target` only; `range_target`, check_defensibility `target` and the new frontier enums keep `_ =>` arms; the schema/accept test (`meridian-mcp/src/main.rs:938-951`) lists enums by hand | Cover every enum field of every tool, the empty string and the lower-case form; extend the schema test to the new fields |
| V16 | See F-19 | Per F-19 |

## 3. Lens 3: one meaning and one independent oracle per new field

Units: gap fields are per-employee means (dollars per person); cost/need/unfunded fields are totals (dollars). Every field's doc comment must say which.

| Field | One meaning (proposed) | Defect in the spec | Independent oracle |
|---|---|---|---|
| `target_gap_reachable` | `Option<bool>`: g_T <= `best_reachable_gap` (optimiser sign); `None` without a target | Behaviour when already met undefined (F-04) | CSV closed form (F-01 sign); V1 |
| `min_reachable_gap` | Rename `best_reachable_gap`: u0 + need_T/n_T (Reference/fixed line), formula C at full payment (Pooled); honours threshold and range target | Sign/name inverted; wrong under Pooled (F-01, F-02) | `overshoot_mean` identity; Python CSV sum; R `lm` for Pooled |
| `shortfall_to_target` | Undefined in the spec. Propose: `max(0, g_T - best_reachable_gap)` in per-person gap units; `None` when reachable | Two plausible readings (per person vs total dollars); no gate | Hand value, V1 unreachable row |
| `budget_used_for_target` | Undefined; equals `total_cost` or cost_T. Propose: drop; keep `target_budget` = B_T before clipping | Duplicate of cost_T / `total_cost` | B_T closed form |
| `budget_binding` | Undefined. Propose: user cap > 0 and cap < min(B_T, need_T) (strict, epsilon) | A cap above B_T is not binding although `unfunded_amount` > 0; "not binding but unfunded" must be possible | V6 rows with cap above and below B_T |
| `unfunded_amount` | need_T - cost_T (totals, compared group) | Duplicate of `need_remaining` | CSV need minus the sum of paid compared rows |
| `need_remaining` | Same as `unfunded_amount` | Two fields, one meaning: drop one | Same |
| `unfunded_count` | Eligible compared rows with paid < diff - 1e-9 (Equitable: all eligible at a cap) | Undefined; overlaps "still below their line" which also includes threshold rows | CSV count |
| `threshold_excluded_count` | Compared rows with 0 < diff and gap_pct < `min_gap_pct`; not populated in non-forensic mode today (those rows never enter `potential_adjustments`, `analysis.rs:733-770`), so it is a new tally | Where it counts is unspecified | CSV count |
| `closure` | `Option<f64>` in [0, 1]: cost_T / need_T; `None` when need_T = 0 (the app shows percent) | Gate oracle shares the result's own flags (V5) | CSV-side need and membership |
| `overshoot_mean` | mean over analysed compared rows of max(0, wage - fair_midpoint), independent of threshold and budget | Midpoint or the chosen line? Unstated; no gate | Python from the CSV; identity with `best_reachable_gap` at threshold 0 |
| `cost_target`, `cost_reference`, `need_target`, `need_reference` | The four totals behind D3's two cost lines | "Tracked" (T4) but not serialised; app cannot rebuild a bound-based need from rows | Python sums split by group from the CSV |
| `need_to_target_line`, `remaining_to_target_line` | Named in T6, absent from T2 and V gates | `required_budget` already is the need to the chosen line (tested by the range-target scoped test, `required_budget == total`); two names for one number | Drop, document `required_budget` as the need to the chosen line, echo the line in a `target_line` field |
| `range_position`, `range_position_before` | Position of `new_wage` / `current_wage` against `[lower, upper]` | No boundary rule; one position only (F-08, F-09) | R `predict.lm` bounds |
| `source` | `Reference` or `Compared` per row, both entry points | Defined only for defensibility rows in T8 text | Group column of the CSV by ordinal |
| `group_test { coefficient, t, p, df, level }` | The pooled-with-indicator coefficient after the schedule | Re-names `FrontierPoint`'s `group_coefficient`, `t_statistic`, `p_value`, `degrees_of_freedom`, `confidence_level` (`types.rs:454-470`): reuse those names | R `lm` (V9) |
| `original_unexplained_gap` / `new_unexplained_gap` | Always on the midpoint line, refit (formula B or C) | Split-brain with defensibility (F-06) | R `lm`, VERIFY(GroupB/Pooled) |

Shared-struct problem: `check_defensibility` returns the same `OptimizationResult` (`defensibility.rs:32`, `:40`, `:513`). T2 says the new fields are "always-serialised", so a defensibility run would carry `unfunded_amount = 0`, `budget_binding = false`, `target_gap_reachable` for a request that has no budget or target. A zero here reads as "nothing unfunded" on a schedule that pays nobody. Fix: give the optimise-only fields `Option` (serialise `null`) or split the struct (`RemedyPlan` fields vs `ScheduleCheck` fields); V16's key allow-list then names which entry point carries which.

## 4. Order of work that removes the rework

1. Settle the sign and the three-state budget rule (F-01, F-04) before any code; they decide every V1 expectation.
2. Land the shared post-schedule gap function (F-06) and the cost/need split with the serialised fields (F-05) before the target rule, because the rule needs formulas B and C to find the budget.
3. Then T1 (Reference closed form, Pooled by walking breakpoints / lambda), T3 validation, T2 fields.
4. Then defensibility fields (F-07 to F-09), frontier (F-11, F-12), MCP strictness, T12 (F-13), baselines (F-15).
5. Gates last, each shown red once against the plants in § 2.

## 5. For David (taste/outcome only)

1. A group target and the reference-raises toggle together (F-03): engine refuses the pair and the toggle goes grey while a group target is typed. Recommended. The alternative is to let the toggle stay on and compute the target from the compared group only, then show the reference raises as their own line and report that the final gap differs from what was typed.
2. Reference employees under a range target (F-10): raised to the same line as the compared group (recommended, one setting reads the same everywhere) or to the midpoint as today.
