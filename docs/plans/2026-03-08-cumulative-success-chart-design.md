# Cumulative Success Chart for `sim` Command

## Purpose

Add a cumulative probability view to the `sim` command output, showing the
chance of meeting or beating (or rolling under) a target value. Answers
"what are my odds of hitting AC 15?" directly in the terminal.

## CLI Interface

```
diceman sim "1d20+5" --cumulative        # shows >= target by default
diceman sim "1d20+5" --cumulative --lte   # shows <= target instead
diceman sim "1d20+5" --lte               # --lte implies --cumulative
```

- `--cumulative` — enables the cumulative chart below the distribution
- `--lte` — switches direction to "roll under" (implies `--cumulative`)

## Output Format

After the existing distribution chart, a second section appears:

```
1d20+5 (n=10000)

   6: ##                                         5.1%
   7: ##                                         4.9%
  ...
  25: ##                                         5.0%

Cumulative (>= target):
   6: ########################################  100.0%
   7: ######################################    94.9%
  ...
  25: ##                                         5.0%

mean: 15.50, std: 5.77
```

With `--lte`, the header reads "Cumulative (<= target):" and values
accumulate from low to high.

## Implementation Scope

1. **CLI args** — Add `--cumulative` and `--lte` boolean flags to the `Sim`
   variant in clap.
2. **Cumulative computation** — A function that takes sorted outcomes and
   direction, returns `Vec<(i64, f64)>` of (value, cumulative_percentage).
3. **Rendering** — A `print_cumulative_histogram` function using the same
   bar-chart style as the existing distribution chart.
4. **Tests** — Unit tests for the cumulative computation logic.

## What Stays the Same

- `SimResult` and `sim.rs` are untouched — computation happens in the CLI
  from existing data.
- The existing distribution chart output is unchanged.
- `--json` output is unchanged.
