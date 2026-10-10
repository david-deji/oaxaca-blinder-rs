# 0122-MERIDIAN engine PR 107 — statistics and arithmetic review — 2026-10-09

> PR david-deji/oaxaca-blinder-rs#107, branch `epic/0122-remedy`, head `8d39342`, against `main` 31ff136.
> Lens: the statistics and arithmetic of the remedy figures (T1, T4/T5, T6, T7, T8/T9, T10), checked by my own
> computations on the fixtures, not by re-reading the PR's tests or its R generator. Oracles written from scratch
> for this review: numpy OLS with the line REFITTED on the adjusted wages and the target budget found by BISECTION
> on that refit (`target/regroundD/review/oracle.py`); R `predict.lm` bounds and `summary(lm)` on the adjusted wages
> (`bounds.R`, `refit.R`). Engine driven through a JSON-job probe over `optimize_inner`, `check_defensibility_on`,
> `calculate_efficient_frontier_inner` (`target/regroundD/review/src/main.rs`). Read-only on the repo; nothing
> written outside `target/regroundD/review/` and this file.
> Standard: would a figure read as something it is not?

## 0. Verdict

The arithmetic is right. 119 remedy runs, 32 schedules, 13 frontiers (109 points), 54 tie-fixture runs: every
reported figure equals my independent computation within float noise (worst 2.8e-9 $ per person on the 10 000-row
roster, 1.4e-5 $ on a 33.2 M$ total, 4e-13 relative). The post-schedule gap on the engine's OWN rows equals a refit
of the line (numpy and R) at 7e-11; the group test equals R `summary(lm)` at 1e-13 on p; the frontier's points equal
`optimize` at the same budget plus a fresh OLS at 6e-11, including a mixed-group tie I built for it.

No HIGH finding. One MEDIUM: under `target: Reference` the result carries two different post-schedule gaps on two
different lines (`new_unexplained_gap` on the reference line, `group_test.group_coefficient` on the pooled line),
and nothing on the wire names the second line; on the kink fixture they differ by 400-740 $, on the six-row roster a
target of 0 reached exactly sits beside a coefficient of −66 $. Four LOW items on labelling and one untested
assumption (positive pooled weights). Nothing blocks the merge; S-01 needs a one-field fix or a Track U/A wording
rule before the group-test sentence ships.

## 1. Findings

### S-01 MEDIUM — two gaps, two lines, one label missing (`group_test` under `target: Reference`)

- **What**: `check_defensibility` returns `new_unexplained_gap` on the line the remedy targeted (reference line when
  `target: Reference`) and `group_test.group_coefficient` on the POOLED line (T9: "the frontier's own OLS"). Both are
  "the compared group's gap after the schedule" in words; they are different statistics. `GroupTest`'s doc comment says
  pooled; no serialised field does, and `target_line` on the same result says `Midpoint` of the REFERENCE fit.
- **Evidence** (same schedule through `check_defensibility_on(..., Reference)`, my run `T7_*`):

  | Case | `new_unexplained_gap` (reference line) | `group_test.group_coefficient` (pooled line) | diff |
  |---|---|---|---|
  | F_ref_full | +96.25 | +95.70 | −0.55 |
  | F_ref_t_cap12k | −879.50 | −874.54 | +4.96 |
  | F_ref_thr5 | −787.98 | −795.93 | −7.95 |
  | K_ref_full | +1 026.19 | +1 425.83 | +399.64 |
  | K_both_cap60k | +81.50 | +734.23 | +652.73 |
  | K_both_eq_cap60k | −196.56 | +542.53 | +739.09 |
  | T_t0 (target 0, reached) | 0.00 | −66.23 | −66.23 |
  | T_t500 (target −500, reached) | −500.00 | −599.34 | −99.34 |

  Under `target: Pooled` the two coincide (diff ≤ 7.3e-11 on five cases), so the trap is Reference-only. The
  frontier's "first budget where the gap no longer differs from zero" is the pooled γ as well, so a consultant who
  types « ramener l'écart à 0 » (reference line) and reads D4's « le coefficient de groupe est X … au-dessus de la
  courbe » sees two zeros that are not the same zero.
- **Why it matters**: D4's sentence and D1's target input will sit on one screen. On K the sign can differ
  (−196 vs +542): « sous la courbe » from one figure, « au-dessus » from the other.
- **Fix** (engine, additive): `GroupTest` gains `line: "Pooled"` (serialised, constant) and the `OptimizationResult`
  doc states that under `Reference` the two figures are on different lines and may differ in sign. Track U/A: the D4
  sentence names the line (« sur la droite commune ») and never pairs the group test's direction with the
  reference-line target; or compute the group test on the reference line too (a t-test of mean_T(y' − X_T β_R'),
  which is not an OLS coefficient test) — not recommended, the pooled test is the standard one.

### S-02 LOW — the Pooled target rule assumes every payable compared row has a positive weight

- **What**: under `Pooled` a dollar to compared row i moves γ by w_i = d̃_i / d̃'d̃ (FWL; the engine's
  `GapWeights::pooled`). `best_reachable_gap` is γ at full payment and the Unreachable branch compares the target to
  it; the Greedy walk takes the first crossing. All of that is "the least budget that reaches g" only when the path is
  monotone, i.e. w_i > 0 for every payable compared row. w_i < 0 happens when the linear-probability fit of the
  indicator on X exceeds 1 at x_i (a high-leverage compared employee). Then γ can exceed `best` mid-path: a target
  above `best` is declared unreachable and the full need is paid although a smaller budget reached it.
- **Evidence**: weights computed by me on the four fixtures, all positive: F min 0.0238 (1/n_T 0.025), K min 0.0074
  (1/n_T 0.0167), T min 0.245, E min 0.000200 (1/n_T 0.000204); Σ w_T = 1.000 exactly on each (as theory says). My
  bisection oracle checks monotonicity on a 41-point grid for every reachable Pooled case and found none violated;
  the PR's R generator does the same (`stopifnot`) but only for the committed cases. Nothing in the engine checks it.
- **Fix**: in `optimize_inner` under `Pooled`, if any payable compared weight is ≤ 0, either refuse by name
  (`POOLED_TARGET_NON_MONOTONE`) or compute `best_reachable_gap` as the path maximum and say so in the doc. One line
  in `docs/DIAGNOSTICS.md` naming the assumption.

### S-03 LOW — two "below" counts on the wire that will meet on one screen

- **What**: D5's « personnes qui restent sous leur courbe » is `unfunded_count + threshold_excluded_count` (below the
  PAY LINE after the schedule); D4's « N sous » is `position_counts.below` (below the prediction interval's lower bound
  minus one cent). Both documented, both correct, different sets.
- **Evidence** (`optimize` then `check_defensibility` on its schedule, F): cap 25 000 Greedy: 22 still below the
  line vs 20 below the range (14 inside); threshold 5 %: 25 vs 23; cap 25 000 Equitable: 31 vs 26; LowerBound full:
  0 vs 0 (34 inside, 6 above).
- **Fix**: no engine change; Track U/A words them differently (« sous leur ligne » vs « sous la fourchette ») and
  never shows both as "below". A cross-reference between the two doc comments would help the app author.

### S-04 LOW — `required_budget` has one basis per entry point when a threshold or a bound is set

- **What**: `optimize.required_budget` is the eligible need to the chosen line under `min_gap_pct`;
  `check_defensibility.required_budget` is the midpoint need at threshold 0 (`target_line: Midpoint`). Equal on the
  default basis (T7 round trips: 0.0 difference on every Midpoint/0 case), different otherwise.
- **Evidence** (F): LowerBound 41 835 vs 51 030; UpperBound 61 220 vs 51 030; 5 % threshold 15 661 vs 51 030;
  Pooled UpperBound 109 022 vs 50 637.
- **Fix**: none in arithmetic (F-05 allowed it and `target_line` is on the wire). The app reads `required_budget` from
  `optimize` only; the `check_defensibility` doc says "threshold 0" explicitly.

### S-05 LOW — `closure` is 0 % for a target already met and 100 % for an unreachable one

- **What**: both are correct under D2 (share of the need funded) and both will surprise beside the target sentence:
  « cible déjà atteinte » with « 0 % du besoin financé »; « cible hors de portée » with « 100 % ».
- **Evidence**: F_ref_met, F_pooled_met, T_met: cost 0, closure 0.0; F_ref_unreach, F_pooled_unreach, T_unreach500:
  cost = full need, closure 1.0, `shortfall_to_target` 100.00 / 100.00 / 166.67 $ per person.
- **Fix**: Track U copy only.

### S-06 INFO — the post-schedule p-value is descriptive

- The adjusted wages are not a sample from the model the t-test assumes: the raises are deterministic functions of the
  fitted line and remove the compared group's negative residuals, so σ² shrinks mechanically. `group_test.p_value`
  and the frontier's p describe the adjusted roster; they are not evidence that the remedy is "fair". Pre-existing in
  the frontier (0120), asked for by D4; the copy must not read p as proof.

## 2. What was verified (per T item)

Tolerances: dollars 1e-6 absolute or 1e-9 relative, per-person gaps 1e-8, t 1e-8 relative, p 1e-9, counts exact.
"Mine" is the numpy oracle unless R is named. Fixtures: F = `0122-fixture-f-noisy.csv` (n_T 40), K =
`diag_kink_overlap.csv` (n_T 60), T = `0122-remedy-tiny.csv` (n_T 3), E = `employers_trust_fixture.csv` (n_T 4 892).

| Item | Check | Cases | Worst deviation |
|---|---|---|---|
| T1 budget rule | engine `total_cost` at a reachable target = budget found by bisection on the REFIT (not the closed form); `new_unexplained_gap` = target | 19 (Reference, Pooled, Greedy, Equitable, thresholds, bounds, F/K/T/E) | cost 1.4e-5 $ on 16.6 M$ (8e-13 rel), else ≤ 3e-9 $; gap 5.5e-12 |
| T1 Pooled is not the closed form | `target_budget` vs n_T(g − u0) | F half 25 207 vs 25 322; K half 18 818 vs 9 934; E half 16 620 330 vs 16 624 442 | engine agrees with the refit, not with the closed form |
| T1 strategy tie | Greedy = Equitable cost on the Reference line; differ under Pooled | F_ref_half 25 515.03 both; T_t500 2 500 both; F_pooled_half 25 207 vs 25 318 | 0 / as expected |
| T1 floor with threshold | `best_reachable_gap` with `min_gap_pct` 0/2/5 % and Lower/Upper = refit at full eligible payment | F thr5 −787.98; F lower −133.63; F upper +351.00 | 1.3e-9 |
| T1 already met / unreachable | pays 0 / pays the full eligible need; `shortfall_to_target` = g − best; `target_budget` 0 / need | 7 cases | exact |
| T1 cap | user cap below the target budget binds: cost = cap, `budget_binding` true; above: false | F_ref_t_cap12k, F_ref_t_cap_loose, F_pooled_t_cap10k, T_cap1000_t500 | exact |
| T3 refusals | `INVALID_BUDGET` (−1), `INVALID_MIN_GAP_PCT` (−0.01), `TARGET_GAP_WITH_REFERENCE_RAISES` | 3 | named |
| T4 cost split | `cost_target`, `cost_reference`, `need_target`, `need_reference`, `total_cost = cost_T + cost_R`, `required_budget = need_target`, from the engine's rows and from mine | all 116 runs | 1.4e-5 $ on 33 M$ (2e-13 rel); ≤ 2.3e-9 elsewhere |
| T5 formula B (reference raises) | `new_unexplained_gap` on the ENGINE's rows = refit of the reference line on the adjusted wages, numpy and R | F_both_* (6), K_both_* (3), schedules `*_both` | 6.8e-11 (R), 2.8e-9 (numpy, E) |
| T5 formula C (Pooled) | = refit γ, numpy and R, with and without reference raises | 17 Pooled runs + 8 Pooled schedules | 7.3e-11 |
| T5 `new_gap` | = orig + cost_T/n_T − cost_R/n_R and = mean(y'_T) − mean(y'_R) on the rows | all | 5.9e-11 |
| T6 midpoint line | `original_unexplained_gap` = mean_T(y − fair_mid) under Lower/Upper; `required_budget` on the bound; `overshoot_mean` = mean max(0, y − fair_mid) and = `best_reachable_gap` under default only | F lower/upper (7), F_pooled_full best 85.879 vs overshoot 85.696 | 1.3e-9 |
| T7 one sign | `check_defensibility` on `optimize`'s own schedule, same target: `original_gap`, `new_gap`, `original_unexplained_gap`, `new_unexplained_gap` equal (not magnitude); cost split equal; `required_budget` equal on the Midpoint/0 basis | 72 round trips (F, K, T; Reference and Pooled; raises on and off) | 0.0 on the four gaps; 1.5e-11 on costs |
| T8 positions | per-row `range_position`/`_before` vs R `predict.lm` bounds ± 0.01; `Below` ⇔ `!is_defensible`; `source`; `position_counts` vs my classification; sum = n_T; reference rows in no count | 32 schedules (partial, 3×, to upper, over upper, to lower, both groups, negative, empty; Reference and Pooled; F and K) + every optimise row | bounds 1.5e-8 $, counts exact |
| T9 group test | `group_coefficient`, `t`, `p`, `df` vs numpy OLS and R `summary(lm)` on the adjusted wages; `is_significant` ⇔ p < 1 − level | 32 schedules + 72 round trips | γ 4.6e-11, t 5.2e-13, p 1.3e-13 |
| T10 frontier follows the request | every point k: `optimize` at `points[k].budget` with the same settings → OLS on its rows = point's γ, t, df; money spent = min(budget, full need); axis end = full cost (both groups under raises); point 0 = unadjusted test | 13 frontiers, 109 points (default, Pooled, Equitable, LowerBound, Upper+Equitable, 5 %, raises, Pooled+Equitable+raises, Pooled+Lower+2 %, explicit max 60 000, K raises, K Pooled+Equitable, T) | γ 6.0e-11, t 5.3e-13, axis 3.8e-9 |
| T10 tie order | my 7-row fixture with a compared row and two reference rows tied at 500 $, raises on: frontier γ = `optimize` refit at 27 budgets, Reference and Pooled lines | 54 points | 0 mismatches |
| T10 explicit `max_budget` past the need | flat tail (95.70 at 52 500 and 60 000) | 1 | as expected |
| Allocation shape | Greedy: full prefix in diff order, ≤ 1 partial, zero suffix; Equitable: constant pay/diff; nobody ineligible paid; nobody paid above their shortfall | all 116 runs | 0 violations |
| Per-row fair wage and bounds | `fair_wage` = my midpoint; bounds = R `predict.lm` | all rows of all runs | 3.8e-9 / 1.5e-8 $ |
| D5 | no statute cited in any changed non-test file (`art. N`, `Pay Equity Act`, `Loi sur l'équité`, `E-12.001`) | 39 files | none |
| Doc claims | CHANGELOG/DIAGNOSTICS/API: "equals R lm refitted … 1e-6", "closure monotone", "ties paid in optimize's order", "Reference cost n_T(g − u0)", "overshoot = best under default" | — | all hold on my numbers |

Harness files: `target/regroundD/review/{Cargo.toml,src/main.rs,oracle.py,gen_jobs.py,check1.py,check2.py,bounds.R,refit.R,tie.csv}`;
outputs `out1.json` (122 jobs), `out2.json` (168), `out3.json`/`out4.json` (tie), `bounds.json`, `refit_out.json`.
Both checkers end with `issues == 0`.

## 3. Not covered by this lens

MCP enum refusals (V13), WASM baselines, D14 determinism, the deprecation and README text, the `-0.0` guard beyond
noting that every zero I saw serialised as `0.0`.
