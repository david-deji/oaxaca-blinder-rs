# Epic 0014-close — Engine Rust slice: 0014 close-out + reference-absent bootstrap guard

```yaml
# Workflow State
phase: executing
completed: []
current: map (2 sonnet scouts — spec-vs-shipped audit; reference-absent edge + publish freshness)
issue: 0014-MERIDIAN (pay-equity-app/issues/) — already open, stages 1-4 landed; this epic closes it honestly
started_at: 2026-08-19
```

## Why this epic

Founder pick after 0036 close ("Engine Rust slice"). Ground truth discovered at epic start:
0014's four build stages ALREADY LANDED in the engine repo (356faab stage 1 determinism,
c005837 stage 2 memory, 65f13b2 stage 3 threading, 623ff98 stage 4 validation, 3858b43
follow-ups) — but the issue file still reads `Status: SPECIFIED`, its log ends at the
/specify handoff, and one statistical edge is carried as unconfirmed: **what happens when a
bootstrap resample loses the reference group entirely** (mean and quantile paths). The app's
suites mock the engine, so none of this is caught frontend-side.

## Round 1 scope (from map verdicts)

1. Audit the 7 phase4-final specs' ACs against HEAD — shipped / partial / absent, file:line.
2. Confirm or refute the reference-absent bootstrap guard; if absent, build it (fail-loud
   into the deterministic discard partition, never silent wrong numbers).
3. Verify published WASM blobs in the app match engine HEAD; republish if stale.
4. Close 0014 honestly: issue log rows for stages 1-4 (retroactive, evidence-linked),
   status → RESOLVED only for what's proven.

## Round 2

Closing walk (fresh :39224): determinism visible from the browser — run the same analysis
twice, byte-compare results incl. run_metadata (seed, rep accounting); exercise the
reference-absent edge through the UI if reachable (tiny group + bootstrap).
