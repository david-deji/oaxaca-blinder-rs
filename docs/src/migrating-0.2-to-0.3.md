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

(In 0.2.2 the setter's documentation said `GroupB` while the code used `GroupA`; the 0.2.2 command line already
defaulted to `group-b`.) A caller who relied on the old reference default now asks for it:

```rust,ignore
builder.reference_coefficients(ReferenceCoefficients::GroupA);
```

### Categorical normalisation

`normalize(..)` and `normalize_all_categoricals()` now express each level against the population-share average of the
levels. In 0.2.2 they used the simple average (equal share), so the per-level rows of a categorical driver change.
Aggregates do not. To keep the 0.2.2 rows, choose the convention first:

```rust,ignore
builder
    .normalization_convention(NormalizationConvention::EqualShare)
    .normalize_all_categoricals();
```

### An empty slice

`.categorical_predictors(&[])` no longer compiles, because the argument type is generic and an empty slice names no
element type. Leave the call out, or write `.categorical_predictors(Vec::<&str>::new())`.

### Match arms

`OaxacaError`, `ReferenceCoefficients`, `WeightsKind`, `NormalizationConvention` and `ExclusionReason` are
`#[non_exhaustive]`. A `match` on one of them from another crate needs a wildcard arm:

```rust,ignore
match err {
    OaxacaError::ColumnNotFound(name) => { /* ... */ }
    _ => { /* any other error */ }
}
```

### Results

`OaxacaResults::explained()` and `unexplained()` return an `Option`, because a run can have no such component:

```rust,ignore
// 0.2.2
let share = results.explained().estimate;
// 0.3
let share = results.explained().expect("a mean decomposition has an explained part").estimate;
```

`OaxacaResults` has a public field `run_metadata` (seed, requested and discarded bootstrap replicates, normalisation,
reference scheme). Code that builds the struct with a literal, or destructures it without `..`, needs the field or
`..`. Read it through `results.run_metadata`.

The intercept row of the detailed tables is named `oaxaca_blinder::INTERCEPT_NAME` (`"__ob_intercept__"`), not
`"intercept"`. Filter on the constant. The printed tables (`summary()`, the command line, the `report`) show the row
as `(intercept)` (`oaxaca_blinder::INTERCEPT_LABEL`); the results and the JSON keep the token.

### Data that is now refused

| Input | 0.2.2 | 0.3 |
|---|---|---|
| A group column with a third value | the first other value was used and the rest ignored | `TOO_MANY_GROUP_VALUES`; recode the column to two values |
| A regression with as many columns as rows | no check | `InsufficientData` |
| A null outcome in the AKM or quantile path | read as 0.0 | an error; drop or fill the null |

The new variants are listed in [Error codes](reference/error-codes.md).

### Reference schemes

`ReferenceCoefficients::PooledNoIndicator` is the pooled regression without a group indicator (Neumark's estimator,
Stata `omega`). `ReferenceCoefficients::Neumark` is deprecated and still computes `Pooled`, so its numbers do not
change; switch to `PooledNoIndicator` only if Neumark's estimator is what you meant, and expect a different number.

### Builders

`OaxacaBuilder` no longer implements `Clone` or `Debug`. Build a new builder for each run, as the command line and
the engine do.

### Experimental methods

AKM, DFL, JMP and matching moved behind the cargo feature `experimental`, off by default:

```toml
oaxaca_blinder = { version = "0.3", features = ["experimental"] }
```

See [Experimental methods](user-guide/experimental.md). Two signatures changed in the same modules:

```rust,ignore
// AkmBuilder: methods take &mut self, run takes &self
// 0.2.2
let result = AkmBuilder::new(df, "wage", "worker", "firm").controls(&["tenure"]).run()?;
// 0.3: a single expression still works; a builder kept in a variable is mutable
let mut builder = AkmBuilder::new(df, "wage", "worker", "firm");
builder.controls(["tenure"]);
let result = builder.run()?;

// MatchingEngine: the distance is a flag, and the DistanceMetric trait is gone
// 0.2.2
let weights = engine.match_nearest_neighbor(1, &EuclideanDistance)?;
// 0.3
let weights = engine.match_nearest_neighbor(1, false)?;   // true for Mahalanobis
```

### Weights

A weights column needs a stated kind. `weights_kind(WeightsKind::Frequency)` or `WeightsKind::Relative` is required
with `weights(..)`. See [Weights](user-guide/weights.md).

### Deprecated

`OaxacaResults::optimize_budget` is deprecated. The remedy the Meridian product runs is in the engine.
`ReferenceCoefficients::Neumark` computes `Pooled`, not Neumark's estimator; use `PooledNoIndicator` for that.

## Command line

- `--bootstrap-reps` defaults to 100. Below 41 the command prints a warning and the 95% interval is `NaN`.
- `--simulations` (default 1000 before, 200 now) is ignored; the quantile analysis is RIF regression.
- The intercept row is printed as `(intercept)` in the tables and the `report`, and is named `__ob_intercept__` in
  `--output-json`.
- The console summary and the `report` label the groups "Compared group (A)" and "Reference group (B)".
- `--weights` needs `--weights-kind frequency|relative`.
- `--normalization` is new. The default is population share; `none` returns the raw coding of 0.2.
- AKM and matching need `cargo install oaxaca_blinder --features experimental`; the default build refuses
  `--analysis-type akm|match` and says so.
- `--outcome` is not needed with `--formula`.

## Python

There is no Python binding. It has not been compiled since it was switched off, and the `python` and `pyo3` features
are gone from the manifest.

## Licence

The crates are licensed `MIT OR Apache-2.0`. 0.2 was `MIT`.
