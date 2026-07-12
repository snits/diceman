# Phase 9 Design: Genesys/Star Wars Narrative Dice + Unified Special-Face Model

**Date:** 2026-07-11
**Status:** Draft (pending design review)
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

## 2. Design decisions

### 2.1 DieFace shape (the bead's headline question)

Candidates considered:

- **(a) Struct face** `DieFace { value: Option<i64>, symbols: SymbolPool }` —
  literal reading of "optional symbol set AND optional numeric contribution."
  Rejected: every numeric die pays the churn; no face in scope carries both a
  numeric value and symbols; the both-and-neither corners are dead states.
- **(b) Enum with a combined variant** `Symbolic { symbols, value: Option<i64> }` —
  keeps `Numeric` cheap, allows a future face with both. Rejected for now:
  `value` would be `None` for every producer we have (M's contribution is
  pool-contextual by design; Genesys faces have no standalone numeric), so the
  field ships dead. YAGNI.
- **(c) Enum, symbols-only variant (CHOSEN):**

```rust
pub enum DieFace {
    Numeric(i64),
    Symbolic(SymbolPool),
}
```

This satisfies the unification intent: *which face is special* is carried by
the face itself (M is `Symbolic` containing `Symbol::M`), not by pool position.
"Optional numeric contribution" is realized as: numeric faces have one,
symbolic faces resolve theirs (if any) in scoring — exactly where the bead says
M's 6/1/rank-7 logic must stay. If a future die needs a face that is
simultaneously numeric and symbolic, variant (b) is a compatible extension.

`DieFace::as_numeric()` changes `i64 -> Option<i64>` (breaking, v0.5.0).
Numeric-only contexts (Sum/CountSuccesses/DigitConcatenate scoring, crit
checks, success markers) unwrap with an expect stating the contract, which
`RollPlan::new` validation makes unreachable (§2.5).

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
  `add(Symbol, n)`, `merge(&SymbolPool)`, `is_empty()`.
- `Copy + Eq + Hash`-able, serde behind the existing feature flag; keeps
  `DieFace` `Copy` (9 bytes of counts).
- A Genesys blank face is `Symbolic(SymbolPool::new())` — a real face that
  rolled, with nothing on it.
- Marvel's M face is `Symbolic(SymbolPool::of(&[Symbol::Marvel]))`.

### 2.3 Marvel retrofit

Positional encoding is removed from *interpretation* sites; die *production*
keeps position-awareness because that is physical reality (the middle die of a
d616 pool IS the Marvel die — it has an M face; the outer dice don't).

- `roll_pool` for a `MarvelD6` group: index 1 rolls the M-capable die (a raw 1
  produces `Symbolic({Marvel})`, 2–6 produce `Numeric`); indices 0 and 2
  produce plain `Numeric` faces. Edge/Trouble rerolls use the same per-index
  producer.
- `marvel_rank(face)` loses its index parameter: a face containing
  `Symbol::Marvel` ranks 7; numeric faces rank at value.
- `marvel_facts` / scoring: `m_shown` = middle face contains `Symbol::Marvel`;
  `auto_fail` = faces are `[1, M, 1]`; M contributes 6, or 1 on auto-fail —
  unchanged pool-contextual logic, now keyed off the face identity instead of
  `index == 1 && face == 1`.
- `format_marvel_roll` renders the face (`Display` for an M face is `"M"`)
  instead of checking position.
- `MarvelOutcome`, `MarvelCheck`, Edge/Trouble notation, typed Marvel APIs,
  and all distribution oracles are behavior-invariant. JSON die-face shape for
  the M face changes (`Numeric(1)` → `Symbolic({Marvel})`) — breaking, flagged.

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
- **(c) RollPlan holds pool groups (CHOSEN):** `RollPlan.pools: Vec<DicePool>`
  (invariant: non-empty; `len > 1` only when every group is
  `DieKind::Narrative`). Existing behavior is `len == 1` everywhere else.
  `pool()` getter becomes `pools() -> &[DicePool]` (breaking; fields are
  already private so the change is contained).

`RollPlan::new` invariants extend (Marvel precedent, type-level enforcement):

- `ScoringMode::SymbolCancel` ⟺ all groups are `Narrative` (both directions).
- Narrative plans: no modifiers (Genesys has no keep/reroll/explode/Edge);
  every group count ≥ 1; annotation rules exactly `[Triumph, Despair]`.
- Non-narrative plans: exactly one pool group; no Triumph/Despair rules.
- New `Error::InvalidNarrativeRoll(String)` mirroring `InvalidMarvelRoll`.

### 2.6 Scoring: SymbolCancel

`score()` merges all (never-dropped) faces' SymbolPools, then cancels:

```rust
pub struct SymbolsOutcome {
    pub successes: i64,   // net: successes (+ triumphs) - failures (- despairs); negative = net failure
    pub advantages: i64,  // net: advantages - threats; negative = net threat
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

Cancellation rules (verified, Appendix A): each Triumph adds one Success and
each Despair adds one Failure to the net computation, while the Triumph/Despair
counts themselves are reported uncancelled; Light/Dark never cancel.
`RollOutcome::as_numeric()` returns `None` for `Symbols` — the first real
producer for `Error::NonNumericOutcome` at the arithmetic seam (`2dAbility + 2`
errors; `evaluate_total`/sim over a narrative expression errors cleanly).

### 2.7 Annotations

`AnnotationRule::Triumph` / `AnnotationRule::Despair`, auto-pushed by the
parser for narrative pools (Marvel precedent). `apply_annotations` pushes
`Annotation::Triumph` / `Annotation::Despair` when the respective outcome count
is > 0 (once each, not per-symbol — matching `Fantastic`'s pool-level style).

### 2.8 Notation surface

Full-word die names joined by `&` into one roll plan:

```
2dAbility&1dProficiency&2dDifficulty&1dSetback
1dForce
```

- Word tokens follow the `marvel` lexer pattern with one-char peek
  disambiguation: `F`+`orce` vs `Fudge`, `D`+`ifficulty` vs `DigitD`,
  `P`+`roficiency` vs penetrating-`p`, `C`+`hallenge` vs `cs`/`cf`,
  `S`etback/`A`bility/`B`oost on currently-unused letters. Case-insensitive
  like `marvel`.
- `&` (new `Token::Ampersand`) is the pool-union operator, valid only between
  narrative groups; using it with non-narrative kinds is a parse error.
  Arithmetic operators around narrative rolls parse but fail at evaluation
  with `NonNumericOutcome` (the seam under test).
- No modifiers or success-counting conditions are accepted on narrative groups.
- Compact single-letter color notation (`ggypp`) rejected for v1: collides
  with existing modifier letters (`k`, `r`, `p`) and diceman's `NdX` grammar;
  the CLI subcommand supplies the ergonomic entry point.

### 2.9 Formatter

- `format_roll` becomes exhaustive over `RollOutcome` variants (kills the
  `_`-arm `.expect()` latent panic flagged in the bead comment): `Marvel` →
  `format_marvel_roll`, `Symbols` → new `format_narrative_roll`,
  `Numeric | Successes` → existing numeric paths with the total bound directly.
- Face display (`Display for DieFace`): numeric as today; symbolic faces as
  concatenated symbol abbreviations — `S` Success, `A` Advantage, `Tr` Triumph,
  `F` Failure, `Th` Threat, `De` Despair, `L` Light, `Dk` Dark, `M` Marvel;
  blank face renders `-`.
- Narrative roll rendering:
  `2dAbility&1dDifficulty[S, SA | Th] = 1 success, 1 advantage, 1 threat`
  (groups separated by ` | ` in pool order; outcome as a comma list of net
  facts, including `1 triumph` / `1 despair` when present; `wash` when
  everything nets to zero).

### 2.10 CLI

New `genesys` subcommand (Marvel precedent):

```
diceman genesys [-a N] [-p N] [-b N] [-d N] [-c N] [-s N] [-f N] [--json]
```

(ability/proficiency/boost/difficulty/challenge/setback/force). Builds the
notation string, runs the normal pipeline, prints the formatted roll; `--json`
serializes the `RollResult`. At least one die required. `roll`/`sim`
subcommands work unchanged (`sim` over narrative errors cleanly with the
NonNumericOutcome message). `diceman notation` gains a narrative section.

### 2.11 Python bindings

Generic `roll()` gains an outcome kind `"symbols"` exposing `successes`,
`advantages`, `triumphs`, `despairs`, `light`, `dark`. The flat `RollOutcome`
pyclass (`kind`, `value`) grows optional symbol fields (`None`/0 for other
kinds); `value` carries net successes for `"symbols"` so existing consumers
keep a numeric handle. No typed `roll_genesys` API in this phase (not in bead
scope; the notation covers it).
`simulate()` over a narrative expression raises the existing `ValueError`
mapping of `NonNumericOutcome`.

### 2.12 Versioning

Breaking: `DieFace::as_numeric` signature, new `DieFace`/`RollOutcome`
variants, `RollPlan::pool()` → `pools()`, JSON shapes (M face, new outcome
kind). Core bumps to v0.5.0; workspace crates follow.

## 3. What does NOT change

- Marvel distribution oracles, Edge/Trouble semantics, typed Marvel APIs.
- Numeric/Percent/Fudge/D66 behavior and notation.
- The pipeline stages and their order.
- `Error::NonNumericOutcome` semantics (it finally fires for real).

## 4. Testing strategy

- TDD throughout, `TestRng` for deterministic faces.
- Face-table oracles: every face of all seven dice asserted against Appendix A.
- Cancellation oracles: hand-computed mixed-pool cases including
  triumph-implicit-success cancellation, net-negative results, wash, Force.
- Marvel regression: existing exhaustive-enumeration oracles must pass
  unchanged (the retrofit is behavior-invariant by construction).
- Arithmetic seam: `2dAbility + 2`, `(1dBoost) * 2`, `sim` over narrative —
  all error with `NonNumericOutcome`.
- Parser/lexer: word-token disambiguation matrix (`1dF` Fudge vs `1dForce`;
  `D66` vs `2dDifficulty`; `!p` vs `1dProficiency`; `cs6` vs `1dChallenge`),
  `&` rejection for non-narrative kinds, modifier rejection on narrative pools.
- Serde snapshots for the new shapes.

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
