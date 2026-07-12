# Capped Exploding Dice — Design

Bead: `diceman-wys` — support ability for a die to only explode X times.

## Motivation

Some systems cap how many times a die may explode. The reference case is
Kal-Arath, where a die can explode at most once. The current explode modifier
chains indefinitely (bounded only by the internal `MAX_EXPLOSIONS = 100` runaway
guard), so there is no way to express "explode at most N times."

## Notation

A bare number immediately after the explode marker(s) is the explosion cap. The
condition grammar always requires a comparison operator (`=`, `<`, `>`), so a
bare number after `!` is unused notation space today and is unambiguous as a cap.

Canonical order of an explode modifier: `!`/`!!` → `p` → limit → condition.

| Notation   | Meaning                                                        |
|------------|----------------------------------------------------------------|
| `1d6!1`    | standard, explode at most once (Kal-Arath)                     |
| `1d6!!2`   | compounding, explode at most twice                             |
| `1d6!p1`   | penetrating, explode at most once                              |
| `1d6!2>4`  | explode at most twice, triggering on rolls > 4                 |
| `1d6!!p2>4`| compounding penetrating, cap 2, on rolls > 4                   |
| `1d6!`     | unchanged: unlimited (guarded by `MAX_EXPLOSIONS`)             |

## AST

Add one field to the existing `RollModifier::Explode` variant (`ast.rs`):

```rust
Explode {
    compounding: bool,
    penetrating: bool,
    limit: Option<u32>,   // None = unlimited (current behavior); Some(n) = at most n explosions per chain
    condition: Option<Condition>,
}
```

`limit` is the number of explosions permitted per originating die's chain, not
a pool-wide total — consistent with how `MAX_EXPLOSIONS` already bounds a single
chain's depth rather than total pool size.

## Lexer

No change. `!` already produces `Token::Explode` and integers already produce
`Token::Number`.

## Parser

In `explode_modifier` (`parser.rs`), after parsing the compounding markers and
the `p` penetrating flag, parse an optional bare `Token::Number` as `limit`,
then parse the optional condition. Ordering is fixed and unambiguous because a
condition always begins with an operator.

## Roller

In `apply_explode` (`roller.rs`), thread `limit` through. The per-chain loop
stops when `explode_count` reaches the user's `limit` — a **normal stop, not an
error**. The global `MAX_EXPLOSIONS` guard remains as the runaway safety net that
returns `Error::ExplodeLimit`.

Distinction that must be preserved:
- Reaching the user-specified `limit`: quiet stop, chain ends, no error.
- Reaching `MAX_EXPLOSIONS`: `Error::ExplodeLimit` (unchanged runaway guard).

When `limit` is `None`, behavior is exactly as today.

## Format

In `format.rs`, render `limit` as a bare number in canonical position (after
`p`, before the condition) so parse → format round-trips.

## Semantics / edge cases

- `1d6!0` → cap 0 → no explosions (loop body never entered). Deterministic and
  valid; needs no special-case validation.
- `limit` ≥ `MAX_EXPLOSIONS`: the safety net still guards and errors if actually
  hit.
- `1d6!` (no number) → `limit: None`, unchanged unlimited behavior.

## Testing

Each unit is developed test-first (red → green):

- **Parser**: `!1`, `!!2`, `!p1`, `!2>4`, `!!p2>4` parse to the expected AST with
  correct `limit`; `!` yields `limit: None`.
- **Roller**: with a `TestRng` that always returns max, `1d6!1` explodes exactly
  once and returns without error; `1d6!!2` compounds exactly twice; `1d6!0`
  produces no explosion; `1d6!` still hits `MAX_EXPLOSIONS` → `Error::ExplodeLimit`.
- **Format round-trip**: parse → format → parse is stable for each capped form.
- Existing explode tests continue to pass (backward compatibility of `limit: None`).

## Docs

- README notation table: add the cap forms.
- CLI `notation` help (`diceman-cli`): add the cap forms.

## Blast radius

Every construction site of `RollModifier::Explode { .. }` gains the `limit`
field. These are: `parser.rs`, `roller.rs`, `format.rs`, and their test modules.
The Python bindings parse notation strings and do not construct the variant
directly, so they are unaffected beyond gaining the notation for free.
