Diceman Refactor Plan: Generalized Dice Engine

Overview

The goal of this refactor is to evolve diceman from a numeric dice roller into a generalized dice expression engine capable of supporting traditional numeric dice systems, symbolic dice systems, and game-specific scoring models while remaining independent of any particular RPG ruleset.

The guiding design principle is:

diceman produces facts about random outcomes; game systems interpret those facts.

For example:

* diceman reports that a d20 rolled a natural 20.
* A D&D rules engine decides that this means a critical hit.

Likewise:

* diceman reports a Marvel Fantastic result.
* The Marvel RPG rules determine its gameplay effects.

This separation keeps diceman reusable across many tabletop RPG systems.

⸻

Design Goals

* Separate parsing from execution.
* Separate rolling from scoring.
* Separate dice mechanics from game mechanics.
* Support both numeric and symbolic dice.
* Make adding new dice systems straightforward.
* Preserve existing functionality throughout the refactor whenever possible.

⸻

Architectural Pipeline

The execution model should be organized as a pipeline:

Notation
    ↓
Parser
    ↓
Roll Plan
    ↓
Raw Dice Rolls
    ↓
Dice Modifiers
    ↓
Scoring
    ↓
Annotations
    ↓
Formatting

Each stage has a single responsibility.

⸻

Responsibilities

Parser

Responsible only for understanding notation.

Examples:

4d6kh3
2d20kl1
5d10>=8
D66
3dMarvel

The parser should not know how the dice are evaluated.

⸻

Roll Plan

The parser lowers syntax into a normalized execution model.

pub struct RollPlan {
    pub pool: DicePool,
    pub modifiers: Vec<RollModifier>,
    pub scoring: ScoringMode,
    pub annotation_rules: Vec<AnnotationRule>,
}

This becomes the input to the evaluator.

⸻

Dice Pool

pub struct DicePool {
    pub count: u32,
    pub kind: DieKind,
}

A dice pool describes what to roll.

⸻

Die Kinds

Replace the current Sides enum with a generalized DieKind.

pub enum DieKind {
    Numeric(u32),
    Percent,
    Fudge,
    MarvelD6,
    Narrative(NarrativeDie),
}

Future systems can add additional die types without affecting the rest of the evaluator.

Examples include:

* Traveller Flux
* Marvel Multiverse
* Genesys
* Star Wars Narrative Dice
* Custom user-defined dice

⸻

Die Faces

The current implementation assumes every die ultimately becomes an integer.

Instead, dice should produce generalized faces.

pub enum DieFace {
    Numeric(i64),
    Symbols(SymbolPool),
}

Numeric dice simply produce:

Numeric(6)

Genesys dice produce:

Symbols(...)

⸻

Die Results

Replace:

pub struct DieResult {
    value: i64,
    rolls: Vec<i64>,
}

with:

pub struct DieResult {
    pub face: DieFace,
    pub history: Vec<DieFace>,
    pub dropped: bool,
    pub annotations: Vec<Annotation>,
}

History preserves rerolls, explosions, etc.

⸻

Roll Modifiers

Modifiers transform dice before scoring.

pub enum RollModifier {
    KeepHighest(u32),
    KeepLowest(u32),
    DropHighest(u32),
    DropLowest(u32),
    Reroll {
        once: bool,
        condition: FaceCondition,
    },
    Explode {
        mode: ExplodeMode,
        condition: FaceCondition,
    },
}

Examples:

* exploding dice
* rerolls
* keep highest
* drop lowest

Modifiers change the dice pool.

They do not determine the final outcome.

⸻

Scoring

Scoring converts modified dice into a final outcome.

pub enum ScoringMode {
    Sum,
    CountSuccesses(FaceCondition),
    DigitConcatenate,
    MarvelMultiverse,
    SymbolCancel,
}

Examples:

D&D

Sum

World of Darkness

CountSuccesses(>=8)

D66

DigitConcatenate

Marvel

MarvelMultiverse

Genesys

SymbolCancel

⸻

Roll Outcomes

Instead of always returning a numeric total:

pub struct RollResult {
    total: i64,
}

return:

pub enum RollOutcome {
    Numeric(i64),
    Successes(i64),
    Symbols(SymbolPool),
    Structured(GameAgnosticOutcome),
}

Then:

pub struct RollResult {
    pub outcome: RollOutcome,
    pub dice: Vec<DieResult>,
    pub expression: String,
}

Arithmetic expressions continue to require Numeric.

⸻

Annotation Rules

Annotations detect interesting outcomes.

They never create gameplay effects.

pub enum AnnotationRule {
    CriticalSuccess(FaceCondition),
    CriticalFailure(FaceCondition),
    MarvelFantastic,
    Triumph,
    Despair,
}

⸻

Annotations

Annotations become facts attached to results.

pub enum Annotation {
    CriticalSuccess,
    CriticalFailure,
    Success,
    Failure,
    Fantastic,
    Triumph,
    Despair,
}

Examples:

Roll	Annotation
natural 20	CriticalSuccess
natural 1	CriticalFailure
Marvel Fantastic	Fantastic
Genesys Triumph	Triumph
Genesys Despair	Despair

These are descriptive only.

⸻

diceman Stops Here

diceman reports:

* rolled faces
* totals
* symbols
* successes
* annotations

It does not report:

* double damage
* critical hit effects
* stun
* momentum
* advantage spending
* spell effects

Those belong to the consuming game engine.

⸻

Evaluation Pipeline

The evaluator becomes:

roll_pool()
↓
apply_modifiers()
↓
score()
↓
apply_annotations()
↓
format()

Pseudo-code:

let mut dice = roll_pool(plan.pool);
apply_modifiers(&mut dice);
let outcome = score(&dice);
apply_annotations(
    &mut dice,
    &outcome,
    &plan.annotation_rules,
);
format(plan, outcome);

Each stage can be tested independently.

⸻

Examples

D&D

RollPlan {
    pool: DicePool {
        count: 1,
        kind: DieKind::Numeric(20),
    },
    modifiers: vec![],
    scoring: ScoringMode::Sum,
    annotation_rules: vec![
        AnnotationRule::CriticalSuccess(eq(20)),
        AnnotationRule::CriticalFailure(eq(1)),
    ],
}

⸻

World of Darkness

RollPlan {
    pool: DicePool {
        count: 8,
        kind: DieKind::Numeric(10),
    },
    modifiers: vec![],
    scoring: CountSuccesses(>=8),
    annotation_rules: vec![],
}

⸻

Shadowrun

RollPlan {
    pool: DicePool {
        count: 12,
        kind: DieKind::Numeric(6),
    },
    modifiers: vec![
        Explode(...)
    ],
    scoring: CountSuccesses(>=5),
}

⸻

Marvel Multiverse

RollPlan {
    pool: DicePool {
        count: 3,
        kind: DieKind::MarvelD6,
    },
    modifiers: vec![],
    scoring: MarvelMultiverse,
    annotation_rules: vec![
        MarvelFantastic,
    ],
}

⸻

Star Wars / Genesys

RollPlan {
    pool: DicePool {
        count: 1,
        kind: DieKind::Narrative(
            NarrativeDie::Ability
        ),
    },
    modifiers: vec![],
    scoring: SymbolCancel,
    annotation_rules: vec![
        Triumph,
        Despair,
    ],
}

⸻

Migration Plan

Phase 1

Rename:

Sides

↓

DieKind

No behavioral changes.

⸻

Phase 2

Introduce:

* DicePool
* RollPlan
* RollModifier
* ScoringMode
* AnnotationRule

Continue using numeric results.

ScoringMode is introduced with only `Sum` and `CountSuccesses`. The
`DigitConcatenate` variant (D66) is deferred — `Expr::DigitRoll` keeps its
existing separate path until Phase 5 folds it into the unified pipeline.

⸻

Phase 3

Move success counting from modifiers into scoring.

No behavior changes.

⸻

Phase 4

Move critical success/failure from Roll into AnnotationRule.

No behavior changes.

⸻

Phase 5

Split evaluator into independent pipeline stages.

roll_pool()
apply_modifiers()
score()
apply_annotations()
format()

Folds the D66 / `Expr::DigitRoll` path into this pipeline as a
`DicePool` with `ScoringMode::DigitConcatenate`, retiring the separate
`evaluate_digit_roll` evaluator path.

⸻

Phase 6

Generalize die results.

Replace:

value: i64

with

face: DieFace

Replace:

rolls: Vec<i64>

with

history: Vec<DieFace>

⸻

Phase 7

Replace:

total: i64

with:

RollOutcome

Update:

* CLI
* simulator
* JSON serialization
* Python bindings

⸻

Phase 8

Marvel Multiverse support.

The evaluator supports Marvel Multiverse d616 rolls through the same pipeline
used by other dice systems:

* DieKind::MarvelD6
* ScoringMode::MarvelMultiverse
* RollOutcome::Marvel
* AnnotationRule::MarvelFantastic
* Annotation::Fantastic
* Annotation::AutoFail

The notation surface is:

* 3dMarvel
* 3dMarveleN
* 3dMarveltN

The middle die is the Marvel die. A 1 on that die displays as M and counts as 6,
except a raw 1 / M / 1 auto-fails with total 3. Edge rerolls the lowest-ranked
die and keeps the better result; Trouble rerolls the highest-ranked die and
keeps the worse result. Edge and Trouble cancel before rolling.

Target checks and simulations are exposed through typed APIs:

* roll_marvel
* simulate_marvel
* simulate_marvel_seeded

Those APIs return MarvelCheck and MarvelSimResult so consumers can use
success, Fantastic success/failure, auto-fail, M-shown, and total-distribution
facts without deriving them from formatted strings. Chase-fantastic Edge is an
API policy only; it has no notation token.

⸻

Phase 9

Implement Genesys / Star Wars support.

Add:

* Narrative dice
* Symbol faces
* Symbol cancellation
* Triumph / Despair annotations

⸻

Design Checklist

When adding a new feature, ask:

Question	Belongs In
What kind of die is this?	DieKind
Does it change rolled dice?	RollModifier
Does it compute the final result?	ScoringMode
Does it identify something interesting about the result?	AnnotationRule
Does it create gameplay consequences?	Game-specific rules layer

⸻

Long-Term Benefits

* Clean separation of concerns.
* Strong testability.
* Easy addition of new dice systems.
* Supports both numeric and symbolic dice.
* Decouples dice mechanics from RPG rules.
* Allows multiple notation frontends to share the same evaluation engine.
* Provides a stable foundation for future systems without continually expanding special cases in the evaluator.
