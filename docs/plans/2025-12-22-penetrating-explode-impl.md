# Penetrating Explode (!p) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add penetrating explode modifier (`!p`) that subtracts 1 from each explosion roll's added value.

**Architecture:** Classic compiler pipeline change - add token to lexer, update AST variant, modify parser to recognize `!p`, update roller to subtract 1 on penetrating explosions. Breaking change: removes `!o` (explode once) which no game system uses.

**Tech Stack:** Rust, cargo test

**Worktree:** `~/.config/superpowers/worktrees/diceman/penetrating-explode`

**Design Doc:** `docs/plans/2025-12-22-penetrating-explode-design.md`

---

## Task 1: Lexer - Add Token::P

**Files:**
- Modify: `crates/diceman/src/lexer.rs:29-49` (Token enum)
- Modify: `crates/diceman/src/lexer.rs:144-168` (next_token match)
- Test: `crates/diceman/src/lexer.rs` (tests module)

**Step 1: Write the failing test**

Add to `lexer.rs` tests module:

```rust
#[test]
fn test_penetrating() {
    let mut lexer = Lexer::new("1d6!p");
    assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
    assert_eq!(lexer.next_token().unwrap(), Token::D);
    assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
    assert_eq!(lexer.next_token().unwrap(), Token::Explode);
    assert_eq!(lexer.next_token().unwrap(), Token::P);
    assert_eq!(lexer.next_token().unwrap(), Token::Eof);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_penetrating --lib -- --nocapture`

Expected: FAIL with `Token::P` not found in enum

**Step 3: Add Token::P to enum**

In `Token` enum, replace `O` with `P`:

```rust
/// Penetrating modifier: 'p'.
P,
```

Remove the `O` variant entirely.

**Step 4: Update next_token match arm**

Replace the `'o' | 'O'` match arm with:

```rust
'p' | 'P' => {
    self.chars.next();
    Ok(Token::P)
}
```

Remove the `'o' | 'O'` match arm entirely.

**Step 5: Run test to verify it passes**

Run: `cargo test test_penetrating --lib -- --nocapture`

Expected: Compilation error - `Token::O` used elsewhere

**Step 6: Note compilation errors for next task**

The compiler will show errors in parser.rs where `Token::O` is referenced. This is expected - we'll fix those in Task 3.

---

## Task 2: AST - Update Explode Variant

**Files:**
- Modify: `crates/diceman/src/ast.rs:87-93` (Explode variant)

**Step 1: Update Explode variant**

Change from:

```rust
Explode {
    /// If true, only explode once per die.
    once: bool,
    /// The condition for explosion (defaults to max value).
    condition: Option<Condition>,
},
```

To:

```rust
Explode {
    /// If true, subtract 1 from each explosion roll's added value.
    penetrating: bool,
    /// The condition for explosion (defaults to max value).
    condition: Option<Condition>,
},
```

**Step 2: Run cargo check**

Run: `cargo check 2>&1 | head -50`

Expected: Errors in parser.rs, roller.rs about `once` field not existing

---

## Task 3: Parser - Update explode_modifier

**Files:**
- Modify: `crates/diceman/src/parser.rs:268-279` (explode_modifier function)
- Test: `crates/diceman/src/parser.rs` (tests module)

**Step 1: Write the failing test**

Add to `parser.rs` tests module:

```rust
#[test]
fn test_parse_penetrating_explode() {
    let expr = parse("1d6!p").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                penetrating: true,
                condition: None,
            }],
        })
    );
}

#[test]
fn test_parse_penetrating_explode_condition() {
    let expr = parse("1d6!p>4").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
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

**Step 2: Update explode_modifier function**

Replace the entire function:

```rust
/// Parse an explode modifier (!, !p, !>5, !p>5).
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

**Step 3: Update existing test**

Find `test_parse_explode` and update it to use `penetrating: false`:

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
                penetrating: false,
                condition: None,
            }],
        })
    );
}
```

**Step 4: Update test_parse_explode_condition**

```rust
#[test]
fn test_parse_explode_condition() {
    let expr = parse("1d6!>4").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                penetrating: false,
                condition: Some(Condition {
                    compare: Compare::GreaterThan,
                    value: 4,
                }),
            }],
        })
    );
}
```

**Step 5: Run parser tests**

Run: `cargo test parser --lib`

Expected: Parser tests pass, but roller tests may still fail

---

## Task 4: Roller - Update apply_explode

**Files:**
- Modify: `crates/diceman/src/roller.rs:148-149` (call site in evaluate_roll)
- Modify: `crates/diceman/src/roller.rs:226-272` (apply_explode function)
- Test: `crates/diceman/src/roller.rs` (tests module)

**Step 1: Write the failing test**

Add to `roller.rs` tests module:

```rust
#[test]
fn test_evaluate_penetrating_explode() {
    let roll = Roll {
        count: 1,
        sides: Sides::Number(6),
        modifiers: vec![Modifier::Explode {
            penetrating: true,
            condition: None,
        }],
    };
    let expr = Expr::Roll(roll);
    // Rolls: 6 (explode), 6 (explode), 4 (stop)
    // Added: 6 + (6-1) + (4-1) = 6 + 5 + 3 = 14
    let mut rng = TestRng::new(vec![6, 6, 4]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert_eq!(result.total, 14);
}

#[test]
fn test_evaluate_penetrating_explode_no_explosion() {
    let roll = Roll {
        count: 1,
        sides: Sides::Number(6),
        modifiers: vec![Modifier::Explode {
            penetrating: true,
            condition: None,
        }],
    };
    let expr = Expr::Roll(roll);
    // Roll: 4 (no explosion)
    // Total: 4 (no -1 because no explosion occurred)
    let mut rng = TestRng::new(vec![4]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert_eq!(result.total, 4);
}
```

**Step 2: Update apply_explode signature and call site**

In `evaluate_roll`, change the call from:

```rust
Modifier::Explode { once, condition } => {
    self.apply_explode(&mut dice, &roll.sides, *once, condition.as_ref())?;
}
```

To:

```rust
Modifier::Explode { penetrating, condition } => {
    self.apply_explode(&mut dice, &roll.sides, *penetrating, condition.as_ref())?;
}
```

**Step 3: Update apply_explode function**

Change the function signature and implementation:

```rust
fn apply_explode(
    &mut self,
    dice: &mut Vec<DieResult>,
    sides: &Sides,
    penetrating: bool,
    condition: Option<&Condition>,
) -> Result<()> {
    let max_val = sides.count() as i64;
    let default_condition = Condition {
        compare: Compare::Equal,
        value: max_val,
    };
    let condition = condition.unwrap_or(&default_condition);

    let mut i = 0;
    while i < dice.len() {
        if dice[i].dropped {
            i += 1;
            continue;
        }

        let mut current_value = dice[i].value;
        let mut explode_count = 0;

        while condition.compare.check(current_value, condition.value) {
            if explode_count >= MAX_EXPLOSIONS {
                return Err(Error::ExplodeLimit(MAX_EXPLOSIONS));
            }

            let new_value = self.roll_die(sides);

            // Penetrating: subtract 1 from added value (not from check)
            let added_value = if penetrating { new_value - 1 } else { new_value };

            dice[i].value += added_value;
            dice[i].rolls.push(new_value);

            current_value = new_value;
            explode_count += 1;
        }
        i += 1;
    }

    Ok(())
}
```

**Step 4: Run roller tests**

Run: `cargo test roller --lib`

Expected: All roller tests pass

---

## Task 5: Formatter - Update format_roll

**Files:**
- Modify: `crates/diceman/src/roller.rs:371-380` (format_roll Explode match arm)

**Step 1: Update Explode formatting**

Change from:

```rust
Modifier::Explode { once, condition } => {
    let mut s = "!".to_string();
    if *once {
        s.push('o');
    }
    if let Some(c) = condition {
        s.push_str(&format!("{}{}", c.compare, c.value));
    }
    s
}
```

To:

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

**Step 2: Run all tests**

Run: `cargo test`

Expected: All 43+ tests pass

**Step 3: Commit all changes**

```bash
git add -A
git commit -s -m "feat: add penetrating explode (!p) modifier

Implements DM-jbt: HackMaster-style penetrating dice where each
explosion roll has 1 subtracted from its added value.

- Add Token::P to lexer, remove Token::O
- Change Explode { once } to Explode { penetrating } in AST
- Update parser to recognize !p notation
- Update roller to subtract 1 on penetrating explosions
- Update formatter to output !p

Breaking change: !o (explode once) notation removed - no real
game system uses this mechanic.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Task 6: Manual Verification

**Step 1: Build release binary**

Run: `cargo build --release`

**Step 2: Test penetrating explode interactively**

Run: `cargo run --release --bin diceman -- roll "1d6!p"`

Verify output shows `!p` notation and values look correct.

**Step 3: Run simulation**

Run: `cargo run --release --bin diceman -- sim "1d6!p" -n 100000`

Compare distribution to regular `1d6!` - penetrating should have lower average due to -1 on each explosion.

**Step 4: Test edge cases**

```bash
cargo run --release --bin diceman -- roll "3d6!p"      # Multiple dice
cargo run --release --bin diceman -- roll "1d6!p>4"   # With condition
cargo run --release --bin diceman -- roll "1d6!"      # Regular explode still works
```

---

## Summary

| Task | Description | Estimated Steps |
|------|-------------|-----------------|
| 1 | Lexer - Add Token::P | 6 steps |
| 2 | AST - Update Explode variant | 2 steps |
| 3 | Parser - Update explode_modifier | 5 steps |
| 4 | Roller - Update apply_explode | 4 steps |
| 5 | Formatter - Update format_roll | 3 steps |
| 6 | Manual Verification | 4 steps |

**Total:** 6 tasks, ~24 steps
