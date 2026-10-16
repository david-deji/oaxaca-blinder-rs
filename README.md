# oaxaca-blinder-rs

[![crates.io](https://img.shields.io/crates/v/oaxaca_blinder.svg)](https://crates.io/crates/oaxaca_blinder)
[![docs.rs](https://docs.rs/oaxaca_blinder/badge.svg)](https://docs.rs/oaxaca_blinder)
[![CI](https://github.com/david-deji/oaxaca-blinder-rs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/david-deji/oaxaca-blinder-rs/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/rustc-1.88%2B-blue.svg)](#rust-version-and-licence)
[![licence](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue.svg)](#rust-version-and-licence)

Oaxaca-Blinder decomposition of the gap between two groups, usually a pay gap, into the part that differences in
measured characteristics explain and the part they do not. It also decomposes the gap at a chosen percentile (a RIF
quantile decomposition) and reports bootstrap standard errors and confidence intervals. It is written for economists,
data scientists and HR or pay-equity analysts.

There are four ways in:

| Way in | What it is | Where |
|---|---|---|
| Library | `oaxaca_blinder`, on Polars `DataFrame`s | [crates.io](https://crates.io/crates/oaxaca_blinder), [docs.rs](https://docs.rs/oaxaca_blinder) |
| Command line | `oaxaca-cli`, on CSV files | `cargo install oaxaca_blinder` |
| MCP server | `meridian-mcp`, the engine as tools for an AI assistant | release binaries |
| WASM engine | `pay-equity-engine`, which costs a pay-gap remedy; used by the Meridian product | built from this repository, not on crates.io |

The documentation site is at <https://david-deji.github.io/oaxaca-blinder-rs/>.

## Status

Version 0.3. The programming interface can change between minor versions and so can numbers; the
[changelog](CHANGELOG.md) lists every change a published result would show, and
[migrating from 0.2](https://david-deji.github.io/oaxaca-blinder-rs/migrating-0.2-to-0.3.html) covers the move to 0.3.

## Install

```bash
cargo add oaxaca_blinder        # the library
cargo install oaxaca_blinder    # the command line, installed as oaxaca-cli
```

The MCP server is run as `meridian-mcp` (standard input and output) or `MCP_API_KEY=... meridian-mcp --port 8084
--transport sse` (HTTP, key required). Download a binary from the
[releases page](https://github.com/david-deji/oaxaca-blinder-rs/releases) or build it with
`cargo build --release -p meridian-mcp`.

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

The gap is `M` minus `F`. The unexplained part is what education and experience do not account for.

The same on the command line, on [`docs/src/sample/wage.csv`](docs/src/sample/wage.csv). Numeric columns need a
decimal point, and a weights column needs its kind:

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --ref-coeffs pooled

oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --weights headcount --weights-kind frequency
```

## Defaults you will hit

| Setting | Default |
|---|---|
| Bootstrap replicates | 100, library and command line. A percentile interval needs at least 41. |
| Seed | fixed, so reruns give the same numbers |
| Reference coefficients | `GroupB`, the reference group's own. 0.2.2 used `GroupA`. The engine and MCP server have no default. |
| Categorical levels | raw in the library, population-share normalised in the command line and the engine |

## What is in it

| Feature | Status |
|---|---|
| Mean decomposition, two-fold and three-fold, five reference schemes | checked against R |
| RIF quantile decomposition | checked against R |
| Categorical normalisation, population-share and equal-share | checked against R |
| Sample weights, frequency and relative | checked against R |
| Bootstrap standard errors and percentile intervals | checked against R |
| Heckman selection correction | tested on simulated data, no R or Stata run |
| AKM, DFL, JMP, matching | experimental, behind the cargo feature `experimental` |
| Pay-gap remedy, defensibility, efficient frontier | engine and MCP server only |

## How the numbers are checked

The expected values come from software that shares no code with this one: R `lm`, `predict.lm`, `quantreg`, `Hmisc`,
`oaxaca` 0.1.5 and `ddecompose` 1.0.0, and Python `statsmodels`. The generators are in [`verification/`](verification);
each writes a committed fixture that records the hash of its generator, and the tests read the fixture offline. The
tolerances and the R and Stata equivalents of each option are in the
[documentation](https://david-deji.github.io/oaxaca-blinder-rs/reproducing/r.html).

## What it is not

The output is statistical evidence about a gap in your data. It is not a finding of legal compliance. Entries in
`warnings` are caveats, not blocks.

## Python

There is no Python package. Call `oaxaca-cli` from Python and read the JSON it writes with `--output-json`.

## Rust version and licence

The library needs Rust 1.88 or newer. Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you state otherwise, a contribution you submit is licensed on the same terms, without
additional conditions.

## Owner, citing, security, contributing

Owned by Telos Machina and maintained by David. Cite it with [`CITATION.cff`](CITATION.cff) (GitHub's "Cite this
repository" button reads it). Report a vulnerability as described in [`SECURITY.md`](SECURITY.md). See
[`CONTRIBUTING.md`](CONTRIBUTING.md) and the [code of conduct](CODE_OF_CONDUCT.md).
