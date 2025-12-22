# Compounding Explode (!!) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement Roll20-standard exploding dice (`!`) where explosions create new dice, and Shadowrun-style compounding (`!!`) where explosions add to the same die.

**Architecture:** Add `compounding: bool` to `Modifier::Explode`. Parser checks for second `!`. Roller creates new `DieResult` entries for standard explode, adds to existing for compounding. Breaking change: current `!` behavior becomes `!!`.

**Tech Stack:** Rust, TDD with `cargo test`

**Worktree:** `~/.config/superpowers/worktrees/diceman/compounding-explode`

---

### Task 1: Update AST with compounding field

**Files:**
- Modify: `crates/diceman/src/ast.rs:88-93`

**Step 1: Update the Explode variant**

Add `compounding: bool` field to `Modifier::Explode`:

```rust
/// Explode dice matching the condition.
Explode {
    /// If true, add explosions to same die (compounding/Shadowrun).
    /// If false, create new dice for each explosion (standard/Roll20).
    compounding: bool,
    /// If true, subtract 1 from each explosion roll's added value.
    penetrating: bool,
    /// The condition for explosion (defaults to max value).
    condition: Option<Condition>,
},
```

**Step 2: Run cargo check to see all compilation errors**

Run: `cargo check 2>&1 | head -50`
Expected: Errors in parser.rs and roller.rs about missing `compounding` field

**Step 3: Commit AST change**

```bash
git add crates/diceman/src/ast.rs
git commit -s -m "ast: add compounding field to Explode modifier

Breaking change preparation for DM-21m. Field added but not yet
used by parser or roller.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Update parser for compounding (!!)

**Files:**
- Modify: `crates/diceman/src/parser.rs:268-279`
- Modify: `crates/diceman/src/parser.rs` (tests section)

**Step 1: Write failing test for compounding parse**

Add to parser tests:

```rust
#[test]
fn test_parse_compounding_explode() {
    let expr = parse("1d6!!").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                compounding: true,
                penetrating: false,
                condition: None,
            }],
        })
    );
}

#[test]
fn test_parse_compounding_penetrating() {
    let expr = parse("1d6!!p").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                compounding: true,
                penetrating: true,
                condition: None,
            }],
        })
    );
}

#[test]
fn test_parse_compounding_condition() {
    let expr = parse("1d6!!>4").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                compounding: true,
                penetrating: false,
                condition: Some(Condition {
                    compare: Compare::GreaterThan,
                    value: 4,
                }),
            }],
        })
    );
}

#[test]
fn test_parse_compounding_penetrating_condition() {
    let expr = parse("1d6!!p>4").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                compounding: true,
                penetrating: true,
                condition: Some(Condition {
                    compare: Compare::GreaterThan,
                    value: 4,
                }),
            }],
        })
    );
}
```

**Step 2: Update existing parser tests to include compounding field**

Update these tests to add `compounding: false`:
- `test_parse_explode` (line ~419)
- `test_parse_explode_condition` (line ~434)
- `test_parse_penetrating_explode` (line ~454)
- `test_parse_penetrating_explode_condition` (line ~470)

Example fix for `test_parse_explode`:

```rust
#[test]
fn test_parse_explode() {
    let expr = parse("1d6!").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                compounding: false,
                penetrating: false,
                condition: None,
            }],
        })
    );
}
```

**Step 3: Update explode_modifier() to check for second !**

Replace `explode_modifier()` function:

```rust
/// Parse an explode modifier (!, !!, !p, !!p, !>5, !!p>5).
fn explode_modifier(&mut self) -> Result<Modifier> {
    // Check for compounding (!!)
    let compounding = if self.current == Token::Explode {
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

**Step 4: Run parser tests**

Run: `cargo test -p diceman parser`
Expected: All parser tests pass

**Step 5: Commit parser changes**

```bash
git add crates/diceman/src/parser.rs
git commit -s -m "parser: add compounding explode (!!) support

- Check for second ! token after first to detect compounding
- Add tests for !!, !!p, !!>N, !!p>N notation
- Update existing ! tests to include compounding: false field

Part of DM-21m.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Update roller for standard explode (new dice)

**Files:**
- Modify: `crates/diceman/src/roller.rs:148-149` (call site)
- Modify: `crates/diceman/src/roller.rs:226-270` (apply_explode function)

**Step 1: Write failing test for standard explode creating new dice**

Add to roller tests:

```rust
#[test]
fn test_evaluate_standard_explode_creates_new_dice() {
    let roll = Roll {
        count: 1,
        sides: Sides::Number(6),
        modifiers: vec![Modifier::Explode {
            compounding: false,
            penetrating: false,
            condition: None,
        }],
    };
    let expr = Expr::Roll(roll);
    // Rolls: 6 (explode), 4 (stop)
    // Should create 2 separate dice: [6, 4]
    let mut rng = TestRng::new(vec![6, 4]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert_eq!(result.total, 10);
    assert_eq!(result.dice.len(), 2); // Two separate dice
    assert_eq!(result.dice[0].value, 6);
    assert_eq!(result.dice[1].value, 4);
}

#[test]
fn test_evaluate_standard_explode_chain() {
    let roll = Roll {
        count: 1,
        sides: Sides::Number(6),
        modifiers: vec![Modifier::Explode {
            compounding: false,
            penetrating: false,
            condition: None,
        }],
    };
    let expr = Expr::Roll(roll);
    // Rolls: 6 (explode), 6 (explode), 3 (stop)
    // Should create 3 separate dice: [6, 6, 3]
    let mut rng = TestRng::new(vec![6, 6, 3]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert_eq!(result.total, 15);
    assert_eq!(result.dice.len(), 3);
}

#[test]
fn test_evaluate_compounding_explode() {
    let roll = Roll {
        count: 1,
        sides: Sides::Number(6),
        modifiers: vec![Modifier::Explode {
            compounding: true,
            penetrating: false,
            condition: None,
        }],
    };
    let expr = Expr::Roll(roll);
    // Rolls: 6 (explode), 6 (explode), 3 (stop)
    // Should compound into 1 die: [15]
    let mut rng = TestRng::new(vec![6, 6, 3]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert_eq!(result.total, 15);
    assert_eq!(result.dice.len(), 1); // Still one die
    assert_eq!(result.dice[0].value, 15);
}

#[test]
fn test_evaluate_explode_with_keep() {
    let roll = Roll {
        count: 2,
        sides: Sides::Number(6),
        modifiers: vec![
            Modifier::Explode {
                compounding: false,
                penetrating: false,
                condition: None,
            },
            Modifier::KeepHighest(2),
        ],
    };
    let expr = Expr::Roll(roll);
    // Rolls: 6 (explode), 5 (new die), 3 (second original die)
    // Dice after explode: [6, 5, 3] - keep highest 2: 6 + 5 = 11
    let mut rng = TestRng::new(vec![6, 3, 5]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert_eq!(result.total, 11);
}
```

**Step 2: Update existing roller tests to use compounding: true**

Update these tests since they expect the old (compounding) behavior:
- `test_evaluate_penetrating_explode`
- `test_evaluate_penetrating_explode_no_explosion`

Example fix:

```rust
#[test]
fn test_evaluate_penetrating_explode() {
    let roll = Roll {
        count: 1,
        sides: Sides::Number(6),
        modifiers: vec![Modifier::Explode {
            compounding: true,  // Add this
            penetrating: true,
            condition: None,
        }],
    };
    // ... rest unchanged
}
```

**Step 3: Run tests to see failures**

Run: `cargo test -p diceman roller`
Expected: FAIL - compilation errors or test failures

**Step 4: Update apply_explode call site**

At line ~148-149, update the match arm:

```rust
Modifier::Explode { compounding, penetrating, condition } => {
    self.apply_explode(&mut dice, &roll.sides, *compounding, *penetrating, condition.as_ref())?;
}
```

**Step 5: Update apply_explode function signature and implementation**

Replace the function:

```rust
fn apply_explode(
    &mut self,
    dice: &mut Vec<DieResult>,
    sides: &Sides,
    compounding: bool,
    penetrating: bool,
    condition: Option<&Condition>,
) -> Result<()> {
    let max_val = sides.count() as i64;
    let default_condition = Condition {
        compare: Compare::Equal,
        value: max_val,
    };
    let condition = condition.unwrap_or(&default_condition);

    // Track original dice count - only check those for explosions
    let original_len = dice.len();
    let mut total_explosions = 0u32;

    let mut i = 0;
    while i < dice.len() {
        if dice[i].dropped {
            i += 1;
            continue;
        }

        // Only process original dice and newly created explosion dice
        let mut current_value = if i < original_len {
            dice[i].value
        } else {
            // For explosion dice, check their own value
            dice[i].value
        };

        while condition.compare.check(current_value, condition.value) {
            if total_explosions >= MAX_EXPLOSIONS {
                return Err(Error::ExplodeLimit(MAX_EXPLOSIONS));
            }

            let new_value = self.roll_die(sides);

            // Penetrating: subtract 1 from added value (not from check)
            let added_value = if penetrating { new_value - 1 } else { new_value };

            if compounding {
                // Compounding: add to same die
                dice[i].value += added_value;
                dice[i].rolls.push(new_value);
            } else {
                // Standard: create new die for explosion
                let new_die = DieResult {
                    value: added_value,
                    rolls: vec![new_value],
                    dropped: false,
                };
                dice.push(new_die);
            }

            current_value = new_value;
            total_explosions += 1;

            // For standard explode, stop checking this die - new die will be checked
            if !compounding {
                break;
            }
        }
        i += 1;
    }

    Ok(())
}
```

**Step 6: Run roller tests**

Run: `cargo test -p diceman roller`
Expected: All roller tests pass

**Step 7: Commit roller changes**

```bash
git add crates/diceman/src/roller.rs
git commit -s -m "roller: implement standard vs compounding explode

Standard explode (!) creates new dice for each explosion.
Compounding (!!) adds explosions to the same die (old behavior).

- Update apply_explode to handle both modes
- Add tests for standard explode creating separate dice
- Add test for explode + keep interaction
- Update penetrating tests to use compounding: true

Part of DM-21m.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Update roller formatter

**Files:**
- Modify: `crates/diceman/src/roller.rs:369-378` (formatter)

**Step 1: Update formatter to output !!**

Replace the Explode arm in format_roll:

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

**Step 2: Run all tests**

Run: `cargo test -p diceman`
Expected: All tests pass

**Step 3: Commit formatter change**

```bash
git add crates/diceman/src/roller.rs
git commit -s -m "roller: update formatter to output !! for compounding

Part of DM-21m.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Update CLAUDE.md documentation

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update dice notation reference**

Find the Explode line and update to:

```
- Explode: `1d6!` (explode on max, new dice), `1d6!!` (compounding), `1d6!p` (penetrating), `1d6!>5` (explode on 5+)
```

**Step 2: Run full test suite**

Run: `cargo test`
Expected: All 48+ tests pass

**Step 3: Commit documentation**

```bash
git add CLAUDE.md
git commit -s -m "docs: update CLAUDE.md with compounding explode notation

Completes DM-21m.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6: Integration test with CLI

**Step 1: Build and test CLI**

```bash
cargo build --release
./target/release/diceman roll "3d6!"
./target/release/diceman roll "3d6!!"
./target/release/diceman roll "1d6!!p"
```

**Step 2: Verify output format**

- `3d6!` should show separate dice when explosions occur (e.g., `[6, 4, 5, 5]` for 4 dice if one exploded)
- `3d6!!` should show combined values (e.g., `[10, 5, 5]` if first die exploded)

**Step 3: Run simulation comparison**

```bash
./target/release/diceman sim "1d6!" -n 100000
./target/release/diceman sim "1d6!!" -n 100000
```

Expected: Both should have similar mean (~4.2) since total values are the same, just displayed differently.

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Add compounding to AST | ast.rs |
| 2 | Parser support for !! | parser.rs |
| 3 | Roller standard vs compounding | roller.rs |
| 4 | Formatter output !! | roller.rs |
| 5 | Update CLAUDE.md | CLAUDE.md |
| 6 | Integration test | CLI |

**Breaking change:** `!` now creates separate dice. Use `!!` for old behavior.
