# Introduction

`oaxaca_blinder` splits the gap in an outcome between two groups, usually the pay gap between women and men, into
two parts:

- the **explained** part, which comes from differences in measured characteristics such as education, experience or
  sector;
- the **unexplained** part, which comes from differences in what those characteristics are paid.

This is the Oaxaca-Blinder decomposition. The same method runs on a chosen percentile of the pay distribution
(a RIF decomposition, see [RIF quantile decomposition](method/rif-quantile.md)). Standard errors and confidence
intervals come from a bootstrap.

It is written for economists, data scientists and HR or pay-equity analysts who need a decomposition they can
script, rerun and compare with R or Stata.

## Four ways in

| Way in | What it is | Where it ships |
|---|---|---|
| Library | The `oaxaca_blinder` crate. Takes a Polars `DataFrame`. | crates.io, docs on [docs.rs](https://docs.rs/oaxaca_blinder) |
| Command line | `oaxaca-cli`. Takes a CSV file. | `cargo install oaxaca_blinder` |
| MCP server | `meridian-mcp`. Exposes the engine as tools to an AI assistant. | Release binaries on GitHub |
| WASM engine | `pay-equity-engine`, the engine behind the Meridian pay-equity product. | Built from this repository |

The engine and the MCP server add a pay-gap remedy on top of the decomposition: who is raised, to which pay line,
for how much, and what that does to the group's gap. See [Costing a remedy](user-guide/remedy.md).

## Status

The crate is at 0.3 and makes no 1.0 promise. The programming interface can change between minor versions, and so
can numbers: the [changelog](changelog.md) lists every change a published result would show. Moving from 0.2 to 0.3
is covered in [Migrating from 0.2 to 0.3](migrating-0.2-to-0.3.md).

The mean and RIF-quantile decompositions are checked against R. The methods marked
[experimental](user-guide/experimental.md) are not.

## What the output is

The output is statistical evidence about a gap in your data. It is not a legal finding of compliance or
non-compliance. Warnings in the result (`warnings[]`) are caveats about the data, not blocks.

## Python

There are no Python bindings. An earlier binding was never compiled into a release and has been removed. Use the
command line from Python with `subprocess`, or read the JSON the command line writes with `--output-json`.

## Where to go next

- [Start here](start-here.md) installs the tools and runs a first decomposition.
- The [user guide](user-guide/data-preparation.md) covers the choices every analysis makes.
- The [method pages](method/oaxaca-blinder.md) give the formulas and the packages each part is checked against.
- [Reproducing results](reproducing/r.md) shows how to get the same numbers in R and Stata.
