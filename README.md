# diceman

A dice notation parser and roller for tabletop RPGs.

## Installation

```bash
cargo install --path crates/diceman-cli
```

## Usage

### Roll dice

```bash
diceman roll "4d6kh3"        # Roll 4d6, keep highest 3
diceman roll "2d6 + 5"       # Roll 2d6 and add 5
diceman roll "1d20 + 7"      # Attack roll with modifier
```

**Note:** Quote expressions containing `>`, `<`, `!`, or `*` to prevent shell interpretation.

### Simulate distributions

```bash
diceman sim "2d6" -n 10000   # Simulate 10,000 rolls
diceman sim "4d6kh3" --json  # Output as JSON
```

### Show notation help

```bash
diceman notation             # Show full notation reference
```

## Dice Notation Reference

diceman uses Roll20-compatible dice notation with some extensions.

### Basic Rolls

| Notation | Description |
|----------|-------------|
| `NdS` | Roll N dice with S sides |
| `dS` | Roll 1 die (shorthand for 1dS) |
| `d%` | Percentile die (d100) |
| `dF` | Fudge die (-1, 0, +1) |

**Examples:** `2d6`, `1d20`, `4dF`, `d%`

### Digit Dice

| Notation | Description |
|----------|-------------|
| `Dnn` | Roll one dS per digit, read the dice as digits of one number |

All digits must be the same and non-zero (`D66`, `D88`, `D444`).

**Examples:** `D66` (roll 2d6, `3, 5` → `35`), `D666` (roll 3d6, `1, 4, 6` → `146`), `D88` (roll 2d8, `7, 8` → `78`)

### Arithmetic

| Notation | Description |
|----------|-------------|
| `+` `-` | Addition and subtraction |
| `*` `/` | Multiplication and division |
| `(...)` | Grouping |

**Examples:** `2d6 + 5`, `(1d6 + 2) * 3`, `1d20 + 7`

### Keep and Drop

| Notation | Description |
|----------|-------------|
| `khN` | Keep highest N dice |
| `klN` | Keep lowest N dice |
| `kN` | Keep highest N (shorthand) |
| `dhN` | Drop highest N dice |
| `dlN` | Drop lowest N dice |

**Examples:** `4d6kh3` (ability scores), `2d20kl1` (disadvantage), `4d6dl1` (drop lowest)

### Exploding Dice

When a die rolls its maximum value, roll again and add the result.

| Notation | Description |
|----------|-------------|
| `!` | Explode on max (each explosion is a new die) |
| `!!` | Compounding explode (explosions add to same die) |
| `!p` | Penetrating explode (subtract 1 from each explosion) |
| `!!p` | Compounding penetrating |

All explode modifiers support conditions:

| Notation | Description |
|----------|-------------|
| `!>N` | Explode on rolls greater than N |
| `!<N` | Explode on rolls less than N |
| `!=N` | Explode on rolls equal to N |
| `!>=N` | Explode on rolls greater than or equal to N |
| `!<=N` | Explode on rolls less than or equal to N |

**Examples:**
- `1d6!` - Standard exploding d6 (Roll20 style)
- `1d6!!` - Compounding/Shadowrun style (6+6+4 = 16 shown as [16])
- `1d6!p` - HackMaster penetrating (6+5+3 shown as [6, 4, 2])
- `1d10!>=8` - Explode on 8, 9, or 10

### Reroll

| Notation | Description |
|----------|-------------|
| `r` | Reroll 1s (keep rerolling until not 1) |
| `ro` | Reroll once (only reroll the first 1) |
| `r<N` | Reroll values less than N |
| `r<=N` | Reroll values less than or equal to N |

**Examples:** `1d6r` (reroll 1s), `2d6r<3` (reroll 1s and 2s), `1d20ro` (reroll first 1 only)

### Success Counting

Count dice that meet a condition instead of summing values.

| Notation | Description |
|----------|-------------|
| `>N` | Count dice greater than N |
| `>=N` | Count dice greater than or equal to N |
| `<N` | Count dice less than N |
| `<=N` | Count dice less than or equal to N |
| `=N` | Count dice equal to N |

**Examples:**
- `5d10>=8` - World of Darkness (count 8, 9, 10)
- `6d6>4` - Count 5s and 6s
- `8d6=6` - Count only 6s

### Critical Markers

Mark dice results as critical successes or failures for visual feedback.

| Notation | Description |
|----------|-------------|
| `csN` | Mark rolls equal to N as critical success |
| `cs>N` | Mark rolls greater than N as critical success |
| `cs>=N` | Mark rolls greater than or equal to N as critical success |
| `cs<N` | Mark rolls less than N as critical success |
| `cs<=N` | Mark rolls less than or equal to N as critical success |
| `cfN` | Mark rolls equal to N as critical failure |
| `cf>N` | Mark rolls greater than N as critical failure |
| `cf>=N` | Mark rolls greater than or equal to N as critical failure |
| `cf<N` | Mark rolls less than N as critical failure |
| `cf<=N` | Mark rolls less than or equal to N as critical failure |

**Output:** Critical successes show `**`, critical failures show `*`.

**Examples:**
- `1d20cs20cf1` → `[20**] = 20` or `[1*] = 1` or `[15] = 15`
- `1d20cs>=19cf1` → Expanded crit range (19 or 20 is crit)
- `1d20cs20cf<=2` → Expanded crit fail range (1 or 2 is crit fail)
- `4d6cs6cf1` → `[6**, 4, 3, 1*] = 14`

**Note:** Critical markers cannot be combined with success counting.

### Modifier Order

Modifiers are applied in this order: **reroll, explode, keep/drop, success count**

This means `4d6r!kh3` will:
1. Reroll any 1s
2. Explode any 6s (creating new dice)
3. Keep the highest 3 dice

## Library Usage

### Rust

```rust
use diceman::{roll, simulate};

let result = roll("4d6kh3")?;
println!("{}", result.expression);  // "4d6kh3[5, 4, 3, 1] = 12"
println!("{}", result.outcome.as_numeric());       // 12

let sim = simulate("2d6", 10000)?;
println!("Mean: {:.2}", sim.mean);
```

### Python

```python
import diceman

result = diceman.roll("4d6kh3")
print(result)  # "4d6kh3[5, 4, 3, 1] = 12"

stats = diceman.simulate("2d6", n=10000)
print(f"Mean: {stats.mean:.2f}")
```

## License

MIT
