# Critical Success/Failure Markers (cs/cf)

**Bead:** DM-676
**Status:** Design Complete
**Date:** 2026-01-02

## Overview

Add `cs` (critical success) and `cf` (critical failure) markers to dice notation. These are display-only annotations that mark special dice results in output without affecting the roll calculation.

## Syntax

```
1d20cs20cf1      # Mark 20s as crit success, 1s as crit fail (exact match)
1d20cs>=19cf1    # Expanded crit range: 19 or 20 is crit success
1d20cs20         # Just crit success, no crit fail marker
1d20cf1          # Just crit fail, no crit success marker
```

### Condition Operators

Reuses existing `Compare` enum:

- `cs20` or `cs=20` - exact match (Equal)
- `cs>N`, `cs>=N`, `cs<N`, `cs<=N` - range comparisons

### Restrictions

- Cannot combine with success counting (`5d10>=8cs10` is an error)
- Order in notation doesn't matter (`cs20cf1` = `cf1cs20`)
- Multiple cs or cf in same roll is an error (`cs20cs19` invalid)

## Output Format

### Text Output

```
1d20cs20cf1[17] = 17           # No crits
1d20cs20cf1[20**] = 20         # Critical success
1d20cs20cf1[1*] = 1            # Critical failure
4d6cs6cf1[6**, 4, 3, 1*] = 14  # Both in same roll
```

- `**` suffix = critical success
- `*` suffix = critical failure

### JSON Output

```json
{
  "dice": [
    {"value": 20, "is_crit_success": true, "is_crit_failure": false},
    {"value": 1, "is_crit_success": false, "is_crit_failure": true}
  ]
}
```

## AST Changes

Modify `Roll` struct in `ast.rs`:

```rust
pub struct Roll {
    pub count: u32,
    pub sides: Sides,
    pub modifiers: Vec<Modifier>,
    pub crit_success: Option<Condition>,  // NEW
    pub crit_failure: Option<Condition>,  // NEW
}
```

cs/cf are not added to the `Modifier` enum since they don't transform dice values.

## Parser Changes

### New Tokens

```rust
pub enum Token {
    // ... existing tokens ...
    CritSuccess,  // "cs"
    CritFail,     // "cf"
}
```

### Parsing Logic

After parsing modifiers, check for cs/cf markers. Accept them in any order. Default to `Equal` comparison when just a number is provided (`cs20` = `cs=20`).

## Modifier Order

cs/cf is not a mechanical modifier. The order remains:

`reroll → explode → keep/drop`

Then cs/cf conditions are evaluated at display time when formatting output.

## Error Handling

| Condition | Error Message |
|-----------|---------------|
| `cs` without value | `"Expected number or comparison after 'cs'"` |
| Duplicate `cs` | `"Duplicate critical success marker"` |
| Duplicate `cf` | `"Duplicate critical failure marker"` |
| `cs` with success counting | `"Critical markers cannot be combined with success counting"` |

### Valid Edge Cases

- `cs1cf1` - Same value for both (crit success wins display)
- `cs>=1` - Matches everything (pointless but valid)
- `4d6kh3cs6` - cs/cf with keep/drop (marks surviving dice)
- `1d6!cs6` - cs/cf with exploding (marks all 6s including explosions)

## Testing Strategy

1. **Lexer tests:** Tokenization of cs/cf with conditions
2. **Parser tests:** AST construction, error cases, order independence
3. **Output formatting tests:** Crit markers in text and JSON output
4. **Integration tests:** Full round-trip through CLI
