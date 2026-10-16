# meridian-mcp

The MCP server of [oaxaca-blinder-rs](https://github.com/david-deji/oaxaca-blinder-rs): the pay-equity engine as tools
for an AI assistant (decomposition, remedy costing, defensibility, efficient frontier). It is not published on
crates.io. Download a binary from the
[releases page](https://github.com/david-deji/oaxaca-blinder-rs/releases) or build it from a clone:

```bash
cargo build --release -p meridian-mcp
```

Run it as `meridian-mcp` (standard input and output) and point an MCP client at that command, or serve HTTP with
`MCP_API_KEY=... meridian-mcp --port 8084 --transport sse` (the key is required; the server listens on `127.0.0.1`
only).

The tools, a client configuration and a sample call are in the
[documentation](https://david-deji.github.io/oaxaca-blinder-rs/latest/reference/mcp.html).
