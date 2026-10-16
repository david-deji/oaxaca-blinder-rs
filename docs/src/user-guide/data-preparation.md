# Preparing your data

## Shape of the table

One row per employee. The analysis needs:

- an **outcome** column, usually pay (`wage`);
- a **group** column with the two groups to compare (`gender`);
- **predictors**: numeric columns (`education`, `experience`) and, optionally, categorical columns (`sector`).

The group column must hold exactly two values. You name one as the **reference group** (`F` in the examples) and
every other value is the compared group. A third value is refused (`TOO_MANY_GROUP_VALUES`), even on a row that
would be left out for a blank elsewhere, so recode or filter first.

## Whole-number columns

A CSV column written as `45600` is read as an integer. The library and the command line both convert a whole-number
outcome, selection outcome or weights column to a decimal, and the numbers equal those of `45600.0`. Predictor columns
may be integers.

## Blank cells

A row with a blank in any column the model uses is left out of that analysis: the outcome, a numeric predictor, a
categorical predictor, the weight column, the selection columns, or the group. The row is not guessed at and not
filled in.

To see which rows were left out and why, ask the builder:

```rust,ignore
let accounting = builder.analysed_rows()?;
println!("{} of {} rows analysed", accounting.reference_rows.len() + accounting.target_rows.len(), accounting.total_rows);
for row in &accounting.excluded_rows {
    println!("row {} left out: blank in {:?}", row.index, row.columns);
}
```

`index` is the zero-based position among the data rows of the table you gave the builder. The engine and the MCP
server return the same accounting in `excluded_rows`, so a caller can tell which employees a result does not
cover.

## Categorical columns

List a text column as categorical and it is split into one 0/1 column per level, with one level dropped. Which level
is dropped does not change the totals. It changes how the per-level rows are read, which is why the command line and
the engine normalise them: see [Categorical drivers](categorical-drivers.md).

Every level must appear in both groups. A level present in one group and absent from the other makes the model
unsolvable and is refused with `EMPTY_LEVEL_IN_GROUP`, naming the column, the level and the group it is missing
from. Merge rare levels before you run.

## Group sizes

Each group needs more rows than the model has columns (predictors plus levels plus the intercept). A group with no
spare degrees of freedom is refused with `INSUFFICIENT_RESIDUAL_DF` in the engine. Below about 10 spare degrees of
freedom the engine adds the warning `few_residual_df`. See [Diagnostics and warnings](../reference/diagnostics.md).
