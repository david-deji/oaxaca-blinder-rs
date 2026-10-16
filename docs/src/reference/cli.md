# Command line

`oaxaca-cli` runs a decomposition on a CSV file and prints the result. Install it with
`cargo install oaxaca_blinder`.

## Common runs

The options go straight after the command name. Do not type `run`; it is the command that runs when none is named:

```bash
# Mean decomposition
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --categorical sector

# R-style formula: the formula names the outcome, so --outcome is not needed
oaxaca-cli --data wage.csv --group gender --reference F \
    --formula "wage ~ education + experience + C(sector)"

# Sample weights: the kind is required
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --weights headcount --weights-kind frequency

# Percentiles
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --analysis-type quantile --quantiles 0.1,0.5,0.9

# Heckman selection correction (needs a file with the selection columns; the sample file has none)
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience \
    --selection-outcome employed --selection-predictors age,education,marital_status

# Export the result
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --output-json result.json --output-markdown result.md
```

List values are separated by commas, without spaces: `--predictors education,experience`.

The sample file has 16 rows per group, so a three-level `sector` can leave a level out of a resample. The run then
prints a warning that some bootstrap replicates failed and were discarded; the analysis uses the others. Use more rows
per level for real work.

A whole-number outcome or weights column (`45600`) is read as a decimal. With fewer than 41 `--bootstrap-reps` the
command prints a warning on standard error and the 95% interval prints as `NaN`.

## HTML report

`report` writes a static HTML report. It reads `--data`, `--outcome`, `--group`, `--reference`, `--predictors`,
`--categorical` and `--output`. Give the options after the word `report`, or before it:

```bash
oaxaca-cli report --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --categorical sector --output report.html
```

## Defaults

| Option | Default |
|---|---|
| `--analysis-type` | `mean` |
| `--ref-coeffs` | `group-b` |
| `--normalization` | `population-share` |
| `--bootstrap-reps` | 100 |
| `--simulations` | 200, ignored |

The command line has no seed option; bootstrap draws use the library's default seed, so reruns match.
AKM and matching (`--analysis-type akm|match`) exist only in a build made with `--features experimental`; see
[Experimental methods](../user-guide/experimental.md).

## Full help

This is the output of the shipped binary, checked by a test.

```text
{{#include cli-help.txt}}
```
