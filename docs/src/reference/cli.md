# Command line

`oaxaca-cli` runs a decomposition on a CSV file and prints the result. Install it with
`cargo install oaxaca_blinder`.

## Common runs

The options go straight after the command name. There is no need to type `run`:

```bash
# Mean decomposition
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --categorical sector

# R-style formula
oaxaca-cli --data wage.csv --group gender --reference F \
    --formula "wage ~ education + experience + C(sector)"

# Sample weights: the kind is required
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --weights headcount --weights-kind frequency

# Percentiles
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --analysis-type quantile --quantiles 0.1,0.5,0.9

# Heckman selection correction
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience \
    --selection-outcome employed --selection-predictors age,education,marital_status

# Export the result
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --output-json result.json --output-markdown result.md
```

List values are separated by commas, without spaces: `--predictors education,experience`.

## HTML report

`report` writes a static HTML report. The options of the command line are required before the word `report` as well
as after it (see the usage line in the help below), so give them twice:

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    report --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --categorical sector --output report.html
```

## Defaults

| Option | Default |
|---|---|
| `--analysis-type` | `mean` |
| `--ref-coeffs` | `group-b` |
| `--normalization` | `population-share` |
| `--bootstrap-reps` | 100 |
| `--simulations` (Machado-Mata) | 200 |

The command line has no seed option; bootstrap draws use the library's default seed, so reruns match.
AKM and matching (`--analysis-type akm|match`) exist only in a build made with `--features experimental`; see
[Experimental methods](../user-guide/experimental.md).

## Full help

This is the output of the shipped binary, checked by a test.

```text
{{#include cli-help.txt}}
```
