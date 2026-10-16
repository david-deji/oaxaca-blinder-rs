# Experimental methods

Four methods have no external check against R or Stata. Their results can change in any release. They are behind a
cargo feature that is off by default.

| Method | Item | What it does |
|---|---|---|
| AKM | `AkmBuilder` | Worker and firm fixed effects (Abowd, Kramarz and Margolis) on linked employer-employee data. |
| DFL | `run_dfl` | Reweighting counterfactual (DiNardo, Fortin and Lemieux). |
| JMP | `decompose_changes` | Change in a gap over time (Juhn, Murphy and Pierce). |
| Matching | `MatchingEngine` | Nearest-neighbour and propensity-score matching weights. |

Turn them on in `Cargo.toml`:

```toml
oaxaca_blinder = { version = "0.3", features = ["experimental"] }
```

For the command line:

```bash
cargo install oaxaca_blinder --features experimental
```

The command line then accepts `--analysis-type akm` and `--analysis-type match` with their options
(`--worker-id`, `--firm-id`, `--k-neighbors`, `--matching-method`).

The mean and RIF-quantile decompositions, the reference schemes, normalisation and weights are checked against R
(see [Reproducing results in R](../reproducing/r.md)). The Heckman correction is tested on simulated data with a known
answer and has no R or Stata run to compare with.
