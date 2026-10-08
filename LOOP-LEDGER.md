# LOOP-LEDGER — oaxaca-blinder-rs (Meridian's engine)

> Updated: 2026-10-08 · Epics since last consolidation: 0 (first engine-scoped cycle)
> Schema: `internal-ops-bureau/knowledge/dev-loop.md` § LOOP-LEDGER.md schema (telos-machina, 0724-OPS).
> Issues live in the app repo (`pay-equity-app/issues/`, prefix MERIDIAN). The app's own ledger is
> `pay-equity-app/LOOP-LEDGER.md`; this one tracks engine-only cycles. Cross-repo cycles are recorded in both.
> Setup state: no `scripts/ground.sh` and no `scripts/verify-live.sh` in this repo yet. GROUND runs by
> hand (dispatched lanes + verifiers), and every probe-derived claim without a file:line is UNVERIFIED.

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

## Dark-gate registry
| Gate | What it guards | Last-ran receipt | Age |
|---|---|---|---|
| CI `WASM Build + Verify` | Shipped WASM bytes match the committed sha256 baselines; threaded build reproducible | red on every main run since 2026-08-20 (sequential sha256 mismatch) | 49 days red as of 2026-10-08 |
| CI `Browser Byte-Parity` | native <-> WASM agreement at 1e-6 across seq/t2/t4 | skipped on main since 2026-09-21 (depends on the red WASM job) | 17+ days |
| CI `Security Audit` | RUSTSEC advisories | red: latest cargo-audit needs rustc 1.96, toolchain pinned 1.90.0 | unknown start |

## Open issues by age
| Issue | Filed | Days open | Named as declined option (count) |
|---|---|---|---|
| 0117-MERIDIAN (engine PR backlog) | 2026-10-08 | 0 | n/a |

## Proposals
| Date | Picked | Appetite | Reason | Declined (ids) | Proposal file |
|---|---|---|---|---|---|
| 2026-10-08 | **A, then B, C, D** (David queued all four, in that order) | scoped | A: the only finding where money reaches the wrong person, reachable with one blank cell; B, C, D follow in order, each re-grounded before it starts | none declined | `ground/2026-10-08-proposal.md` |

## Receipts
| Epic | verify-live receipt | Result | Commit |
|---|---|---|---|
| 0117 (PR triage) | none: no `verify-live.sh` in this repo. Local gates: fmt clean, clippy -D warnings clean, 211/211 tests; CI Quality Gates pass on #96 | pass (gates only) | 9e4bd97 |

## Cost log
| Epic | Output tokens by model | Orchestrator share | Wall-clock | Source |
|---|---|---|---|---|

## Consolidations
| Date | Epics since previous | What was paid down |
|---|---|---|
