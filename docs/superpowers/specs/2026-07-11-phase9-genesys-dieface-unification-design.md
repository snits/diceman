# Phase 9 Design: Genesys/Star Wars Narrative Dice + Unified Special-Face Model

**Date:** 2026-07-11
**Status:** Reviewed (type-design + architecture review findings incorporated)
**Bead:** diceman-dqh
**Prior art:** docs/diceman-refactor.md (Phase 9 sketch), docs/diceman-phase8-marvel-plan.md (§3.1 positional-M rationale, superseded by this design)

## 1. Goal

Implement Genesys / Star Wars narrative dice through the existing generic
pipeline, AND unify the "special face" mechanism so Marvel's M and Genesys
symbols share one `DieFace` model. Jerry's ratified decision (bead, 2026-06-30):
unify — retrofit Marvel onto the symbol/face model so there is one mechanism.

Constraint from the bead: unification removes the *positional encoding of which
face is special*. It does NOT move Marvel's pool-level resolution (auto-fail on
1/M/1, rank-7 for Edge/Trouble, M's 6-or-1 contribution) out of scoring — that
logic is irreducibly pool-contextual and stays in `score()` / the modifier pass.

Scope call: the Force die (and its Light/Dark axis) is IN scope. The bead and
refactor doc name "Star Wars narrative dice" alongside Genesys, and the Force
die is the only Star Wars-specific die; omitting it would leave "Star Wars
support" hollow.

## 2. Design decisions

### 2.1 DieFace shape (the bead's headline question)

Candidates considered:

- **(a) Struct face** `DieFace { value: Option<i64>, symbols: SymbolPool }` —
  literal reading of "optional symbol set AND optional numeric contribution."
  Rejected: every numeric die pays the churn; no face in scope carries both a
  numeric value and symbols; the both-and-neither corners are dead states.
- **(b) Enum with a combined variant** `Symbols { symbols, value: Option<i64> }` —
  keeps `Numeric` cheap, allows a future face with both. Rejected: the `value`
  field would ship dead. No in-scope face has an intrinsic number to carry —
  Genesys faces have none, and M's contribution is 6-or-1 *pool-contextual*
  (roller.rs:587-595), which even `(b)` could not store on the face. That is
  the strongest argument for (c): the face's job is identity, and any numeric
  meaning a special face has is resolved in scoring by design.
- **(c) Enum, symbols-only variant (CHOSEN):**

```rust
pub enum DieFace {
    Numeric(i64),
    Symbols(SymbolPool),
}
```

(Variant named `Symbols` matching the bead and refactor doc.) *Which face is
special* is carried by the face itself (M is `Symbols` containing
`Symbol::Marvel`), not by pool position. Moving to (b) later is a mechanical
breaking refactor (tuple → struct variant, every match arm + serde shape), not
a compatible extension — acceptable for a v0.x crate and not a reason to ship
a dead field now.

**`DieFace::as_numeric()` changes `i64 -> Option<i64>` (breaking, v0.5.0).**
The migration has two distinct classes of call site — implementers must not
blur them:

1. **Numeric-by-construction sites** (Sum/CountSuccesses/DigitConcatenate
   scoring at roller.rs:557,566,573; crit checks roller.rs:641,644; reroll /
   explode / keep-drop sorts; format success markers): faces are `Numeric`
   because parser validation pairs those scoring modes and modifiers only with
   numeric die kinds. These use a private `DieFace::numeric_value(self) -> i64`
   helper carrying the contract panic message once, instead of scattering
   `.expect()` strings.
2. **Marvel identity sites — genuine face-matching rewrites, NEVER
   `numeric_value`** (a `.expect()` here would panic exactly when M shows):
   - `score()` Marvel arm middle-die extraction (roller.rs:583-585) and
     `marvel_facts` (roller.rs:613-619): match the face — `Symbols` containing
     `Marvel` ⇒ M (contribute 6, or 1 on auto-fail); `Numeric(n)` ⇒ n.
   - `marvel_rank` (roller.rs:478) becomes `rank(face)`: M face ⇒ 7, numeric ⇒
     value; `lowest/highest_rank_index` (roller.rs:534,543) pass faces, not
     extracted i64s.
   - ChaseFantastic guard (roller.rs:495): "middle die is not the M face"
     replaces `as_numeric() != 1`.
   - `apply_edge`/`apply_trouble` old/new face handling (roller.rs:502-508,
     521-527) — see §2.3.
   - `format_marvel_roll` (format.rs:49) renders via face identity/`Display`.

### 2.2 Symbol and SymbolPool

```rust
pub enum Symbol {
    Success, Advantage, Triumph,
    Failure, Threat, Despair,
    Light, Dark,
    Marvel,           // the Marvel Multiverse M
}

/// Multiset of symbols on a face (or aggregated across faces).
/// Copy: fixed-size count array indexed by Symbol.
pub struct SymbolPool { counts: [u8; 9] }
```

- Constructors/helpers: `SymbolPool::new()`, `of(&[Symbol])`, `count(Symbol)`,
  `add(Symbol, n)`, `merge(&SymbolPool)`, `contains(Symbol)`, `is_empty()`.
- `Copy + Eq + Hash`-able; keeps `DieFace` `Copy` (9 bytes of counts).
- Counts saturate on add/merge (`u8::MAX` cap). Realistic pools are <12 dice
  with ≤2 symbols per face; saturation is a documented safety net, not a
  behavior anyone should reach.
- Serde: custom `Serialize`/`Deserialize` as a self-describing map of symbol
  name → count with zero counts omitted (e.g. `{"Success":1,"Advantage":2}`),
  NOT the opaque positional array the derive would emit. The JSON die-face
  shape is a public CLI surface.
- `Symbol` spans two disjoint systems (Genesys/SW symbols vs Marvel's M);
  pools from the two systems never merge — plan validation keeps Marvel and
  narrative dice out of each other's scoring modes, and `SymbolsOutcome` has
  no Marvel field by design.
- A Genesys blank face is `Symbols(SymbolPool::new())` — a real face that
  rolled, with nothing on it.
- Marvel's M face is `Symbols(SymbolPool::of(&[Symbol::Marvel]))`.

### 2.3 Marvel retrofit

Positional encoding is removed from *interpretation* sites; die *production*
keeps position-awareness because that is physical reality (the middle die of a
d616 pool IS the Marvel die — it has an M face; the outer dice don't).

- A per-index Marvel face producer (`roll 1..=6; index 1 && raw 1 ⇒
  Symbols({Marvel}); else Numeric(raw)`) is used by BOTH `roll_pool` for
  `MarvelD6` groups AND every Edge/Trouble reroll. `apply_edge`/`apply_trouble`
  currently hard-write `DieFace::Numeric(new_face)` into `.face` and
  `.history` (roller.rs:506,508,525,527); they must thread through the
  producer so a rerolled natural 1 on index 1 becomes the M face — in history
  too. An outer die (index 0/2) rolling 1 stays `Numeric(1)`, rank 1.
- Rank comparisons in Edge/Trouble compare produced faces via `rank(face)`
  (M ⇒ 7), replacing the extract-then-rank-by-index dance.
- `marvel_facts` / scoring: `m_shown` = middle face is the M face; `auto_fail`
  = faces are `[Numeric(1), M, Numeric(1)]`; M contributes 6, or 1 on
  auto-fail — unchanged pool-contextual logic, keyed off face identity.
- **Behavior invariance, stated precisely:** all runtime verdicts (totals,
  auto_fail, m_shown, Edge/Trouble selection, every exhaustive-enumeration
  distribution oracle at roller.rs:2175-2249) are preserved. Two
  representation-level assertions change and must be updated, not deleted:
  roller.rs:2164 (`dice[1].face == Numeric(1)` ⇒ the M face) and
  roller.rs:2166-2168 (history `[Numeric(1), Numeric(1)]` ⇒ two M faces).
  JSON die-face shape for the M face changes accordingly — breaking, flagged.
- `MarvelOutcome`, `MarvelCheck`, Edge/Trouble notation, and typed Marvel APIs
  are unchanged.

### 2.4 Narrative dice

```rust
pub enum DieKind {
    Number(u32), Percent, Fudge, MarvelD6,
    Narrative(NarrativeDie),
}

pub enum NarrativeDie {
    Boost,       // blue d6
    Setback,     // black d6
    Ability,     // green d8
    Difficulty,  // purple d8
    Proficiency, // yellow d12
    Challenge,   // red d12
    Force,       // white d12 (Star Wars)
}
```

Face tables are const data in the roller (`NarrativeDie::face(roll) ->
SymbolPool`), verified against published dice (Appendix A). `DieKind::count()`
returns 6/6/8/8/12/12/12.

### 2.5 Mixed pools

A Genesys check mixes die kinds in one pool (2 Ability + 1 Proficiency + 2
Difficulty + …). Candidates:

- **(a) Parallel narrative path** (separate plan/evaluator) — rejected: the
  contained-evaluator anti-pattern Phase 8 explicitly retired.
- **(b) Heterogeneous DicePool** `{ groups: Vec<(u32, DieKind)> }` — honest but
  forces churn/allocation onto every numeric roll site.
- **(c) RollPlan holds pool groups (CHOSEN):** internal storage becomes
  `pools: Vec<DicePool>` (invariant: non-empty; `len > 1` only when every
  group is `DieKind::Narrative`).

Churn containment (the getter/constructor strategy):

- `pools() -> &[DicePool]` replaces the `pool()` getter (breaking; call sites:
  roller.rs:328-329, format.rs:69,131,139,150,207,218, parser tests).
- `RollPlan::new(pool, ...)` keeps its single-pool signature (existing
  callers/tests compile modulo the validation additions); a new
  `RollPlan::new_narrative(pools, ...)` takes the group list. Internally both
  fill `pools`. `new_unchecked(pool, ...)` likewise stays single-pool; a
  `pub(crate) new_unchecked_pools(Vec<DicePool>, ...)` serves the parser's
  narrative arm.
- `evaluate_roll`/`roll_pool` iterate the groups and concatenate `DieResult`s
  in group order, then score once over the flat slice.

**Where validation actually lives (corrected attribution):** the production
path is the parser — `validate_marvel_roll` (parser.rs:160) today, joined by a
`validate_narrative_roll` mirror. `RollPlan::new` is the *external-caller*
surface (the parser and roller use `new_unchecked`); it mirrors the same
checks so library consumers constructing plans programmatically get the same
`Result`-based runtime enforcement. None of this is type-level; the type-level
piece of this design is `DieFace` itself carrying face identity.

Invariants (enforced in both `validate_narrative_roll` and `RollPlan::new*`):

- `ScoringMode::SymbolCancel` ⟺ all groups are `Narrative` (both directions).
- Narrative plans: no modifiers (Genesys has no keep/reroll/explode/Edge);
  every group count ≥ 1; annotation rules exactly `[Triumph, Despair]`.
- Non-narrative plans: exactly one pool group; no Triumph/Despair rules.
- New `Error::InvalidNarrativeRoll(String)` mirroring `InvalidMarvelRoll`.

What keeps symbolic faces out of numeric-only scoring arms is this validation
plus production: numeric kinds produce only `Numeric` faces; narrative kinds
are locked to `SymbolCancel`; MarvelD6 is locked to `MarvelMultiverse`.

### 2.6 Scoring: SymbolCancel

`score()` merges all faces' SymbolPools (narrative pools have no dropped
dice — no drop-producing modifiers exist on them), then cancels:

```rust
pub struct SymbolsOutcome {
    pub successes: i64,   // net: (S + TR) - (F + D); negative = net failure
    pub advantages: i64,  // net: A - T; negative = net threat
    pub triumphs: u8,     // reported regardless of cancellation
    pub despairs: u8,
    pub light: u8,        // Force points; never cancel
    pub dark: u8,
}

pub enum RollOutcome {
    Numeric(i64), Successes(i64), Marvel(MarvelOutcome),
    Symbols(SymbolsOutcome),
}
```

`SymbolsOutcome` derives `Debug, Clone, Copy, PartialEq, Eq` + serde behind
the feature flag, matching `MarvelOutcome` (ast.rs:281-282), so `RollOutcome`
stays `Copy`/`Eq`.

Cancellation rules (verified, Appendix A): each Triumph adds one Success and
each Despair adds one Failure to the net computation, while the Triumph/Despair
counts themselves are reported uncancelled; Light/Dark never cancel. Reporting
*signed nets* is information-equivalent to the rulebook's max(0, …) pairs; "a
check succeeds iff net ≥ 1" is the consuming game's interpretation — diceman
reports facts and deliberately has no `success` boolean here.

`RollOutcome::as_numeric()` returns `None` for `Symbols` — the first real
producer for `Error::NonNumericOutcome` at the arithmetic seam (`2dAbility + 2`
errors; `evaluate_total`/sim over a narrative expression errors cleanly). The
"unreachable until Phase 9" comments at roller.rs:125-126 and 273-275 are
removed as part of wiring the real tests.

### 2.7 Annotations

`AnnotationRule::Triumph` / `AnnotationRule::Despair` (new variants),
auto-pushed by the parser for narrative pools (Marvel precedent).
`apply_annotations` keeps its `(dice, rules, scoring)` signature and
*re-derives* triumph/despair presence by re-merging the faces — exactly the
`marvel_facts` pattern (dice are the source of truth; no outcome threading).
It pushes `Annotation::Triumph` / `Annotation::Despair` (new variants) once
each when present (pool-level style, matching `Fantastic`).

### 2.8 Notation surface

Full-word die names joined by `&` into one roll plan:

```
2dAbility&1dProficiency&2dDifficulty&1dSetback
1dForce
```

**Lexer.** Word tokens follow the `marvel` pattern, case-insensitive. Letter
dispositions (verified against lexer.rs:99-217):

- `A`bility, `B`oost, `S`etback: first letters currently unused — plain word
  match.
- `F`+`orce` vs `Fudge`, `D`+`ifficulty` vs `DigitD`, `C`+`h`allenge vs
  `cs`/`cf`: one-char peek suffices — in today's grammar those second
  characters are errors, so no valid notation changes meaning.
- **`P`roficiency vs penetrating-`p`: one-char peek is NOT safe.** `1d6!pr`
  is valid today (penetrating explode + reroll: parser.rs:401 consumes `!p`,
  the modifier loop takes `r`), so `p`+peek-`r` must not commit to a word.
  Use full-word lookahead-with-restore: attempt to match `roficiency` on a
  cloned iterator; on full match emit the word token, on any mismatch emit
  `Token::P` having consumed only the `p`. Regression test `1d6!pr` required.
- `&` becomes `Token::Ampersand`.

**Parser.** `&` is not a BinOp — it is a pool-union production inside the roll
factor, entered only after the first group's kind parses as narrative:
`narrative_roll := group ('&' group)*` where `group := count 'd' narrative_kind`.
Semantics:

- Binds tighter than all arithmetic (it never leaves the roll production);
  `2dAbility&1dBoost+2` parses as `(2dAbility&1dBoost) + 2` and then fails at
  evaluation with `NonNumericOutcome` (the seam under test).
- `2dAbility&3d6` (second group non-narrative) → `InvalidNarrativeRoll` at
  parse time. `3d6&…` (first group non-narrative) → the roll production ends
  before `&`; `parse()`'s end-of-input check yields the existing
  `Error::Expected` (current behavior for trailing garbage).
- Duplicate groups (`2dAbility&1dAbility`) are explicitly allowed — they just
  add dice to the pool.
- Whitespace around `&` follows the lexer's existing whitespace handling.
- No modifiers, success-counting conditions, or crit markers on narrative
  groups; scoring is forced to `SymbolCancel`; `[Triumph, Despair]`
  auto-pushed.
- Compact single-letter color notation (`ggypp`) rejected for v1: collides
  with existing modifier letters (`k`, `r`, `p`) and diceman's `NdX` grammar;
  the CLI subcommand supplies the ergonomic entry point.

### 2.9 Formatter

- `format_roll` becomes exhaustive over `RollOutcome` variants (kills the
  `_`-arm `.expect()` latent panic flagged in the bead comment): `Marvel` →
  `format_marvel_roll`, `Symbols` → new `format_narrative_roll`,
  `Numeric(n) | Successes(n)` → existing numeric paths with `n` bound
  directly. The inner `plan.scoring()` match in the numeric arm gains
  `SymbolCancel => unreachable!()` (routed via the outer `Symbols` arm) to
  stay exhaustive.
- Face display (`Display for DieFace`): numeric as today; symbol faces as
  concatenated abbreviations — `S` Success, `A` Advantage, `Tr` Triumph,
  `F` Failure, `Th` Threat, `De` Despair, `L` Light, `Dk` Dark, `M` Marvel;
  blank face renders `-`.
- Narrative roll rendering:
  `2dAbility&1dDifficulty[S, SA | Th] = 1 success, 1 advantage, 1 threat`
  — groups separated by ` | ` in pool order; outcome as a comma list of net
  facts (`N success(es)`/`N failure(s)` from the signed net, same for
  advantage/threat, plus `N triumph(s)` / `N despair(s)` / `N light` /
  `N dark` when nonzero); `wash` when everything nets to zero.
  Group boundaries are re-derived from `pools()` counts — valid precisely
  because narrative pools admit no dice-count-changing modifiers; if a
  narrative modifier is ever added, this derivation must be revisited (noted
  in code).

### 2.10 CLI

New `genesys` subcommand (Marvel precedent, long flags matching the `marvel`
subcommand style):

```
diceman genesys [--ability N] [--proficiency N] [--boost N]
                [--difficulty N] [--challenge N] [--setback N]
                [--force N] [--json]
```

Builds the notation string and runs the normal `roll` pipeline (no typed
Genesys API — not in bead scope; the notation covers it), prints the formatted
roll; `--json` serializes the `RollResult`. At least one die required.
`roll`/`sim` subcommands work unchanged (`sim` over narrative errors cleanly
with the NonNumericOutcome message). `diceman notation` gains a narrative
section.

### 2.11 Python bindings

Generic `roll()` gains an outcome kind `"symbols"` exposing `successes`,
`advantages`, `triumphs`, `despairs`, `light`, `dark`. No typed
`roll_genesys` API in this phase. `simulate()` over a narrative expression
raises the existing `ValueError` mapping of `NonNumericOutcome`.

### 2.12 Versioning and breaking changes

Core bumps to v0.5.0; workspace crates follow. Breaking surface:

- `DieFace::as_numeric` returns `Option<i64>`; new `DieFace::Symbols` variant.
- New `RollOutcome::Symbols` variant.
- `RollPlan::pool()` getter → `pools()`; `new_narrative` added.
- JSON shapes: M face serializes as `Symbols` with the symbol map (was
  `Numeric(1)`); new `symbols` outcome kind; `SymbolPool` map form.
- New `Error::InvalidNarrativeRoll` variant.

## 3. What does NOT change

- Marvel distribution oracles, Edge/Trouble semantics, typed Marvel APIs,
  Marvel notation.
- Numeric/Percent/Fudge/D66 behavior and notation (`1d6!pr` regression-locked).
- The pipeline stages and their order.
- `Error::NonNumericOutcome` semantics (it finally fires for real).

## 4. Testing strategy

- TDD throughout, `TestRng` for deterministic faces.
- Face-table oracles: every face of all seven dice asserted against Appendix A.
- Cancellation oracles: hand-computed mixed-pool cases including
  triumph-implicit-success cancellation, net-negative results, wash (tie →
  zero nets), Force light/dark isolation.
- Marvel regression: exhaustive-enumeration distribution oracles pass
  unchanged; the two representation-level tests (roller.rs:2164, 2166-2168)
  updated to the M face per §2.3; rerolled-M (Edge reroll landing on middle-die
  1) asserted as the M face in face AND history.
- Arithmetic seam: `2dAbility + 2`, `(1dBoost) * 2`, `sim` over narrative —
  all error with `NonNumericOutcome`.
- Parser/lexer disambiguation matrix: `1dF` Fudge vs `1dForce`; `D66` vs
  `2dDifficulty`; **`1d6!pr` unchanged** vs `1dProficiency`; `cs6`/`cf1` vs
  `1dChallenge`; `&` with non-narrative kinds rejected; modifiers rejected on
  narrative pools; duplicate groups accepted.
- Serde snapshots for the new shapes (M face, symbol-map SymbolPool,
  `symbols` outcome kind).

## Appendix A: Verified narrative die face tables

Cross-checked face-for-face against two independently-authored open-source
implementations that agree exactly (swrpg-online/dice `src/diceFaces.ts`;
thisSIDEofRANDOM/swrpgdice), corroborated by the Edge of the Empire SRD and the
Star Wars RPG (FFG) wiki. No source disagreements found. Genesys and Star Wars
use the identical dice; Genesys has no Force die. (S=Success, A=Advantage,
F=Failure, T=Threat, TR=Triumph, D=Despair, L=Light, K=Dark.)

| Die | Shape | Faces (roll 1..N in this order) |
|---|---|---|
| Boost (blue) | d6 | blank, blank, S, S+A, A+A, A |
| Setback (black) | d6 | blank, blank, F, F, T, T |
| Ability (green) | d8 | blank, S, S, S+S, A, A, S+A, A+A |
| Difficulty (purple) | d8 | blank, F, F+F, T, T, T, T+T, F+T |
| Proficiency (yellow) | d12 | blank, S, S, S+S, S+S, A, S+A, S+A, S+A, A+A, A+A, TR |
| Challenge (red) | d12 | blank, F, F, F+F, F+F, T, T, F+T, F+T, T+T, T+T, D |
| Force (white) | d12 | K, K, K, K, K, K, K+K, L, L, L+L, L+L, L+L (no blanks; 8 dark pips / 8 light pips) |

Resolution rules (verified):

- Cancellation is strictly Success↔Failure and Advantage↔Threat, as simple
  differences; a tie zeroes both sides.
- Triumph contributes 1 to the success pool for cancellation; the Triumph
  symbol itself is never cancelled and is always reported. Despair mirrors
  this on the failure side. Triumph and Despair do NOT cancel each other.
- A check succeeds iff net successes ≥ 1 (ties fail) — this is the consuming
  game's interpretation; diceman reports the net counts as facts.
- Force light/dark pips take no part in cancellation; they are two separate
  totals.

Caveat: corroboration is SRD + community/open-source (two codebases in exact
agreement), not a rulebook page cite — the official PDFs could not be parsed.
