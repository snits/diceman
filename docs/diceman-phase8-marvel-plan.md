# Phase 8 Implementation Plan: Marvel Multiverse d616 (Full Scope)

**Date:** 2026-06-25
**Status:** Plan, ratified by Jerry 2026-06-25 (scope = full; notation = new token)
**Supersedes:** the contained-evaluator fork in bead `diceman-4xy` and scratchpad `20260622-diceman-marvel-d616-design.md` §2 (D1). Post-refactor, `score()` is slice-aware, so Marvel folds into the generic pipeline via the refactor doc's extension points (`DieKind`, `RollModifier`, `ScoringMode`, `AnnotationRule`). See `.scratchpad/20260625-diceman-phase8-marvel-arch-applicability.md` for the analysis.

**Workflow:** Subagent-driven development. One implementer per task + two-stage review (spec compliance → code quality). TDD mandatory. Follow the active repository git profile for commit and push behavior. Use signed-off commits with the current agent attribution required by `AGENTS.md`.

---

## 1. Scope (ratified)

Full scope, all via the generic pipeline (no contained `marvel.rs` module):

- **Base d616 roll**: `DieKind::MarvelD6` + `ScoringMode::MarvelMultiverse` (cross-die M resolution, positional middle die) + `MarvelFantastic` annotation. Notation token `3dMarvel`.
- **Edge/Trouble rerolls**: new `RollModifier::Edge { count, policy }` and `RollModifier::Trouble { count }` (in-place reroll-keep-better/worse by Marvel rank). Notation `eN` / `tN`.
- **`simulate_marvel` + `simulate_marvel_seeded`**: rich aggregation (total histogram + auto_fail / m_shown / fantastic-success / fantastic-failure rates) — an i64 histogram alone is provably insufficient (`P(M | total=14) = 0.25`).
- **CLI `marvel` subcommand**: `--edges --troubles --target --modifier --policy --sim N --seed --json`.
- **Python bindings**: `roll_marvel` / `simulate_marvel` returning pyclasses with the rich fields.

**Deferred (NOT in this phase):** `18 = auto-success` (unverified, bead D6); target-sweep simulation (bead D5, single target v1); notation token for ChaseFantastic policy (exposed via API only — see §3 decision).

## 2. Verified rules (authoritative, from scratchpad §1)

- 3d6 summed; **middle die** (index 1) is the Marvel die; its **1-face is M** (faces 2–6 normal).
- **M counts as 6** for the total **except** raw `1 / M / 1`, where M reverts to **1** and the check **auto-fails regardless of target**.
- **M ranks 7** (> 6) for best-die selection (Edge/Trouble). A die has three axes: raw face, contribution, rank.
- **Edge** = reroll one chosen die in place, keep the **better by rank**. **Trouble** = reroll the current best die (M is always best), keep the **worse by rank**. N edges / N troubles = N sequential rerolls; edges and troubles **cancel 1:1** down to a net count of the majority side. Pool is always exactly 3 dice.
- **Fantastic success** = M shown AND meets target. **Fantastic failure** = M shown AND misses target. (Default D3 ruling: on `1/M/1`, report `auto_fail=true` AND `fantastic=Some(Failure)` independently — info-preserving; one-line change if Jerry rules "suppress".)

## 3. Key design decisions (concrete choices; flag alternates to Jerry if they prove wrong)

### 3.1 DieFace — no new variant
`DieFace` stays `Numeric(i64)`. **M = middle die (index 1) showing face 1**, detectable by position + face value. No `Symbols` / `Marvel` face variant needed for Phase 8. (Phase 9 Genesys will introduce `DieFace::Symbols`.) Rationale: avoids a DieFace ranking extension; Marvel's rank is position-aware (index 1, face 1 → rank 7), computed inside the Edge/Trouble modifier and the scoring arm.

### 3.2 Pool shape contract
`DieKind::MarvelD6` pools **must have count == 3**; the middle die (index 1) is the Marvel die. Enforce at parse time (`Error` for count != 3). This is the same kind of shape contract `DigitConcatenate` already carries (assumes `DieKind::Number`).

### 3.3 Scoring — `ScoringMode::MarvelMultiverse`
Slice-aware arm in `roller::score`. Pseudocode:
```rust
ScoringMode::MarvelMultiverse => {
    // Contract: 3 dice, index 1 = Marvel die, all DieFace::Numeric.
    let l = dice[0].face.as_numeric();
    let m = dice[1].face.as_numeric();
    let r = dice[2].face.as_numeric();
    let m_shown = m == 1;
    let auto_fail = l == 1 && m == 1 && r == 1;
    let m_contrib = if m == 1 { if auto_fail { 1 } else { 6 } } else { m };
    let total = l + m_contrib + r;
    RollOutcome::Marvel(MarvelOutcome { total, auto_fail, m_shown })
}
```
Dropped dice: Marvel pools don't use keep/drop (Edge/Trouble reroll in place, never drop), so all 3 are active. (If a Marvel pool somehow has dropped dice, that's a parser error — MarvelD6 rejects kh/kl/dh/dl? **Decision: reject keep/drop modifiers on MarvelD6** at parse time, since Edge/Trouble are the selection mechanic. Flag if this is too strict.)

### 3.4 Outcome — `RollOutcome::Marvel(MarvelOutcome)`
```rust
pub enum RollOutcome {
    Numeric(i64),
    Successes(i64),
    Marvel(MarvelOutcome),   // Phase 8
    // Structured(GameAgnosticOutcome) deferred to Phase 9
}

pub struct MarvelOutcome {
    pub total: i64,       // 3..=18
    pub auto_fail: bool,  // raw 1/M/1
    pub m_shown: bool,    // middle die showed M
}
```
`RollOutcome::as_numeric()` returns `total` for Marvel (lenient arithmetic extraction, consistent with the Phase 7 BinOp decision). **`as_numeric()` must remain exhaustive** — add the Marvel arm. Arithmetic `3dMarvel + 2` produces `Numeric(total + 2)` (BinOp extracts via `as_numeric`). **Alternate (flag):** `RollOutcome::Structured(GameAgnosticOutcome)` per the doc's game-agnostic intent — deferred to Phase 9 when a second structured system (Genesys) justifies generalizing. Using a concrete `Marvel` variant now is YAGNI-correct.

### 3.5 Fantastic — target-applied, not in the roll outcome
`MarvelOutcome` is **target-independent** (total, auto_fail, m_shown). Fantastic success/failure requires a target → computed in `simulate_marvel` and the CLI `marvel` subcommand, NOT by `roll("3dMarvel")`. The `MarvelFantastic` **annotation** (§3.6) records "M was shown" on the roll; the success/failure verdict is target-applied elsewhere. This matches the doc's target-independent/target-applied split.

### 3.6 Annotation — `AnnotationRule::MarvelFantastic` + pool-level `RollResult.annotations`
- Add `AnnotationRule::MarvelFantastic` (no condition). Parser **auto-pushes** it for `DieKind::MarvelD6` pools (no notation token needed).
- Add `pub annotations: Vec<Annotation>` to `RollResult` (pool-level facts — new field; per-die `is_crit_success`/`is_crit_failure` stay as-is for existing crits).
- `pub enum Annotation { CriticalSuccess, CriticalFailure, Fantastic, AutoFail }` (Phase 8 needs `Fantastic`, `AutoFail`; `CriticalSuccess`/`CriticalFailure` included per doc, wired later). `apply_annotations` pushes `Fantastic` when `m_shown`, `AutoFail` when `auto_fail`.
- **Serde:** `RollResult` already derives `Serialize` (serde feature); adding `annotations: Vec<Annotation>` is additive. `Annotation` derives `Serialize` (and `Deserialize`? — match `RollOutcome`/`DieFace` leaf style: both Serialize+Deserialize; `Annotation` is a leaf → both). **Breaking change to JSON shape** (new `annotations` field) — acceptable per Phase 7 precedent (outcome rewrite was breaking) and Jerry's "no backward compat without approval" rule (we're not adding compat). Flag in the commit message.

### 3.7 Edge/Trouble modifiers
```rust
pub enum RollModifier {
    // ... existing ...
    Edge { count: u32, policy: EdgePolicy },
    Trouble { count: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EdgePolicy { #[default] RerollLowest, ChaseFantastic }
```
- **Edge** (RerollLowest): for each of `count` steps, find the lowest-**rank** die, breaking ties by current pool order, roll a fresh face for it, keep the better by rank. **ChaseFantastic**: target the Marvel die (index 1) unless it already shows M, keep better by rank.
- **Trouble**: for each of `count` steps, find the highest-rank die (M is rank 7, always best when shown), breaking ties by current pool order, roll fresh, keep the **worse** by rank.
- **Net cancellation**: edges and troubles cancel 1:1. **Implementation decision:** apply as sequential steps in modifier order `reroll → explode → edge/trouble → ...`? The doc's modifier order is `reroll → explode → keep/drop`. Edge/Trouble are reroll-family; place them **after explode, before keep/drop** (though keep/drop is rejected on Marvel). Cancellation: if both Edge and Trouble present, compute `net = edges.saturating_sub(troubles)` (or vice versa) and apply only the majority side's `count`. **Flag:** the cancellation + sequential-observe semantics need care; the implementer should follow scratchpad §5 exactly and add oracle tests.
- **Rank function** (Marvel-only, position-aware): `rank(index, face) = if index == 1 && face == 1 { 7 } else { face_value }`. The Edge/Trouble modifier implementation computes this for a MarvelD6 pool.
- **Notation:** `eN` = `Edge { count: N, policy: RerollLowest }`; `tN` = `Trouble { count: N }`. **ChaseFantastic has NO notation token** (see §3.8) — reachable via `roll_marvel`/`simulate_marvel` API.
- **Lexer:** add `Token::Edge` ('e'/'E') and `Token::Trouble` ('t'/'T'). 'e' and 't' are currently unused. **Care:** 'e' doesn't clash (explode is '!'); 't' doesn't clash. Parser `modifiers()` adds arms for `Token::Edge` → `edge_modifier()`, `Token::Trouble` → `trouble_modifier()`.

### 3.8 EdgePolicy exposure — API, not notation
ChaseFantastic is a player decision that changes the distribution (different mean AND `P(M)`). Expose it via typed APIs that override the Edge modifiers' policy:
- `roll_marvel(edges, troubles, target, modifier, policy) -> MarvelCheck`
- `roll_marvel_with_rng(...)`
- `simulate_marvel(edges, troubles, target, modifier, policy, n) -> MarvelSimResult`
- `simulate_marvel_seeded(..., seed)`
These construct the `RollPlan` programmatically (or override policy on a parsed plan) and call a policy-aware evaluate path. The generic `roll("3dMarvele2")` uses RerollLowest. **Flag:** the implementer decides whether to (a) build the plan in the API and call `evaluate_with_rng`, or (b) add a policy override parameter to the evaluator. Prefer (a) — keep the generic evaluator signature stable.

### 3.9 Notation token — `Marvel`
- Lexer: add a word-matcher for `marvel` / `Marvel` (case-insensitive) → `Token::Marvel`. 'm' is currently unused. **Care:** match the whole word; don't single-char it (avoid clashing with future tokens). Reuse the `cs`/`cf` two-char-peek pattern, generalized to a 6-char word.
- Parser `kind()`: `Token::Marvel` → `DieKind::MarvelD6`. After kind, if kind==MarvelD6: set `scoring = MarvelMultiverse` (default), reject success-counting tokens (`>`/`<`/`=` → error), reject keep/drop modifiers, auto-push `AnnotationRule::MarvelFantastic`.
- Enforce `count == 3` (`Error` variant or reuse `Expected`).
- `DieKind::MarvelD6` → `count()` returns 6 (six faces) for `roll_die`.

### 3.10 `roll_die` for MarvelD6
`roll_die(&DieKind::MarvelD6) -> i64` = `self.rng.roll(6) as i64` (1..=6). The M-ness is positional, not face-variant, so the roll is a plain d6. All three dice roll 1..6; scoring/modifiers interpret index 1's face-1 as M.

### 3.11 `simulate_marvel` + `MarvelSimResult`
```rust
pub struct MarvelSimResult {
    pub n: usize,
    pub target: i64,
    pub modifier: i64,
    pub success_rate: f64,
    pub fantastic_success_rate: f64,
    pub fantastic_failure_rate: f64,
    pub auto_fail_rate: f64,
    pub m_shown_rate: f64,
    pub total: SimResult,   // reuse existing histogram for the 3..18 total distribution
}
```
Per trial: evaluate the plan with the rng → `RollOutcome::Marvel(o)`; `success = (o.total + modifier >= target) && !o.auto_fail`; `fantastic = o.m_shown.then(|| if success { Fantastic::Success } else { Fantastic::Failure })`. Aggregate counts; build `SimResult` from the totals. **Pitfalls (enforce via tests):** aggregate booleans per-trial — never re-derive from the histogram (the total=14 collision); auto-fail overrides success before the `>= target` test. Provide `simulate_marvel_seeded` mirroring `simulate_seeded`.

### 3.12 CLI `marvel` subcommand
`diceman marvel --edges N --troubles N --target T --modifier M --policy {reroll_lowest,chase_fantastic} [--sim N] [--seed S] [--json]`. Without `--sim`: single `roll_marvel` → print expression + verdict (success / fantastic / auto-fail). With `--sim N`: `simulate_marvel` → print rates (and `--json` serializes `MarvelSimResult`). **Enable core's `serde` feature for the CLI** when `--json` is used (the CLI currently doesn't enable serde; it hand-rolls sim JSON — bead §8 note). Add a `diceman notation` reference section for Marvel.

### 3.13 Python bindings
- Pyclass `MarvelOutcome { total, auto_fail, m_shown }` and `MarvelCheck { outcome: MarvelOutcome, target, modifier, success, fantastic }` (fantastic as `"success"`/`"failure"`/`None` string).
- `roll_marvel(edges, troubles, target, modifier=0, policy="reroll_lowest") -> MarvelCheck`; `simulate_marvel(edges, troubles, target, modifier=0, policy="reroll_lowest", n=10000) -> MarvelSimResult` (pyclass). Invalid `policy` → `PyValueError`.
- `RollOutcome` pyclass: add `marvel` kind carrying the rich fields (or a nested `MarvelOutcome`). Register new pyclasses before use.
- `simulate`/`roll` (generic) gain a `marvel` outcome kind mapping in the existing `roll` function (total only, for the generic path).

## 4. Task breakdown (subagent-driven development)

Each task: dispatch implementer (TDD, commit, self-review) → spec-compliance reviewer → code-quality reviewer → mark complete. Tasks are sequential (each builds on the prior types). Use `bd` to track: update `diceman-4xy` to in_progress, file sub-task beads if useful.

### Task 1 — AST + lexer + parser: MarvelD6 die kind, MarvelMultiverse scoring, MarvelFantastic annotation, notation `3dMarvel`
**Files:** `ast.rs` (DieKind::MarvelD6, ScoringMode::MarvelMultiverse, AnnotationRule::MarvelFantastic), `lexer.rs` (Token::Marvel word-match), `parser.rs` (kind() arm, count==3 enforcement, auto-push MarvelFantastic, reject success-counting + keep/drop on Marvel), `error.rs` (new error variant if needed), `lib.rs` (re-exports).
**TDD oracles:** `parse("3dMarvel")` → `RollPlan { count:3, kind:MarvelD6, scoring: MarvelMultiverse, annotation_rules:[MarvelFantastic], modifiers:[] }`. Reject `2dMarvel`, `3dMarvel>=8`, `3dMarvelkh2`. `DieKind::MarvelD6.count() == 6`.
**Gates:** `cargo test -p diceman`, clippy, fmt.

### Task 2 — Roller: MarvelMultiverse scoring + MarvelOutcome + RollResult.annotations + Annotation enum
**Files:** `ast.rs` (RollOutcome::Marvel, MarvelOutcome, Annotation enum), `roller.rs` (score arm, RollResult.annotations field, apply_annotations pushes Fantastic/AutoFail for Marvel), `format.rs` (format Marvel rolls: render `3dMarvel[l, M, r] = total` with M marker + auto-fail/fantastic), `lib.rs` (re-exports).
**TDD oracles (scratchpad §9, exact):** total distribution counts/216 `3:1,4:1,5:3,6:6,7:10,8:15,9:22,10:26,11:28,12:28,13:26,14:20,15:14,16:9,17:5,18:2`; `E[total]=2443/216`; `P(M)=1/6`; `P(autofail)=1/216`. Deterministic TestRng tests for specific face sequences (e.g., `[1,1,1]` → total 3, auto_fail true; `[1,1,6]` → M shown, total 1+6+6=13; `[6,1,6]` → M shown, total 6+6+6=18). `as_numeric()` exhaustive. Serde: `RollOutcome::Marvel` + `annotations` field serialize.
**Gates:** `cargo test --workspace --features diceman/serde`, clippy (with serde), fmt.

### Task 3 — Edge/Trouble modifiers: AST + lexer + parser + roller
**Files:** `ast.rs` (RollModifier::Edge/Trouble, EdgePolicy), `lexer.rs` (Token::Edge, Token::Trouble), `parser.rs` (edge_modifier/trouble_modifier, `eN`/`tN`), `roller.rs` (apply_edge, apply_trouble with position-aware rank, net cancellation), `format.rs` (render `eN`/`tN`).
**TDD oracles (scratchpad §9):** with pool-order tie break, 1-edge RerollLowest `E[total]=16849/1296 = 13.0007716`, `P(M)=16/81 = 0.19753086`; add exhaustive-enumeration regression oracles for N=1,2,3 because Marvel's positional M semantics are not equivalent to ordinary top-3-of-(3+N) d6. Rank-vs-value keep case: reroll Marvel 3 → keep face 1 = M, rank 7 > 3. Trouble mirror oracles. Cancellation: `e2t1` == `e1` net.
**Gates:** full workspace test + clippy + fmt.

### Task 4 — `roll_marvel` / `simulate_marvel` / `simulate_marvel_seeded` + MarvelSimResult (core)
**Files:** new `marvel.rs` module? **Decision: NO** — Jerry ratified generic pipeline. Put the typed API + sim in `sim.rs` or a small `marvel_api.rs`? **Prefer:** add `simulate_marvel*` + `MarvelSimResult` to `sim.rs`, and `roll_marvel*` + `MarvelCheck` to `roller.rs` (or a thin `marvel.rs` that reuses the pipeline — NOT a contained evaluator). The typed APIs build/override a `RollPlan` and call `evaluate_with_rng`. **Flag:** exact module placement is the implementer's call; keep it additive, reuse the pipeline, no parallel evaluator.
**TDD oracles:** `P(M|total=14)=0.25` (fantastic NOT histogram-derivable — assert the sim's fantastic count != histogram-derived); auto-fail overrides success; seeded reproducibility; target-applied fantastic success/failure.
**Gates:** full workspace test + clippy + fmt.

### Task 5 — CLI `marvel` subcommand + enable serde for `--json`
**Files:** `crates/diceman-cli/src/main.rs` (Commands::Marvel, arg parsing, print verdict / print rates / `--json`), `crates/diceman-cli/Cargo.toml` (enable `diceman/serde` feature), notation reference section.
**TDD oracles:** CLI arg parsing tests; `--json` output shape; single-roll verdict rendering. (E2E CLI tests via assert_cmd or std::process::Command — match existing CLI test style; the CLI currently has only unit tests on formatters, so add a thin integration test or test the dispatch functions directly.)
**Gates:** `cargo test --workspace`, clippy, fmt.

### Task 6 — Python bindings: `roll_marvel` / `simulate_marvel` + pyclasses
**Files:** `crates/diceman-py/src/lib.rs` (MarvelOutcome, MarvelCheck, MarvelSimResult pyclasses; roll_marvel/simulate_marvel pyfunctions; RollOutcome marvel kind; module registration), `crates/diceman-py/Cargo.toml` if needed.
**TDD oracles:** `maturin develop` + Python smoke test (or unit-test the mapping functions in Rust). Invalid policy → PyValueError. Keyword-arg style matching existing `simulate(expr, n=10000)`.
**Gates:** `cargo test -p diceman-py`, `maturin develop` smoke test, clippy, fmt.

### Task 7 — Final whole-implementation code review + doc/bead hygiene
- Dispatch a final code-reviewer over the whole Phase 8 diff.
- Update `docs/diceman-refactor.md` Phase 8 section to reflect what shipped (evergreen — describe the as-built state, not the history).
- Update README.md notation reference with Marvel.
- Close bead `diceman-4xy` with the as-built summary + commit SHAs; `bd remember` the key learnings (generic-pipeline Marvel, rank-vs-value, the scratchpad-design-applicability method).

## 5. Open sub-decisions to surface (implementer should flag, not silently pick)

1. **Reject keep/drop on MarvelD6?** (§3.3) — likely yes, but confirm.
2. **`RollOutcome::Marvel` vs `Structured(GameAgnosticOutcome)`** (§3.4) — chose Marvel variant (YAGNI); confirm at review.
3. **Edge/Trouble cancellation + sequential semantics** (§3.7) — follow scratchpad §5 exactly; add oracle tests; flag any deviation.
4. **Module placement for the typed Marvel API** (§4 Task 4) — implementer's call, additive, reuse pipeline.
5. **ChaseFantastic notation** (§3.8) — none (API only); confirm acceptable.
6. **`annotations` field is a breaking JSON-shape change** (§3.6) — flag in commit message; Jerry approved breaking changes in Phase 7, but this is a new field — call it out.
7. **D3 ruling** (1/M/1 ∩ Fantastic: report both flags) — default applied; one-line change if Jerry rules "suppress."

## 6. Validation (run after each task + final)

```bash
cargo test --workspace --features diceman/serde
cargo clippy --workspace --all-targets --features diceman/serde -- -D warnings
cargo fmt --workspace -- --check
```
Python: `cd crates/diceman-py && maturin develop && python -c "import diceman; ..."` (smoke).

## 7. Session-close protocol (conservative profile)

1. Close `diceman-4xy` (and any sub-task beads) with as-built summary + SHAs.
2. Run all gates (§6).
3. `git status` — handle commit, sync, and push behavior according to the active repository profile and Jerry's current instructions.
4. Hand off: summarize tasks completed, reviews, gate results, deferred items (Phase 9 Genesys; ChaseFantastic notation; target-sweep; 18-auto-success).
