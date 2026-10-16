# Reproducing results in R

Every decomposition this crate reports can be reproduced in R with the `oaxaca` and `ddecompose` packages. This page
runs the sample file from [Start here](../start-here.md) both ways and lists how the crate is checked in
`verification/`.

## The sample, five schemes

The file is [`sample/wage.csv`](../sample/wage.csv): 16 rows for group `F` (the reference group) and 16 for `M`.

```r
library(oaxaca)

d <- read.csv("wage.csv")
d$female <- as.numeric(d$gender == "F")   # R oaxaca wants 1 for the reference group

o <- oaxaca(wage ~ education + experience | female, data = d, R = NULL)
o$twofold$overall
```

`o$twofold$overall` has one row per `group.weight`. They line up with this crate's schemes as follows. The estimates
are from R 4.6.1 with `oaxaca` 0.1.5; the last two columns are what `oaxaca-cli --ref-coeffs ...` prints for the
same file.

| `group.weight` | This crate | R explained | R unexplained | Crate explained | Crate unexplained |
|---|---|---|---|---|---|
| 0 | `GroupB` | 1943.891 | 6031.109 | 1943.8905 | 6031.1095 |
| 1 | `GroupA` | 2360.853 | 5614.147 | 2360.8530 | 5614.1470 |
| the compared group's share of rows (0.5 here) | `Weighted` | 2152.372 | 5822.628 | 2152.3718 | 5822.6282 |
| -1 | `PooledNoIndicator` | 2409.076 | 5565.924 | 2409.0757 | 5565.9243 |
| -2 | `Pooled` | 2105.824 | 5869.176 | 2105.8236 | 5869.1764 |

A test in this repository (`docs_examples_test`) runs the five schemes on the sample and holds the crate's numbers
to the R values above.

The same split with `ddecompose`, for `GroupB` and `GroupA`:

```r
library(ddecompose)

grp <- factor(d$gender, levels = c("F", "M"))   # the first level is group 0
dd <- ob_decompose(wage ~ education + experience, data = d, group = grp,
                   reference_0 = TRUE,            # TRUE = GroupB, FALSE = GroupA
                   reweighting = FALSE, normalize_factors = FALSE, bootstrap = FALSE)
dd$ob_decompose$decomposition_terms
```

`Composition_effect` is the explained part and `Structure_effect` the unexplained part. With `reference_0 = TRUE`
they are 1943.891 and 6031.109 on the sample, and with `FALSE` 2360.853 and 5614.147.

## A percentile

```r
dq <- ob_decompose(wage ~ education + experience, data = d, group = grp,
                   reference_0 = TRUE, reweighting = FALSE, normalize_factors = FALSE,
                   rifreg_statistic = "quantiles", rifreg_probs = 0.5, bootstrap = FALSE)
dq[["quantile_0.5"]]$decomposition_terms
```

On the sample this gives 1498.398 explained and 8551.602 unexplained. `oaxaca-cli ... --ref-coeffs group-b
--analysis-type quantile --quantiles 0.5` prints 1498.3607 and 8551.6393. The two differ in the fifth significant
figure. The repository's trust tests pin the agreement they measured on a larger file.

## Categorical drivers

`ddecompose(normalize_factors = TRUE)` and R `oaxaca` with a third formula part print the equal-share
normalisation. Reproduce them with `--normalization equal-share`. The default population-share convention has no
package implementation. It is checked against a refit by hand in base R. See
[Categorical normalisation](../method/normalization.md).

## What `verification/` checks

Each generator computes its expected values with software that shares no code with the engine, writes them to a
committed fixture, and records the SHA-256 of the generator and of every input in that fixture. The Rust tests read
the fixture offline and refuse it when a recorded hash no longer matches the files. `cargo test` never runs R.

| Generator | Oracle | What it covers | Tolerance |
|---|---|---|---|
| `gen_parity_golden.py` | statsmodels OLS | regression fit | 1e-6 |
| `gen_trust_goldens.R` | R `lm`, `quantreg`, `ddecompose` | OLS, quantile regression, decomposition arithmetic, bootstrap standard errors, RIF quantile detail | 1e-6 relative; bootstrap standard errors 1e-3 relative |
| `gen_norm_goldens.R` | R `oaxaca`, `ddecompose`, base `lm` refit | normalisation, both conventions, five schemes | 1e-10 package, 1e-9 refit |
| `gen_diag_goldens.R` | base R, `ddecompose`, `Hmisc` | support block, normalised difference, prediction intervals, percentile gap, weighted quantiles | 1e-9 to 1e-12 |
| `gen_pooled_target_goldens.R` | base R `lm`, R `oaxaca` weight -2 | the pooled pay line, its bounds and extrapolation flags | 1e-9 |
| `gen_remedy_goldens.R` | base R `lm`, `predict.lm`, `uniroot` | every remedy figure on 62 cases, refitting the line on each schedule | 1e-6 dollars |
| `gen_remedy_bruteforce.py` | exhaustive enumeration on a 250-dollar grid | the cost of reaching a gap | exact on the grid |

Never edit a generator, even a comment: its hash is recorded in the fixture and the tests go red until the fixture is
regenerated in the original R or Python environment. To regenerate you need R with `oaxaca` 0.1.5, `ddecompose`
1.0.0, `quantreg`, `Hmisc`, `jsonlite` and `digest`, and Python with `statsmodels` 0.14 or later. Each generator's
header gives the exact command.

## What is not checked against a package

The Heckman correction and the [experimental methods](../user-guide/experimental.md) have no R or Stata oracle. The
remedy is not a published method; it is checked against base-R arithmetic and brute-force enumeration.
