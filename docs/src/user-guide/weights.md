# Weights

A weights column always comes with a statement of what the weights mean. There is no default, because no single
convention makes both "uniform fractional weights change nothing" and "a weight of 2 is the row twice" true.

| Kind | Meaning | Estimates | Quantiles |
|---|---|---|---|
| `Frequency` | whole-number counts (a headcount) | a row with weight 2 is that row twice | type 7 on the expanded sample |
| `Relative` | relative importance (FTE, survey design weights) | rescaled to sum to the row count, so only ratios matter | `Hmisc::wtd.quantile(type = "quantile", normwt = TRUE)` |

Under `Frequency` the bootstrap follows the same rule. A replicate draws as many employees as the weights sum to,
so standard errors, intervals and p-values equal those of the data written out with repeated rows. A fractional,
negative or non-finite weight is refused with `INVALID_WEIGHT`, naming the column, the row and the value. Under
`Relative`, uniform weights of any size change nothing, and zero-weight rows carry no mass under both kinds.

```rust,ignore
builder.weights("headcount").weights_kind(WeightsKind::Frequency);
```

```bash
oaxaca-cli ... --weights headcount --weights-kind frequency
```

Naming a weights column without a kind is an error (`WEIGHTS_KIND_REQUIRED` from the library, a usage error from the
command line). The engine and the MCP server take no weights.

`oaxaca_blinder::weighted_quantile(values, weights, tau, kind)` is the function the quantile paths use, and is public.
