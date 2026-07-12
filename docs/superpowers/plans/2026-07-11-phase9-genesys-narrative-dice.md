# Phase 9: Genesys Narrative Dice + Unified DieFace — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Per orchestrator directive, tasks specify goal, constraints, and acceptance criteria; implementation steps are the implementer's to choose (TDD mandatory). Checkbox syntax tracks task completion.

**Goal:** Implement Genesys/Star Wars narrative dice (symbol faces, symbol-cancellation scoring, Triumph/Despair annotations, `&` pool-union notation, CLI subcommand, Python exposure) AND unify the special-face mechanism so Marvel's M and Genesys symbols share one `DieFace` model.

**Spec:** `docs/superpowers/specs/2026-07-11-phase9-genesys-dieface-unification-design.md` — the authoritative design. Every task brief below cites its sections; the implementer MUST read the cited sections before coding. Bead: `diceman-dqh`.

**Architecture:** Everything flows through the existing generic pipeline (`roll_pool → apply_modifiers → score → apply_annotations → format`). `DieFace` gains a `Symbols(SymbolPool)` variant; Marvel's M becomes a symbol face produced positionally at roll time (production keeps position; interpretation goes face-driven). `RollPlan` internally holds `Vec<DicePool>` groups for mixed narrative pools.

**Tech stack:** Rust workspace (crates/diceman core, diceman-cli clap v4, diceman-py PyO3), serde behind `feature = "serde"`, TestRng injection for deterministic tests.

## Global Constraints

- TDD mandatory: failing test first, minimal implementation, refactor green. Every task ends with the full gate suite passing:
  `cargo test --workspace --features diceman/serde`
  `cargo clippy --workspace --all-targets --features diceman/serde -- -D warnings`
  `cargo fmt --workspace -- --check`
- Commits: `git commit -s`, trailer `Assisted-by: Claude:claude-fable-5`, conventional-commit style matching `git log`. Never bypass hooks (cargo-husky runs fmt+clippy pre-commit). Commit at each green milestone within a task, not one squash at the end.
- All `.rs` files start with two `// ABOUTME:` lines (existing convention — check any new file).
- Marvel behavior invariance: every existing exhaustive-enumeration distribution oracle passes UNCHANGED. Only representation-level literals (a middle-die raw 1 now being the M face in `face`/`history`/JSON) may be updated — never weakened or deleted.
- No latent panics: spec §2.1's two call-site classes are law. `DieFace::numeric_value` (invariant-backed panic helper) only at numeric-by-construction sites incl. ALL sort/comparison keys; Marvel sites match face identity and never numerically unwrap the middle die.
- Exhaustive matches: never add a `_` arm to a match on `RollOutcome`, `DieFace`, `Symbol`, `DieKind`, or `NarrativeDie` to silence exhaustiveness — new variants must break the build, that's the design's safety mechanism.
- Face tables and resolution rules come ONLY from spec Appendix A. Do not "correct" them from memory or the web.
- Breaking changes are sanctioned (v0.5.0, spec §2.12); do NOT add backward-compatibility shims.

---

### Task 1: DieFace::Symbols + SymbolPool + Marvel retrofit

**Spec:** §2.1, §2.2, §2.3 (+§2.9 face-Display bullets; format_roll exhaustive rewrite included here).

**Goal:** Introduce `Symbol`, `SymbolPool`, `DieFace::Symbols`; change `DieFace::as_numeric` to `Option<i64>`; retrofit every Marvel positional-interpretation site to face identity; make `format_roll`'s outer match exhaustive (killing the format.rs:25 `.expect()`).

**Files:** Modify `crates/diceman/src/ast.rs`, `roller.rs`, `format.rs`. No other crate touches DieFace (verified).

**Interfaces produced (later tasks rely on exact names):**
- `pub enum Symbol { Success, Advantage, Triumph, Failure, Threat, Despair, Light, Dark, Marvel }`
- `pub struct SymbolPool` with `new()`, `of(&[Symbol])`, `count(Symbol) -> u8`, `add(Symbol, u8)` (saturating), `merge(&SymbolPool)`, `contains(Symbol) -> bool`, `is_empty() -> bool`; `Copy + Clone + Debug + PartialEq + Eq`; serde as name→count map, zero counts omitted (custom impl, NOT positional-array derive). Index derived from the enum in ONE place with a count assertion (§2.2).
- `pub enum DieFace { Numeric(i64), Symbols(SymbolPool) }`; `as_numeric(self) -> Option<i64>`; `pub(crate) numeric_value(self) -> i64` (panic message cites the RollPlan invariant); `Display`: numeric unchanged; symbol faces per §2.9 abbreviations (`S`,`A`,`Tr`,`F`,`Th`,`De`,`L`,`Dk`,`M`), blank = `-`.
- `pub(crate)` per-index Marvel face producer used by `roll_pool` AND Edge/Trouble reroll+history writes (§2.3).

**Constraints:**
- ChaseFantastic guard: `contains(Symbol::Marvel)` — the naive `as_numeric() != Some(1)` port INVERTS the policy (spec §2.1); regression-test this exact case.
- Sort keys (keep/drop + rank ordering) via `numeric_value` — never sort `Option<i64>`.
- `marvel_rank(index, face)` becomes `rank(face: &DieFace)` (M face ⇒ 7).
- Rerolled middle-die raw 1 becomes the M face in `face` AND `history`.

**Acceptance:**
- All existing Marvel distribution/Edge/Trouble/cancellation oracles pass unchanged; representation literals updated only where a middle-die raw 1 appears (e.g. the Trouble-reroll history test ~roller.rs:2164-2170: face and both history entries become the M face).
- New tests: M face Display renders `M` and formatted Marvel output strings are byte-identical to before; rerolled-M history; ChaseFantastic still chases M (deterministic TestRng); every `Symbol` round-trips through `of()`/`count()`; serde snapshot for the M face JSON (`{"Symbols":{"Marvel":1}}` shape per custom impl) and for a numeric face (unchanged shape).
- `format_roll` matches `Numeric(n) | Successes(n)` binding `n` directly — zero `.expect()` in format.rs; inner scoring match gains `SymbolCancel => unreachable!()` only if SymbolCancel exists yet (it does not in this task — add that arm in Task 2).

### Task 2: NarrativeDie + face tables + SymbolCancel + RollOutcome::Symbols + annotations

**Spec:** §2.4, §2.6, §2.7, Appendix A (+§2.9 for the single-group narrative format arm).

**Goal:** Add the seven narrative die kinds with exact face tables; `ScoringMode::SymbolCancel`; `SymbolsOutcome` + `RollOutcome::Symbols`; Triumph/Despair annotation rules/annotations; wire every exhaustive `RollOutcome` match in the workspace.

**Files:** Modify `crates/diceman/src/ast.rs`, `roller.rs`, `format.rs`, `sim.rs` (match at ~238-245), `roller.rs` (~214-221), `crates/diceman-py/src/lib.rs` (match at ~268-272 — minimal `Symbols` arm now, full py exposure is Task 6; workspace gate forces this).

**Interfaces produced:**
- `pub enum NarrativeDie { Boost, Setback, Ability, Difficulty, Proficiency, Challenge, Force }`; `DieKind::Narrative(NarrativeDie)`; `DieKind::count()` → 6/6/8/8/12/12/12; `NarrativeDie::face(roll: i64) -> SymbolPool` const-table-backed, face order exactly as Appendix A rows (roll 1..N maps left to right).
- `pub struct SymbolsOutcome { pub successes: i64, pub advantages: i64, pub triumphs: u8, pub despairs: u8, pub light: u8, pub dark: u8 }` — derives matching `MarvelOutcome` (Debug, Clone, Copy, PartialEq, Eq + serde feature-gated).
- `RollOutcome::Symbols(SymbolsOutcome)`; `as_numeric()` returns `None` for it.
- `ScoringMode::SymbolCancel`; `AnnotationRule::Triumph`, `AnnotationRule::Despair`; `Annotation::Triumph`, `Annotation::Despair`.

**Constraints:**
- Netting: `successes = (S+Tr) − (F+De)`, `advantages = A − T`; triumph/despair/light/dark counts reported uncancelled; Triumph NEVER touches the advantage axis (spec §4 oracle set is mandatory, verbatim).
- `apply_annotations` re-derives from dice (marvel_facts pattern), keeps its `(dice, rules, scoring)` signature; pushes each annotation once when count > 0.
- Tests construct plans via `new_unchecked` + TestRng (notation lands in Task 4; do not touch parser).
- Removing the "unreachable until Phase 9" comments (roller.rs ~125-126, ~273-275) and adding the REAL seam tests: `evaluate_total` over a hand-built narrative plan errors `NonNumericOutcome`.

**Acceptance:**
- Face-table enumeration oracles: full-cycle TestRng over each die asserts per-die aggregate symbol counts — Boost {S:2, A:4}, Setback {F:2, T:2}, Ability {S:5, A:5}, Difficulty {F:4, T:6}, Proficiency {S:9, A:8, Tr:1}, Challenge {F:8, T:8, De:1}, Force {L:8, Dk:8, light faces 5, dark faces 7} — plus at least one exact single-face assertion per die (e.g. Ability roll 4 = S+S).
- Cancellation oracles from spec §4 verbatim: lone Triumph → `{successes:1, advantages:0, triumphs:1}`; Triumph+1F → `{successes:0, triumphs:1}`; Triumph+Despair → `{successes:0, triumphs:1, despairs:1}`; Triumph+4F → `{successes:-3, triumphs:1}`; a hand-netted mixed multi-die case; Force isolation (pips never enter the axes).
- Single-group narrative `format_narrative_roll` arm renders per §2.9 (full grouping in Task 3); wash predicate = ALL six fields zero; zero-net-with-Triumph and pure-Force render their facts (formatter oracle each).
- Serde snapshot: `symbols` outcome kind JSON shape.
- Workspace compiles: py `roll()` match maps `Symbols` to `("symbols", net successes)` minimally.

### Task 3: RollPlan pool groups + narrative validation

**Spec:** §2.5 (+§2.9 group rendering).

**Goal:** `RollPlan` internal storage becomes `pools: Vec<DicePool>`; single-pool constructors retained; `new_narrative` added; narrative invariants enforced for library callers; roller/format iterate groups.

**Interfaces produced:**
- `pools() -> &[DicePool]` REPLACES `pool()` (deliberate rename — forces every call site to consciously handle multi-group semantics; ~17 sites in roller.rs/format.rs + tests).
- `RollPlan::new(pool, ...)` / `new_unchecked(pool, ...)` keep single-pool signatures (wrap `vec![pool]`); `RollPlan::new_narrative(pools: Vec<DicePool>, modifiers, scoring, annotation_rules) -> Result<Self>`; `pub(crate) new_unchecked_pools(Vec<DicePool>, ...)`.
- `Error::InvalidNarrativeRoll(String)`.

**Constraints:**
- Invariants both in `RollPlan::new*` and (Task 4) parser validation: `SymbolCancel` ⟺ all groups Narrative; narrative plans have no modifiers, every group count ≥ 1, annotation rules exactly `[Triumph, Despair]`; non-narrative plans exactly one group. Invariant comment words the narrative-only union as current scope, not domain law (spec §2.5). Mixed Force+skill unions allowed.
- `evaluate_roll`/`roll_pool` concatenate group dice in group order; score once over the flat slice.
- `format_narrative_roll` derives group boundaries from `pools()` counts (valid: no dice-count-changing modifiers on narrative; leave the §2.9-mandated code note), ` | ` group separator.

**Acceptance:** existing suite green (proves single-pool paths intact); `new_narrative` accept/reject matrix (each invariant violated singly → `InvalidNarrativeRoll`, plus happy paths incl. duplicate kinds and Force+skill mix); multi-group evaluate: deterministic 2-group roll nets across groups and renders `2dAbility&1dDifficulty[S, SA | Th] = 1 success, 1 advantage, 1 threat`-style output (exact string asserted).

### Task 4: Notation — lexer word tokens, `&`, parser narrative production

**Spec:** §2.8.

**Goal:** `2dAbility&1dProficiency&2dDifficulty` (etc.) parses to a narrative RollPlan; all disambiguation hazards regression-locked.

**Files:** Modify `crates/diceman/src/lexer.rs`, `parser.rs` (+`lib.rs` re-exports if needed).

**Constraints:**
- Word tokens case-insensitive per the `marvel` pattern; letter dispositions per §2.8. **`P` requires full-word lookahead-with-restore on a cloned iterator** — `1d6!pr` must lex/parse exactly as today (regression test REQUIRED). `F`/`D`/`C` one-char peeks; `A`/`B`/`S` plain word matches. `Token::Ampersand` added.
- Parser: `&` is a pool-union production inside the roll factor (NOT a BinOp), entered only when the first group's kind is narrative; `2dAbility&3d6` → `InvalidNarrativeRoll` at parse; `3d6&...` → existing trailing-garbage `Expected` error; binds tighter than arithmetic (`2dAbility&1dBoost+2` parses, then `NonNumericOutcome` at eval).
- `validate_narrative_roll` mirrors Task 3 invariants; auto-pushes `[Triumph, Despair]`; scoring forced to `SymbolCancel`; modifiers/conditions/crit markers on narrative groups rejected at parse.

**Acceptance:** disambiguation matrix from spec §4 verbatim (`1dF` vs `1dForce`, `D66` vs `2dDifficulty`, `1d6!pr`, `cs6`/`cf1` vs `1dChallenge`, `&` with non-narrative rejected, modifiers on narrative rejected, duplicate groups accepted); end-to-end `roll("2dAbility&1dProficiency&2dDifficulty")` returns a `Symbols` outcome; arithmetic seam notation tests (`2dAbility + 2`, `(1dBoost) * 2`, `simulate("1dAbility", ...)`) error `NonNumericOutcome`; every existing notation test unchanged.

### Task 5: CLI `genesys` subcommand + notation reference

**Spec:** §2.10.

**Goal:** `diceman genesys --ability 2 --proficiency 1 --difficulty 2 [--json]` rolls via the normal pipeline; docs updated.

**Files:** Modify `crates/diceman-cli/src/main.rs`, README.md (notation section), `print_notation_reference`.

**Constraints:** builds the notation string from flags (no typed API); at least one die required (clap or explicit error); `--json` = serde `RollResult` (existing pattern, main.rs:146); document shell-quoting for raw `&` notation in both README and `diceman notation` output; subcommand is the documented primary entry point.

**Acceptance:** flag→notation mapping tests (all seven kinds, multi-kind ordering deterministic — fixed flag order: boost, setback, ability, difficulty, proficiency, challenge, force); zero-dice rejection; `--json` shape test incl. `symbols` outcome kind + a symbol face (main.rs test-module style, ~729-789); notation reference mentions quoting.

### Task 6: Python bindings — symbols outcome

**Spec:** §2.11.

**Goal:** py `roll()` exposes the full symbols outcome.

**Files:** Modify `crates/diceman-py/src/lib.rs`.

**Constraints:** flat `RollOutcome` pyclass gains optional symbol fields (`successes`, `advantages`, `triumphs`, `despairs`, `light`, `dark` — 0 defaults for other kinds); `kind == "symbols"`; `value` = net successes (numeric handle precedent); `simulate()` over narrative raises the existing `ValueError` mapping. No `roll_genesys` typed API.

**Acceptance:** Rust-side mapping tests (existing binding-test style, lib.rs ~365-475) for a symbols roll incl. all six fields; ValueError path test; `cargo test -p diceman-py` green. `maturin develop` smoke only if a venv is already available — do not install toolchains.

### Task 7: Version bump + as-built docs + final review

**Spec:** §2.12, §3.

**Goal:** v0.5.0 across the workspace; refactor doc reflects as-built Phase 9; final whole-branch review.

**Files:** Modify workspace `Cargo.toml` (+ member versions per existing bump pattern — see commit 3afcee1b), `docs/diceman-refactor.md` (Phase 9 section describes as-built state, evergreen voice — model on the Phase 8 section; update the As-Built Type Reconciliation list), README.md if not done in Task 5.

**Acceptance:** full gates; `git log` shows breaking changes flagged in commit messages (DieFace/RollOutcome variants, as_numeric signature, pool()→pools(), JSON shapes); final code-review dispatch over the whole branch diff comes back clean or findings addressed.

---

## Execution notes (orchestrator)

- Sequential tasks — each builds on the previous task's types. No parallel dispatch.
- Per task: implementer subagent (TDD, commits) → spec-compliance review (against this plan + cited spec sections) → code-quality review → roborev review of the commits (`roborev review <sha> --local` from INSIDE the worktree — daemon can't see `.claude/worktrees/*`) → next task.
- Controller pre-flight per task (Phase 8 lesson): re-grep for new exhaustive-match sites and cross-crate breakage BEFORE dispatching (`grep -rn "match.*outcome\|match.*RollOutcome" crates/`), and hand the implementer the exact list.
- The concurrent-collaborator caveat: another session may amend commits on this branch. `git log` before each dispatch; never assume HEAD.
- Deferred (file follow-up beads at session close): Genesys simulation support (needs a non-numeric SimResult shape); fatescroll-side YAML round-trip integration test.
