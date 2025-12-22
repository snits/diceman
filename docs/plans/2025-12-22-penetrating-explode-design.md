# Penetrating Explode (!p) Design

**Bead:** DM-jbt
**Date:** 2025-12-22
**Status:** Approved

## Overview

Add penetrating explode modifier (`!p`) following Roll20/HackMaster conventions. Each explosion roll has 1 subtracted from the value added to the total, but the raw roll is used for the explosion check.

## Notation

- `!p` - penetrating explode on max
- `!p>N` - penetrating explode on > N
- `!p<N` - penetrating explode on < N

## Example

`1d6!p` rolls 6 → 6 → 4:
- First 6: add 6, explode (hit max)
- Second 6: add 5 (6-1), explode (raw roll hit max)
- Roll 4: add 3 (4-1), stop (raw roll didn't hit max)
- **Total: 14**

## Breaking Change

The `!o` (explode once) notation is removed. No real game system uses this mechanic. Compounding (`!!`) is tracked separately in DM-21m.

## Implementation

### Lexer (`lexer.rs`)

Add `Token::P` for the 'p' character. Remove `Token::O`.

```rust
// Add to Token enum:
P,

// Add to match in next_token():
'p' | 'P' => {
    self.chars.next();
    Ok(Token::P)
}
```

### AST (`ast.rs`)

Replace `once: bool` with `penetrating: bool`:

```rust
Explode {
    /// If true, subtract 1 from each explosion roll's added value.
    penetrating: bool,
    /// The condition for explosion (defaults to max value).
    condition: Option<Condition>,
}
```

### Parser (`parser.rs`)

Update `explode_modifier()`:

```rust
fn explode_modifier(&mut self) -> Result<Modifier> {
    let penetrating = if self.current == Token::P {
        self.advance()?;
        true
    } else {
        false
    };

    let condition = self.optional_condition()?;

    Ok(Modifier::Explode { penetrating, condition })
}
```

### Roller (`roller.rs`)

Update `apply_explode()` to subtract 1 when penetrating:

```rust
fn apply_explode(
    &mut self,
    dice: &mut Vec<DieResult>,
    sides: &Sides,
    penetrating: bool,
    condition: Option<&Condition>,
) -> Result<()> {
    // ... setup code unchanged ...

    while condition.compare.check(current_value, condition.value) {
        // ... limit check unchanged ...

        let new_value = self.roll_die(sides);

        // Penetrating: subtract 1 from added value (not from check)
        let added_value = if penetrating { new_value - 1 } else { new_value };

        dice[i].value += added_value;
        dice[i].rolls.push(new_value); // Store raw roll for display

        current_value = new_value; // Check uses raw roll
        // ...
    }
}
```

Update formatter to output `!p`:

```rust
Modifier::Explode { penetrating, condition } => {
    let mut s = "!".to_string();
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

### Lexer
- `test_penetrating`: Tokenize `1d6!p`

### Parser
- `test_parse_penetrating_explode`: Parse `1d6!p`
- `test_parse_penetrating_explode_condition`: Parse `1d6!p>4`

### Roller
- `test_evaluate_penetrating_explode`: Verify -1 subtraction on explosions
- `test_evaluate_penetrating_explode_chain`: Verify chained explosions each subtract 1

## Files Changed

| File | Changes |
|------|---------|
| `lexer.rs` | Add `Token::P`, remove `Token::O` |
| `ast.rs` | Change `Explode { once, condition }` → `Explode { penetrating, condition }` |
| `parser.rs` | Update `explode_modifier()` to check for `P` instead of `O` |
| `roller.rs` | Update `apply_explode()`, call site, and formatter |
| tests | Remove `!o` tests, add `!p` tests |
