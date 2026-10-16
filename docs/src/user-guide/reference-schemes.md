# Choosing a reference scheme

The decomposition depends on the coefficient vector \\( \beta^\ast  \\) that stands for "pay without a gap". The scheme
you choose is the most important setting after the predictors. The same data gives different explained and
unexplained shares under different schemes, and all of them are correct answers to different questions.

| Scheme | \\( \beta^\ast  \\) is | Use it when |
|---|---|---|
| `GroupB` | the reference group's own coefficients | you want to price the compared group as if paid like the reference group |
| `GroupA` | the compared group's own coefficients | you want the reverse question |
| `Pooled` | one regression on both groups **with** a group indicator | you want a neutral line and the indicator to carry the unexplained gap (Jann 2008 `pooled`) |
| `PooledNoIndicator` | one regression on both groups **without** an indicator | you follow Neumark 1988 (Stata `omega`) |
| `Weighted` | the group-size-weighted average of the two groups' coefficients | you follow Cotton 1988 |

`GroupB` is the default when you do not call `reference_coefficients`. Version 0.2.2 defaulted to `GroupA`.
`Cotton` is accepted as another name for `Weighted`. `Neumark` is a deprecated name that computes `Pooled`, not
Neumark's estimator; use `PooledNoIndicator` for that.

```rust,ignore
let results = OaxacaBuilder::new(df, "wage", "gender", "F")
    .predictors(["education", "experience"])
    .reference_coefficients(ReferenceCoefficients::Pooled)
    .run()?;
```

On the command line the option is `--ref-coeffs` with `group-a`, `group-b`, `pooled`, `pooled-no-indicator` or
`weighted`.

The engine, the WASM package and the MCP server have no default. `reference_coefficients` is required and must be
one of `GroupA`, `GroupB`, `Pooled`, `PooledNoIndicator`, `Weighted`, spelled exactly. A missing or misspelt value
is refused with `UNKNOWN_REFERENCE_COEFFICIENTS`, so a result never silently uses a scheme you did not choose.

Which one to report is a modelling decision. State it with the result. The [method page](../method/reference-coefficients.md)
gives the formulas and the packages each scheme is checked against.
