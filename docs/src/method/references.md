# References

## Software the numbers are checked against

| Software | Version | Used for |
|---|---|---|
| R `oaxaca` | 0.1.5 | Two-fold schemes, pooled weights, equal-share normalisation |
| R `ddecompose` | 1.0.0 | Aggregate and detailed arithmetic, RIF quantile decomposition, `normalize_factors` |
| R `quantreg` | see generator header | Quantile-regression solver |
| R `Hmisc` | see generator header | Weighted quantile (`wtd.quantile`) |
| R base `lm`, `predict.lm`, `quantile` | R 4.1 or newer | Regression, prediction intervals, type-7 percentiles |
| Python `statsmodels` | 0.14 or newer | Ordinary least squares |

The generator scripts are in `verification/` and each states the exact versions it ran with.

## Methods named on these pages

The pages name each method by its authors and year, as the library's own documentation does.

| Method | Authors |
|---|---|
| Pooled coefficients with a group indicator | Jann 2008; Elder, Goddeeris and Haider (IZA discussion paper 4159) |
| Pooled coefficients without an indicator | Neumark 1988 |
| Group-size-weighted coefficients | Cotton 1988 |
| Categorical normalisation | Yun 2005; Gardeazabal and Ugidos 2004 |
| RIF regression and decomposition | Firpo, Fortin and Lemieux 2009 |
| Normalised difference of a predictor | Imbens and Rubin 2015 |
| Reweighting (experimental) | DiNardo, Fortin and Lemieux 1996 |
