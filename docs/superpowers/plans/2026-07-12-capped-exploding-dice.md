# Capped Exploding Dice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let dice notation cap how many times a die explodes (e.g. `1d6!1` = explode at most once, for Kal-Arath).

**Architecture:** Add a `limit: Option<u32>` field to the existing `RollModifier::Explode` AST variant. The parser reads a bare number after the explode/penetrating markers as the cap; the roller stops a chain quietly when the cap is reached (distinct from the `MAX_EXPLOSIONS` runaway guard, which still errors); the formatter round-trips it.

**Tech Stack:** Rust (workspace crate `diceman`). Tests are inline `#[cfg(test)]` modules. Run with `cargo test -p diceman`.

## Global Constraints

- Field order on the `Explode` variant everywhere: `compounding, penetrating, limit, condition`.
- `limit: None` == unlimited == current behavior. Existing explode tests MUST stay green unchanged (backward compatibility).
- Reaching the user `limit` is a normal quiet stop, NOT an error. `Error::ExplodeLimit` remains reserved for the `MAX_EXPLOSIONS` runaway guard only.
- Sign-off + attribution on every commit: `git commit -s` and trailer `Assisted-by: Claude:claude-opus-4-8`.
- `limit` is per originating-die chain depth, not a pool-wide total.

---

## File Structure

- `crates/diceman/src/ast.rs` — `Explode` variant gains `limit: Option<u32>`.
- `crates/diceman/src/parser.rs` — `explode_modifier` parses the cap; test module updated.
- `crates/diceman/src/roller.rs` — `apply_explode` enforces the cap; call site + test modules updated.
- `crates/diceman/src/format.rs` — renders the cap.
- `README.md`, `crates/diceman-cli/src/main.rs` — notation docs.

---

## Task 1: Add `limit` field to the AST and thread `None` through all sites

**Goal:** Add the field with no behavior change; the whole workspace compiles and every existing test passes with `limit: None`.

**Files:**
- Modify: `crates/diceman/src/ast.rs:335-343` (add field)
- Modify: `crates/diceman/src/parser.rs:542` (real construction site) + test sites at lines 803, 824, 848, 869, 893, 914, 935, 959
- Modify: `crates/diceman/src/roller.rs:458` (destructure at call site) + test sites at lines 1534, 1557, 1580, 1604, 1628, 1653, 1681, 1716, 1734, 2217, 2240, 2266
- Modify: `crates/diceman/src/format.rs:165` (match destructure)

**Interfaces:**
- Produces: `RollModifier::Explode { compounding: bool, penetrating: bool, limit: Option<u32>, condition: Option<Condition> }`.

- [ ] **Step 1: Add the field to the AST variant**

In `crates/diceman/src/ast.rs`, change the `Explode` variant to:

```rust
    /// Explode dice matching the condition.
    Explode {
        /// If true, add explosions to same die (compounding/Shadowrun).
        /// If false, create new dice for each explosion (standard/Roll20).
        compounding: bool,
        /// If true, subtract 1 from each explosion roll's added value.
        penetrating: bool,
        /// Maximum explosions per originating die's chain. None = unlimited
        /// (bounded only by the internal runaway guard).
        limit: Option<u32>,
        /// The condition for explosion (defaults to max value).
        condition: Option<Condition>,
    },
```

- [ ] **Step 2: Run the build to see every broken construction site**

Run: `cargo build -p diceman 2>&1 | grep -E "missing field|error\[" | head`
Expected: FAIL — `missing field `limit`` at parser.rs:542, format.rs:165, roller.rs:458, and each test site listed above.

- [ ] **Step 3: Fix the real (non-test) construction and match sites**

`crates/diceman/src/parser.rs:542` — add `limit: None,` in canonical position:

```rust
        Ok(RollModifier::Explode {
            compounding,
            penetrating,
            limit: None,
            condition,
        })
```

`crates/diceman/src/format.rs:165` — add `limit,` to the destructure (binds it for later use; ignore for now — it is consumed in Task 4). To keep the build warning-free in this task, bind it as `limit: _`:

```rust
            RollModifier::Explode {
                compounding,
                penetrating,
                limit: _,
                condition,
            } => {
```

`crates/diceman/src/roller.rs:458` — add `limit: _,` to the destructure at the phase dispatch (it is consumed in Task 3):

```rust
                RollModifier::Explode {
                    compounding,
                    penetrating,
                    limit: _,
                    condition,
                } => {
                    self.apply_explode(dice, kind, *compounding, *penetrating, condition.as_ref())?;
                }
```

Note: the `modifier_phase` match at `roller.rs:415` uses `RollModifier::Explode { .. }` and needs no change.

- [ ] **Step 4: Fix every test construction site**

In each `RollModifier::Explode { ... }` literal in the test modules of `parser.rs` (lines ~803, 824, 848, 869, 893, 914, 935, 959) and `roller.rs` (lines ~1534, 1557, 1580, 1604, 1628, 1653, 1681, 1716, 1734, 2217, 2240, 2266), insert `limit: None,` between the `penetrating:` line and the `condition:` line. Example transform:

```rust
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                limit: None,
                condition: None,
            }],
```

- [ ] **Step 5: Verify the whole workspace compiles and all tests pass**

Run: `cargo test -p diceman 2>&1 | tail -20`
Expected: PASS — all existing tests green, zero warnings about `limit`.

- [ ] **Step 6: Commit**

```bash
git add crates/diceman/src/ast.rs crates/diceman/src/parser.rs crates/diceman/src/format.rs crates/diceman/src/roller.rs
git commit -s -m "feat(diceman): add explode limit field to AST (no behavior change)

Assisted-by: Claude:claude-opus-4-8"
```

---

## Task 2: Parse the bare-number explosion cap

**Goal:** `explode_modifier` reads an optional bare number after the compounding/penetrating markers into `limit`, before the condition.

**Files:**
- Modify: `crates/diceman/src/parser.rs:525-547` (`explode_modifier`)
- Test: `crates/diceman/src/parser.rs` (test module, add new tests)

**Interfaces:**
- Consumes: `RollModifier::Explode { .., limit: Option<u32>, .. }` from Task 1.
- Produces: parser now sets `limit` from notation like `!1`, `!!2`, `!p1`, `!2>4`, `!!p2>4`.

- [ ] **Step 1: Write failing tests**

Existing parser tests (see `parser.rs:794` `test_parse_explode`) assert the whole
`Expr::Roll(RollPlan::new_unchecked(DicePool { count, kind: DieKind::Number(6) }, vec![..modifiers..], ScoringMode::Sum, vec![]))`.
Mirror that exact shape. Add to the `parser.rs` test module:

```rust
    #[test]
    fn test_parse_explode_limit() {
        let expr = parse("1d6!1").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool { count: 1, kind: DieKind::Number(6) },
                vec![RollModifier::Explode {
                    compounding: false,
                    penetrating: false,
                    limit: Some(1),
                    condition: None,
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }

    #[test]
    fn test_parse_compounding_penetrating_limit_condition() {
        let expr = parse("1d6!!p2>4").unwrap();
        assert_eq!(
            expr,
            Expr::Roll(RollPlan::new_unchecked(
                DicePool { count: 1, kind: DieKind::Number(6) },
                vec![RollModifier::Explode {
                    compounding: true,
                    penetrating: true,
                    limit: Some(2),
                    condition: Some(Condition {
                        compare: Compare::GreaterThan,
                        value: 4,
                    }),
                }],
                ScoringMode::Sum,
                vec![],
            ))
        );
    }
```

(No separate `limit: None` parse test is needed here — the existing `test_parse_explode` already asserts `1d6!` and Task 1 updated it to include `limit: None`.)

Note: `Token::Number` carries a `u32`, so `limit: Some(n)` needs no cast.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p diceman test_parse_explode_limit test_parse_compounding_penetrating_limit_condition 2>&1 | tail -20`
Expected: FAIL — `limit` is `None` (parser does not yet read the number), assertion mismatch on `test_parse_explode_limit` and the compounding one.

- [ ] **Step 3: Implement the limit parse**

In `crates/diceman/src/parser.rs`, update `explode_modifier` to read the optional number between penetrating and condition:

```rust
    /// Parse an explode modifier (!, !!, !p, !!p, !2, !>5, !!p2>5).
    fn explode_modifier(&mut self) -> Result<RollModifier> {
        let compounding = if self.current == Token::Explode {
            self.advance()?;
            true
        } else {
            false
        };

        let penetrating = if self.current == Token::P {
            self.advance()?;
            true
        } else {
            false
        };

        let limit = if let Token::Number(n) = self.current {
            self.advance()?;
            Some(n)
        } else {
            None
        };

        let condition = self.optional_condition()?;

        Ok(RollModifier::Explode {
            compounding,
            penetrating,
            limit,
            condition,
        })
    }
```

Note: `Token::Number(u32)` matches `limit: Option<u32>` directly — no cast needed.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p diceman explode 2>&1 | tail -20`
Expected: PASS — new limit tests pass and all existing explode parser tests still pass.

- [ ] **Step 5: Commit**

```bash
git add crates/diceman/src/parser.rs
git commit -s -m "feat(diceman): parse bare-number explosion cap (1d6!1)

Assisted-by: Claude:claude-opus-4-8"
```

---

## Task 3: Enforce the cap in the roller

**Goal:** `apply_explode` stops a chain when it reaches the user `limit` — a quiet stop, no error. `MAX_EXPLOSIONS` still guards and errors when `limit` is `None` or too large.

**Files:**
- Modify: `crates/diceman/src/roller.rs:458-464` (pass `limit` to `apply_explode`)
- Modify: `crates/diceman/src/roller.rs:805-870` (`apply_explode` signature + loop)
- Test: `crates/diceman/src/roller.rs` (test module, add new tests)

**Interfaces:**
- Consumes: `RollModifier::Explode { .., limit, .. }`.
- Produces: `fn apply_explode(&mut self, dice, kind, compounding: bool, penetrating: bool, limit: Option<u32>, condition: Option<&Condition>) -> Result<()>`.

- [ ] **Step 1: Write failing tests**

Existing roller tests (see `roller.rs:1528` `test_evaluate_penetrating_explode`) build a
`RollPlan::new_unchecked(DicePool { count, kind: DieKind::Number(6) }, vec![..], ScoringMode::Sum, vec![])`,
wrap it in `Expr::Roll(plan)`, roll with `evaluate_with_rng(&expr, &mut TestRng::new(vec![..]))`,
and assert `result.outcome == RollOutcome::Numeric(N)`. The summed outcome proves the cap:
with an all-max `TestRng`, an *uncapped* chain would run to `MAX_EXPLOSIONS` and error, so a
finite `Numeric` total is itself evidence the cap stopped the chain quietly. Mirror that shape:

```rust
    #[test]
    fn test_evaluate_standard_explode_limit_once() {
        // 1d6!1, TestRng all 6: initial 6 explodes ONCE into a new 6, then the
        // cap stops the chain quietly. Sum = 6 + 6 = 12, no error.
        let plan = RollPlan::new_unchecked(
            DicePool { count: 1, kind: DieKind::Number(6) },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                limit: Some(1),
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6, 6, 6, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(12));
    }

    #[test]
    fn test_evaluate_compounding_explode_limit_two() {
        // 1d6!!2, TestRng all 6: compound exactly twice → 6 + 6 + 6 = 18, no error.
        let plan = RollPlan::new_unchecked(
            DicePool { count: 1, kind: DieKind::Number(6) },
            vec![RollModifier::Explode {
                compounding: true,
                penetrating: false,
                limit: Some(2),
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6, 6, 6, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(18));
    }

    #[test]
    fn test_evaluate_penetrating_explode_limit_once() {
        // 1d6!p1, TestRng all 6: natural 6 explodes once; penetrating subtracts 1
        // from the added value → new die = 6 - 1 = 5, then the cap stops. Sum = 11.
        let plan = RollPlan::new_unchecked(
            DicePool { count: 1, kind: DieKind::Number(6) },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: true,
                limit: Some(1),
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6, 6, 6, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(11));
    }

    #[test]
    fn test_evaluate_explode_limit_zero_no_explosion() {
        // 1d6!0 → cap 0 → no explosion. Sum = the single natural face = 6.
        let plan = RollPlan::new_unchecked(
            DicePool { count: 1, kind: DieKind::Number(6) },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                limit: Some(0),
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(6));
    }
```

Note: use the real names from the existing roller tests (`RollPlan::new_unchecked`, `DicePool`, `DieKind::Number`, `ScoringMode::Sum`, `Expr::Roll`, `evaluate_with_rng`, `RollOutcome::Numeric`, `TestRng::new`) — do not invent names.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p diceman explode_limit 2>&1 | tail -20`
Expected: FAIL by panic — the roller still ignores `limit` (Task 1 left the call site binding it as `limit: _`), so the all-max `TestRng` drives an uncapped chain into the `MAX_EXPLOSIONS` guard, which returns `Err(ExplodeLimit)` and makes `.unwrap()` panic. (The tests compile; they fail at runtime, which is the red.)

- [ ] **Step 3: Thread `limit` through the call site**

`crates/diceman/src/roller.rs:458`:

```rust
                RollModifier::Explode {
                    compounding,
                    penetrating,
                    limit,
                    condition,
                } => {
                    self.apply_explode(
                        dice,
                        kind,
                        *compounding,
                        *penetrating,
                        *limit,
                        condition.as_ref(),
                    )?;
                }
```

- [ ] **Step 4: Add `limit` to `apply_explode` and enforce it**

`crates/diceman/src/roller.rs:805` — new signature and loop guard:

```rust
    fn apply_explode(
        &mut self,
        dice: &mut Vec<DieResult>,
        kind: &DieKind,
        compounding: bool,
        penetrating: bool,
        limit: Option<u32>,
        condition: Option<&Condition>,
    ) -> Result<()> {
```

Inside the `while condition.compare.check(...)` loop, keep the existing `MAX_EXPLOSIONS` guard and add the user-cap stop *before* it. Replace the top of the loop body:

```rust
            while condition.compare.check(current_value, condition.value) {
                if let Some(max) = limit {
                    if explode_count >= max {
                        break;
                    }
                }
                if explode_count >= MAX_EXPLOSIONS {
                    return Err(Error::ExplodeLimit(MAX_EXPLOSIONS));
                }
```

The rest of the loop body (roll, penetrating adjust, compound/standard append, `explode_count += 1`) is unchanged.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p diceman explode 2>&1 | tail -20`
Expected: PASS — new limit tests pass; existing explode tests (including `test_explode_limit` / `test_standard_explode_limit` which use `limit: None` and still expect `Error::ExplodeLimit`) stay green.

- [ ] **Step 6: Commit**

```bash
git add crates/diceman/src/roller.rs
git commit -s -m "feat(diceman): enforce per-chain explosion cap in roller

Assisted-by: Claude:claude-opus-4-8"
```

---

## Task 4: Render the cap in the formatter

**Goal:** `format.rs` renders `limit` as a bare number after `p` and before the condition, so parse → format → parse round-trips.

**Files:**
- Modify: `crates/diceman/src/format.rs:165-181`
- Test: `crates/diceman/src/roller.rs` test module (the notation string is reconstructed via `modifiers_str` and surfaces on `RollResult.expression`, so a parse→roll→inspect-string round-trip is the natural test and needs no access to the private formatter).

**Interfaces:**
- Consumes: `RollModifier::Explode { .., limit, .. }`.

- [ ] **Step 1: Write failing round-trip tests**

`format_standard_roll` builds the notation prefix from `modifiers_str(plan)`, and that
prefix appears at the start of `RollResult.expression` (e.g. `1d6!1[6, 6] = 12`). So parse
the capped notation, roll it, and assert the rendered expression begins with the same
notation. Add to the `roller.rs` test module:

```rust
    #[test]
    fn test_format_roundtrip_explode_limit() {
        let expr = parse("1d6!1").unwrap();
        let mut rng = TestRng::new(vec![6, 6, 6, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(
            result.expression.starts_with("1d6!1"),
            "expected expression to start with 1d6!1, got: {}",
            result.expression
        );
    }

    #[test]
    fn test_format_roundtrip_explode_limit_full() {
        let expr = parse("1d6!!p2>4").unwrap();
        let mut rng = TestRng::new(vec![6, 6, 6, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(
            result.expression.starts_with("1d6!!p2>4"),
            "expected expression to start with 1d6!!p2>4, got: {}",
            result.expression
        );
    }
```

Note: `parse` and `evaluate_with_rng` are the same helpers the surrounding roller tests use;
confirm `parse` is in scope in this test module (it is used by other tests in the file). If
`parse` is not imported in `roller.rs`'s test module, place these two tests in `parser.rs`'s
test module instead, where `parse` is already in scope, and import `evaluate_with_rng`/`TestRng`
the same way the roller tests do.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p diceman roundtrip_explode_limit 2>&1 | tail -20`
Expected: FAIL — the rendered expression omits the cap (`1d6![...]` instead of `1d6!1[...]`, and `1d6!!p>4[...]` instead of `1d6!!p2>4[...]`), so `starts_with` is false.

- [ ] **Step 3: Render the limit**

`crates/diceman/src/format.rs:165` — add the limit between the `p` push and the condition:

```rust
            RollModifier::Explode {
                compounding,
                penetrating,
                limit,
                condition,
            } => {
                let mut s = "!".to_string();
                if *compounding {
                    s.push('!');
                }
                if *penetrating {
                    s.push('p');
                }
                if let Some(n) = limit {
                    s.push_str(&n.to_string());
                }
                if let Some(c) = condition {
                    s.push_str(&format!("{}{}", c.compare, c.value));
                }
                s
            }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p diceman 2>&1 | tail -20`
Expected: PASS — round-trip stable for capped forms; all prior tests green.

- [ ] **Step 5: Commit**

```bash
git add crates/diceman/src/format.rs
git commit -s -m "feat(diceman): render explosion cap in formatter

Assisted-by: Claude:claude-opus-4-8"
```

---

## Task 5: Document the notation

**Goal:** README notation table and CLI `notation` help list the cap forms.

**Files:**
- Modify: `README.md` (Exploding Dice section, ~lines 104-129)
- Modify: `crates/diceman-cli/src/main.rs` (notation help, ~lines 517-532)

- [ ] **Step 1: Update the README**

In `README.md`, in the Exploding Dice section, add a cap row/examples. After the existing condition table add:

```markdown
Explosions can be capped with a bare number after the marker(s):

| Notation | Meaning |
|----------|---------|
| `1d6!1` | Explode at most once (e.g. Kal-Arath) |
| `1d6!!2` | Compounding, at most twice |
| `1d6!p1` | Penetrating, at most once |
| `1d6!2>4` | At most twice, on rolls greater than 4 |

Without a number, explosions are unlimited (bounded only by an internal safety guard).
```

- [ ] **Step 2: Update the CLI notation help**

In `crates/diceman-cli/src/main.rs` around the explode help block (lines ~517-532), add lines mirroring the existing style:

```
  !N        Cap explosions at N times (e.g. 1d6!1 explodes at most once)
  !!N       Compounding, capped at N
  !pN       Penetrating, capped at N
```

Match the exact column alignment of the surrounding help lines.

- [ ] **Step 3: Verify the CLI still builds and the help renders**

Run: `cargo run --bin diceman -- notation 2>&1 | grep -A1 "Cap explosions"`
Expected: the new cap lines appear in the notation output.

- [ ] **Step 4: Sanity-check end-to-end**

Run: `cargo run --bin diceman -- roll "1d6!1" 2>&1`
Expected: a valid roll with at most one explosion (no error).

- [ ] **Step 5: Commit**

```bash
git add README.md crates/diceman-cli/src/main.rs
git commit -s -m "docs(diceman): document capped exploding dice notation

Assisted-by: Claude:claude-opus-4-8"
```

---

## Self-Review Notes

- **Spec coverage:** AST field (T1), notation/parse (T2), roller cap + quiet-stop-vs-error distinction (T3), format round-trip (T4), README + CLI docs (T5), edge cases `!0` and `limit: None` (T3 tests + T1 regression). All spec sections mapped.
- **Type consistency:** field order `compounding, penetrating, limit, condition` used in every task. `apply_explode` signature adds `limit: Option<u32>` in the position matching the call site in T3.
- **Real patterns confirmed (no placeholders):** parser/roller tests assert full `Expr::Roll(RollPlan::new_unchecked(DicePool { count, kind: DieKind::Number(n) }, vec![..], ScoringMode::Sum, vec![]))`; roller behavior is checked via `evaluate_with_rng(&expr, &mut TestRng::new(..))` and `result.outcome == RollOutcome::Numeric(N)`; format is checked via `result.expression.starts_with(..)`. `Token::Number` is `u32` (no cast). `format.rs` has no test module — format tests live in the roller/parser test module.
