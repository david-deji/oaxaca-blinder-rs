# Verification

External oracles for the numbers this workspace reports. Each generator computes its expected values with software
that shares no code with the engine (R, statsmodels, or a brute-force enumeration), writes them to a committed
fixture, and records the SHA-256 of the generator and of every input fixture in that golden. The Rust tests read
the committed golden offline and refuse it when any recorded hash no longer matches the files on disk.

`cargo test` never runs R or Python. A generator is run by hand when an oracle is extended.

**Never edit a generator, even a comment.** Its hash is recorded in the golden, and the tests go red until the golden
is regenerated with the original R or Python environment.

| Generator | Oracle | Golden it writes | Read by | Tolerance |
|---|---|---|---|---|
| `gen_parity_golden.py` | statsmodels OLS | `oaxaca_blinder/tests/fixtures/parity_golden.json` | `oaxaca_blinder/tests/parity_test.rs` | 1e-6 |
| `gen_trust_goldens.R` | R `lm`, `quantreg`, `ddecompose` | `oaxaca_blinder/tests/fixtures/trust_goldens_r.json` | `trust_golden_r_test.rs`, `bootstrap_se_golden_test.rs`, `quantile_detail_golden_test.rs`, `qr_location_scale_test.rs` | 1e-6 relative; bootstrap standard errors 1e-3 relative; quantile detail per test |
| `gen_norm_goldens.R` (via `regen_norm_goldens.sh`) | R `oaxaca`, `ddecompose` | `oaxaca_blinder/tests/fixtures/norm_goldens_r.json` | `normalization_oracle_test.rs`, `cli_normalization_test.rs`, `engine/tests/null_free_regression_test.rs` | 1e-10 package, 1e-9 refit, 1e-12 identity |
| `gen_diag_goldens.R` | base R, `ddecompose`, `Hmisc` | `oaxaca_blinder/tests/fixtures/diag_goldens_r.json` | `engine/tests/{intervals,support_diagnostics,quantile_report}_test.rs`, `weights_kind_test.rs` | see `oaxaca_blinder/tests/support/diag_golden.rs` and each test |
| `gen_pooled_target_goldens.R` | base R `lm`, cross-checked against R `oaxaca` | `oaxaca_blinder/tests/fixtures/pooled_target_goldens_r.json` | `engine/tests/pooled_target_test.rs` | 1e-9, critical values 1e-10 |
| `gen_remedy_goldens.R` | base R `lm`, `predict.lm` | `engine/tests/fixtures/remedy_goldens_r.json` | `engine/tests/remedy_oracle_test.rs` | 1e-6 dollars, 1e-10 relative |
| `gen_remedy_bruteforce.py` | exhaustive enumeration on a 250-dollar grid | `engine/tests/fixtures/remedy_bruteforce.json` | `engine/tests/remedy_rules_test.rs` | exact on the grid |

Other files:

- `bench_methods.R` is the R side of a timing comparison, not a golden.
- `browser-parity/` runs the threaded WASM build in a browser and compares it with the native result across the
  sequential and threaded modes. It runs in CI as `Browser Byte-Parity`.
- `gen_diag_goldens.R` names `ground/2026-10-09-C-reground.md` in a comment. That file is now
  `records/ground/2026-10-09-C-reground.md`.

- Some generators carry paths from the original author's machine: the default raw data file of
  `gen_trust_goldens.R` (set `TRUST_RAW_CSV`; when the file is absent the committed fixture is used) and an R library
  path in the usage comment of others (set `R_LIBS_USER` to your own). They are left as written because editing a
  generator changes its recorded hash.

## Requirements to regenerate

R with `oaxaca` 0.1.5, `ddecompose` 1.0.0, `quantreg`, `Hmisc`, `jsonlite` and `digest`; Python with
`statsmodels` 0.14 or later, `numpy` and `pandas`. The generator headers state the exact versions and the command.
