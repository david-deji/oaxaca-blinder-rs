# Reference coefficients

The two-fold decomposition needs a coefficient vector \\( \beta^\ast  \\) that stands for pay without a gap
(see [Oaxaca-Blinder decomposition](oaxaca-blinder.md)). This library offers five. A is the compared group and B is
the reference group.

| Name | \\( \beta^\ast  \\) | Equals |
|---|---|---|
| `GroupA` | \\( \beta\_A \\) | the compared group's own coefficients |
| `GroupB` | \\( \beta\_B \\) | the reference group's own coefficients |
| `Pooled` | from one regression on both groups **with** a group indicator, whose coefficient is dropped | Jann 2008 `pooled`; Stata `oaxaca, pooled` |
| `PooledNoIndicator` | from one regression on both groups **without** an indicator | Neumark 1988; Stata `omega` |
| `Weighted` | \\( w \beta\_A + (1 - w) \beta\_B \\), with \\( w \\) the compared group's share of the rows | Cotton 1988 |

## Reading the schemes

With `GroupB`, the explained part prices the groups' different characteristics at the reference group's rewards, and
the unexplained part is what the compared group would gain from being rewarded like the reference group. `GroupA`
asks the reverse question.

With `Pooled`, the group indicator is in the regression, so the pooled coefficients are not tilted by the gap itself.
The indicator's coefficient is exactly the unexplained gap, which is why that coefficient is dropped from
\\( \beta^\ast  \\) and the remedy tools use this line as the neutral pay line.

`PooledNoIndicator` omits the indicator, so its pooled coefficients absorb part of the group difference. It is the
estimator Neumark proposed.

The deprecated name `ReferenceCoefficients::Neumark` computes `Pooled`, not Neumark's estimator.

## Defaults

The library defaults to `GroupB` (0.2.2 used `GroupA`). The command line defaults to `group-b`. The engine, the
WASM package and the MCP server have no default and refuse a request without one.

## How each scheme is checked

| Scheme | Compared with |
|---|---|
| `GroupA`, `GroupB`, `Weighted` | R `oaxaca` with `group.weights` of 1, 0 and the compared group's share: aggregate and detailed rows |
| `Pooled` | R `oaxaca` with the pooled-with-indicator weight (-2), and a base-R refit with an indicator: aggregate rows |
| `PooledNoIndicator` | R `oaxaca` with the pooled-without-indicator weight (-1): aggregate rows |

R `oaxaca` does not re-estimate the pooled coefficients for the detailed rows, so those are checked by the refit and
by identities (the parts add up to the gap). The generators are in `verification/`; see
[Reproducing results in R](../reproducing/r.md).
