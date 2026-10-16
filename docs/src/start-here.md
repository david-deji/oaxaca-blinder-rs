# Start here

## Install

The library, from a Rust project:

```bash
cargo add oaxaca_blinder
```

The command line:

```bash
cargo install oaxaca_blinder
oaxaca-cli --help
```

The crate needs Rust 1.88 or newer.

The MCP server is not on crates.io. Download the archive for your system from the
[releases page](https://github.com/david-deji/oaxaca-blinder-rs/releases) or build it from a clone of the
repository:

```bash
cargo build --release -p meridian-mcp
```

## A first decomposition: library

The data below is inline, so the example runs as written. Group `F` is the reference group.

```rust
{{#include ../../oaxaca_blinder/examples/first_decomposition.rs}}
```

It prints:

```text
{{#include outputs/first_decomposition.txt}}
```

The gap is `M` minus `F`. About 2,100 of the 7,975 is explained by education and experience and the rest is
unexplained by them. With 16 rows per group the confidence interval of the explained part includes zero. A real
analysis needs real group sizes.

The example is the file `oaxaca_blinder/examples/first_decomposition.rs`. CI compiles it and a test runs it and
compares the output with the text above.

## A first decomposition: command line

[`sample/wage.csv`](sample/wage.csv) holds the same 32 rows and a `headcount` column. Numeric columns need a decimal point (`45600.0`, not
`45600`): an integer outcome column is refused.

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --ref-coeffs pooled
```

```text
{{#include outputs/cli_run.txt}}
```

With weights, `--weights-kind` is required next to `--weights`:

```bash
oaxaca-cli --data wage.csv --outcome wage --group gender --reference F \
    --predictors education,experience --weights headcount --weights-kind frequency
```

All options are listed in [Command line](reference/cli.md).

## A first decomposition: MCP server

The server speaks the Model Context Protocol over standard input and output by default:

```bash
meridian-mcp
```

Point your MCP client at that command. To serve HTTP instead, give a port and an API key. The server refuses to
start in HTTP mode without a key:

```bash
MCP_API_KEY=choose-a-long-random-value meridian-mcp --port 8084 --transport sse
```

It listens on `127.0.0.1` only. The tools and their arguments are in [MCP server](reference/mcp.md).

## Defaults you will meet

| Setting | Default | Page |
|---|---|---|
| Bootstrap replicates | 100 | [Bootstrap and seeds](user-guide/bootstrap-and-seeds.md) |
| Smallest bootstrap with a percentile interval | 41 | [Bootstrap and seeds](user-guide/bootstrap-and-seeds.md) |
| Seed | fixed, so reruns match | [Bootstrap and seeds](user-guide/bootstrap-and-seeds.md) |
| Reference coefficients | `GroupB`, the reference group's own | [Choosing a reference scheme](user-guide/reference-schemes.md) |
| Categorical levels | Raw in the library, normalised in the command line and the engine | [Categorical drivers](user-guide/categorical-drivers.md) |
