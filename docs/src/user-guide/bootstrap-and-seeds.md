# Bootstrap and seeds

Standard errors, p-values and confidence intervals come from a bootstrap: the analysis is repeated on resamples of
the rows and the spread of the results is the uncertainty. The standard error is the standard deviation of the
replicates, the interval is their 2.5th and 97.5th percentiles, and the p-value is twice the smaller share of
replicates on either side of zero. The definitions are in
[Bootstrap inference](../method/oaxaca-blinder.md#bootstrap-inference).

## Replicates

The default is 100 replicates in the library (`DEFAULT_BOOTSTRAP_REPS`) and on the command line
(`--bootstrap-reps`). Both use the same number. Before 0.3 the library, the command line and the engine used
different defaults.

A percentile confidence interval needs at least **41** replicates. Below 41 the two ends of a 95% interval would sit on
the smallest and largest draws, which is the full range, so the interval is reported as `NaN` instead. Because a p-value is a count out of the
replicates, it is a multiple of 1 over the number of replicates: with 100 replicates the smallest non-zero p-value is
0.01, and `0.0000` means "no replicate disagreed", not "exactly zero".

More replicates give steadier intervals and take proportionally longer. The bootstrap runs on all cores.

```rust,ignore
builder.bootstrap_reps(500);
```

## Failed replicates

A resample can fail when a categorical level is absent from a resampled group. The failed replicate is discarded and
counted. The result's `run_metadata` records how many were requested and how many were discarded, and the command
line prints a warning. Few rows per level make this likely: merge rare levels.

## Seeds

The seed is fixed by default (`DEFAULT_SEED`), so two runs on the same data give the same numbers, on any number of
threads. Each replicate draws from its own stream derived from the seed, so the thread schedule cannot change a
result.

```rust,ignore
builder.seed(12345);          // choose a seed
builder.seed_from_entropy();  // draw one from the operating system; the result records it
```

`run_metadata.seed` holds the seed actually used, as a decimal string, so any run can be repeated with
`.seed(recorded_value)`. The numbers are identical for any thread count on one platform.

The engine and the MCP server take `bootstrap_reps` in the request and use 100 when it is absent. The command line
has no seed option and uses the default seed.
