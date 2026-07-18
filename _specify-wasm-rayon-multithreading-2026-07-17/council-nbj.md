# Council — NBJ seat (mandatory)

> Lens: the ten agentic failure modes + ten anti-patterns. Hunt for silent failure (#6),
> incorrect verification (#9), information withholding across the 7-spec seams (#8), and
> plausible-but-wrong (reading fluency as correctness). Every finding cites a line I Read.
> Date: 2026-07-18. Verdict: SHIP_WITH_FIXES.

## Standing assessment

This is a genuinely strong spec set. The anchor fidelity is real — I re-verified a dozen
of the audit's citations against the live tree and they hold (`analysis.rs:168` = MM
builder; `builder.rs:720-766` = RIF `decompose_quantile`; `math/rif.rs:14` = the RIF
formula; `builder.rs:825` = the seeded rep loop). The two applied corrections are
**directionally correct**. The problem is not what the corrections did — it is what they
*failed to propagate*. Correction 1 (MM→RIF wiring) touched the engine and trust-layer
specs but **never touched the deterministic-rng spec**, and correction 2 (thread-cap.js
ownership) fixed *who writes the file* but left *what value the worker enforces* still
contradictory. Both gaps are the classic post-edit failure mode: the edit is locally
coherent in the file it lands in, and silently incoherent at the seam with the file it
didn't touch. For a tool whose Charter says "wrong numbers = defensibility harm"
(`spec-charter.md:16`), the two seam gaps below are must-fix-before-build.

---

## Finding 1 — CRITICAL — The `seed` field does not propagate through `decompose_quantile`; correction 1 was never carried into the deterministic-rng spec (#8 information withholding, #9 incorrect verification, #6 silent failure)

**This is the sharpest defect in the set and it is invisible in every file read in isolation.**

The deterministic-rng spec makes `seed` a **field on `OaxacaBuilder`**, set via `.seed(u64)`
and resolved to `DEFAULT_SEED` at `run()` (`phase4-final-deterministic-rng.md:40-49`). The
whole INV-02 bit-identity proof rides on `master` = the resolved seed flowing into
`unit_rng(master, purpose, unit)` (`:69-80`).

Correction 1 rewired the WASM quantile branch to call `OaxacaBuilder::decompose_quantile(q)`
(`phase4-final-engine-parallel-surface.md` D5, the pasted `analysis.rs` block). But
`decompose_quantile` **constructs a fresh inner `OaxacaBuilder` and copies only six config
fields — not the seed** (verified `builder.rs:752-759`): it copies `predictors`,
`categorical_predictors`, `bootstrap_reps`, `reference_coefficients`, `normalize`,
`weights`. There is no `seed` copy because the field doesn't exist yet, and **the
deterministic-rng spec has zero mention of `decompose_quantile`, RIF, or the reroute**
(grepped the whole file — the string does not appear). Correction 1 updated
engine-parallel-surface and statistical-trust-layer and stopped there.

Consequence for a literal /build worker who implements both specs faithfully:
- They add `.seed()` / the `seed` field to `OaxacaBuilder` per rng-spec D1.
- They wire `analysis.rs` to `decompose_quantile(q)` per engine-spec D5/step 6.
- Nothing in either spec tells them to add `seed` propagation to the inner-builder
  construction at `builder.rs:752`. So the outer builder's seed is **silently dropped**
  for the entire quantile path.

Why this survives the acceptance gate (the incorrect-verification part): the inner builder
defaults `seed=None → DEFAULT_SEED`, so the quantile path is still *deterministic across
thread counts* — AC-6 (`phase4-final-deterministic-rng.md:214`, "byte-identical … for both
mean and quantile requests") **passes**, because it exercises the default-seed path where
a constant default masks the dropped seed. The test passes for the wrong reason.

Where it bites (the silent-failure part): the moment a caller uses `.seed(custom)` or
`seed_from_entropy()` (the spec's own opt-in for run-to-run variety, `:43-45`), the outer
builder records seed *X* in `RunMetadata` while the inner builder actually ran on
`DEFAULT_SEED`. For an **audit defensibility tool**, provenance metadata that records a seed
which does not reproduce the reported numbers is precisely the trust defect the whole
determinism workstream exists to eliminate. INV-02's letter ("same input + same seed →
bit-identical") is satisfied only because the seed is ignored — which is not the property
anyone wanted.

**Fix (gate, no re-research):** (a) add a `seed`-copy line to `decompose_quantile`'s inner
builder construction (`builder.rs:752-759`) and say so explicitly in **both** the engine
spec build steps and the deterministic-rng spec; (b) add an AC that sets a *non-default*
custom seed and asserts the quantile JSON changes with the seed and reproduces on repeat —
an AC the current default-only AC-6 cannot express.

---

## Finding 2 — MAJOR — The 8-thread ceiling is asserted in three places but enforced in none; the test encodes the 2-term formula as correct (#6 silent failure, #9 incorrect verification)

Correction 2 (RESOLUTION, `phase4-build-readiness-audit.md:176`) fixed *file ownership* of
`thread-cap.js` — good, that seam is now single-owner. It did **not** fix the audit's
"secondary defect" (Finding B secondary, `phase4-build-readiness-audit.md:144`): the
2-term-vs-3-term cap formula. The RESOLUTION only lists MINOR-1 (signature line) and MINOR-2
(intercept line) as fixed; the formula discrepancy was left open. It is still open:

- memory-budget says the worker applies a **3-term** min including the explicit `8`:
  `N = min(N_max_const, hardwareConcurrency, 8)` (`phase4-final-memory-budget.md:172`,
  restated AC-M8 `:393`).
- meridian-integration's **actual worked code** applies a **2-term** min with no `8`:
  `const cap = Math.min(navigator.hardwareConcurrency || 1, MEMORY_THREAD_CAP)`
  (`phase4-final-meridian-integration.md:105`; prose `:89` also 2-term).
- `N_max_const` itself is **not** pre-clamped to 8: AC-M6 defines it as
  `floor((M_max − H_res − Marg)/(Sc + St))` (`phase4-final-memory-budget.md:390`) — a pure
  memory quotient, no min-with-8.

So on a corp PC with `hardwareConcurrency > 8` **and** enough measured headroom that
`N_max_const ≥ 8`, the shipped worker spawns `min(hwConc, N_max_const)` > 8 threads — the
deliberate ceiling of 8 is never applied anywhere in code. Whether that OOMs depends on
whether 8 is memory-binding (it may be a conservative diminishing-returns cap), but the
Charter flags H2 memory margins as **placeholder** (`phase4.5-reconciliation.md:29`,
"memory of utmost importance … may want larger headroom"), so silently exceeding a
conservative cap is exactly the wrong direction.

The incorrect-verification layer makes it worse: **AC-M1.1 codifies the 2-term formula as
the pass condition** — `threads === Math.min(navigator.hardwareConcurrency||1,
MEMORY_THREAD_CAP)` (`phase4-final-meridian-integration.md:238`). The test asserts the buggy
formula, so it can never catch the missing `8`. A green test bar here means "the worker
matches the under-enforcing formula," not "the ceiling holds."

**Fix (gate):** pick one representation and make code + AC agree. Cleanest: bake the clamp
into the value — `N_max_const := min(floor(...), 8)` at emission (memory-budget D3),
document it in the `thread-cap.js` comment, and the worker's 2-term min is then correct.
Then AC-M6 must assert the `min(_, 8)` is present, not just the quotient.

---

## Finding 3 — MAJOR — Under option (a), the CLI and the browser/MCP silently disagree on the quantile method; the founder decision is framed as if only the WASM number changes (#6 silent failure, #8 information withholding)

The a/b/c decision table (`phase4-final-engine-parallel-surface.md` D5) is honest that
option (a) changes the **WASM** quantile aggregate from MM to RIF. What it does **not**
surface is the cross-surface consequence, which I verified by grepping every call site:

- `engine/src/analysis.rs:168` (feeds **WASM + meridian-mcp**) → currently
  `QuantileDecompositionBuilder` (MM). Correction 1 switches this to RIF.
- `oaxaca_blinder/src/main.rs:247` (the **`oaxaca-cli`** binary) →
  `QuantileDecompositionBuilder` (MM), **unchanged** — INV-01 keeps native byte-equivalent
  (`spec-charter.md:69`), and the Charter's files-to-modify list does not include `main.rs`.

So after option (a), the **same tool, same input** returns an MM quantile decomposition from
the CLI and a RIF quantile decomposition from Meridian/MCP — different estimators, different
numbers (D5 itself says "different estimators and will produce different numbers"). Engine
build-step 9 only checks that the MM path "still compiles" and "its own tests pass"
(`phase4-final-engine-parallel-surface.md` step 9) — it does not raise "you now ship two
different quantile methods across two surfaces of a defensibility tool." For a Charter whose
blast radius is *client-facing, wrong-numbers-is-harm*, two entry surfaces disagreeing on
the headline "90th-percentile gap" is a latent defensibility trap, and it is a decision the
founder is currently making **blind** because the a/b/c table scopes it to the WASM number
only.

This is not an argument against option (a) — RIF is the coherent, additive, tested choice.
It is an argument that the founder decision must **also** decide the CLI: either switch
`main.rs:247` to `decompose_quantile` in the same change (one method everywhere) or
explicitly accept and document the CLI-vs-browser divergence in the engine repo's CLAUDE.md
alongside the INV-08 ruling. Right now neither is specified.

---

## Finding 4 — MINOR — The deterministic-rng spec still describes the pre-correction (MM) world; its quantile determinism work and AC-6 quantile assertion point at a path the WASM no longer uses under option (a) (#8 information withholding)

Same root cause as Finding 1 (correction 1 not propagated), separate symptom. The rng spec's
D4 seeds the **MM** simulation at `quantile_decomposition.rs:215,244`
(`phase4-final-deterministic-rng.md:104-120`), and its In-Scope-3 framing
(`:15-16,26`) treats the MM path as *the* quantile path. Under option (a) the WASM quantile
request routes through `decompose_quantile → run()` (the RIF/`builder.rs:825` loop), so:
- D4's MM seeding matters only for the **CLI's** MM path now, not the shipped WASM surface —
  it is not wasted (CLI reproducibility is still nice) but it is mis-scoped as "the quantile
  determinism deliverable."
- AC-6's "quantile request byte-identical … across thread counts" (`:214`) will, if the
  parity harness sends a `quantile` request through the *wired* engine, actually exercise the
  RIF/`builder.rs` path — whose determinism depends on D3, not D4. A worker reading the rng
  spec alone will believe WASM quantile = MM and may point the parity fixture at the wrong
  builder.

**Fix (gate):** add one paragraph to the deterministic-rng spec noting the MM→RIF reroute,
that WASM quantile determinism now rides on D3 (`builder.rs` bootstrap) via
`decompose_quantile`, that D4/MM seeding now serves the CLI path, and that the seed field
must propagate into `decompose_quantile`'s inner builder (Finding 1).

---

## Finding 5 — MINOR — The RIF golden validates RIF-against-R-RIF, not RIF-against-the-actual-quantile-gap the UI labels (reading fluency as correctness)

The corrected trust-layer golden is `ddecompose(..., reweighting=FALSE)`
(`phase4-final-engine-parallel-surface.md` D5, "Golden"), which is the *same* one-stage
RIF-OLS estimator `decompose_quantile` computes. That correctly closes the
implementation-fidelity gap (today `rif_test.rs` only asserts `total_gap > 0.0` — verified
`oaxaca_blinder/tests/rif_test.rs:50`, a pure smoke test). But it validates RIF-vs-RIF. It
does **not** assert that the RIF `total_gap` (= mean(RIF_A) − mean(RIF_B), which equals the
empirical quantile gap only *approximately*, to the quality of the `f_Y(q_τ)` Silverman-KDE
density estimate, `math/rif.rs:37-72`) is close to the actual empirical Q_τ gap that the
Meridian UI labels "quantile gap." D5's limitation notes (i)/(iii) acknowledge the density
sensitivity in prose, so this is documented, not hidden — but for a defensibility tool the
identity/property suite should include one test asserting `mean(RIF) ≈ empirical Q_τ` within
a stated tolerance at realistic n, so the approximation quality is *checked*, not just
*disclosed*. Otherwise "silent failure" lives in the gap between the label and the estimator
on adversarial (heavy-tailed, tied, small-group) inputs — precisely the cases In-Scope 9
says the current QR tests miss.

---

## What a smart /build worker would get wrong given these specs

1. Wire `decompose_quantile` per engine-spec D5, add the `seed` field per rng-spec D1, and
   **never connect the two** — shipping a quantile path whose seed silently doesn't
   propagate, with a green AC-6 (Finding 1).
2. Copy meridian D2's 2-term `Math.min(...)` verbatim and pass AC-M1.1 — shipping a worker
   that ignores the 8-ceiling on >8-core machines (Finding 2).
3. Switch `analysis.rs:168` to RIF and leave `main.rs:247` on MM because INV-01 says "don't
   touch native," producing a CLI/browser numeric split nobody signed off on (Finding 3).

All three are seam gaps, not missing research — gate-fixable in one editing pass across the
three specs. None require re-running Phase 3.

## Verdict

**SHIP_WITH_FIXES.** Findings 1–3 are must-fix before /build dispatch (client-facing blast
radius, wrong-numbers-is-harm). Findings 4–5 are same-pass cleanups. The corrections that
were applied are sound in the files they touched; the remaining risk is entirely in the
seams they did not.
