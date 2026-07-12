# Phase 9: Genesys Narrative Dice + Unified DieFace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Genesys/Star Wars narrative dice through the generic pipeline, with Marvel's M retrofitted onto a unified `DieFace::Symbols` model (bead diceman-dqh).

**Architecture:** Per `docs/superpowers/specs/2026-07-11-phase9-genesys-dieface-unification-design.md` (READ IT FIRST — every task below cites its sections as the authority). Face identity moves into `DieFace`; pool-contextual resolution (M's 6/1/rank-7, symbol cancellation) stays in scoring; mixed narrative pools are groups on `RollPlan`.

**Tech Stack:** Rust workspace (crates: diceman, diceman-cli, diceman-py/PyO3), thiserror, serde (feature-gated), fastrand; TestRng for deterministic tests.

## Global Constraints

- **TDD is mandatory**: every behavior lands as failing test → minimal code → green. Task acceptance criteria are the test oracles.
- Spec is authoritative: `docs/superpowers/specs/2026-07-11-phase9-genesys-dieface-unification-design.md`. Face tables + cancellation rules: spec Appendix A (verified data — transcribe exactly).
- Gates after every task: `cargo test --workspace --features diceman/serde` && `cargo clippy --workspace --all-targets --features diceman/serde -- -D warnings` && `cargo fmt --workspace -- --check`.
- Commits: one per task minimum, `git commit -s`, trailer `Assisted-by: Claude:<model-id>`. Never `--no-verify`.
- Match existing file style; keep existing comments unless provably false.
- Never delete a failing test; update the two representation-level Marvel tests exactly as spec §2.3 prescribes, nothing else.
- Existing Marvel distribution/verdict oracles and all numeric-dice behavior must pass unchanged in every task.

---

### Task 1: `Symbol` + `SymbolPool` core types

**Files:**
- Modify: `crates/diceman/src/ast.rs` (new types near `DieFace`)
- Modify: `crates/diceman/src/lib.rs` (re-export `Symbol`, `SymbolPool`)

**Interfaces (Produces):**
```rust
pub enum Symbol { Success, Advantage, Triumph, Failure, Threat, Despair, Light, Dark, Marvel }
pub struct SymbolPool { counts: [u8; 9] }   // private field
impl SymbolPool {
    pub fn new() -> Self;                    // empty (also Default)
    pub fn of(symbols: &[Symbol]) -> Self;
    pub fn count(&self, s: Symbol) -> u8;
    pub fn add(&mut self, s: Symbol, n: u8); // saturating
    pub fn merge(&mut self, other: &SymbolPool); // saturating per symbol
    pub fn contains(&self, s: Symbol) -> bool;
    pub fn is_empty(&self) -> bool;
}
```

**Goal:** The symbol vocabulary and multiset, standalone (no `DieFace` change yet).

**Constraints (spec §2.2):**
- `Symbol::index()` is the single source of the enum→array mapping; compile-time count assertion (e.g. `const _: () = assert!(...)` or exhaustive-match length); never hand-duplicate indices.
- Both types `Debug, Clone, Copy, PartialEq, Eq, Hash`; `Default` for `SymbolPool`.
- Saturating add/merge, documented as a safety net.
- Serde behind the `serde` feature: `Symbol` derives; `SymbolPool` custom `Serialize`/`Deserialize` as a name→count map with zero counts omitted (`{"Success":1,"Advantage":2}`), round-trippable.

**Acceptance criteria:**
- Round-trip: every `Symbol` through `of(&[s])`/`count(s)` returns 1; other symbols 0.
- `of(&[Success, Success, Advantage])` counts Success=2, Advantage=1.
- `merge` sums counts; saturation at `u8::MAX` verified for `add` and `merge`.
- Serde (with `--features serde`): pool ↔ `{"Success":2,"Threat":1}` both directions; empty pool serializes as `{}`.
- Gates pass.

---

### Task 2: `DieFace::Symbols` + `as_numeric` migration + Marvel retrofit

**Files:**
- Modify: `crates/diceman/src/ast.rs` (`DieFace` variant, `as_numeric`, `numeric_value`, `Display`)
- Modify: `crates/diceman/src/roller.rs` (per-index Marvel producer; `score` Marvel arm; `marvel_facts`; `rank`; `apply_edge`/`apply_trouble`; `lowest/highest_rank_index`; numeric-site migration; representation-level test updates — see acceptance)
- Modify: `crates/diceman/src/format.rs` (`format_marvel_roll` via face identity; numeric-site migration)
- Modify: `crates/diceman/src/sim.rs` if `as_numeric` callers exist there

**Interfaces (Produces):**
```rust
pub enum DieFace { Numeric(i64), Symbols(SymbolPool) }
impl DieFace {
    pub fn as_numeric(self) -> Option<i64>;          // None for Symbols
    pub(crate) fn numeric_value(self) -> i64;        // panics with contract msg; numeric-by-construction sites ONLY
}
// Display: Numeric as today; symbol faces per spec §2.9 abbreviations
// (S A Tr F Th De L Dk M, concatenated in Symbol declaration order,
// repeated per count); blank face renders "-".
```

**Goal:** The unification itself. M's face becomes `Symbols({Marvel})`; every consumer reads face identity instead of pool position. This task is atomic: representation and consumers move together.

**Constraints (spec §2.1 site classification, §2.3 retrofit — follow them exactly):**
- Two migration classes; NEVER `numeric_value` on Marvel identity sites (`score` Marvel arm, `marvel_facts`, rank helpers, ChaseFantastic guard, Edge/Trouble face handling, `format_marvel_roll`).
- One per-index Marvel face producer shared by `roll_pool` and Edge/Trouble rerolls; reroll results (face AND history entries) go through it.
- `rank(face: DieFace) -> i64` (M ⇒ 7) replaces index-aware `marvel_rank`.
- Runtime verdicts preserved: all existing distribution/enumeration oracles pass untouched. Exactly FIVE representation-level M-die assertions update to the `Symbols({Marvel})` face: roller.rs:2164, the history at roller.rs:2166-2168, and the three ChaseFantastic tests asserting `dice[1].face.as_numeric() == 1` on a shown M at roller.rs:2402, 2421, 2441. In addition, the `as_numeric() -> Option` signature change forces mechanical compile fixes (`Some(_)` / `.unwrap()` wrapping) at numeric-face test assertions (roller.rs:1267, 1268, 1495, 1613, 1619 and the outer-die lines 2401, 2403, 2420, 2422, 2440, 2442) — these are compile adjustments, not oracle changes; no runtime verdict may change anywhere.

**Acceptance criteria:**
- New tests: TestRng `[1,1,6]` → `dice[1].face == DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]))`, total 13, m_shown; Edge reroll of middle die landing on raw 1 yields M face in `.face` and `.history` (rank 7 beats old face, M kept); outer die (index 0) rolling 1 stays `Numeric(1)` rank 1; ChaseFantastic still targets middle until M.
- `DieFace::Numeric(4).as_numeric() == Some(4)`; M face → `None`.
- M face `Display` == `"M"`; `format_marvel_roll` output strings unchanged for all existing cases.
- Entire existing suite green with only the five prescribed representation updates + the mechanical `Option` compile fixes listed above.
- Gates pass. Serde: no existing snapshot asserts the M die's face (the Marvel serde tests use empty `dice` vecs) — ADD a snapshot asserting the M face's new `Symbols` map JSON shape.

---

### Task 3: `NarrativeDie` kinds + verified face tables

**Files:**
- Modify: `crates/diceman/src/ast.rs` (`DieKind::Narrative`, `NarrativeDie`)
- Modify: `crates/diceman/src/roller.rs` (face production for narrative kinds)
- Modify: `crates/diceman/src/format.rs` (two exhaustive `DieKind` matches gain arms — see constraints)
- Modify: `crates/diceman/src/lib.rs` (re-export `NarrativeDie`)

**Interfaces (Produces):**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]   // required: DieKind derives these (ast.rs:364)
pub enum NarrativeDie { Boost, Setback, Ability, Difficulty, Proficiency, Challenge, Force }
// DieKind gains Narrative(NarrativeDie); DieKind::count() -> 6/6/8/8/12/12/12
impl NarrativeDie {
    pub(crate) fn face(self, roll: u32) -> SymbolPool;  // roll is 1..=count
}
// roll_pool/roll production: Narrative kinds produce DieFace::Symbols(face(roll))
```

**Goal:** The seven dice exist and roll correct symbol faces.

**Constraints:** Face tables transcribed EXACTLY from spec Appendix A, in the listed roll order (roll 1 = first listed face). Blank = empty pool. Force die has no blanks. Const/match data in the roller or on `NarrativeDie` — one source.

**Build-boundary edits this task MUST make (exhaustive `DieKind` matches with no wildcard):**
- `format.rs:131-137` (`format_digit_roll`) and `format.rs:150-155` (`format_standard_roll` `kind_str`): add `DieKind::Narrative(_) => unreachable!(...)` arms — narrative dice never reach digit/standard formatting (real rendering arrives in Task 5).
- `roll_die` (roller.rs:664-671) returns `i64` and must NOT try to represent symbol faces: give it a `Narrative(_) => unreachable!(...)` arm and branch in `roll_pool` (roller.rs:349-362) — numeric kinds via `roll_die`, narrative kinds via `self.rng.roll(count)` + `NarrativeDie::face()` producing `DieFace::Symbols`.

**Acceptance criteria:**
- Exhaustive face-table oracle: for each die, every roll 1..=N asserts the exact expected `SymbolPool` (e.g. Ability roll 4 = SS; Proficiency roll 12 = TR; Challenge roll 12 = D; Force roll 7 = KK, roll 10 = LL). All 64 faces covered.
- `DieKind::Narrative(Boost).count() == 6`, …, `Force.count() == 12`.
- TestRng-driven `roll_pool` on a single-kind narrative pool produces `DieFace::Symbols` faces matching the table.
- Gates pass.

---

### Task 4: `RollPlan` pool groups (mechanical refactor, behavior-neutral)

**Files:**
- Modify: `crates/diceman/src/ast.rs` (`RollPlan` storage + constructors + getter; caller at ast.rs:685)
- Modify: `crates/diceman/src/roller.rs` (evaluate_roll/roll_pool group iteration; `marvel_plan`; callers roller.rs:328-329)
- Modify: `crates/diceman/src/parser.rs` (callers parser.rs:1012-1013, 1111-1112), `crates/diceman/src/format.rs` (callers format.rs:69, 131, 139, 150, 207, 218)
- Modify: `crates/diceman/src/lib.rs` — the `pool()` DOCTEST at lib.rs:117-118 (runs under `cargo test`) and `test_parse` at lib.rs:212-213 (CLI and py crates have no `.pool()` callers)

**Interfaces (Produces):**
```rust
impl RollPlan {
    pub fn new(pool: DicePool, ...) -> Result<Self>;                     // unchanged signature
    pub fn new_narrative(pools: Vec<DicePool>, ...) -> Result<Self>;     // validation lands in Task 5; here it may reject non-narrative kinds only
    pub(crate) fn new_unchecked(pool: DicePool, ...) -> Self;            // unchanged signature
    pub(crate) fn new_unchecked_pools(pools: Vec<DicePool>, ...) -> Self;
    pub fn pools(&self) -> &[DicePool];                                  // replaces pool()
}
```

**Goal:** Internal storage becomes `pools: Vec<DicePool>` with zero behavior change; multi-group rolling works (dice concatenated in group order, scored once over the flat slice).

**Constraints (spec §2.5):** invariant `pools` non-empty; single-pool constructors wrap `vec![pool]`; existing tests compile with at most the `pool()`→`pools()[0]` rename; no other observable change.

**Acceptance criteria:**
- Full existing suite green after the rename.
- New test: `new_unchecked_pools` with two narrative groups (from Task 3) rolls `count_a + count_b` dice in group order (TestRng), faces from the right tables. **Do NOT call `evaluate`/`score` on a narrative pool in this task** — scoring routes `Symbols` faces into the `Sum` arm's `numeric_value()` and panics until Task 5 exists; inspect the rolled dice directly.
- Gates pass.

---

### Task 5: `SymbolCancel` scoring + `SymbolsOutcome` + annotations + validation + formatter

**Files:**
- Modify: `crates/diceman/src/ast.rs` (`ScoringMode::SymbolCancel`, `SymbolsOutcome`, `RollOutcome::Symbols`, `AnnotationRule::Triumph/Despair`, `Annotation::Triumph/Despair`, `RollPlan::new`/`new_narrative` invariants)
- Modify: `crates/diceman/src/error.rs` (`Error::InvalidNarrativeRoll(String)`)
- Modify: `crates/diceman/src/roller.rs` (score arm; apply_annotations re-derivation; remove stale "unreachable until Phase 9" comments at roller.rs:125-126, 273-275; `roll_marvel_with_rng`'s outcome match at roller.rs:214-221 gains a `Symbols` rejection arm)
- Modify: `crates/diceman/src/sim.rs` (`simulate_marvel_with_rng`'s outcome match at sim.rs:238-245 is exhaustive — gains `RollOutcome::Symbols(_) => Err(InvalidMarvelRoll(...))`)
- Modify: `crates/diceman/src/format.rs` (exhaustive `format_roll`; `format_narrative_roll`; the third exhaustive `ScoringMode` match at format.rs:165-167 gains `SymbolCancel => unreachable!()`)
- Modify: `crates/diceman-py/src/lib.rs` — **compile-keeping only**: the exhaustive `core::RollOutcome` match at diceman-py/src/lib.rs:268-272 breaks when `Symbols` lands (the workspace gate compiles diceman-py). Add a minimal `Symbols` arm now; Task 8 fleshes out the full attribute surface.
- Modify: `crates/diceman/src/lib.rs` (re-exports)

**Interfaces (Produces):**
```rust
pub struct SymbolsOutcome {   // Debug, Clone, Copy, PartialEq, Eq (+serde cfg), like MarvelOutcome
    pub successes: i64,   // net (S+TR)-(F+D); negative = net failure
    pub advantages: i64,  // net A-T; negative = net threat
    pub triumphs: u8, pub despairs: u8, pub light: u8, pub dark: u8,
}
// RollOutcome gains Symbols(SymbolsOutcome); as_numeric() -> None for it
// ScoringMode gains SymbolCancel
```

**Goal:** Narrative rolls evaluate and format end-to-end via programmatic plan construction (no notation yet).

**Constraints (spec §2.5 invariants, §2.6, §2.7, §2.9):**
- Validation in `RollPlan::new`/`new_narrative`: SymbolCancel ⟺ all-narrative groups; narrative plans: no modifiers, group counts ≥ 1, annotation rules exactly `[Triumph, Despair]`; non-narrative: single group, no Triumph/Despair rules. (Parser mirror arrives in Task 6.)
- `apply_annotations` keeps `(dice, rules, scoring)` signature; re-derives from faces (marvel_facts pattern); pushes `Annotation::Triumph`/`Despair` at most once each.
- `format_roll` restructured exhaustively per spec §2.9 (this REMOVES the format.rs:25 `.expect()` catch-all — the bead-flagged latent panic); inner scoring match gains `SymbolCancel => unreachable!()`; narrative rendering per spec §2.9 exactly (notation from pools, ` | ` group separator, net-fact list, `wash`).
- Group display boundaries re-derived from `pools()` counts; code comment noting the no-count-changing-modifiers dependency.

**Acceptance criteria (hand-computed cancellation oracles):**
- TestRng, 1 Ability(roll 4 = SS) + 1 Difficulty(roll 4 = T): outcome `successes: 2, advantages: -1`, triumphs/despairs/light/dark 0; formatted `1dAbility&1dDifficulty[SS | Th] = 2 successes, 1 threat`.
- Proficiency roll 12 (TR) + Challenge roll 12 (D): `successes: 0` (1-1), `triumphs: 1, despairs: 1`; both annotations present; formatted outcome includes `1 triumph, 1 despair`.
- Triumph-implicit-success cancellation: Proficiency TR + Difficulty roll 2 (F) + Setback roll 3 (F) → `successes: -1`, `triumphs: 1` (symbol survives, net negative).
- Tie → `wash`: Ability roll 2 (S) + Difficulty roll 2 (F) → all-zero outcome, formatted `= wash`.
- Force isolation: 1 Force roll 7 (KK) → `dark: 2`, everything else 0; light/dark never enter nets.
- Blank face renders `-`.
- Arithmetic seam: `Expr::BinOp` adding a narrative roll to `Number(2)` → `Error::NonNumericOutcome`; `evaluate_total`/`simulate_seeded` over a narrative expr → same error (sim.rs path).
- `RollOutcome::Symbols(..).as_numeric() == None`.
- Validation rejections: SymbolCancel with numeric group; narrative with `KeepHighest`; empty/zero-count group; missing/extra annotation rules; numeric plan with `AnnotationRule::Triumph` — each `Err(InvalidNarrativeRoll)`.
- Serde snapshot: `symbols` outcome kind + annotations serialize.
- Gates pass.

---

### Task 6: Notation — lexer + parser

**Files:**
- Modify: `crates/diceman/src/lexer.rs` (word tokens `Ability/Boost/Setback/Difficulty/Proficiency/Challenge/Force`, `Token::Ampersand`; `p` full-word lookahead-with-restore)
- Modify: `crates/diceman/src/parser.rs` (`kind()` arms, `narrative_roll` production, `validate_narrative_roll`, auto-push `[Triumph, Despair]`)

**Interfaces (Consumes):** Task 5's types/validation. **Produces:** `parse("2dAbility&1dProficiency&2dDifficulty")` → single `Expr::Roll` whose plan has three groups, `SymbolCancel`, `[Triumph, Despair]`.

**Goal:** The notation surface per spec §2.8.

**Constraints (spec §2.8 — the letter-disambiguation table is normative):**
- `A`/`B`/`S` plain word match (case-insensitive, like `marvel`); `F`+peek-`o`, `D`+peek-`i`, `C`+peek-`h` one-char peek; **`P` uses full-word lookahead-with-restore** (clone iterator, match `roficiency`; on any mismatch emit `Token::P` consuming only the `p`).
- `&` = `Token::Ampersand`; pool-union inside the roll production only (not a BinOp); binds tighter than arithmetic; second-and-later groups must be narrative → `InvalidNarrativeRoll`; first group non-narrative → `&` left unconsumed (existing trailing-garbage `Error::Expected` at parse end); duplicate groups allowed.
- No modifiers/conditions/crit markers on narrative groups; parser mirror `validate_narrative_roll` enforces the Task 5 invariant set.

**Acceptance criteria (disambiguation matrix — every row a test):**
- `1dF` → Fudge; `1dForce` → Narrative(Force); `1dfudge`? — no such word: `1dF` then trailing `udge` errors (assert current-style error, don't invent support).
- `D66` digit roll unchanged; `2dDifficulty` → Narrative(Difficulty).
- **`1d6!pr` parses exactly as today** (penetrating explode + reroll) — regression-locked; `1dProficiency` → Narrative(Proficiency); `1d6!p` unchanged.
- `1d20cs20cf1` unchanged; `1dChallenge` → Narrative(Challenge).
- `3dmarvel` still Marvel (word matching untouched).
- `2dAbility&3d6` → `InvalidNarrativeRoll`; `3d6&1dAbility` → `Error::Expected` (trailing); `2dAbility&1dAbility` → one plan, two groups, 3 dice; `2dAbilitykh1` → `InvalidNarrativeRoll`; `2dAbility>=5` → `InvalidNarrativeRoll`.
- End-to-end: `roll("2dAbility&1dDifficulty")` returns `RollOutcome::Symbols`; `roll("2dAbility + 2")` → `NonNumericOutcome`; `roll("(1dBoost) * 2")` → `NonNumericOutcome`.
- Case-insensitivity: `2dability&1dPROFICIENCY` parses.
- Gates pass.

---

### Task 7: CLI `genesys` subcommand + notation reference + README

**Files:**
- Modify: `crates/diceman-cli/src/main.rs` (`Commands::Genesys`, dispatch, notation reference section)
- Modify: `README.md` (notation reference: narrative dice + `&`)

**Interfaces (Consumes):** `diceman::roll` + the Task 6 notation. No typed Genesys API (spec §2.10).

**Goal:** `diceman genesys --ability 2 --proficiency 1 --difficulty 2 [--boost N] [--challenge N] [--setback N] [--force N] [--json]` builds the notation string, rolls once, prints the formatted result (or serde JSON of `RollResult`).

**Constraints:** Long flags only (Marvel subcommand style, main.rs:44-76); at least one die required (clap-level or explicit error); flag order → notation group order stable (ability, proficiency, boost, difficulty, challenge, setback, force); `sim` subcommand over narrative notation must print the `NonNumericOutcome` error cleanly (no panic).

**Acceptance criteria:** dispatch/format unit tests in the existing CLI test style (notation-string builder tested for each flag combo incl. zero-flag rejection); `--json` shape test; `diceman notation` includes the narrative section; `cargo run --bin diceman -- genesys --ability 2 --difficulty 1` smoke (in test or documented manual gate). Gates pass.

---

### Task 8: Python bindings — `symbols` outcome kind

**Files:**
- Modify: `crates/diceman-py/src/lib.rs`

**Interfaces (Consumes):** `RollOutcome::Symbols(SymbolsOutcome)`. **Produces:** generic `roll()`/`RollOutcome` pyclass gains kind `"symbols"` with `successes/advantages/triumphs/despairs/light/dark` attributes (follow the existing `marvel` kind mapping pattern exactly).

**Goal:** Python callers can roll narrative notation.

**Constraints:** No typed `roll_genesys` (spec §2.11). `simulate()` over narrative notation raises the existing `ValueError` mapping of `NonNumericOutcome` — test it. Match existing pyclass registration + keyword-arg style.

**Acceptance criteria:** `cargo test -p diceman-py` green; Rust-side mapping unit tests for the new kind (existing style); `maturin develop` + `python -c "import diceman; r = diceman.roll('2dAbility&1dDifficulty'); print(r.outcome.kind, r.outcome.successes)"` smoke if a venv is available (else document the skip). Gates pass.

---

### Task 9: Version bump + as-built docs

**Files:**
- Modify: `Cargo.toml` / crate manifests (core v0.5.0; dependent crates per workspace convention; `Cargo.lock`)
- Modify: `docs/diceman-refactor.md` (Phase 9 section: as-built summary in the evergreen style of Phase 8's; update the "As-Built Type Reconciliation" list — e.g. `DieFace::Symbols` shipped, `RollOutcome::Symbols` shipped, `Condition` still numeric-only, annotations Triumph/Despair shipped)

**Goal:** Breaking-change version bump (spec §2.12) and evergreen docs describing what shipped.

**Constraints:** Docs describe as-built state, never history/changes ("shipped as", not "changed from"). Commit message flags the breaking JSON/API surfaces (spec §2.12 list).

**Acceptance criteria:** workspace builds/tests green at new versions; refactor doc's Phase 9 section matches the code; reconciliation list accurate against `ast.rs`. Gates pass.

---

## Task dependency notes

Strictly sequential 1→2→3→4→5→6; Tasks 7 and 8 are independent of each other (parallelizable) after 6; Task 9 last. The 1→2 split exists so the symbol vocabulary is reviewable before the high-risk retrofit; 4 is deliberately behavior-neutral so its review is a pure-refactor check.
