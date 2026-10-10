# LOOP-LEDGER — oaxaca-blinder-rs (Meridian's engine)

> Updated: 2026-10-08 · Epics since last consolidation: 0 (first engine-scoped cycle)
> Schema: `internal-ops-bureau/knowledge/dev-loop.md` § LOOP-LEDGER.md schema (telos-machina, 0724-OPS).
> Issues live in the app repo (`pay-equity-app/issues/`, prefix MERIDIAN). The app's own ledger is
> `pay-equity-app/LOOP-LEDGER.md`; this one tracks engine-only cycles. Cross-repo cycles are recorded in both.
> Setup state (since 0119): `scripts/ground.sh` (probes JSON, exit 0, errors[] validator) and
> `scripts/verify-live.sh <epic>` (engine receipt embedding the app receipt) exist; main is protected by the `gate` check.

## Carried seams
| Seam | First seen (epic) | Still open? | Evidence |
|---|---|---|---|
| MCP enum strings fall through to a default analysis instead of erroring | 0096 | yes (re-verify at GROUND) | `pay-equity-app/issues/0096-MERIDIAN-engine-audit.md` MEDIUM-1 |
| MCP clamps `bootstrap_reps` to 10,000 without reporting it | 0096 | yes (re-verify at GROUND) | 0096 MEDIUM-2 |
| `t_stat` reads 0.0 when the standard error is NaN | 0096 | yes (re-verify at GROUND) | 0096 MEDIUM-3 |
| Golden generators (R, Python) never re-run in CI | 0096 | yes | 0097 § Not done |

## Flake registry
| Test / gate | First seen | Epics carried | Last status | Root cause (or "unknown") |
|---|---|---|---|---|
| `null_free_regression_test` `decompose/parity_fixture` (CI only) | 0118 | 0 (fixed in-epic) | pass | last-bit float differences between machines on the largest design (CPU-dispatched matrix kernels, inferred); a bit-identity hash cannot hold across machines, so the golden is now the pre-0118 JSON compared at 1e-9 relative |

## Dark-gate registry
| Gate | What it guards | Last-ran receipt | Age |
|---|---|---|---|
| CI `WASM Sequential` / `WASM Threaded` | Shipped WASM bytes equal the committed baselines; CI and the dev box now build byte-identical blobs (0119 S1: rust-src remapped to /rustc/<hash>) | main run 37911861290 success (2026-10-09); was red on every main run 2026-07-19..2026-10-05 | re-armed 2026-10-09 |
| CI `WASM Threaded Reproducibility` | threaded build-std double-build byte-identical | first CI execution ever: 0119, main run 37911861290 success | re-armed 2026-10-09 |
| CI `Browser Byte-Parity` | native <-> WASM at 1e-6 across seq/t2/t4, 75 numeric leaves | main run 37911861290 success; independent of the hash jobs since 0119 S2 (mutant M3 37908856468) | re-armed 2026-10-09 (last green before: 2026-08-20) |
| CI `Security Audit` | `cargo audit --deny warnings` + dated-ignore validator | main run 37911861290 success; mutants A1-A3 red | re-armed 2026-10-09 |
| CI `gate` | every gating job `success`; the only required check on main (protection applied 2026-10-09, owner bypass on) | main run 37911861290 success; mutant M4 red on a skipped job | new 2026-10-09 |
| Golden generators (R `gen_trust_goldens.R`, Python `gen_parity_golden.py`) | committed goldens still match their generators | never run in CI (0097 § Not done, TRUST-12) | carried |

## Open issues by age
| Issue | Filed | Days open | Named as declined option (count) |
|---|---|---|---|
| 0117-MERIDIAN (engine PR backlog) | 2026-10-08 | 0 | n/a |

## Proposals
| Date | Picked | Appetite | Reason | Declined (ids) | Proposal file |
|---|---|---|---|---|---|
| 2026-10-08 | **A, then B, C, D** (David queued all four, in that order) | scoped | A: the only finding where money reaches the wrong person, reachable with one blank cell; B, C, D follow in order, each re-grounded before it starts | none declined | `ground/2026-10-08-proposal.md` |
| 2026-10-09 | **Meridian phone layout after D, before the release** (David): the app shell keeps its 240 px sidebar at 390 px (about 150 px of content); one app epic for the shell plus 0120's deferred screen items | scoped | the release review should start from a usable phone app | before D; after the release | `pay-equity-app/issues/0120-*` Deferred table |
| 2026-10-09 | **Release 0.3.0 after D** (David): the consolidation after D becomes a release review of everything since v0.2.0 (2025-11-26, 184 commits), one breaking release carrying C and D; 0.3.0, no 1.0 stability promise yet | scoped | crates.io holds oaxaca_blinder 0.2.2 for outside users; C and D both change outputs, so one break instead of two | release before D; library-only release; 1.0.0 | `ground/2026-10-09-release-review.md` (inventory, in progress) |

## Receipts
| Epic | verify-live receipt | Result | Commit |
|---|---|---|---|
| 0117 (PR triage) | none: no `verify-live.sh` in this repo. Local gates: fmt clean, clippy -D warnings clean, 211/211 tests; CI Quality Gates pass on #96. App receipt `pay-equity-app/ground/receipts/0117-live.json` pass 10/10 | pass | 9e4bd97 |
| 0119 (CI carries a signal) | `ground/receipts/0119-live.json` pass 5/5 (first engine-side receipt: wasm_verify, published_vs_pkg, app real-blob specs 63/63, app verify-live embedded, app tree committed); main CI 37911861290 all green incl. gate; 7 CI mutants red at the expected step | pass | engine 6ec23be, app aac33ef5 |
| 0118 (rows land on own employee) | `pay-equity-app/ground/receipts/0118-live.json` pass 11/11, new permanent check `remedy_dollars_land_on_own_employee` (red, 239 violations, on a build from the pre-fix blobs); engine CI Quality Gates pass on #97; V8 mutation table (a)-(h) all red-then-restored | pass | engine cf7a2af, app 1fc7143a |
| 0120 (numbers read as findings are artefacts) | engine `ground/receipts/0120-live.json` pass 10/10 (wasm_verify, published_vs_pkg, app real-blob specs 73/73, app verify-live embedded, app tree committed, main CI 37997358357 incl. gate, CI raw hashes = baselines = built here = shipped manifests, shipped-blob relabel and carve-out invariance, T8 zero-budget identity); app `pay-equity-app/ground/receipts/0120-live.json` pass 18/18 with 7 new checks each shown red on a planted change; engine PRs #104 and #105 merged | pass | engine 6f3113e + receipt PR, app c1698b96 |

## Cost log
| Epic | Output tokens by model | Orchestrator share | Wall-clock | Source |
|---|---|---|---|---|

## Consolidations
| Date | Epics since previous | What was paid down |
|---|---|---|
