# Epic 0014-close — Round-2 Closing Walk Report

> Fresh profile (`:39224`, `fresh-27`), dev server `:5199`, against the **freshly republished** blobs
> (`scripts/build-wasm.sh`, seq `3491352075bb…`, threaded `8b0672200f78…`).
> Steps `w14-01` … `w14-03`. Verdict: **AC-3 (browser leg), AC-5 and AC-7 all pass. No defects found.**

This walk exists because neither build lane could run the browser leg — lane A's sandbox
cannot build the pinned-nightly threaded artifact and drive headless Chromium, and adversary B
correctly refused to sign off D6 from the frontend diff alone (its MAJOR finding: the localized
refusal was unreachable until D7 republished). Everything below ran against the real engine
binary the browser actually loads.

## w14-01 — the trust line (D5, AC-4)

Reopened « Provenance Walk 0033 » → Analyse/Décomposition. Store carried:

```
run_metadata {seed: "6840134480574414850", rng_algorithm: "ChaCha8",
              rand_chacha_version: "0.3.1",
              bootstrap_reps_requested: 10, succeeded: 10, discarded: 0}
```

Rendered: **« Graine 0x5eed0a11ca8a0002 · 10/10 réplications bootstrap »**

The hex is `DEFAULT_SEED` (`0x5EED_0A11_CA8A_0002`) exactly — the decimal-string → `BigInt` →
hex conversion is lossless across the full u64 range, which is why the engine serializes the
seed as a string in the first place (it exceeds `Number.MAX_SAFE_INTEGER`). Since stage 1 this
field shipped in every result with **zero consumers**; an auditor asking "is this reproducible"
had nothing on screen. Now the answer is on the results card. `w14-01-trust-line.png`

The `discarded > 0` warning branch was not walked — crafting data that reliably fails some but
not all bootstrap replicates is not worth the fixture gymnastics; it is unit-pinned instead.

## w14-02 — determinism from the browser (INV-02, AC-3's spirit)

Ran the identical analysis twice through the live worker and byte-compared the serialized
result payloads:

```
[run 1 bytes] 834  [run 2 bytes] 834
[byte-identical] YES
```

INV-02's within-platform leg was already CI-tested on native and wasm separately. This is the
same property observed where it actually matters — the browser an auditor uses, against the
blob just published. `w14-02-determinism.png`

## w14-03 — the headline: named refusal, localized (D1 + D6, AC-1/AC-5/AC-7)

Uploaded a crafted level-confined CSV (`job_title = "Direction"` exists only among `female`
rows), set the reference to `male`, added `job_title` as a categorical predictor, ran.

Engine returned, verbatim across the WASM boundary:

```
EMPTY_LEVEL_IN_GROUP: column=job_title, level=Direction, missing_from_group=male
```

Displayed to the operator:

> « L'analyse d'équité salariale n'a pas pu être complétée : Le niveau « Direction » de
> « job_title » est absent du groupe « male » : la décomposition ne peut pas comparer ce
> niveau entre groupes. »

Leak check for `EMPTY_LEVEL_IN_GROUP` / `Cholesky` / `multicollinearity` anywhere in the page:
**none**. `w14-03-localized-refusal.png`

Before this round the same CSV produced « Failed to perform Cholesky decomposition…
multicollinearity » — naming neither the column, the level, nor the group (map B v4, confirmed
empirically there). The operator's next action went from "guess which of your columns is the
problem" to "Direction only appears among women; either drop it or widen the extract."

## Orchestrator fixes applied before the walk (adversary-A MINORs)

- **MINOR-1** (real coverage gap): candidate levels now come from the **unsplit** frame, not the
  compared pair's union. With a 3+-valued group column, a level living only in the excluded
  group is absent from both compared frames — yet the dummy column was encoded from the full
  frame and is constant-zero in both design matrices. The union scan could never see it.
  Pinned by `test_level_confined_to_an_excluded_third_group`.
- **MINOR-2**: presence is weight-aware. A level whose rows all carry `weight == 0` contributes
  an effectively-zero column to `X'WX`. Pinned by `test_zero_weight_level_is_absent_for_estimation`.
- **MINOR-3**: the parity spec refuses a native baseline older than the engine source, so a
  direct `npx playwright test` (which skips `pretest`) cannot silently compare against a stale
  gitignored file.

Both new tests mutation-verified: reverting each fix fails exactly its own test and nothing else
(7 passed / 2 failed under mutation, 9 passed restored).

## Gates

- Engine: 38 lib + 9 integration green; full `pay-equity-engine` suite green including the
  within-platform thread-parity test.
- App: **266 files / 4321 passed**, i18n lint clean, eslint clean. (One earlier full-suite run
  showed 5 failures including `engineRowKeyContract` — all 5000 ms timeouts under host
  contention; each passes isolated, and a clean re-run was fully green.)
- Blobs republished and sha256-verified into `frontend/src/{wasm,wasm-threaded}/`.

## Accepted gaps carried out of this epic (not built, deliberately)

Founder-reviewed deferrals from the original 0014 pass, re-confirmed still open:
OOM structured self-report (item H); R-machine-blocked golden items E and G (bootstrap-SE
consuming test, tail-tau quantile SE); Playwright infrastructure in the app repo; an automated
test for the sequential-fallback path (the `crossOriginIsolated` gate has shipped code but no
test in either repo); DFL two-stage reweighting.

## Observations (not taken)

- `cargo clippy --workspace --all-targets --all-features -- -D warnings` is **already red at
  HEAD** on two pre-existing findings in files this epic never touched
  (`engine/tests/row_key_integration_test.rs:181` deny-level `overly_complex_bool_expr`;
  `oaxaca_blinder/src/akm.rs:376` `needless_range_loop`). That CI gate cannot pass until they
  are fixed — worth its own small issue.
- Repo-wide `cargo fmt` drift exists in ~12 untouched files; lane A correctly reverted its
  incidental reformatting to keep the diff scoped.
- The walk's ad-hoc CSV upload cleared « Provenance Walk 0033 »'s record provenance by design;
  that project is walk scratch and restages its own state each round.
