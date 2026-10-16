# Migrating from 0.2 to 0.3

This page lists what a 0.2 user has to change, with before and after code. The [changelog](changelog.md) is the
complete list of changes, including the numbers a published result would show.

## Library

### Column names

Column-name arguments take anything that is `AsRef<str>` again. Code that compiled on 0.2.2 with `&["education"]`
compiles on 0.3.

```rust,ignore
OaxacaBuilder::new(df, "wage", "gender", "F")
    .predictors(&["education", "experience"])      // a slice of &str
    .categorical_predictors(vec!["sector".to_string()])  // a Vec<String>
    .run()?;
```

### Defaults

| Setting | 0.2.2 | 0.3 |
|---|---|---|
| Bootstrap replicates, library | 100 | 100 |
| Bootstrap replicates, `oaxaca-cli --bootstrap-reps` | 500 | 100 |
| Reference coefficients | `GroupA` | `GroupB` |

A caller who relied on the old reference default now asks for it:

```rust,ignore
builder.reference_coefficients(ReferenceCoefficients::GroupA);
```

### Match arms

`OaxacaError`, `ReferenceCoefficients`, `WeightsKind`, `NormalizationConvention` and `ExclusionReason` are
`#[non_exhaustive]`. A `match` on one of them from another crate needs a wildcard arm:

```rust,ignore
match err {
    OaxacaError::ColumnNotFound(name) => { /* ... */ }
    _ => { /* any other error */ }
}
```

### Experimental methods

AKM, DFL, JMP and matching moved behind the cargo feature `experimental`, off by default:

```toml
oaxaca_blinder = { version = "0.3", features = ["experimental"] }
```

See [Experimental methods](user-guide/experimental.md).

### Weights

A weights column needs a stated kind. `weights_kind(WeightsKind::Frequency)` or `WeightsKind::Relative` is required
with `weights(..)`. See [Weights](user-guide/weights.md).

### Deprecated

`OaxacaResults::optimize_budget` is deprecated. The remedy the Meridian product runs is in the engine.
`ReferenceCoefficients::Neumark` computes `Pooled`, not Neumark's estimator; use `PooledNoIndicator` for that.

## Command line

- `--bootstrap-reps` defaults to 100.
- `--weights` needs `--weights-kind frequency|relative`.
- `--normalization` is new. The default is population share; `none` returns the raw coding of 0.2.
- AKM and matching need `cargo install oaxaca_blinder --features experimental`.

## Python

There is no Python binding. It was never compiled into a release.

## Licence

The crates are licensed `MIT OR Apache-2.0`. 0.2 was `MIT`.
