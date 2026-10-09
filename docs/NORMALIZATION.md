# Categorical normalisation: which surface does what (0120-MERIDIAN)

One rule: **a result that names a categorical level as a driver is normalised**; a result that
only predicts, or exposes the raw terms of a prediction model, is not, and says so in its doc.
Normalised means every level of every categorical, the alphabetically first one included, is a
deviation from the pooled-sample population-share-weighted average of the levels, the intercept
absorbs the average, and `run_metadata.normalization` records the convention and the shares.
Aggregates are identical with and without it.

| Surface | Normalised | Why |
|---|---|---|
| WASM `decompose`, engine `decompose_inner` (OLS and quantile) | yes, population-share | the per-level rows are the drivers the app ranks |
| WASM `verify_adjustments`, engine `verify_inner` | yes | same builder as `decompose` |
| MCP `forensic_decomposition`, `verify_adjustments` | yes | call the two functions above |
| WASM `optimize`, `check_defensibility`, `calculate_efficient_frontier`; MCP `simulate_remediation`, `check_defensibility`, `generate_efficient_frontier` | raw by design | they fit a fair-wage model for prediction (coding-invariant). `model_coefficients` and per-employee `contributions` are RAW model terms, documented as such on `Contribution`; a consumer never ranks them as drivers |
| CLI `run` (mean and quantile), `report` | yes, population-share | CLI and WASM parity is itself tested (`cli_wasm_parity_test.rs`); `--normalization equal-share` reproduces Stata / R `oaxaca` / `ddecompose`, `--normalization none` returns raw coding |
| CLI `run --analysis-type akm`, `match` | n/a | no per-level contributions |
| Python `fit`, `fit_quantile` | yes in code | module is not compiled (`pub mod python` is commented out in `lib.rs`) |
| Library `OaxacaBuilder::run`, `decompose_quantile`, `from_formula` | **raw unless `.normalize(vars)` / `.normalize_all_categoricals()`** | opt-in per call, so library goldens stay raw (`trust_golden_r_test.rs`, `quantile_detail_golden_test.rs`, parity baselines) |
| Library `QuantileDecompositionBuilder` (Machado-Mata) | raw by design | library-only, off the shipped surface, no external oracle |

The shipped engine and the CLI call `.normalize_all_categoricals()` on every run; the convention is
population-share. `NormalizationConvention::EqualShare` exists so the engine can be checked against
the packages (they implement only the equal-share restriction).

## Goldens

| File | Status | Oracle |
|---|---|---|
| `oaxaca_blinder/tests/fixtures/trust_goldens_r.json` | existing blocks byte-identical, one block ADDED (`quantile_detail_normalized`) | `ddecompose(rifreg_statistic = "quantiles", normalize_factors = TRUE)`; shift compared at a measured tolerance |
| `engine/tests/fixtures/0118-null-free-golden.txt` | NOT regenerated | field-scoped comparator in `null_free_regression_test.rs` |
| `oaxaca_blinder/tests/fixtures/norm_goldens_r.json` (+ `norm_skewed_fixture.csv`, `norm_balanced_fixture.csv`, `norm_skewed_rif.csv`) | NEW | base-R `lm()` weighted-effect-coding refit, `ddecompose`, R `oaxaca`; `verification/regen_norm_goldens.sh` |
| `engine/pay_equity_engine*.wasm.sha256` | re-recorded with `scripts/build-wasm.sh --record --no-publish` | n/a |
