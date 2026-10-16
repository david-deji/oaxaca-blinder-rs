# Reproducing results in Stata

No Stata run is committed in this repository, and no Stata is available to CI. This page gives the option mapping
and a do-file to run by hand. **The do-file has not been run.** The Stata column of the table comes from the
documentation of `oaxaca` by Jann (2008) and from the option names this crate's own documentation cites. Compare
the group labels in Stata's header (`Group 1`, `Group 2`) with this crate's compared group (A) and reference group
(B) before comparing signs.

## Option mapping

| What | This crate | R `oaxaca` | R `ddecompose` | Stata `oaxaca` |
|---|---|---|---|---|
| Reference group's own coefficients | `GroupB`, `--ref-coeffs group-b` | `group.weights = 0` | `reference_0 = TRUE` | `weights(...)` set to the reference group's coefficients |
| Compared group's own coefficients | `GroupA`, `--ref-coeffs group-a` | `group.weights = 1` | `reference_0 = FALSE` | `weights(...)` set to the compared group's coefficients |
| Pooled with a group indicator | `Pooled`, `--ref-coeffs pooled` | `group.weights = -2` | none | `pooled` |
| Pooled without an indicator | `PooledNoIndicator`, `--ref-coeffs pooled-no-indicator` | `group.weights = -1` | none | `omega` |
| Group-share weighted | `Weighted`, `--ref-coeffs weighted` | `group.weights =` the compared group's share | none | `weights(...)` set to the two group shares |
| Three-fold | `OaxacaResults::three_fold()`; `three_fold` in the MCP tool | `$threefold` | none | `threefold` |
| Equal-share normalisation of a categorical | `--normalization equal-share` | third formula part | `normalize_factors = TRUE` | `categorical(varlist)` |
| Bootstrap standard errors | `--bootstrap-reps 100` | `R = 100` | `bootstrap = TRUE` | `vce(bootstrap, reps(100))` |
| Frequency weights | `--weights w --weights-kind frequency` | `weights` | `weights` | `[fweight = w]` |
| Detailed contributions | always reported | `$twofold$variables` | `decomposition_terms` | reported by default |

Stata prints one table per choice of reference. This crate runs one choice per call, so run the crate once for each
Stata choice you want to compare.

## A do-file

Save `wage.csv` from [Start here](../start-here.md) next to the file. It has a `gender` column with `F` and `M`.

```stata
* Not run. See the note at the top of this page.
import delimited "wage.csv", clear
generate byte female = (gender == "F")

* Pooled with a group indicator; compare with `oaxaca-cli --ref-coeffs pooled`
oaxaca wage education experience, by(female) pooled

* Pooled without an indicator; compare with `--ref-coeffs pooled-no-indicator`
oaxaca wage education experience, by(female) omega

* Standard errors from a bootstrap
oaxaca wage education experience, by(female) pooled vce(bootstrap, reps(100) seed(1))
```

Standard errors will differ from this crate's even with the same number of replicates, because the two programs
draw different resamples. Compare point estimates. The R recipe in [Reproducing results in R](r.md) is the check
that has been run.
