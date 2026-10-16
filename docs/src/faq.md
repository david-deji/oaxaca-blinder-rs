# FAQ

## Why does the unexplained part change when I change the reference scheme?

The unexplained part is defined relative to a pay structure \\( \beta^\ast  \\) that stands for "no gap". Each scheme names
a different one. See [Choosing a reference scheme](user-guide/reference-schemes.md). Report the scheme with the number.

## Why do my per-level rows differ from Stata or R?

Stata `categorical()`, R `oaxaca` and `ddecompose(normalize_factors = TRUE)` weight every level of a categorical
equally. The command line and the engine default to weighting levels by their share of the data. Run
`--normalization equal-share` to compare. See [Categorical drivers](user-guide/categorical-drivers.md).

## Why do per-level rows change when I rename a category?

They do not, once the categorical is normalised. Raw treatment coding measures every level against whichever level
sorts first, so renaming that level changes every row. The aggregates never change.

## Why is a confidence interval `NaN`?

A percentile interval needs at least 41 bootstrap replicates. With fewer, the interval is `NaN`; the command line
says so on standard error. Raise
`--bootstrap-reps` or `bootstrap_reps(..)`. See [Bootstrap and seeds](user-guide/bootstrap-and-seeds.md).

## Why is a p-value a multiple of 0.01, and why is it `0.0000`?

The p-value is a count out of the bootstrap replicates, so with 100 replicates it moves in steps of 0.01. `0.0000`
means no replicate disagreed with the estimate, not that the probability is zero. Use more replicates for a finer
value.

## Why does the command line warn that replications failed?

A resample can lack a categorical level that a group has few rows of, and then the model cannot be fitted. The
replicate is discarded and counted. Merge rare levels, or use more rows. The count is in `run_metadata`.

## Why do two runs give the same numbers?

The bootstrap seed is fixed. In the library, pass a different seed to change the draws, or `seed_from_entropy()` to draw one; the
result records the seed it used. The command line has no seed option. See [Bootstrap and seeds](user-guide/bootstrap-and-seeds.md).

## Why was a row left out?

A blank in any column the model uses removes the row. `analysed_rows()` lists the rows left out and why. See
[Preparing your data](user-guide/data-preparation.md).

## What if one group is small?

The model needs more rows in each group than it has columns. A group with few spare degrees of freedom gets the
warning `few_residual_df`, and a group with none is refused. Pay estimated for people unlike the reference group is
an extrapolation: the engine marks those rows `extrapolated`. See [Diagnostics and warnings](reference/diagnostics.md).

## Does it work with more than two groups?

No. The decomposition compares two groups. Filter to the two you want, or run it once for each comparison.

## Is there a Python package?

No. Run `oaxaca-cli` from Python and read the JSON written by `--output-json`.

## How do I cite it?

The repository has a `CITATION.cff`; GitHub shows a "Cite this repository" button from it.
