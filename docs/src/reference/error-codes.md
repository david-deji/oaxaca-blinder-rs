# Error codes

Errors that a caller can act on start with an uppercase code, then `name=value` pairs, then a sentence on what to
give instead. The codes are stable. The text after them can change.

## Data and groups

| Code | When | Pairs |
|---|---|---|
| `REFERENCE_GROUP_ABSENT` | the reference value does not occur in the group column | `column`, `reference_group` |
| `TOO_MANY_GROUP_VALUES` | the group column holds a third value besides the reference | `column`, `reference_group`, the other values |
| `EMPTY_LEVEL_IN_GROUP` | a categorical level is absent from one of the two groups | `column`, `level`, `missing_from_group` |
| `INSUFFICIENT_RESIDUAL_DF` | a fitted group has no spare degrees of freedom | `group`, `rows`, `model_columns`, `residual_df` |

Group values are compared exactly: `"F "`, `" "` and `"f"` are values of their own.

## Scheme and weights

| Code | When | Pairs |
|---|---|---|
| `UNKNOWN_REFERENCE_COEFFICIENTS` | `reference_coefficients` is absent or is not one of the five names | the value given |
| `WEIGHTS_KIND_REQUIRED` | a weights column is named without a kind | `column` |
| `UNKNOWN_WEIGHTS_KIND` | the kind is not `frequency` or `relative` | the value given |
| `INVALID_WEIGHT` | a weight the kind cannot take: not finite, negative, or fractional under `frequency` | `column`, `row`, `value` |
| `UNSUPPORTED_COMBINATION` | a Heckman selection model with `Pooled`, `PooledNoIndicator` or the deprecated `Neumark` | `option`, `with`, the schemes that work |

## Remedy tools (engine and MCP server)

| Code | When |
|---|---|
| `INVALID_BUDGET` | `budget` or `max_budget` is negative or not finite |
| `INVALID_TARGET_GAP` | `target_gap` is not finite |
| `TARGET_GAP_WITH_REFERENCE_RAISES` | `target_gap` is combined with `adjust_both_groups` |
| `INVALID_MIN_GAP_PCT` | `min_gap_pct` is negative or not finite |
| `INVALID_CONFIDENCE_LEVEL` | `confidence_level` is outside 0.50 to 0.999, or not a fraction (`95` instead of `0.95`) |
| `INVALID_STEPS` | `steps` is 0 in the frontier tool |
| `UNKNOWN_TARGET` | `target` is not `Reference` or `Pooled` (MCP server) |
| `UNKNOWN_STRATEGY` | `strategy` is not `Greedy` or `Equitable` (MCP server) |
| `UNKNOWN_RANGE_TARGET` | `range_target` is not `Midpoint`, `LowerBound` or `UpperBound` (MCP server) |

Enumerated values are exact and case-sensitive.

## In Rust

The library returns `OaxacaError`. The variants carrying these codes print them as the first word of their message.
`OaxacaError` is `#[non_exhaustive]`: a `match` on it from another crate needs a wildcard arm.
