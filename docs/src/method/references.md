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
| Pooled coefficients with a group indicator | [Jann 2008](#jann-2008); Elder, Goddeeris and Haider (IZA discussion paper 4159) |
| Pooled coefficients without an indicator | [Neumark 1988](#neumark-1988) |
| Group-size-weighted coefficients | [Cotton 1988](#cotton-1988) |
| Categorical normalisation | [Yun 2005](#yun-2005); [Gardeazabal and Ugidos 2004](#gardeazabal-ugidos-2004) |
| RIF regression and decomposition | [Firpo, Fortin and Lemieux 2009](#firpo-fortin-lemieux-2009) |
| Normalised difference of a predictor | Imbens and Rubin 2015 |
| Selection correction | [Heckman 1979](#heckman-1979) |
| Two-fold decomposition | [Oaxaca 1973](#oaxaca-1973); [Blinder 1973](#blinder-1973) |
| Reweighting (experimental) | DiNardo, Fortin and Lemieux 1996 |

## Publications

Bibliographic details from the authors' publications.

<a id="blinder-1973"></a>**Blinder, A. S.** (1973). Wage discrimination: reduced form and structural estimates.
*Journal of Human Resources* 8(4): 436-455.

<a id="cotton-1988"></a>**Cotton, J.** (1988). On the decomposition of wage differentials. *Review of Economics and
Statistics* 70(2): 236-243.

<a id="firpo-fortin-lemieux-2009"></a>**Firpo, S., Fortin, N. M. and Lemieux, T.** (2009). Unconditional quantile
regressions. *Econometrica* 77(3): 953-973.

<a id="gardeazabal-ugidos-2004"></a>**Gardeazabal, J. and Ugidos, A.** (2004). More on identification in detailed wage
decompositions. *Review of Economics and Statistics* 86(4): 1034-1036.

<a id="heckman-1979"></a>**Heckman, J. J.** (1979). Sample selection bias as a specification error. *Econometrica*
47(1): 153-161.

<a id="jann-2008"></a>**Jann, B.** (2008). The Blinder-Oaxaca decomposition for linear regression models. *Stata
Journal* 8(4): 453-479.

<a id="neumark-1988"></a>**Neumark, D.** (1988). Employers' discriminatory behavior and the estimation of wage
discrimination. *Journal of Human Resources* 23(3): 279-295.

<a id="oaxaca-1973"></a>**Oaxaca, R.** (1973). Male-female wage differentials in urban labor markets. *International
Economic Review* 14(3): 693-709.

<a id="yun-2005"></a>**Yun, M.-S.** (2005). A simple solution to the identification problem in detailed wage
decompositions. *Economic Inquiry* 43(4): 766-772.
