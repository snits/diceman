# Critical Success/Failure Markers Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `cs` (critical success) and `cf` (critical failure) display markers to dice notation.

**Architecture:** Extend lexer to recognize `cs`/`cf` keywords, add fields to `Roll` struct, parse after modifiers, apply markers at display time in `format_roll`.

**Tech Stack:** Rust, existing diceman crate infrastructure

---

## Task 1: Add cs/cf Fields to Roll Struct

**Files:**
- Modify: `crates/diceman/src/ast.rs:24-32`

**Step 1: Write the failing test**

In `crates/diceman/src/ast.rs`, add to the existing test module (or create one):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roll_has_crit_fields() {
        let roll = Roll {
            count: 1,
            sides: Sides::Number(20),
            modifiers: vec![],
            crit_success: Some(Condition {
                compare: Compare::Equal,
                value: 20,
            }),
            crit_failure: Some(Condition {
                compare: Compare::Equal,
                value: 1,
            }),
        };
        assert!(roll.crit_success.is_some());
        assert!(roll.crit_failure.is_some());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p diceman test_roll_has_crit_fields`
Expected: FAIL with "no field `crit_success` on type `Roll`"

**Step 3: Write minimal implementation**

Modify the `Roll` struct:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Roll {
    /// Number of dice to roll.
    pub count: u32,
    /// Type of dice (number of sides, percent, or fudge).
    pub sides: Sides,
    /// Modifiers applied to the roll.
    pub modifiers: Vec<Modifier>,
    /// Critical success marker condition.
    pub crit_success: Option<Condition>,
    /// Critical failure marker condition.
    pub crit_failure: Option<Condition>,
}
```

**Step 4: Fix all compilation errors**

The parser creates `Roll` structs without these fields. Update `crates/diceman/src/parser.rs:146-150`:

```rust
Ok(Expr::Roll(Roll {
    count,
    sides,
    modifiers,
    crit_success: None,
    crit_failure: None,
}))
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p diceman`
Expected: PASS (all 57+ tests)

**Step 6: Commit**

```bash
git add crates/diceman/src/ast.rs crates/diceman/src/parser.rs
git commit -s -m "feat(ast): add crit_success and crit_failure fields to Roll

Add optional Condition fields for critical success/failure markers.
These are display-only annotations that don't affect roll calculation.

Part of DM-676"
```

---

## Task 2: Add CritSuccess and CritFail Tokens to Lexer

**Files:**
- Modify: `crates/diceman/src/lexer.rs`

**Step 1: Write the failing test**

Add to the `tests` module in `lexer.rs`:

```rust
#[test]
fn test_crit_success() {
    let mut lexer = Lexer::new("1d20cs20");
    assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
    assert_eq!(lexer.next_token().unwrap(), Token::D);
    assert_eq!(lexer.next_token().unwrap(), Token::Number(20));
    assert_eq!(lexer.next_token().unwrap(), Token::CritSuccess);
    assert_eq!(lexer.next_token().unwrap(), Token::Number(20));
    assert_eq!(lexer.next_token().unwrap(), Token::Eof);
}

#[test]
fn test_crit_failure() {
    let mut lexer = Lexer::new("1d20cf1");
    assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
    assert_eq!(lexer.next_token().unwrap(), Token::D);
    assert_eq!(lexer.next_token().unwrap(), Token::Number(20));
    assert_eq!(lexer.next_token().unwrap(), Token::CritFail);
    assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
    assert_eq!(lexer.next_token().unwrap(), Token::Eof);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p diceman test_crit_success`
Expected: FAIL with "no variant `CritSuccess`"

**Step 3: Write minimal implementation**

Add variants to `Token` enum:

```rust
pub enum Token {
    // ... existing variants ...
    /// Critical success marker: 'cs'.
    CritSuccess,
    /// Critical failure marker: 'cf'.
    CritFail,
    // ... rest of variants ...
}
```

Handle 'c' in `next_token()` - after the existing single-char matches, handle 'c':

```rust
'c' | 'C' => {
    self.chars.next(); // consume 'c'
    if let Some(&(_, next_ch)) = self.chars.peek() {
        match next_ch {
            's' | 'S' => {
                self.chars.next();
                Ok(Token::CritSuccess)
            }
            'f' | 'F' => {
                self.chars.next();
                Ok(Token::CritFail)
            }
            _ => Err(Error::UnexpectedChar(ch, pos)),
        }
    } else {
        Err(Error::UnexpectedChar(ch, pos))
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p diceman test_crit`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/diceman/src/lexer.rs
git commit -s -m "feat(lexer): add CritSuccess and CritFail tokens

Recognize 'cs' and 'cf' as two-character keywords for critical markers.

Part of DM-676"
```

---

## Task 3: Parse cs/cf Markers in Parser

**Files:**
- Modify: `crates/diceman/src/parser.rs`

**Step 1: Write the failing test**

Add to `tests` module in `parser.rs`:

```rust
#[test]
fn test_parse_crit_success() {
    let expr = parse("1d20cs20").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(20),
            modifiers: vec![],
            crit_success: Some(Condition {
                compare: Compare::Equal,
                value: 20,
            }),
            crit_failure: None,
        })
    );
}

#[test]
fn test_parse_crit_failure() {
    let expr = parse("1d20cf1").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(20),
            modifiers: vec![],
            crit_success: None,
            crit_failure: Some(Condition {
                compare: Compare::Equal,
                value: 1,
            }),
        })
    );
}

#[test]
fn test_parse_crit_both() {
    let expr = parse("1d20cs20cf1").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(20),
            modifiers: vec![],
            crit_success: Some(Condition {
                compare: Compare::Equal,
                value: 20,
            }),
            crit_failure: Some(Condition {
                compare: Compare::Equal,
                value: 1,
            }),
        })
    );
}

#[test]
fn test_parse_crit_with_comparison() {
    let expr = parse("1d20cs>=19cf1").unwrap();
    assert_eq!(
        expr,
        Expr::Roll(Roll {
            count: 1,
            sides: Sides::Number(20),
            modifiers: vec![],
            crit_success: Some(Condition {
                compare: Compare::GreaterOrEqual,
                value: 19,
            }),
            crit_failure: Some(Condition {
                compare: Compare::Equal,
                value: 1,
            }),
        })
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p diceman test_parse_crit_success`
Expected: FAIL - crit_success is None instead of Some

**Step 3: Write minimal implementation**

Add import for CritSuccess/CritFail tokens at top of parser.rs.

Modify `roll_or_number()` to parse crits after modifiers:

```rust
fn roll_or_number(&mut self) -> Result<Expr> {
    // ... existing count and sides parsing ...

    // Parse any modifiers
    let modifiers = self.modifiers()?;

    // Parse critical markers (order doesn't matter, can appear in any order)
    let (crit_success, crit_failure) = self.crit_markers()?;

    Ok(Expr::Roll(Roll {
        count,
        sides,
        modifiers,
        crit_success,
        crit_failure,
    }))
}
```

Add new method:

```rust
/// Parse critical success/failure markers (cs, cf).
fn crit_markers(&mut self) -> Result<(Option<Condition>, Option<Condition>)> {
    let mut crit_success = None;
    let mut crit_failure = None;

    loop {
        match self.current {
            Token::CritSuccess => {
                if crit_success.is_some() {
                    return Err(Error::DuplicateCritMarker("cs".to_string()));
                }
                self.advance()?;
                crit_success = Some(self.crit_condition()?);
            }
            Token::CritFail => {
                if crit_failure.is_some() {
                    return Err(Error::DuplicateCritMarker("cf".to_string()));
                }
                self.advance()?;
                crit_failure = Some(self.crit_condition()?);
            }
            _ => break,
        }
    }

    Ok((crit_success, crit_failure))
}

/// Parse a condition for crit markers. Defaults to Equal if just a number.
fn crit_condition(&mut self) -> Result<Condition> {
    // If we see a comparison operator, use optional_condition
    if matches!(self.current, Token::Gt | Token::Lt | Token::Eq) {
        self.optional_condition()?.ok_or_else(|| Error::Expected {
            expected: "condition after critical marker".to_string(),
            found: format!("{:?}", self.current),
        })
    } else if let Token::Number(n) = self.current {
        // Just a number means Equal
        let n = n;
        self.advance()?;
        Ok(Condition {
            compare: Compare::Equal,
            value: n as i64,
        })
    } else {
        Err(Error::Expected {
            expected: "number or comparison after critical marker".to_string(),
            found: format!("{:?}", self.current),
        })
    }
}
```

**Step 4: Add DuplicateCritMarker error variant**

In `crates/diceman/src/error.rs`, add:

```rust
#[error("Duplicate critical marker: {0}")]
DuplicateCritMarker(String),
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p diceman test_parse_crit`
Expected: PASS (all 4 new tests)

**Step 6: Commit**

```bash
git add crates/diceman/src/parser.rs crates/diceman/src/error.rs
git commit -s -m "feat(parser): parse cs/cf critical markers

Parse critical success (cs) and failure (cf) markers after modifiers.
Supports exact match (cs20) and comparison operators (cs>=19).
Validates no duplicate markers.

Part of DM-676"
```

---

## Task 4: Add Crit Flags to DieResult and Format Output

**Files:**
- Modify: `crates/diceman/src/roller.rs`

**Step 1: Write the failing test**

Add to `tests` module in `roller.rs`:

```rust
#[test]
fn test_crit_success_marker_output() {
    let roll = Roll {
        count: 1,
        sides: Sides::Number(20),
        modifiers: vec![],
        crit_success: Some(Condition {
            compare: Compare::Equal,
            value: 20,
        }),
        crit_failure: None,
    };
    let expr = Expr::Roll(roll);
    let mut rng = TestRng::new(vec![20]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert!(result.expression.contains("20**"));
    assert_eq!(result.dice[0].is_crit_success, true);
}

#[test]
fn test_crit_failure_marker_output() {
    let roll = Roll {
        count: 1,
        sides: Sides::Number(20),
        modifiers: vec![],
        crit_success: None,
        crit_failure: Some(Condition {
            compare: Compare::Equal,
            value: 1,
        }),
    };
    let expr = Expr::Roll(roll);
    let mut rng = TestRng::new(vec![1]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert!(result.expression.contains("1*"));
    assert_eq!(result.dice[0].is_crit_failure, true);
}

#[test]
fn test_crit_both_markers_output() {
    let roll = Roll {
        count: 3,
        sides: Sides::Number(20),
        modifiers: vec![],
        crit_success: Some(Condition {
            compare: Compare::Equal,
            value: 20,
        }),
        crit_failure: Some(Condition {
            compare: Compare::Equal,
            value: 1,
        }),
    };
    let expr = Expr::Roll(roll);
    let mut rng = TestRng::new(vec![20, 10, 1]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert!(result.expression.contains("20**"));
    assert!(result.expression.contains("1*"));
    assert!(!result.expression.contains("10*"));
}

#[test]
fn test_crit_no_effect_on_total() {
    let roll = Roll {
        count: 2,
        sides: Sides::Number(20),
        modifiers: vec![],
        crit_success: Some(Condition {
            compare: Compare::Equal,
            value: 20,
        }),
        crit_failure: Some(Condition {
            compare: Compare::Equal,
            value: 1,
        }),
    };
    let expr = Expr::Roll(roll);
    let mut rng = TestRng::new(vec![20, 1]);
    let result = evaluate_with_rng(&expr, &mut rng).unwrap();
    assert_eq!(result.total, 21); // 20 + 1, crits don't change value
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p diceman test_crit_success_marker`
Expected: FAIL with "no field `is_crit_success` on type `DieResult`"

**Step 3: Write minimal implementation**

Add fields to `DieResult`:

```rust
#[derive(Debug, Clone)]
pub struct DieResult {
    pub value: i64,
    pub rolls: Vec<i64>,
    pub dropped: bool,
    /// Whether this die is marked as a critical success.
    pub is_crit_success: bool,
    /// Whether this die is marked as a critical failure.
    pub is_crit_failure: bool,
}
```

Update where `DieResult` is created in `evaluate_roll()`:

```rust
let mut dice: Vec<DieResult> = (0..roll.count)
    .map(|_| {
        let value = self.roll_die(&roll.sides);
        DieResult {
            value,
            rolls: vec![value],
            dropped: false,
            is_crit_success: false,
            is_crit_failure: false,
        }
    })
    .collect();
```

Also update the explode modifier where new dice are created:

```rust
dice.push(DieResult {
    value: added_value,
    rolls: vec![new_value],
    dropped: false,
    is_crit_success: false,
    is_crit_failure: false,
});
```

Add crit marking after modifiers are applied, before formatting:

```rust
// Mark critical successes/failures (display-only)
self.mark_crits(&mut dice, &roll.crit_success, &roll.crit_failure);
```

Add the method:

```rust
fn mark_crits(
    &self,
    dice: &mut [DieResult],
    crit_success: &Option<Condition>,
    crit_failure: &Option<Condition>,
) {
    for die in dice.iter_mut() {
        if let Some(ref cond) = crit_success {
            die.is_crit_success = cond.compare.check(die.value, cond.value);
        }
        if let Some(ref cond) = crit_failure {
            die.is_crit_failure = cond.compare.check(die.value, cond.value);
        }
    }
}
```

Update `format_roll` signature to take the Roll:

```rust
fn format_roll(
    &self,
    roll: &Roll,
    dice: &[DieResult],
    total: i64,
    success_condition: Option<&Condition>,
) -> String {
```

Update dice formatting in `format_roll`:

```rust
let dice_str: String = dice
    .iter()
    .map(|d| {
        let base = if d.dropped {
            format!("({})", d.value)
        } else {
            d.value.to_string()
        };

        // Add crit markers (crit success takes precedence in display)
        if !d.dropped {
            if d.is_crit_success {
                format!("{}**", d.value)
            } else if d.is_crit_failure {
                format!("{}*", d.value)
            } else if let Some(condition) = success_condition {
                if condition.compare.check(d.value, condition.value) {
                    format!("{}*", d.value) // Success counting marker
                } else {
                    d.value.to_string()
                }
            } else {
                d.value.to_string()
            }
        } else {
            format!("({})", d.value)
        }
    })
    .collect::<Vec<_>>()
    .join(", ");
```

Add cs/cf to the modifiers string output:

```rust
// After modifiers_str, add crit markers to output
let crit_str = format!(
    "{}{}",
    roll.crit_success.as_ref().map_or(String::new(), |c| {
        if c.compare == Compare::Equal {
            format!("cs{}", c.value)
        } else {
            format!("cs{}{}", c.compare, c.value)
        }
    }),
    roll.crit_failure.as_ref().map_or(String::new(), |c| {
        if c.compare == Compare::Equal {
            format!("cf{}", c.value)
        } else {
            format!("cf{}{}", c.compare, c.value)
        }
    }),
);
```

Then include `crit_str` in the format string.

**Step 4: Run test to verify it passes**

Run: `cargo test -p diceman test_crit`
Expected: PASS (all crit tests)

**Step 5: Run full test suite**

Run: `cargo test -p diceman`
Expected: PASS (all tests including existing ones)

**Step 6: Commit**

```bash
git add crates/diceman/src/roller.rs
git commit -s -m "feat(roller): display crit success/failure markers

Add is_crit_success and is_crit_failure to DieResult.
Mark dice at display time after all modifiers applied.
Output format: 20** for crit success, 1* for crit failure.

Part of DM-676"
```

---

## Task 5: Validate cs/cf Cannot Combine with Success Counting

**Files:**
- Modify: `crates/diceman/src/parser.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_crit_with_success_counting_error() {
    let result = parse("5d10>=8cs10");
    assert!(result.is_err());
    // Verify the error message mentions incompatibility
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p diceman test_crit_with_success_counting`
Expected: FAIL - currently parses without error

**Step 3: Write minimal implementation**

Add validation in `roll_or_number()` after parsing both modifiers and crit markers:

```rust
// Validate: cs/cf cannot combine with success counting
let has_success_counting = modifiers.iter().any(|m| matches!(m, Modifier::CountSuccesses(_)));
if has_success_counting && (crit_success.is_some() || crit_failure.is_some()) {
    return Err(Error::CritWithSuccessCounting);
}
```

Add error variant in `error.rs`:

```rust
#[error("Critical markers cannot be combined with success counting")]
CritWithSuccessCounting,
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p diceman test_crit_with_success`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/diceman/src/parser.rs crates/diceman/src/error.rs
git commit -s -m "feat(parser): validate cs/cf incompatible with success counting

Critical markers are display-only annotations that don't make sense
when combined with success counting mode. Return clear error.

Part of DM-676"
```

---

## Task 6: Integration Test and Documentation

**Files:**
- Modify: `crates/diceman/src/lib.rs` (integration test)
- Modify: `README.md` (documentation)

**Step 1: Write integration test**

Add to `tests` module in `lib.rs`:

```rust
#[test]
fn test_crit_markers_integration() {
    // Basic crit success
    let result = roll_with_seed("1d20cs20cf1", 12345).unwrap();
    // Verify it parses and evaluates without error
    assert!(result.total >= 1 && result.total <= 20);

    // With expanded crit range
    let result = roll_with_seed("1d20cs>=19cf1", 12345).unwrap();
    assert!(result.total >= 1 && result.total <= 20);
}
```

**Step 2: Run integration test**

Run: `cargo test -p diceman test_crit_markers_integration`
Expected: PASS

**Step 3: Update README.md**

Add new section after "Success Counting":

```markdown
### Critical Markers

Mark dice results as critical successes or failures for visual feedback.

| Notation | Description |
|----------|-------------|
| `csN` | Mark rolls equal to N as critical success |
| `cs>N` | Mark rolls greater than N as critical success |
| `cs>=N` | Mark rolls greater than or equal to N as critical success |
| `cfN` | Mark rolls equal to N as critical failure |
| `cf<N` | Mark rolls less than N as critical failure |

**Output:** Critical successes show `**`, critical failures show `*`.

**Examples:**
- `1d20cs20cf1` → `[20**] = 20` or `[1*] = 1` or `[15] = 15`
- `1d20cs>=19cf1` → Expanded crit range (19 or 20 is crit)
- `4d6cs6cf1` → `[6**, 4, 3, 1*] = 14`

**Note:** Critical markers cannot be combined with success counting.
```

**Step 4: Commit**

```bash
git add crates/diceman/src/lib.rs README.md
git commit -s -m "docs: add critical markers documentation

Add README section for cs/cf notation with examples.
Add integration test for crit markers feature.

Closes DM-676"
```

---

## Task 7: Final Verification

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 3: Test CLI manually**

```bash
cargo run --bin diceman -- roll "1d20cs20cf1"
cargo run --bin diceman -- roll "1d20cs>=19cf1"
cargo run --bin diceman -- roll "4d6cs6cf1"
```

**Step 4: Final commit if any fixes needed**

If any issues found, fix and commit.

**Step 5: Update bead status**

```bash
bd close DM-676 -r "Implemented cs/cf critical markers"
bd sync
```
