# Costing a remedy

This page applies to the engine, the WASM package and the MCP server. The library and the command line do not cost
remedies.

After a decomposition shows an unexplained gap, the next question is what closing it would cost. The engine answers
it as a scenario:

1. It fits a **pay line** from the reference group's pay: what an employee with these characteristics is paid.
2. It raises each compared employee who is paid below the line up to it, never above, within a budget.
3. It reports the cost, who is still below the line, and what the raises do to the group's gap.

The gap in a remedy result is the compared group's figure minus the line, so it is negative while the group is
paid below the line and rises as money is paid.

## Arguments that shape a remedy

| Argument | Meaning |
|---|---|
| `budget` | The most the remedy may spend in total. `0` means no cap. |
| `target_gap` | The gap to reach. The engine derives the cheapest budget that reaches it. |
| `strategy` | `Greedy` pays the largest shortfalls first. `Equitable` pays everyone the same share of their own shortfall. |
| `target` | `Reference` (the reference group's own line) or `Pooled` (the pooled line). |
| `range_target` | `Midpoint` (default), `LowerBound` or `UpperBound` of each employee's fair range. |
| `min_gap_pct` | Smallest shortfall worth paying, as a fraction of current pay. |
| `adjust_both_groups` | Also raise reference employees who are below the line. |

Enumerated values are exact and case-sensitive. The MCP server refuses an unknown value by name
(`UNKNOWN_TARGET`, `UNKNOWN_STRATEGY`, `UNKNOWN_RANGE_TARGET`).

`check_defensibility` takes a proposed schedule of raises and reports where each employee's new pay sits against the
range of comparable reference employees. `generate_efficient_frontier` traces how the remaining gap and its
significance change as the budget grows.

The full argument list and result fields are in [MCP server](../reference/mcp.md) and
[Diagnostics and warnings](../reference/diagnostics.md).

## What a remedy is not

A remedy result prices a set of raises. It does not decide who should be paid, and it does not say whether a
schedule meets any legal requirement. Read `warnings[]` and each row's `extrapolated` flag before relying on a
figure.
