# oaxaca_blinder

[![crates.io](https://img.shields.io/crates/v/oaxaca_blinder.svg)](https://crates.io/crates/oaxaca_blinder)
[![docs.rs](https://docs.rs/oaxaca_blinder/badge.svg)](https://docs.rs/oaxaca_blinder)

Oaxaca-Blinder decomposition of the gap between two groups, usually a pay gap, on Polars `DataFrame`s. It splits the
gap into the part that differences in measured characteristics explain and the part they do not, and does the same at
a chosen percentile (a RIF quantile decomposition). Standard errors and confidence intervals come from a seeded
bootstrap. It also installs the command line `oaxaca-cli`.

Documentation: <https://david-deji.github.io/oaxaca-blinder-rs/>. Source and issues:
<https://github.com/david-deji/oaxaca-blinder-rs>.

## Status

Version 0.3. The interface can change between minor versions and so can numbers; the
[changelog](https://github.com/david-deji/oaxaca-blinder-rs/blob/main/CHANGELOG.md) lists every change a published
result would show.

## Install

```bash
cargo add oaxaca_blinder        # the library
cargo install oaxaca_blinder    # the command line, installed as oaxaca-cli
```

The crate needs Rust 1.88 or newer.

## A first decomposition

```rust
use oaxaca_blinder::{OaxacaBuilder, ReferenceCoefficients};
use polars::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let df = df!(
        "wage" => &[
            45600.0, 34900.0, 55600.0, 44800.0, 46600.0, 41100.0, 47200.0, 61600.0, 56300.0, 44800.0,
            40600.0, 50500.0, 55400.0, 60000.0, 49400.0, 44400.0, 57300.0, 55700.0, 46600.0, 58100.0,
            47000.0, 58200.0, 62400.0, 60500.0, 56600.0, 54500.0, 51800.0, 65700.0, 63400.0, 55000.0,
            53700.0, 59900.0,
        ],
        "education" => &[
            16.0, 12.0, 16.0, 18.0, 16.0, 12.0, 18.0, 20.0, 18.0, 14.0, 14.0, 18.0, 18.0, 18.0, 16.0,
            12.0, 14.0, 16.0, 14.0, 16.0, 16.0, 17.0, 16.0, 18.0, 16.0, 12.0, 13.0, 20.0, 20.0, 19.0,
            17.0, 17.0,
        ],
        "experience" => &[
            5.0, 3.0, 19.0, 7.0, 14.0, 18.0, 4.0, 19.0, 13.0, 2.0, 10.0, 4.0, 22.0, 19.0, 4.0, 19.0,
            20.0, 17.0, 10.0, 13.0, 5.0, 13.0, 24.0, 13.0, 21.0, 18.0, 12.0, 12.0, 17.0, 4.0, 10.0,
            17.0,
        ],
        "gender" => &[
            "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F", "F",
            "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M", "M",
        ]
    )?;

    // "F" is the reference group. The default bootstrap (100 replicates, fixed seed) is used,
    // so a second run prints the same numbers.
    let results = OaxacaBuilder::new(df, "wage", "gender", "F")
        .predictors(["education", "experience"])
        .reference_coefficients(ReferenceCoefficients::Pooled)
        .run()?;

    println!("Gap (M minus F): {:.0}", results.total_gap());
    for part in results.two_fold().aggregate() {
        println!(
            "{:<12} {:>8.0}  (95% CI {:.0} to {:.0})",
            part.name(),
            part.estimate(),
            part.ci_lower(),
            part.ci_upper()
        );
    }
    Ok(())
}
```

It prints:

```text
Gap (M minus F): 7975
explained        2106  (95% CI -1303 to 5347)
unexplained      5869  (95% CI 3056 to 8084)
```

The gap is `M` minus `F`. Group `F` is the reference group.

On the command line, numeric columns need a decimal point and a weights column needs its kind:

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --ref-coeffs pooled

oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --weights headcount --weights-kind frequency
```

## Defaults you will hit

| Setting | Default |
|---|---|
| Bootstrap replicates | 100 (`DEFAULT_BOOTSTRAP_REPS`). A percentile interval needs at least 41. |
| Seed | fixed (`DEFAULT_SEED`), so reruns give the same numbers |
| Reference coefficients | `GroupB`, the reference group's own. 0.2.2 used `GroupA`. |
| Categorical levels | raw unless you call `normalize_all_categoricals()` |

## Cargo features

| Feature | Default | What it adds |
|---|---|---|
| `display` | yes | the output methods of `OaxacaResults`: `summary`, `format_summary`, `to_markdown`, `to_latex`, `to_json` |
| `experimental` | no | AKM, DFL, JMP and matching. They have no external oracle and may change in any release. |

## How the numbers are checked

The mean and quantile decompositions, the reference schemes, normalisation and weights are compared with R
(`oaxaca` 0.1.5, `ddecompose` 1.0.0, `lm`, `quantreg`, `Hmisc`) and with Python `statsmodels`. The generators and
tolerances are in the repository's `verification/` directory. The Heckman correction is tested on simulated data and
has no R or Stata run to compare with.

## What it is not

The output is statistical evidence about a gap in your data. It is not a finding of legal compliance.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you state otherwise, a contribution you submit is licensed on the same terms, without
additional conditions.
