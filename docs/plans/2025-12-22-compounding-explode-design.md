# Compounding Explode (!!) Design

**Bead:** DM-21m
**Date:** 2025-12-22
**Status:** Approved

## Overview

Implement Roll20-standard exploding dice (`!`) where each explosion creates a new die, and add Shadowrun-style compounding (`!!`) where explosions add to the same die. This is a breaking change to the current `!` behavior.

## Notation

- `!` - standard explode (each explosion = new die)
- `!!` - compounding explode (explosions add to same die)
- Both support conditions: `!>4`, `!!>4`, `!<3`, `!!<3`
- Both support penetrating: `!p`, `!!p`, `!p>5`, `!!p>5`

## Examples

**Standard explode (`!`):**
```
3d6![6, 4, 5, 5] = 20
      ^--- explosion became its own die
```

**Compounding (`!!`):**
```
3d6!![10, 5, 5] = 20
       ^--- explosion added to same die (6+4=10)
```

**Penetrating compounding (`!!p`):**
```
1d6!!p[14] = 14    # rolls 6→6→4, adds 6+(6-1)+(4-1)=14
```

## Breaking Change

`!` behavior changes from compounding to standard. Users expecting `1d6!` to show `[11]` will now see `[6, 5]`. Migration: use `!!` for old behavior.

## Implementation

### AST (`ast.rs`)

Add `compounding: bool` to `Modifier::Explode`:

```rust
Explode {
    /// If true, add explosions to same die (compounding).
    /// If false, create new dice for each explosion (standard).
    compounding: bool,
    /// If true, subtract 1 from each explosion roll's added value.
    penetrating: bool,
    /// The condition for explosion (defaults to max value).
    condition: Option<Condition>,
}
```

**Notation combinations:**
- `!` → `compounding: false, penetrating: false`
- `!!` → `compounding: true, penetrating: false`
- `!p` → `compounding: false, penetrating: true`
- `!!p` → `compounding: true, penetrating: true`

### Parser (`parser.rs`)

Update `explode_modifier()` to check for second `!`:

```rust
fn explode_modifier(&mut self) -> Result<Modifier> {
    // Check for compounding (!!)
    let compounding = if self.current == Token::Bang {
        self.advance()?;
        true
    } else {
        false
    };

    // Check for penetrating (p)
    let penetrating = if self.current == Token::P {
        self.advance()?;
        true
    } else {
        false
    };

    let condition = self.optional_condition()?;

    Ok(Modifier::Explode { compounding, penetrating, condition })
}
```

**Parsing examples:**

| Input | Tokens after first `!` | Result |
|-------|------------------------|--------|
| `1d6!` | (end) | `compounding: false` |
| `1d6!!` | `!` | `compounding: true` |
| `1d6!p` | `P` | `penetrating: true` |
| `1d6!p>5` | `P`, `>`, `5` | `penetrating: true`, condition |
| `1d6!!p` | `!`, `P` | both true |
| `1d6!!p>5` | `!`, `P`, `>`, `5` | all three |

### Roller (`roller.rs`)

Update `apply_explode()`:

```rust
if compounding {
    // Add to same die (current behavior)
    dice[i].value += added_value;
    dice[i].rolls.push(new_value);
} else {
    // Create new die for explosion
    let new_die = DieResult {
        value: added_value,
        rolls: vec![new_value],
        dropped: false,
    };
    dice.push(new_die);
}
```

When creating new dice, the loop must process newly added dice that can themselves explode. Explosion limit (100) still applies per roll total.

Update formatter to output `!!`:

```rust
Modifier::Explode { compounding, penetrating, condition } => {
    let mut s = "!".to_string();
    if *compounding {
        s.push('!');
    }
    if *penetrating {
        s.push('p');
    }
    if let Some(c) = condition {
        s.push_str(&format!("{}{}", c.compare, c.value));
    }
    s
}
```

## Tests

### Parser
- `test_parse_compounding_explode`: Parse `1d6!!`
- `test_parse_compounding_penetrating`: Parse `1d6!!p`
- `test_parse_compounding_condition`: Parse `1d6!!>4`
- `test_parse_compounding_penetrating_condition`: Parse `1d6!!p>4`

### Roller
- `test_evaluate_standard_explode_creates_new_dice`: Verify `1d6!` creates separate dice
- `test_evaluate_compounding_explode`: Verify `1d6!!` adds to same die
- `test_evaluate_standard_explode_chain`: Multiple explosions each create new dice
- `test_evaluate_explode_with_keep`: Verify `4d6!kh3` works with explosion-created dice

### Migration
Existing `!` tests need updating: change notation to `!!` or update expected behavior.

## Files Changed

| File | Changes |
|------|---------|
| `ast.rs` | Add `compounding: bool` to `Explode` variant |
| `parser.rs` | Update `explode_modifier()` to check for second `!` |
| `roller.rs` | Update `apply_explode()` to create new dice when not compounding |
| `roller.rs` | Update formatter to output `!!` when compounding |
| tests | Update existing `!` tests, add new `!!` tests |
| `CLAUDE.md` | Update notation reference |
