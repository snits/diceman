// ABOUTME: Dice rolling and expression evaluation logic.
// ABOUTME: Evaluates parsed AST nodes to produce roll results.

use crate::ast::{
    Annotation, AnnotationRule, Compare, Condition, DicePool, DieFace, DieKind, EdgePolicy, Expr,
    Fantastic, MarvelCheck, MarvelOutcome, Op, RollModifier, RollOutcome, RollPlan, ScoringMode,
    Symbol, SymbolPool,
};
use crate::error::{Error, Result};
use crate::format;

/// Maximum number of explosions/rerolls allowed to prevent infinite loops.
const MAX_EXPLOSIONS: u32 = 100;
pub(crate) const MAX_REROLLS: u32 = 100;

/// Build the face a Marvel die shows for a raw roll at a given pool index.
///
/// The middle die (index 1) of a 3dMarvel pool is the Marvel die: a raw 1
/// there is the M face (`Symbols` containing `Symbol::Marvel`). Every other
/// die and value is a plain `Numeric` face. Shared by pool rolling and every
/// Edge/Trouble reroll so a rerolled natural 1 on the middle die becomes M in
/// both `.face` and `.history`.
pub(crate) fn marvel_face(index: usize, raw: i64) -> DieFace {
    if index == 1 && raw == 1 {
        DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]))
    } else {
        DieFace::Numeric(raw)
    }
}

/// True when a face is the Marvel M face (a symbol face containing `Marvel`).
fn is_marvel_face(face: DieFace) -> bool {
    matches!(face, DieFace::Symbols(pool) if pool.contains(Symbol::Marvel))
}

/// Marvel rank of a face: the M face ranks 7 (above 6); numeric faces rank by
/// value. Only Marvel-pool faces reach here (`Numeric` or the M face).
fn rank(face: DieFace) -> i64 {
    match face {
        DieFace::Numeric(n) => n,
        DieFace::Symbols(pool) if pool.contains(Symbol::Marvel) => 7,
        DieFace::Symbols(_) => unreachable!("non-Marvel symbol face in a Marvel pool"),
    }
}

/// Trait for random number generation, allowing for testing with fixed values.
pub trait Rng {
    /// Generate a random number in the range [1, max].
    fn roll(&mut self, max: u32) -> u32;
}

/// Persistable checkpoint for a random number generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RngCheckpoint {
    state: u64,
}

impl RngCheckpoint {
    /// Create a checkpoint from a previously persisted state value.
    pub fn from_state(state: u64) -> Self {
        Self { state }
    }

    /// Return the persistable state value for this checkpoint.
    pub fn state(self) -> u64 {
        self.state
    }
}

/// Default RNG using fastrand.
pub struct FastRng(fastrand::Rng);

impl FastRng {
    pub fn new() -> Self {
        Self(fastrand::Rng::new())
    }

    pub fn with_seed(seed: u64) -> Self {
        Self(fastrand::Rng::with_seed(seed))
    }

    pub fn checkpoint(&self) -> RngCheckpoint {
        RngCheckpoint {
            state: self.0.get_seed(),
        }
    }

    pub fn restore(&mut self, checkpoint: RngCheckpoint) {
        self.0.seed(checkpoint.state);
    }
}

impl Default for FastRng {
    fn default() -> Self {
        Self::new()
    }
}

impl Rng for FastRng {
    fn roll(&mut self, max: u32) -> u32 {
        self.0.u32(1..=max)
    }
}

/// Result of a single die roll.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DieResult {
    /// The final face of this die (after any modifications).
    pub face: DieFace,
    /// The history of faces this die landed on (initial roll, rerolls, explosions).
    pub history: Vec<DieFace>,
    /// Whether this die was dropped/discarded.
    pub dropped: bool,
    /// Whether this die is marked as a critical success.
    pub is_crit_success: bool,
    /// Whether this die is marked as a critical failure.
    pub is_crit_failure: bool,
}

/// Result of evaluating a dice expression.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RollResult {
    /// The scored outcome of the expression.
    pub outcome: RollOutcome,
    /// Individual die results (if the expression was a roll).
    pub dice: Vec<DieResult>,
    /// Formatted expression showing the roll.
    pub expression: String,
    /// Pool-level annotations describing interesting outcomes (descriptive only).
    pub annotations: Vec<Annotation>,
}

/// Evaluate a dice expression with the default RNG.
pub fn evaluate(expr: &Expr) -> Result<RollResult> {
    evaluate_with_rng(expr, &mut FastRng::new())
}

/// Evaluate a dice expression with a custom RNG.
pub fn evaluate_with_rng(expr: &Expr, rng: &mut impl Rng) -> Result<RollResult> {
    let mut evaluator = Evaluator {
        rng,
        total_only: false,
    };
    evaluator.evaluate(expr)
}

/// Evaluate a dice expression, returning only the total (skips expression formatting).
pub(crate) fn evaluate_total(expr: &Expr, rng: &mut impl Rng) -> Result<i64> {
    let mut evaluator = Evaluator {
        rng,
        total_only: true,
    };
    evaluator
        .evaluate(expr)?
        .outcome
        .as_numeric()
        .ok_or(Error::NonNumericOutcome)
}

/// Evaluate a dice expression and return the full `RollOutcome` without
/// formatting an expression string or computing annotations.
///
/// Mirrors `evaluate_total` but preserves the typed outcome (e.g., the
/// Marvel `auto_fail`/`m_shown` flags) instead of collapsing via `as_numeric`.
pub(crate) fn evaluate_outcome(expr: &Expr, rng: &mut impl Rng) -> Result<RollOutcome> {
    let mut evaluator = Evaluator {
        rng,
        total_only: true,
    };
    Ok(evaluator.evaluate(expr)?.outcome)
}

/// Build the canonical 3dMarvel `RollPlan` for the typed Marvel API.
///
/// 0-count Edge/Trouble modifiers are omitted so the formatted expression
/// matches the canonical notation (e.g., `3dMarvel` for the base roll rather
/// than `3dMarvele0t0`). The roller's `apply_marvel_edge_trouble` cleanly
/// no-ops when no Edge/Trouble modifiers are present.
pub(crate) fn marvel_plan(edges: u32, troubles: u32, policy: EdgePolicy) -> RollPlan {
    let mut modifiers = Vec::new();
    if edges > 0 {
        modifiers.push(RollModifier::Edge {
            count: edges,
            policy,
        });
    }
    if troubles > 0 {
        modifiers.push(RollModifier::Trouble { count: troubles });
    }
    RollPlan::new_unchecked(
        DicePool {
            count: 3,
            kind: DieKind::MarvelD6,
        },
        modifiers,
        ScoringMode::MarvelMultiverse,
        vec![AnnotationRule::MarvelFantastic],
    )
}

/// Derive the target-applied `(success, fantastic)` verdict from a Marvel
/// outcome.
///
/// `success` is `(total + modifier >= target) && !auto_fail` — auto-fail
/// forces failure even when the total would meet the target. `fantastic` is
/// `Some(Success)` when the Marvel die showed M and the check succeeded,
/// `Some(Failure)` when it showed M and the check failed, and `None` when M
/// did not show. Shared by `roll_marvel_with_rng` and
/// `simulate_marvel_with_rng` so the verdict logic has one source of truth.
pub(crate) fn marvel_verdict(
    outcome: &MarvelOutcome,
    target: i64,
    modifier: i64,
) -> (bool, Option<Fantastic>) {
    let success = (outcome.total + modifier >= target) && !outcome.auto_fail;
    let fantastic = outcome.m_shown.then_some(if success {
        Fantastic::Success
    } else {
        Fantastic::Failure
    });
    (success, fantastic)
}

/// Roll a 3dMarvel check against a target with a custom RNG.
///
/// Builds the canonical Marvel `RollPlan` with the given Edge/Trouble counts
/// and Edge policy, runs the full pipeline (including expression formatting),
/// and applies the target/modifier to derive `success` and `fantastic`.
pub fn roll_marvel_with_rng(
    edges: u32,
    troubles: u32,
    target: i64,
    modifier: i64,
    policy: EdgePolicy,
    rng: &mut impl Rng,
) -> Result<MarvelCheck> {
    let plan = marvel_plan(edges, troubles, policy);
    let expr = Expr::Roll(plan);
    let result = evaluate_with_rng(&expr, rng)?;
    let outcome = match result.outcome {
        RollOutcome::Marvel(o) => o,
        RollOutcome::Numeric(_) | RollOutcome::Successes(_) | RollOutcome::Symbols(_) => {
            return Err(Error::InvalidMarvelRoll(
                "Marvel plan produced a non-Marvel outcome".to_string(),
            ));
        }
    };
    let (success, fantastic) = marvel_verdict(&outcome, target, modifier);
    Ok(MarvelCheck {
        outcome,
        target,
        modifier,
        success,
        fantastic,
        expression: result.expression,
    })
}

/// Roll a 3dMarvel check against a target with the default RNG.
pub fn roll_marvel(
    edges: u32,
    troubles: u32,
    target: i64,
    modifier: i64,
    policy: EdgePolicy,
) -> Result<MarvelCheck> {
    roll_marvel_with_rng(
        edges,
        troubles,
        target,
        modifier,
        policy,
        &mut FastRng::new(),
    )
}

struct Evaluator<'a, R: Rng> {
    rng: &'a mut R,
    total_only: bool,
}

impl<R: Rng> Evaluator<'_, R> {
    fn evaluate(&mut self, expr: &Expr) -> Result<RollResult> {
        match expr {
            Expr::Number(n) => Ok(RollResult {
                outcome: RollOutcome::Numeric(*n),
                dice: vec![],
                expression: if self.total_only {
                    String::new()
                } else {
                    n.to_string()
                },
                annotations: vec![],
            }),
            Expr::Roll(plan) => self.evaluate_roll(plan),
            Expr::BinOp { op, left, right } => {
                let left_result = self.evaluate(left)?;
                let right_result = self.evaluate(right)?;
                let left = left_result
                    .outcome
                    .as_numeric()
                    .ok_or(Error::NonNumericOutcome)?;
                let right = right_result
                    .outcome
                    .as_numeric()
                    .ok_or(Error::NonNumericOutcome)?;
                let total = match op {
                    Op::Add => left + right,
                    Op::Sub => left - right,
                    Op::Mul => left * right,
                    Op::Div => {
                        if right == 0 {
                            return Err(Error::DivisionByZero);
                        }
                        left / right
                    }
                };
                let expression = if self.total_only {
                    String::new()
                } else {
                    format!(
                        "{} {} {} = {}",
                        left_result.expression, op, right_result.expression, total
                    )
                };
                Ok(RollResult {
                    outcome: RollOutcome::Numeric(total),
                    dice: vec![],
                    expression,
                    annotations: vec![],
                })
            }
            Expr::Group(inner) => {
                let result = self.evaluate(inner)?;
                Ok(RollResult {
                    outcome: result.outcome,
                    dice: result.dice,
                    expression: if self.total_only {
                        String::new()
                    } else {
                        format!("({})", result.expression)
                    },
                    annotations: vec![],
                })
            }
        }
    }

    fn evaluate_roll(&mut self, plan: &RollPlan) -> Result<RollResult> {
        // Pipeline: roll_pools -> apply_modifiers -> score -> apply_annotations -> format
        let mut dice = self.roll_pools(plan.pools());
        self.apply_modifiers(&mut dice, &plan.pools()[0].kind, plan.modifiers())?;
        let outcome = Self::score(&dice, plan.scoring())?;

        let (expression, annotations) = if self.total_only {
            (String::new(), vec![])
        } else {
            let annotations =
                Self::apply_annotations(&mut dice, plan.annotation_rules(), plan.scoring());
            (format::format_roll(plan, &dice, outcome), annotations)
        };

        Ok(RollResult {
            outcome,
            dice,
            expression,
            annotations,
        })
    }

    /// Roll every pool group in order, concatenating their dice into one
    /// flat slice that is scored as a unit.
    fn roll_pools(&mut self, pools: &[DicePool]) -> Vec<DieResult> {
        let mut dice = Vec::new();
        for pool in pools {
            dice.extend(self.roll_pool(pool));
        }
        dice
    }

    /// Roll a fresh pool of dice, one `DieResult` per die in the pool.
    fn roll_pool(&mut self, pool: &DicePool) -> Vec<DieResult> {
        (0..pool.count)
            .map(|i| {
                let face = match pool.kind {
                    DieKind::Narrative(die) => {
                        let roll = self.rng.roll(die.count());
                        DieFace::Symbols(die.face(roll))
                    }
                    DieKind::MarvelD6 => marvel_face(i as usize, self.roll_die(&pool.kind)),
                    DieKind::Number(_) | DieKind::Percent | DieKind::Fudge => {
                        DieFace::Numeric(self.roll_die(&pool.kind))
                    }
                };
                DieResult {
                    face,
                    history: vec![face],
                    dropped: false,
                    is_crit_success: false,
                    is_crit_failure: false,
                }
            })
            .collect()
    }

    /// Canonical application phase for a modifier: reroll (0), explode (1),
    /// then keep/drop (2). Edge/Trouble are consumed by the Marvel pre-pass
    /// and never reach this ordering.
    fn modifier_phase(modifier: &RollModifier) -> u8 {
        match modifier {
            RollModifier::Reroll { .. } => 0,
            RollModifier::Explode { .. } => 1,
            RollModifier::KeepHighest(_)
            | RollModifier::KeepLowest(_)
            | RollModifier::DropHighest(_)
            | RollModifier::DropLowest(_) => 2,
            RollModifier::Edge { .. } | RollModifier::Trouble { .. } => {
                unreachable!("Edge/Trouble are handled by the Marvel pre-pass")
            }
        }
    }

    /// Apply modifiers. Marvel Edge/Trouble are normalized and applied as a
    /// pre-pass before the standard reroll -> explode -> keep/drop sequence.
    fn apply_modifiers(
        &mut self,
        dice: &mut Vec<DieResult>,
        kind: &DieKind,
        modifiers: &[RollModifier],
    ) -> Result<()> {
        let (marvel, mut rest): (Vec<&RollModifier>, Vec<&RollModifier>) = modifiers
            .iter()
            .partition(|m| matches!(m, RollModifier::Edge { .. } | RollModifier::Trouble { .. }));

        if !marvel.is_empty() {
            if *kind != DieKind::MarvelD6 {
                return Err(Error::InvalidMarvelRoll(
                    "Edge/Trouble modifiers require a 3dMarvel pool".to_string(),
                ));
            }
            self.apply_marvel_edge_trouble(dice, &marvel)?;
        }

        // Apply phases in the canonical reroll -> explode -> keep/drop order
        // regardless of how the modifiers were written in the notation. The
        // sort is stable, so multiple modifiers within one phase keep their
        // relative order.
        rest.sort_by_key(|m| Self::modifier_phase(m));

        for modifier in rest {
            match modifier {
                RollModifier::Reroll { once, condition } => {
                    self.apply_reroll(dice, kind, *once, condition.as_ref())?;
                }
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
                RollModifier::KeepHighest(n) => self.apply_keep_highest(dice, *n),
                RollModifier::KeepLowest(n) => self.apply_keep_lowest(dice, *n),
                RollModifier::DropHighest(n) => self.apply_drop_highest(dice, *n),
                RollModifier::DropLowest(n) => self.apply_drop_lowest(dice, *n),
                // Edge/Trouble are consumed by the Marvel pre-pass above.
                RollModifier::Edge { .. } | RollModifier::Trouble { .. } => {
                    unreachable!("Edge/Trouble are handled by the Marvel pre-pass")
                }
            }
        }
        Ok(())
    }

    /// Apply Marvel Edge/Trouble modifiers with 1:1 net cancellation.
    ///
    /// Edge and Trouble steps cancel each other one-for-one before any reroll
    /// occurs. The net majority side is applied: net Edge steps reroll the
    /// lowest-ranked die (keeping the better result by rank); net Trouble steps
    /// reroll the highest-ranked die (keeping the worse result by rank).
    fn apply_marvel_edge_trouble(
        &mut self,
        dice: &mut [DieResult],
        modifiers: &[&RollModifier],
    ) -> Result<()> {
        let mut reroll_lowest: u32 = 0;
        let mut chase_fantastic: u32 = 0;
        let mut trouble: u32 = 0;
        for modifier in modifiers {
            match modifier {
                RollModifier::Edge { count, policy } => match policy {
                    EdgePolicy::RerollLowest => reroll_lowest += count,
                    EdgePolicy::ChaseFantastic => chase_fantastic += count,
                },
                RollModifier::Trouble { count } => trouble += count,
                _ => unreachable!("only Edge/Trouble reach apply_marvel_edge_trouble"),
            }
        }

        let cancel = reroll_lowest.min(trouble);
        reroll_lowest -= cancel;
        trouble -= cancel;
        let cancel_cf = chase_fantastic.min(trouble);
        chase_fantastic -= cancel_cf;
        trouble -= cancel_cf;

        if reroll_lowest > 0 {
            self.apply_edge(dice, reroll_lowest, EdgePolicy::RerollLowest)?;
        }
        if chase_fantastic > 0 {
            self.apply_edge(dice, chase_fantastic, EdgePolicy::ChaseFantastic)?;
        }
        if trouble > 0 {
            self.apply_trouble(dice, trouble)?;
        }
        Ok(())
    }

    /// Apply one or more Edge reroll steps, keeping the better die by rank.
    fn apply_edge(&mut self, dice: &mut [DieResult], count: u32, policy: EdgePolicy) -> Result<()> {
        if count > MAX_REROLLS {
            return Err(Error::RerollLimit(MAX_REROLLS));
        }
        for _ in 0..count {
            let target = match policy {
                EdgePolicy::RerollLowest => Self::lowest_rank_index(dice),
                EdgePolicy::ChaseFantastic => {
                    if dice.len() > 1 && !is_marvel_face(dice[1].face) {
                        1
                    } else {
                        Self::lowest_rank_index(dice)
                    }
                }
            };
            let old_rank = rank(dice[target].face);
            let raw = self.roll_die(&DieKind::MarvelD6);
            let new_face = marvel_face(target, raw);
            let new_rank = rank(new_face);
            dice[target].history.push(new_face);
            if new_rank > old_rank {
                dice[target].face = new_face;
            }
        }
        Ok(())
    }

    /// Apply one or more Trouble reroll steps, keeping the worse die by rank.
    fn apply_trouble(&mut self, dice: &mut [DieResult], count: u32) -> Result<()> {
        if count > MAX_REROLLS {
            return Err(Error::RerollLimit(MAX_REROLLS));
        }
        for _ in 0..count {
            let target = Self::highest_rank_index(dice);
            let old_rank = rank(dice[target].face);
            let raw = self.roll_die(&DieKind::MarvelD6);
            let new_face = marvel_face(target, raw);
            let new_rank = rank(new_face);
            dice[target].history.push(new_face);
            if new_rank < old_rank {
                dice[target].face = new_face;
            }
        }
        Ok(())
    }

    /// Find the index of the lowest-ranked die, ties broken by lowest pool index.
    fn lowest_rank_index(dice: &[DieResult]) -> usize {
        dice.iter()
            .enumerate()
            .min_by_key(|(_, d)| rank(d.face))
            .map(|(i, _)| i)
            .expect("Marvel pool must be non-empty")
    }

    /// Find the index of the highest-ranked die, ties broken by lowest pool index.
    fn highest_rank_index(dice: &[DieResult]) -> usize {
        dice.iter()
            .enumerate()
            .min_by_key(|(_, d)| std::cmp::Reverse(rank(d.face)))
            .map(|(i, _)| i)
            .expect("Marvel pool must be non-empty")
    }

    /// Convert modified dice into a final outcome per the scoring mode.
    fn score(dice: &[DieResult], scoring: &ScoringMode) -> Result<RollOutcome> {
        let outcome = match scoring {
            ScoringMode::Sum => RollOutcome::Numeric(
                dice.iter()
                    .filter(|d| !d.dropped)
                    .map(|d| d.face.numeric_value())
                    .sum(),
            ),
            ScoringMode::CountSuccesses(condition) => RollOutcome::Successes(
                dice.iter()
                    .filter(|d| !d.dropped)
                    .filter(|d| {
                        condition
                            .compare
                            .check(d.face.numeric_value(), condition.value)
                    })
                    .count() as i64,
            ),
            ScoringMode::DigitConcatenate => RollOutcome::Numeric(
                dice.iter()
                    .filter(|d| !d.dropped)
                    .fold(0i64, |acc, d| acc * 10 + d.face.numeric_value()),
            ),
            ScoringMode::SymbolCancel => {
                let merged = Self::merge_symbol_faces(dice);
                let s = merged.count(Symbol::Success) as i64;
                let a = merged.count(Symbol::Advantage) as i64;
                let tr = merged.count(Symbol::Triumph);
                let f = merged.count(Symbol::Failure) as i64;
                let th = merged.count(Symbol::Threat) as i64;
                let de = merged.count(Symbol::Despair);
                RollOutcome::Symbols(crate::ast::SymbolsOutcome {
                    successes: (s + tr as i64) - (f + de as i64),
                    advantages: a - th,
                    triumphs: tr,
                    despairs: de,
                    light: merged.count(Symbol::Light),
                    dark: merged.count(Symbol::Dark),
                })
            }
            ScoringMode::MarvelMultiverse => {
                // Contract: 3 dice, index 1 = the Marvel die (its M face is
                // the only symbol face a Marvel pool can produce); the outer
                // dice are always numeric.
                if dice.len() != 3 {
                    return Err(Error::InvalidMarvelRoll(format!(
                        "Marvel Multiverse scoring requires exactly 3 dice, got {}",
                        dice.len()
                    )));
                }
                let (m_shown, auto_fail) = Self::marvel_facts(dice);
                let l = dice[0].face.numeric_value();
                let r = dice[2].face.numeric_value();
                let m_contrib = match dice[1].face {
                    DieFace::Symbols(pool) if pool.contains(Symbol::Marvel) => {
                        if auto_fail {
                            1
                        } else {
                            6
                        }
                    }
                    DieFace::Numeric(n) => n,
                    DieFace::Symbols(_) => {
                        unreachable!("non-Marvel symbol face on the Marvel die")
                    }
                };
                let total = l + m_contrib + r;
                RollOutcome::Marvel(crate::ast::MarvelOutcome {
                    total,
                    auto_fail,
                    m_shown,
                })
            }
        };

        Ok(outcome)
    }

    /// Merge every non-dropped die's symbol face into one pool.
    ///
    /// Narrative pools have no drop-producing modifiers, so the `!dropped`
    /// filter is a no-op there; it mirrors the other scoring arms. Numeric
    /// faces carry no symbols and contribute nothing.
    fn merge_symbol_faces(dice: &[DieResult]) -> SymbolPool {
        let mut merged = SymbolPool::new();
        for die in dice.iter().filter(|d| !d.dropped) {
            if let DieFace::Symbols(pool) = die.face {
                merged.merge(&pool);
            }
        }
        merged
    }

    /// Derive the Marvel Multiverse facts from the dice.
    ///
    /// The Marvel die is the middle die (index 1); its 1 face is M.
    /// `m_shown` is true when the middle die showed M; `auto_fail` is true
    /// when the raw roll was `1 / M / 1`. The dice are the source of truth.
    fn marvel_facts(dice: &[DieResult]) -> (bool, bool) {
        debug_assert!(dice.len() >= 3, "Marvel Multiverse scoring requires 3 dice");
        let m_shown = is_marvel_face(dice[1].face);
        let auto_fail =
            m_shown && dice[0].face == DieFace::Numeric(1) && dice[2].face == DieFace::Numeric(1);
        (m_shown, auto_fail)
    }

    /// Mark per-die crit annotations and return pool-level annotations.
    ///
    /// For Marvel rolls, pushes `Fantastic` when the middle die showed M
    /// and `AutoFail` when the roll was raw `1 / M / 1`. For narrative
    /// (`SymbolCancel`) rolls, pushes `Triumph`/`Despair` once each when those
    /// symbols are present. In both cases the facts are re-derived from the
    /// dice (the source of truth), not from the outcome.
    fn apply_annotations(
        dice: &mut [DieResult],
        rules: &[AnnotationRule],
        scoring: &ScoringMode,
    ) -> Vec<Annotation> {
        let success_cond = rules.iter().find_map(|r| match r {
            AnnotationRule::CriticalSuccess(c) => Some(c),
            _ => None,
        });
        let failure_cond = rules.iter().find_map(|r| match r {
            AnnotationRule::CriticalFailure(c) => Some(c),
            _ => None,
        });
        for die in dice.iter_mut().filter(|d| !d.dropped) {
            if let Some(c) = success_cond {
                die.is_crit_success = c.compare.check(die.face.numeric_value(), c.value);
            }
            if let Some(c) = failure_cond {
                die.is_crit_failure = c.compare.check(die.face.numeric_value(), c.value);
            }
        }

        match scoring {
            ScoringMode::MarvelMultiverse => {
                debug_assert!(dice.len() >= 3, "Marvel Multiverse scoring requires 3 dice");
                let (m_shown, auto_fail) = Self::marvel_facts(dice);
                let mut annotations = Vec::new();
                if m_shown {
                    annotations.push(Annotation::Fantastic);
                }
                if auto_fail {
                    annotations.push(Annotation::AutoFail);
                }
                annotations
            }
            ScoringMode::SymbolCancel => {
                let merged = Self::merge_symbol_faces(dice);
                let mut annotations = Vec::new();
                if merged.contains(Symbol::Triumph) {
                    annotations.push(Annotation::Triumph);
                }
                if merged.contains(Symbol::Despair) {
                    annotations.push(Annotation::Despair);
                }
                annotations
            }
            ScoringMode::Sum | ScoringMode::CountSuccesses(_) | ScoringMode::DigitConcatenate => {
                Vec::new()
            }
        }
    }

    fn roll_die(&mut self, kind: &DieKind) -> i64 {
        match kind {
            DieKind::Number(n) => self.rng.roll(*n) as i64,
            DieKind::Percent => self.rng.roll(100) as i64,
            DieKind::Fudge => self.rng.roll(3) as i64 - 2, // -1, 0, 1
            DieKind::MarvelD6 => self.rng.roll(6) as i64,
            // Narrative dice have no numeric face; roll_pool routes them
            // through NarrativeDie::face instead of roll_die.
            DieKind::Narrative(_) => unreachable!("narrative dice do not roll numeric values"),
        }
    }

    fn apply_reroll(
        &mut self,
        dice: &mut [DieResult],
        kind: &DieKind,
        once: bool,
        condition: Option<&Condition>,
    ) -> Result<()> {
        let default_condition = Condition {
            compare: Compare::Equal,
            value: 1,
        };
        let condition = condition.unwrap_or(&default_condition);

        for die in dice.iter_mut() {
            if die.dropped {
                continue;
            }

            let mut reroll_count = 0;
            while condition
                .compare
                .check(die.face.numeric_value(), condition.value)
            {
                if reroll_count >= MAX_REROLLS {
                    return Err(Error::RerollLimit(MAX_REROLLS));
                }
                let new_value = self.roll_die(kind);
                die.history.push(DieFace::Numeric(new_value));
                die.face = DieFace::Numeric(new_value);
                reroll_count += 1;

                if once {
                    break;
                }
            }
        }

        Ok(())
    }

    fn apply_explode(
        &mut self,
        dice: &mut Vec<DieResult>,
        kind: &DieKind,
        compounding: bool,
        penetrating: bool,
        limit: Option<u32>,
        condition: Option<&Condition>,
    ) -> Result<()> {
        let max_val = kind.count() as i64;
        let default_condition = Condition {
            compare: Compare::Equal,
            value: max_val,
        };
        let condition = condition.unwrap_or(&default_condition);

        // Each originating die owns one explosion chain. New dice from a
        // non-compounding chain are appended past `original_len` so they are
        // not treated as fresh originating dice, and `explode_count` bounds the
        // depth of a single chain (not the total pool size).
        let original_len = dice.len();
        for i in 0..original_len {
            if dice[i].dropped {
                continue;
            }

            // The chain continues based on the natural roll, even for
            // penetrating explosions where the stored face is `natural - 1`.
            let mut current_value = dice[i].face.numeric_value();
            let mut explode_count = 0;

            while condition.compare.check(current_value, condition.value) {
                if let Some(max) = limit {
                    if explode_count >= max {
                        break;
                    }
                }
                if explode_count >= MAX_EXPLOSIONS {
                    return Err(Error::ExplodeLimit(MAX_EXPLOSIONS));
                }

                let new_value = self.roll_die(kind);

                // Penetrating: subtract 1 from added value (not from check)
                let added_value = if penetrating {
                    new_value - 1
                } else {
                    new_value
                };

                if compounding {
                    // Compounding: add to same die
                    dice[i].face = DieFace::Numeric(dice[i].face.numeric_value() + added_value);
                    dice[i].history.push(DieFace::Numeric(new_value));
                } else {
                    // Standard: create new die
                    dice.push(DieResult {
                        face: DieFace::Numeric(added_value),
                        history: vec![DieFace::Numeric(new_value)],
                        dropped: false,
                        is_crit_success: false,
                        is_crit_failure: false,
                    });
                }

                current_value = new_value;
                explode_count += 1;
            }
        }

        Ok(())
    }

    fn apply_keep_highest(&mut self, dice: &mut [DieResult], n: u32) {
        let n = n as usize;
        let active_count = dice.iter().filter(|d| !d.dropped).count();
        if n >= active_count {
            return;
        }

        // Get indices sorted by value (ascending)
        let mut indices: Vec<usize> = dice
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.dropped)
            .map(|(i, _)| i)
            .collect();
        indices.sort_by_key(|&i| dice[i].face.numeric_value());

        // Drop the lowest (active_count - n)
        let to_drop = active_count - n;
        for &i in indices.iter().take(to_drop) {
            dice[i].dropped = true;
        }
    }

    fn apply_keep_lowest(&mut self, dice: &mut [DieResult], n: u32) {
        let n = n as usize;
        let active_count = dice.iter().filter(|d| !d.dropped).count();
        if n >= active_count {
            return;
        }

        // Get indices sorted by value (descending)
        let mut indices: Vec<usize> = dice
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.dropped)
            .map(|(i, _)| i)
            .collect();
        indices.sort_by_key(|&i| std::cmp::Reverse(dice[i].face.numeric_value()));

        // Drop the highest (active_count - n)
        let to_drop = active_count - n;
        for &i in indices.iter().take(to_drop) {
            dice[i].dropped = true;
        }
    }

    fn apply_drop_highest(&mut self, dice: &mut [DieResult], n: u32) {
        let n = n as usize;
        let mut indices: Vec<usize> = dice
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.dropped)
            .map(|(i, _)| i)
            .collect();
        indices.sort_by_key(|&i| std::cmp::Reverse(dice[i].face.numeric_value()));

        for &i in indices.iter().take(n) {
            dice[i].dropped = true;
        }
    }

    fn apply_drop_lowest(&mut self, dice: &mut [DieResult], n: u32) {
        let n = n as usize;
        let mut indices: Vec<usize> = dice
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.dropped)
            .map(|(i, _)| i)
            .collect();
        indices.sort_by_key(|&i| dice[i].face.numeric_value());

        for &i in indices.iter().take(n) {
            dice[i].dropped = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{DicePool, NarrativeDie, SymbolsOutcome};
    use crate::test_support::TestRng;

    /// Build an unchecked narrative `RollPlan` from `(count, die)` groups.
    fn narrative_plan(groups: &[(u32, NarrativeDie)]) -> Expr {
        let pools = groups
            .iter()
            .map(|&(count, die)| DicePool {
                count,
                kind: DieKind::Narrative(die),
            })
            .collect();
        Expr::Roll(RollPlan::new_unchecked_pools(
            pools,
            vec![],
            ScoringMode::SymbolCancel,
            vec![AnnotationRule::Triumph, AnnotationRule::Despair],
        ))
    }

    /// Evaluate a narrative plan and return its `SymbolsOutcome`.
    fn narrative_outcome(groups: &[(u32, NarrativeDie)], rolls: Vec<u32>) -> SymbolsOutcome {
        let expr = narrative_plan(groups);
        let mut rng = TestRng::new(rolls);
        match evaluate_with_rng(&expr, &mut rng).unwrap().outcome {
            RollOutcome::Symbols(o) => o,
            other => panic!("expected Symbols outcome, got {other:?}"),
        }
    }

    #[test]
    fn symbol_cancel_nets_success_against_threat() {
        // Ability roll 4 = SS, Difficulty roll 4 = Th.
        let o = narrative_outcome(
            &[(1, NarrativeDie::Ability), (1, NarrativeDie::Difficulty)],
            vec![4, 4],
        );
        assert_eq!(
            o,
            SymbolsOutcome {
                successes: 2,
                advantages: -1,
                triumphs: 0,
                despairs: 0,
                light: 0,
                dark: 0,
            }
        );
    }

    #[test]
    fn symbol_cancel_reports_triumph_and_despair_uncancelled() {
        // Proficiency roll 12 = Tr, Challenge roll 12 = De. Triumph's implicit
        // success and Despair's implicit failure cancel to net 0, but the
        // Triumph and Despair counts survive.
        let o = narrative_outcome(
            &[(1, NarrativeDie::Proficiency), (1, NarrativeDie::Challenge)],
            vec![12, 12],
        );
        assert_eq!(
            o,
            SymbolsOutcome {
                successes: 0,
                advantages: 0,
                triumphs: 1,
                despairs: 1,
                light: 0,
                dark: 0,
            }
        );
    }

    #[test]
    fn symbol_cancel_triumph_success_can_be_outvoted() {
        // Proficiency roll 12 = Tr (implicit +1 success), Difficulty roll 2 = F,
        // Setback roll 3 = F. Net successes (0 + 1) - (2 + 0) = -1; Triumph
        // symbol still reported.
        let o = narrative_outcome(
            &[
                (1, NarrativeDie::Proficiency),
                (1, NarrativeDie::Difficulty),
                (1, NarrativeDie::Setback),
            ],
            vec![12, 2, 3],
        );
        assert_eq!(
            o,
            SymbolsOutcome {
                successes: -1,
                advantages: 0,
                triumphs: 1,
                despairs: 0,
                light: 0,
                dark: 0,
            }
        );
    }

    #[test]
    fn symbol_cancel_tie_is_all_zero() {
        // Ability roll 2 = S, Difficulty roll 2 = F.
        let o = narrative_outcome(
            &[(1, NarrativeDie::Ability), (1, NarrativeDie::Difficulty)],
            vec![2, 2],
        );
        assert_eq!(
            o,
            SymbolsOutcome {
                successes: 0,
                advantages: 0,
                triumphs: 0,
                despairs: 0,
                light: 0,
                dark: 0,
            }
        );
    }

    #[test]
    fn symbol_cancel_force_pips_never_enter_nets() {
        // Force roll 7 = Dk Dk. Dark pips reported; nets stay zero.
        let o = narrative_outcome(&[(1, NarrativeDie::Force)], vec![7]);
        assert_eq!(
            o,
            SymbolsOutcome {
                successes: 0,
                advantages: 0,
                triumphs: 0,
                despairs: 0,
                light: 0,
                dark: 2,
            }
        );
    }

    #[test]
    fn narrative_outcome_is_not_numeric() {
        let expr = narrative_plan(&[(1, NarrativeDie::Ability)]);
        let mut rng = TestRng::new(vec![4]);
        let outcome = evaluate_with_rng(&expr, &mut rng).unwrap().outcome;
        assert_eq!(outcome.as_numeric(), None);
    }

    #[test]
    fn narrative_pushes_triumph_and_despair_annotations() {
        // Proficiency roll 12 = Tr, Challenge roll 12 = De.
        let expr = narrative_plan(&[(1, NarrativeDie::Proficiency), (1, NarrativeDie::Challenge)]);
        let mut rng = TestRng::new(vec![12, 12]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(
            result.annotations,
            vec![Annotation::Triumph, Annotation::Despair]
        );
    }

    #[test]
    fn narrative_omits_absent_annotations() {
        // Ability roll 4 = SS, Difficulty roll 4 = Th: no Triumph or Despair.
        let expr = narrative_plan(&[(1, NarrativeDie::Ability), (1, NarrativeDie::Difficulty)]);
        let mut rng = TestRng::new(vec![4, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.annotations.is_empty());
    }

    #[test]
    fn narrative_pushes_each_annotation_at_most_once() {
        // Two Proficiency dice both rolling Tr merge to two Triumph symbols,
        // but the annotation is pushed only once.
        let expr = narrative_plan(&[(2, NarrativeDie::Proficiency)]);
        let mut rng = TestRng::new(vec![12, 12]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.annotations, vec![Annotation::Triumph]);
    }

    /// Evaluate a narrative plan and return its formatted expression.
    fn narrative_expression(groups: &[(u32, NarrativeDie)], rolls: Vec<u32>) -> String {
        let expr = narrative_plan(groups);
        let mut rng = TestRng::new(rolls);
        evaluate_with_rng(&expr, &mut rng).unwrap().expression
    }

    #[test]
    fn narrative_formats_success_against_threat() {
        assert_eq!(
            narrative_expression(
                &[(1, NarrativeDie::Ability), (1, NarrativeDie::Difficulty)],
                vec![4, 4],
            ),
            "1dAbility&1dDifficulty[SS | Th] = 2 successes, 1 threat"
        );
    }

    #[test]
    fn narrative_formats_spec_worked_example() {
        // Spec §2.9: faces S+A, A+A | Th ⇒ net 1 success, 2 advantages.
        assert_eq!(
            narrative_expression(
                &[(2, NarrativeDie::Ability), (1, NarrativeDie::Difficulty)],
                vec![7, 8, 4],
            ),
            "2dAbility&1dDifficulty[SA, AA | Th] = 1 success, 2 advantages"
        );
    }

    #[test]
    fn narrative_formats_triumph_and_despair() {
        assert_eq!(
            narrative_expression(
                &[(1, NarrativeDie::Proficiency), (1, NarrativeDie::Challenge)],
                vec![12, 12],
            ),
            "1dProficiency&1dChallenge[Tr | De] = 1 triumph, 1 despair"
        );
    }

    #[test]
    fn narrative_formats_tie_as_wash() {
        assert_eq!(
            narrative_expression(
                &[(1, NarrativeDie::Ability), (1, NarrativeDie::Difficulty)],
                vec![2, 2],
            ),
            "1dAbility&1dDifficulty[S | F] = wash"
        );
    }

    #[test]
    fn narrative_formats_net_failure() {
        // Difficulty roll 2 = F, Ability roll 1 = blank: net one failure.
        assert_eq!(
            narrative_expression(
                &[(1, NarrativeDie::Difficulty), (1, NarrativeDie::Ability)],
                vec![2, 1],
            ),
            "1dDifficulty&1dAbility[F | -] = 1 failure"
        );
    }

    #[test]
    fn narrative_formats_force_pips() {
        assert_eq!(
            narrative_expression(&[(1, NarrativeDie::Force)], vec![7]),
            "1dForce[DkDk] = 2 dark"
        );
    }

    #[test]
    fn narrative_formats_blank_face_as_dash() {
        // Boost roll 1 is a blank face; the roll nets to wash.
        assert_eq!(
            narrative_expression(&[(1, NarrativeDie::Boost)], vec![1]),
            "1dBoost[-] = wash"
        );
    }

    #[test]
    fn narrative_roll_in_arithmetic_errors_non_numeric() {
        // 1dAbility + 2: the narrative operand has no numeric value.
        let expr = Expr::BinOp {
            op: Op::Add,
            left: Box::new(narrative_plan(&[(1, NarrativeDie::Ability)])),
            right: Box::new(Expr::Number(2)),
        };
        let mut rng = TestRng::new(vec![4]);
        let result = evaluate_with_rng(&expr, &mut rng);
        assert!(matches!(result, Err(Error::NonNumericOutcome)));
    }

    #[test]
    fn evaluate_total_over_narrative_errors_non_numeric() {
        // The sim path (simulate_with_rng -> evaluate_total) rejects a
        // narrative expression cleanly rather than scoring it.
        let expr = narrative_plan(&[(1, NarrativeDie::Ability)]);
        let mut rng = TestRng::new(vec![4]);
        let result = evaluate_total(&expr, &mut rng);
        assert!(matches!(result, Err(Error::NonNumericOutcome)));
    }

    #[test]
    fn narrative_pool_union_in_arithmetic_errors_non_numeric() {
        // 2dAbility&1dBoost + 2: the pool-union operand has no numeric value.
        // Spec §2.8 seam case: pool-union binds tighter than arithmetic.
        let expr = Expr::BinOp {
            op: Op::Add,
            left: Box::new(narrative_plan(&[
                (2, NarrativeDie::Ability),
                (1, NarrativeDie::Boost),
            ])),
            right: Box::new(Expr::Number(2)),
        };
        let mut rng = TestRng::new(vec![4, 3, 2]);
        let result = evaluate_with_rng(&expr, &mut rng);
        assert!(matches!(result, Err(Error::NonNumericOutcome)));
    }

    /// Build the lowered `RollPlan` for a digit-dice expression (Dnn).
    fn digit_plan(count: u32, sides: u32) -> Expr {
        Expr::Roll(RollPlan::new_unchecked(
            DicePool {
                count,
                kind: DieKind::Number(sides),
            },
            vec![],
            ScoringMode::DigitConcatenate,
            vec![],
        ))
    }

    #[test]
    fn fast_rng_restore_repeats_rolls_after_checkpoint() {
        let mut rng = FastRng::with_seed(42);

        let _before_checkpoint = rng.roll(20);
        let checkpoint = rng.checkpoint();

        let first_after_checkpoint = rng.roll(20);
        let _advanced = rng.roll(20);

        rng.restore(checkpoint);

        assert_eq!(rng.roll(20), first_after_checkpoint);
    }

    #[test]
    fn rng_checkpoint_rebuilds_from_persisted_state() {
        let mut rng = FastRng::with_seed(99);

        let _before_checkpoint = rng.roll(12);
        let checkpoint = rng.checkpoint();
        let persisted_state = checkpoint.state();

        let first_after_checkpoint = rng.roll(12);

        let rebuilt = RngCheckpoint::from_state(persisted_state);
        rng.restore(rebuilt);

        assert_eq!(rng.roll(12), first_after_checkpoint);
    }

    #[test]
    fn test_evaluate_number() {
        let expr = Expr::Number(42);
        let result = evaluate(&expr).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(42));
    }

    #[test]
    fn test_evaluate_basic_roll() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(7));
    }

    #[test]
    fn roll_pools_concatenates_multiple_groups_in_order() {
        // Do NOT route this plan through evaluate/score: ScoringMode::SymbolCancel
        // doesn't exist yet, and Symbols faces panic in the Sum arm's
        // numeric_value(). Drive roll_pools directly and inspect the dice.
        let plan = RollPlan::new_unchecked_pools(
            vec![
                DicePool {
                    count: 2,
                    kind: DieKind::Narrative(NarrativeDie::Ability),
                },
                DicePool {
                    count: 1,
                    kind: DieKind::Narrative(NarrativeDie::Difficulty),
                },
            ],
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(vec![4, 7, 8]);
        let mut evaluator = Evaluator {
            rng: &mut rng,
            total_only: true,
        };
        let dice = evaluator.roll_pools(plan.pools());

        assert_eq!(dice.len(), 3, "count_a (2) + count_b (1) dice expected");
        assert_eq!(
            dice[0].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Success, Symbol::Success])),
            "first Ability die (roll 4)"
        );
        assert_eq!(
            dice[1].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Success, Symbol::Advantage])),
            "second Ability die (roll 7), still ahead of the Difficulty group"
        );
        assert_eq!(
            dice[2].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Failure, Symbol::Threat])),
            "Difficulty die (roll 8) rolled after both Ability dice"
        );
    }

    #[test]
    fn test_evaluate_keep_highest() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::KeepHighest(3)],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Should keep 5, 3, 6 = 14
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(14));
    }

    #[test]
    fn test_evaluate_expression() {
        let expr = Expr::BinOp {
            op: Op::Add,
            left: Box::new(Expr::Roll(RollPlan::new_unchecked(
                DicePool {
                    count: 2,
                    kind: DieKind::Number(6),
                },
                vec![],
                ScoringMode::Sum,
                vec![],
            ))),
            right: Box::new(Expr::Number(5)),
        };
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(12)); // 3 + 4 + 5
    }

    #[test]
    fn test_evaluate_fudge() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Fudge,
            },
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 2, 3, 2]); // -1, 0, 1, 0 = 0
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(0));
    }

    #[test]
    fn test_evaluate_drop_lowest() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::DropLowest(1)],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Drop 1, keep 5+3+6 = 14
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(14));
    }

    #[test]
    fn test_evaluate_drop_highest() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::DropHighest(1)],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Drop 6, keep 1+5+3 = 9
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(9));
    }

    #[test]
    fn test_evaluate_count_successes() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 5,
                kind: DieKind::Number(10),
            },
            vec![],
            ScoringMode::CountSuccesses(Condition {
                compare: Compare::GreaterOrEqual,
                value: 8,
            }),
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![10, 7, 8, 3, 9]); // 10, 8, 9 >= 8 = 3 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Successes(3));
    }

    #[test]
    fn test_evaluate_count_successes_zero() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 3,
                kind: DieKind::Number(6),
            },
            vec![],
            ScoringMode::CountSuccesses(Condition {
                compare: Compare::Equal,
                value: 6,
            }),
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 2, 3]); // No 6s = 0 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Successes(0));
    }

    #[test]
    fn test_evaluate_count_successes_lte_boundary() {
        // A roll exactly equal to the target must count for <=.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 6,
                kind: DieKind::Number(6),
            },
            vec![],
            ScoringMode::CountSuccesses(Condition {
                compare: Compare::LessOrEqual,
                value: 3,
            }),
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![5, 6, 6, 4, 5, 3]); // only 3 <= 3 = 1 success
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Successes(1));
    }

    #[test]
    fn test_evaluate_count_successes_ne_boundary() {
        // A roll exactly equal to the target must NOT count for <>.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 6,
                kind: DieKind::Number(6),
            },
            vec![],
            ScoringMode::CountSuccesses(Condition {
                compare: Compare::NotEqual,
                value: 3,
            }),
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![2, 1, 3, 3, 1, 1]); // four dice != 3 = 4 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Successes(4));
    }

    #[test]
    fn test_evaluate_count_successes_output_format() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(10),
            },
            vec![],
            ScoringMode::CountSuccesses(Condition {
                compare: Compare::GreaterOrEqual,
                value: 8,
            }),
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![10, 5, 8, 3]); // 10*, 5, 8*, 3 = 2 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.expression.contains("successes"));
        assert!(result.expression.contains("10*")); // Success marked
        assert!(result.expression.contains("8*")); // Success marked
    }

    #[test]
    fn test_success_counting_renders_condition_after_modifiers() {
        // Input "5d10>=8kh3" writes the success condition before the keep
        // modifier. The rendered expression normalizes to the canonical form
        // with the condition rendered after the modifiers.
        let expr = crate::parse("5d10>=8kh3").unwrap();
        let mut rng = TestRng::new(vec![10, 7, 8, 3, 9]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        // 5d10 -> [10, 7, 8, 3, 9]; keep highest 3 keeps 10, 8, 9 and drops
        // 7, 3; successes >= 8 among kept = 10, 8, 9 = 3.
        assert_eq!(result.outcome, RollOutcome::Successes(3));
        assert_eq!(
            result.expression,
            "5d10kh3>=8[10*, (7), 8*, (3), 9*] = 3 successes"
        );
    }

    #[test]
    fn test_evaluate_penetrating_explode() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: true,
                penetrating: true,
                limit: None,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        // Rolls: 6 (explode), 6 (explode), 4 (stop)
        // Added: 6 + (6-1) + (4-1) = 6 + 5 + 3 = 14
        let mut rng = TestRng::new(vec![6, 6, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(14));
    }

    #[test]
    fn test_evaluate_penetrating_explode_no_explosion() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: true,
                penetrating: true,
                limit: None,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        // Roll: 4 (no explosion)
        // Total: 4 (no -1 because no explosion occurred)
        let mut rng = TestRng::new(vec![4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(4));
    }

    #[test]
    fn test_evaluate_standard_explode_limit_once() {
        // 1d6!1, TestRng all 6: initial 6 explodes ONCE into a new 6, then the
        // cap stops the chain quietly. Sum = 6 + 6 = 12, no error.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
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
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
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
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
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
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
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

    #[test]
    fn test_evaluate_standard_explode_creates_new_dice() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                limit: None,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        // Rolls: 6 (explode, create new die), 4 (stop)
        // Result: 2 dice with values 6 and 4
        let mut rng = TestRng::new(vec![6, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(10)); // 6 + 4
        assert_eq!(result.dice.len(), 2); // Two separate dice
    }

    #[test]
    fn test_evaluate_standard_explode_chain() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                limit: None,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        // Rolls: 6 (explode), 6 (explode), 6 (explode), 4 (stop)
        // Result: 4 dice with values 6, 6, 6, 4
        let mut rng = TestRng::new(vec![6, 6, 6, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(22)); // 6 + 6 + 6 + 4
        assert_eq!(result.dice.len(), 4); // Four separate dice
    }

    #[test]
    fn test_evaluate_compounding_explode() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: true,
                penetrating: false,
                limit: None,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        // Rolls: 6 (explode), 6 (explode), 4 (stop)
        // Result: 1 die with value 6 + 6 + 4 = 16
        let mut rng = TestRng::new(vec![6, 6, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(16)); // 6 + 6 + 4
        assert_eq!(result.dice.len(), 1); // One die with compounded value
    }

    #[test]
    fn test_evaluate_explode_with_keep() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            vec![
                RollModifier::Explode {
                    compounding: false,
                    penetrating: false,
                    limit: None,
                    condition: None,
                },
                RollModifier::KeepHighest(2),
            ],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        // Initial rolls: 6 (explode), 3
        // Explosion: 5 (stop)
        // Result: 3 dice (6, 5, 3), keep highest 2 (6, 5)
        let mut rng = TestRng::new(vec![6, 3, 5]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(11)); // 6 + 5 (3 dropped)
        assert_eq!(result.dice.len(), 3); // Three dice total
        assert_eq!(result.dice.iter().filter(|d| !d.dropped).count(), 2); // 2 kept
    }

    #[test]
    fn test_evaluate_standard_penetrating_explode() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: true,
                limit: None,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        // Rolls: 6 (explode, create new die with -1), 4 (stop)
        // Result: 2 dice with values 6 and 3 (4-1 penetrating)
        let mut rng = TestRng::new(vec![6, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(9)); // 6 + 3 (4-1)
        assert_eq!(result.dice.len(), 2); // Two separate dice
        assert_eq!(result.dice[0].face.as_numeric(), Some(6));
        assert_eq!(result.dice[1].face.as_numeric(), Some(3)); // 4-1 penetrating
    }

    #[test]
    fn test_penetrating_explode_chains_on_natural_roll() {
        // Penetrating explosions chain on the natural roll, not the stored
        // (penetrated) face. Non-compounding `!p` and compounding `!!p` must
        // make the same continue/stop decisions on the same stream, differing
        // only in how results are grouped.
        //
        // Stream [6, 6, 4]: chain through both natural 6s, stop at natural 4.
        // Added penetrated faces: 6, (6-1)=5, (4-1)=3 -> total 14.
        let stream = vec![6, 6, 4];

        let standard = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: true,
                limit: None,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(stream.clone());
        let standard_result = evaluate_with_rng(&Expr::Roll(standard), &mut rng).unwrap();
        assert_eq!(standard_result.outcome, RollOutcome::Numeric(14)); // 6 + 5 + 3
        assert_eq!(standard_result.dice.len(), 3); // separate dice

        let compounding = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: true,
                penetrating: true,
                limit: None,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let mut rng = TestRng::new(stream);
        let compounding_result = evaluate_with_rng(&Expr::Roll(compounding), &mut rng).unwrap();
        assert_eq!(compounding_result.outcome, RollOutcome::Numeric(14)); // 6 + 5 + 3
        assert_eq!(compounding_result.dice.len(), 1); // compounded into one die
    }

    #[test]
    fn test_crit_success_marker_output() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(20),
            },
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::CriticalSuccess(Condition {
                compare: Compare::Equal,
                value: 20,
            })],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![20]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.expression.contains("20**"));
        assert!(result.dice[0].is_crit_success);
    }

    #[test]
    fn test_crit_failure_marker_output() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(20),
            },
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::CriticalFailure(Condition {
                compare: Compare::Equal,
                value: 1,
            })],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.expression.contains("1*"));
        assert!(result.dice[0].is_crit_failure);
    }

    #[test]
    fn test_crit_both_markers_output() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 3,
                kind: DieKind::Number(20),
            },
            vec![],
            ScoringMode::Sum,
            vec![
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 20,
                }),
                AnnotationRule::CriticalFailure(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            ],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![20, 10, 1]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.expression.contains("20**"));
        assert!(result.expression.contains("1*"));
        assert!(!result.expression.contains("10*"));
    }

    #[test]
    fn test_crit_no_effect_on_total() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(20),
            },
            vec![],
            ScoringMode::Sum,
            vec![
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 20,
                }),
                AnnotationRule::CriticalFailure(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            ],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![20, 1]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(21)); // 20 + 1, crits don't change value
    }

    #[test]
    fn test_evaluate_digit_dice_d66() {
        let mut rng = TestRng::new(vec![3, 5]);
        let result = evaluate_with_rng(&digit_plan(2, 6), &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(35)); // digits: 3, 5 → 35
        assert_eq!(result.dice.len(), 2);
        assert_eq!(result.expression, "D66[3, 5] = 35");
    }

    #[test]
    fn test_evaluate_digit_dice_d666() {
        let mut rng = TestRng::new(vec![1, 4, 6]);
        let result = evaluate_with_rng(&digit_plan(3, 6), &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(146)); // digits: 1, 4, 6 → 146
        assert_eq!(result.dice.len(), 3);
        assert_eq!(result.expression, "D666[1, 4, 6] = 146");
    }

    #[test]
    fn test_evaluate_digit_dice_d44() {
        let mut rng = TestRng::new(vec![2, 3]);
        let result = evaluate_with_rng(&digit_plan(2, 4), &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(23));
        assert_eq!(result.expression, "D44[2, 3] = 23");
    }

    #[test]
    fn test_evaluate_digit_dice_single() {
        let mut rng = TestRng::new(vec![4]);
        let result = evaluate_with_rng(&digit_plan(1, 6), &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(4));
        assert_eq!(result.expression, "D6[4] = 4");
    }

    #[test]
    fn test_evaluate_digit_dice_max_d66() {
        let mut rng = TestRng::new(vec![6, 6]);
        let result = evaluate_with_rng(&digit_plan(2, 6), &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(66));
    }

    #[test]
    fn test_evaluate_digit_dice_min_d66() {
        let mut rng = TestRng::new(vec![1, 1]);
        let result = evaluate_with_rng(&digit_plan(2, 6), &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(11));
    }

    #[test]
    fn test_dropped_dice_no_crit_marker() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::KeepHighest(3)],
            ScoringMode::Sum,
            vec![
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 6,
                }),
                AnnotationRule::CriticalFailure(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            ],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6, 4, 3, 1]); // 1 will be dropped
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();

        // The dropped die (value 1) should NOT have is_crit_failure set
        let dropped_die = result.dice.iter().find(|d| d.dropped).unwrap();
        assert!(!dropped_die.is_crit_failure);

        // The kept die (value 6) SHOULD have is_crit_success set
        let crit_die = result
            .dice
            .iter()
            .find(|d| d.face.as_numeric() == Some(6))
            .unwrap();
        assert!(crit_die.is_crit_success);
    }

    // --- Reroll evaluation tests ---

    #[test]
    fn test_reroll_basic() {
        // 2d6r: default reroll condition is =1
        // Rolls: 1 (rerolled), 4 (replacement), 5 (second die, no reroll)
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Reroll {
                once: false,
                condition: None, // defaults to =1
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 5, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(9)); // 4 (rerolled from 1) + 5
    }

    #[test]
    fn test_reroll_once() {
        // 1d6ro: reroll once, even if replacement still matches
        // Rolls: 1 (matches =1, reroll), 1 (still matches but once=true, stop)
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Reroll {
                once: true,
                condition: None, // defaults to =1
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 1]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(1)); // Rerolled once to 1, kept because once=true
    }

    #[test]
    fn test_reroll_with_condition() {
        // 1d6r<3: reroll if value < 3
        // Rolls: 2 (matches <3, reroll), 4 (does not match, stop)
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Reroll {
                once: false,
                condition: Some(Condition {
                    compare: Compare::LessThan,
                    value: 3,
                }),
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![2, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(4));
    }

    #[test]
    fn test_reroll_no_match() {
        // 2d6r: default condition =1, but no dice roll 1
        // Rolls: 5, 3 — neither matches =1, no reroll
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Reroll {
                once: false,
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![5, 3]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(8));
    }

    #[test]
    fn test_reroll_keep_phase_order_independent_of_notation() {
        // Modifier phases apply as reroll -> keep/drop regardless of the order
        // written in the notation. `4d6kh3r` and `4d6rkh3` must compute the
        // same result on the same RNG stream.
        let stream = vec![1, 2, 3, 4, 6];

        let written_keep_first = crate::parse("4d6kh3r").unwrap();
        let mut rng = TestRng::new(stream.clone());
        let keep_first = evaluate_with_rng(&written_keep_first, &mut rng).unwrap();

        let written_reroll_first = crate::parse("4d6rkh3").unwrap();
        let mut rng = TestRng::new(stream);
        let reroll_first = evaluate_with_rng(&written_reroll_first, &mut rng).unwrap();

        assert_eq!(keep_first.outcome, reroll_first.outcome);
        let kept_keep_first: Vec<i64> = keep_first
            .dice
            .iter()
            .filter(|d| !d.dropped)
            .map(|d| d.face.as_numeric().unwrap())
            .collect();
        let kept_reroll_first: Vec<i64> = reroll_first
            .dice
            .iter()
            .filter(|d| !d.dropped)
            .map(|d| d.face.as_numeric().unwrap())
            .collect();
        assert_eq!(kept_keep_first, kept_reroll_first);
    }

    #[test]
    fn test_low_die_rerolled_before_keep_highest_drops_it() {
        // A die low enough that keep-highest would drop it must still be
        // rerolled first. Stream [1, 2, 3, 4] then 6 as the reroll: the 1 is
        // rerolled to 6, then keep-highest-3 drops the 2. Total = 6 + 3 + 4.
        // Applying keep first would drop the 1 and the reroll would skip it,
        // yielding 2 + 3 + 4 = 9.
        let expr = crate::parse("4d6kh3r").unwrap();
        let mut rng = TestRng::new(vec![1, 2, 3, 4, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(13));
    }

    // --- Error path tests ---

    #[test]
    fn test_division_by_zero() {
        let expr = Expr::BinOp {
            op: Op::Div,
            left: Box::new(Expr::Number(10)),
            right: Box::new(Expr::Number(0)),
        };
        let result = evaluate(&expr);
        assert!(matches!(result, Err(Error::DivisionByZero)));
    }

    #[test]
    fn test_reroll_limit() {
        // 1d6r=1 with TestRng always returning 1 — should hit reroll limit
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Reroll {
                once: false,
                condition: Some(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1]); // Wraps around, always returns 1
        let result = evaluate_with_rng(&expr, &mut rng);
        assert!(matches!(result, Err(Error::RerollLimit(_))));
    }

    #[test]
    fn test_evaluate_keep_lowest() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::KeepLowest(1)],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![2, 5, 3, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(2)); // Keep lowest: 2
        assert_eq!(result.dice.iter().filter(|d| d.dropped).count(), 3);
    }

    #[test]
    fn test_evaluate_keep_lowest_all() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::KeepLowest(5)], // Keep more than rolled
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(7)); // All kept
    }

    #[test]
    fn test_evaluate_keep_highest_all() {
        // Keeping more dice than were rolled must not panic (active_count - n
        // would underflow without the n >= active_count guard). Keeping more
        // than the pool keeps everything: no dice are dropped.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::KeepHighest(5)], // Keep more than rolled
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(7)); // All kept
        assert_eq!(result.dice.iter().filter(|d| d.dropped).count(), 0);
    }

    #[test]
    fn test_evaluate_drop_highest_all() {
        // apply_drop_highest has no active_count guard: `indices.iter().take(n)`
        // naturally caps at the active count, so this must not panic. Dropping
        // more than the pool drops everything.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::DropHighest(5)], // Drop more than rolled
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(0)); // All dropped
        assert_eq!(result.dice.iter().filter(|d| d.dropped).count(), 2);
    }

    #[test]
    fn test_evaluate_drop_lowest_all() {
        // Mirrors test_evaluate_drop_highest_all for apply_drop_lowest.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::DropLowest(5)], // Drop more than rolled
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(0)); // All dropped
        assert_eq!(result.dice.iter().filter(|d| d.dropped).count(), 2);
    }

    #[test]
    fn test_evaluate_percent_dice() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Percent,
            },
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![42]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(42));
    }

    #[test]
    fn test_evaluate_percent_dice_max() {
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Percent,
            },
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![100]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(100));
    }

    #[test]
    fn test_evaluate_negative_number() {
        let expr = Expr::BinOp {
            op: Op::Sub,
            left: Box::new(Expr::Number(0)),
            right: Box::new(Expr::Number(5)),
        };
        let result = evaluate(&expr).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(-5));
    }

    #[test]
    fn test_evaluate_group() {
        let expr = Expr::Group(Box::new(Expr::Number(42)));
        let result = evaluate(&expr).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(42));
        assert!(result.expression.contains("(42)"));
    }

    #[test]
    fn test_arithmetic_with_success_counting_operand() {
        // 5d10>=8 + 2: success count (3) + 2, produced as a Numeric outcome.
        let expr = crate::parse("5d10>=8 + 2").unwrap();
        let mut rng = TestRng::new(vec![10, 7, 8, 3, 9]); // 10, 8, 9 >= 8 → 3 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(5));
    }

    #[test]
    fn test_group_propagates_successes_outcome() {
        // (5d10>=8): grouping preserves the success-counting outcome.
        let expr = crate::parse("(5d10>=8)").unwrap();
        let mut rng = TestRng::new(vec![10, 7, 8, 3, 9]); // 10, 8, 9 >= 8 → 3 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Successes(3));
    }

    #[test]
    fn test_explode_limit() {
        // 1d6!! (compounding) with TestRng always returning 6 — should hit explode limit
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: true,
                penetrating: false,
                limit: None,
                condition: None, // defaults to =max (6)
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6]); // Wraps around, always returns 6
        let result = evaluate_with_rng(&expr, &mut rng);
        assert!(matches!(result, Err(Error::ExplodeLimit(_))));
    }

    #[test]
    fn test_standard_explode_limit() {
        // 1d6! (non-compounding) with TestRng always returning 6 must hit the
        // explode limit rather than growing the pool without bound.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                limit: None,
                condition: None, // defaults to =max (6)
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6]); // Wraps around, always returns 6
        let result = evaluate_with_rng(&expr, &mut rng);
        assert!(matches!(result, Err(Error::ExplodeLimit(_))));
    }

    #[test]
    fn test_wide_pool_single_explosions_do_not_trip_limit() {
        // The explode limit bounds the depth of a single chain, not the total
        // pool size. A wide pool where many independent dice each explode once
        // must not trip the limit, even when the number of explosions exceeds
        // MAX_EXPLOSIONS.
        let count = (MAX_EXPLOSIONS as usize) + 20;
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: count as u32,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                limit: None,
                condition: None, // defaults to =max (6)
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        // Initial rolls (all 6, every die explodes) then one stopping roll per
        // die (all 1, every chain has depth 1).
        let mut values = vec![6; count];
        values.extend(std::iter::repeat_n(1, count));
        let mut rng = TestRng::new(values);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        // Each originating die plus its single exploded die.
        assert_eq!(result.dice.len(), count * 2);
    }

    #[test]
    fn test_explode_limit_large_user_cap_still_hits_runaway_guard() {
        // 1d6!200 (limit far above MAX_EXPLOSIONS) with TestRng always
        // returning 6: the chain never reaches the user cap, so the runaway
        // guard must still fire. The user cap does not disable the safety net.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                limit: Some(MAX_EXPLOSIONS + 100),
                condition: None,
            }],
            ScoringMode::Sum,
            vec![],
        );
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6]); // Wraps around, always returns 6
        let result = evaluate_with_rng(&expr, &mut rng);
        assert!(matches!(result, Err(Error::ExplodeLimit(_))));
    }

    #[test]
    fn test_explode_limit_is_per_chain_not_pool_wide() {
        // 2d6!1, TestRng all 6: each of the two originating dice may explode
        // once independently, so the pool grows from 2 to 4 dice (2 initial +
        // 2 exploded), sum = 6 * 4 = 24. A pool-wide cap of 1 would instead
        // give 3 dice and a sum of 18.
        let plan = RollPlan::new_unchecked(
            DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
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
        assert_eq!(result.outcome, RollOutcome::Numeric(24));
    }

    // --- Marvel Multiverse scoring tests ---

    /// Build a 3dMarvel `Expr` for testing.
    fn marvel_expr() -> Expr {
        Expr::Roll(RollPlan::new_unchecked(
            DicePool {
                count: 3,
                kind: DieKind::MarvelD6,
            },
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        ))
    }

    /// Score a 3-die face triple directly via the `score` function.
    fn score_marvel(l: i64, m: i64, r: i64) -> RollOutcome {
        // Build faces the way `roll_pool` does: a raw 1 on the middle die is
        // the M face, so the middle argument is routed through the producer.
        let dice = vec![
            DieResult {
                face: marvel_face(0, l),
                history: vec![marvel_face(0, l)],
                dropped: false,
                is_crit_success: false,
                is_crit_failure: false,
            },
            DieResult {
                face: marvel_face(1, m),
                history: vec![marvel_face(1, m)],
                dropped: false,
                is_crit_success: false,
                is_crit_failure: false,
            },
            DieResult {
                face: marvel_face(2, r),
                history: vec![marvel_face(2, r)],
                dropped: false,
                is_crit_success: false,
                is_crit_failure: false,
            },
        ];
        Evaluator::<TestRng>::score(&dice, &ScoringMode::MarvelMultiverse).unwrap()
    }

    #[test]
    fn marvel_score_rejects_non_three_die_pool() {
        // `score` self-guards the 3-die Marvel contract as defense-in-depth,
        // independent of the invariant `RollPlan` enforces at construction.
        // A pool that is not exactly three dice must return an error rather than
        // panic in debug or index out of bounds in release.
        let short = vec![
            DieResult {
                face: DieFace::Numeric(3),
                history: vec![DieFace::Numeric(3)],
                dropped: false,
                is_crit_success: false,
                is_crit_failure: false,
            },
            DieResult {
                face: DieFace::Numeric(4),
                history: vec![DieFace::Numeric(4)],
                dropped: false,
                is_crit_success: false,
                is_crit_failure: false,
            },
        ];
        let result = Evaluator::<TestRng>::score(&short, &ScoringMode::MarvelMultiverse);
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn marvel_score_total_distribution_matches_oracle() {
        // Enumerate all 216 ordered triples and build a histogram of totals.
        let mut counts: [usize; 19] = [0; 19]; // index by total 0..=18
        let mut m_shown_count = 0usize;
        let mut auto_fail_count = 0usize;
        let mut sum_total: i64 = 0;

        for l in 1..=6 {
            for m in 1..=6 {
                for r in 1..=6 {
                    let outcome = match score_marvel(l, m, r) {
                        RollOutcome::Marvel(o) => o,
                        other => panic!("expected Marvel outcome, got {other:?}"),
                    };
                    counts[outcome.total as usize] += 1;
                    sum_total += outcome.total;
                    if outcome.m_shown {
                        m_shown_count += 1;
                    }
                    if outcome.auto_fail {
                        auto_fail_count += 1;
                    }
                }
            }
        }

        let expected = [
            0, 0, 0, 1, 1, 3, 6, 10, 15, 22, 26, 28, 28, 26, 20, 14, 9, 5, 2,
        ];
        for total in 3..=18 {
            assert_eq!(
                counts[total], expected[total],
                "total {total}: got {}, expected {}",
                counts[total], expected[total]
            );
        }
        assert_eq!(sum_total, 2443, "E[total] * 216 = 2443");
        assert_eq!(m_shown_count, 36, "P(M) = 36/216 = 1/6");
        assert_eq!(auto_fail_count, 1, "P(auto_fail) = 1/216");
    }

    #[test]
    fn marvel_score_deterministic_sequences() {
        // [1,1,1] -> M reverts to 1, total 3, auto_fail, m_shown.
        let o = match score_marvel(1, 1, 1) {
            RollOutcome::Marvel(o) => o,
            other => panic!("expected Marvel, got {other:?}"),
        };
        assert_eq!(o.total, 3);
        assert!(o.auto_fail);
        assert!(o.m_shown);

        // [1,1,6] -> M counts as 6, total 1+6+6=13, m_shown, not auto_fail.
        let o = match score_marvel(1, 1, 6) {
            RollOutcome::Marvel(o) => o,
            other => panic!("expected Marvel, got {other:?}"),
        };
        assert_eq!(o.total, 13);
        assert!(o.m_shown);
        assert!(!o.auto_fail);

        // [6,1,6] -> M counts as 6, total 6+6+6=18, m_shown.
        let o = match score_marvel(6, 1, 6) {
            RollOutcome::Marvel(o) => o,
            other => panic!("expected Marvel, got {other:?}"),
        };
        assert_eq!(o.total, 18);
        assert!(o.m_shown);
        assert!(!o.auto_fail);

        // [2,3,4] -> middle is not M, total 9, m_shown false.
        let o = match score_marvel(2, 3, 4) {
            RollOutcome::Marvel(o) => o,
            other => panic!("expected Marvel, got {other:?}"),
        };
        assert_eq!(o.total, 9);
        assert!(!o.m_shown);
        assert!(!o.auto_fail);
    }

    #[test]
    fn marvel_as_numeric_returns_total() {
        let o = RollOutcome::Marvel(crate::ast::MarvelOutcome {
            total: 7,
            auto_fail: false,
            m_shown: true,
        });
        assert_eq!(o.as_numeric(), Some(7));
    }

    #[test]
    fn marvel_binop_coerces_to_numeric_via_as_numeric() {
        // 3dMarvel + 2 with deterministic rolls [2, 3, 4] -> total 9 + 2 = 11.
        let expr = crate::parse("3dMarvel + 2").unwrap();
        let mut rng = TestRng::new(vec![2, 3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(11));
    }

    #[test]
    fn marvel_roll_end_to_end_deterministic() {
        // Rolls [1,1,6]: middle shows M, total 13, Fantastic annotation, no AutoFail.
        let mut rng = TestRng::new(vec![1, 1, 6]);
        let result = evaluate_with_rng(&marvel_expr(), &mut rng).unwrap();
        match result.outcome {
            RollOutcome::Marvel(o) => {
                assert_eq!(o.total, 13);
                assert!(o.m_shown);
                assert!(!o.auto_fail);
            }
            other => panic!("expected Marvel outcome, got {other:?}"),
        }
        assert!(result.annotations.contains(&Annotation::Fantastic));
        assert!(!result.annotations.contains(&Annotation::AutoFail));
        assert!(
            result.expression.contains("[1, M, 6]"),
            "expected M marker in middle position, got: {}",
            result.expression
        );
        assert!(result.expression.contains("13"));
    }

    #[test]
    fn marvel_roll_auto_fail_end_to_end() {
        // Rolls [1,1,1]: M reverts to 1, total 3, both Fantastic and AutoFail.
        let mut rng = TestRng::new(vec![1, 1, 1]);
        let result = evaluate_with_rng(&marvel_expr(), &mut rng).unwrap();
        match result.outcome {
            RollOutcome::Marvel(o) => {
                assert_eq!(o.total, 3);
                assert!(o.m_shown);
                assert!(o.auto_fail);
            }
            other => panic!("expected Marvel outcome, got {other:?}"),
        }
        assert!(result.annotations.contains(&Annotation::Fantastic));
        assert!(result.annotations.contains(&Annotation::AutoFail));
        assert!(result.expression.contains("auto-fail"));
    }

    #[test]
    fn marvel_roll_no_m_shown() {
        // Rolls [2, 3, 4]: middle is 3 (not M), total 9, no annotations.
        let mut rng = TestRng::new(vec![2, 3, 4]);
        let result = evaluate_with_rng(&marvel_expr(), &mut rng).unwrap();
        match result.outcome {
            RollOutcome::Marvel(o) => {
                assert_eq!(o.total, 9);
                assert!(!o.m_shown);
                assert!(!o.auto_fail);
            }
            other => panic!("expected Marvel outcome, got {other:?}"),
        }
        assert!(result.annotations.is_empty());
        assert!(!result.expression.contains("M shown"));
        assert!(!result.expression.contains("auto-fail"));
    }

    #[test]
    fn marvel_evaluate_total_returns_numeric_total() {
        // The sim path (evaluate_total) returns the Marvel total as an i64.
        let expr = marvel_expr();
        let mut rng = TestRng::new(vec![6, 1, 6]);
        let total = crate::roller::evaluate_total(&expr, &mut rng).unwrap();
        assert_eq!(total, 18);
    }

    // --- Marvel Edge/Trouble deterministic rank-vs-value tests ---

    #[test]
    fn marvel_edge_keeps_m_over_3_by_rank() {
        // Initial [5,3,4] (ranks 5,3,4 → lowest is index1 rank3).
        // Edge rerolls index1 → 1. New rank 7 > 3 ⇒ keep M.
        // Result [5,1,4] → total 5+6+4=15, m_shown=true.
        let mut rng = TestRng::new(vec![5, 3, 4, 1]);
        let result = evaluate_with_rng(&crate::parse("3dMarvele1").unwrap(), &mut rng).unwrap();
        match result.outcome {
            RollOutcome::Marvel(o) => {
                assert_eq!(o.total, 15);
                assert!(o.m_shown);
                assert!(!o.auto_fail);
            }
            other => panic!("expected Marvel outcome, got {other:?}"),
        }
    }

    #[test]
    fn marvel_edge_keeps_old_when_reroll_worse_by_rank() {
        // Initial [5,3,4], reroll index1 → 2. New rank 2 < 3 ⇒ keep old (3).
        // Result [5,3,4] → total 12, m_shown=false.
        let mut rng = TestRng::new(vec![5, 3, 4, 2]);
        let result = evaluate_with_rng(&crate::parse("3dMarvele1").unwrap(), &mut rng).unwrap();
        match result.outcome {
            RollOutcome::Marvel(o) => {
                assert_eq!(o.total, 12);
                assert!(!o.m_shown);
                assert!(!o.auto_fail);
            }
            other => panic!("expected Marvel outcome, got {other:?}"),
        }
    }

    #[test]
    fn marvel_edge_records_rejected_reroll_in_history() {
        // Initial [5,3,4], Edge targets index1. Reroll 2 is rejected, but the
        // attempted face remains in history for consumers inspecting traces.
        let mut rng = TestRng::new(vec![5, 3, 4, 2]);
        let result = evaluate_with_rng(&crate::parse("3dMarvele1").unwrap(), &mut rng).unwrap();
        assert_eq!(result.dice[1].face, DieFace::Numeric(3));
        assert_eq!(
            result.dice[1].history,
            vec![DieFace::Numeric(3), DieFace::Numeric(2)]
        );
    }

    #[test]
    fn marvel_trouble_keeps_4_over_m_worse_by_rank() {
        // Initial [2,1,3] (ranks 2,7,3 → highest is index1 M rank7).
        // Trouble rerolls index1 → 4. New rank 4 < 7 ⇒ keep worse (4).
        // Result [2,4,3] → total 9, m_shown=false.
        let mut rng = TestRng::new(vec![2, 1, 3, 4]);
        let result = evaluate_with_rng(&crate::parse("3dMarvelt1").unwrap(), &mut rng).unwrap();
        match result.outcome {
            RollOutcome::Marvel(o) => {
                assert_eq!(o.total, 9);
                assert!(!o.m_shown);
                assert!(!o.auto_fail);
            }
            other => panic!("expected Marvel outcome, got {other:?}"),
        }
    }

    #[test]
    fn marvel_trouble_records_rejected_reroll_in_history() {
        // Initial [2,1,3], Trouble targets the Marvel die. Reroll M has equal
        // rank, so the current face stays while the attempted face is recorded.
        let mut rng = TestRng::new(vec![2, 1, 3, 1]);
        let result = evaluate_with_rng(&crate::parse("3dMarvelt1").unwrap(), &mut rng).unwrap();
        let m_face = DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]));
        assert_eq!(result.dice[1].face, m_face);
        assert_eq!(result.dice[1].history, vec![m_face, m_face]);
    }

    #[test]
    fn marvel_middle_die_raw_one_is_m_face_with_total_13() {
        // rng [1,1,6]: outer 1, middle raw 1 ⇒ M face, outer 6. No auto-fail
        // (right die is 6), so M contributes 6; total 1 + 6 + 6 = 13, m_shown.
        let mut rng = TestRng::new(vec![1, 1, 6]);
        let result = evaluate_with_rng(&crate::parse("3dMarvel").unwrap(), &mut rng).unwrap();
        assert_eq!(
            result.dice[1].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]))
        );
        assert_eq!(result.expression, "3dMarvel[1, M, 6] = 13 (M shown)");
        match result.outcome {
            RollOutcome::Marvel(o) => {
                assert_eq!(o.total, 13);
                assert!(o.m_shown);
                assert!(!o.auto_fail);
            }
            other => panic!("expected Marvel outcome, got {other:?}"),
        }
    }

    #[test]
    fn marvel_edge_reroll_to_one_on_middle_die_yields_m_face() {
        // Initial [3,2,4]: ranks 3,2,4 ⇒ lowest is the middle die (rank 2).
        // Edge rerolls it to raw 1 ⇒ M face (rank 7 > 2), kept in both face
        // and history. Total 3 + 6 + 4 = 13, m_shown.
        let mut rng = TestRng::new(vec![3, 2, 4, 1]);
        let result = evaluate_with_rng(&crate::parse("3dMarvele1").unwrap(), &mut rng).unwrap();
        let m_face = DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]));
        assert_eq!(result.dice[1].face, m_face);
        assert_eq!(result.dice[1].history, vec![DieFace::Numeric(2), m_face]);
        match result.outcome {
            RollOutcome::Marvel(o) => {
                assert_eq!(o.total, 13);
                assert!(o.m_shown);
                assert!(!o.auto_fail);
            }
            other => panic!("expected Marvel outcome, got {other:?}"),
        }
    }

    #[test]
    fn marvel_outer_die_raw_one_stays_numeric_rank_one() {
        // rng [1,5,4]: only the middle die can be M. An outer die rolling a
        // raw 1 stays Numeric(1) and ranks 1.
        let mut rng = TestRng::new(vec![1, 5, 4]);
        let result = evaluate_with_rng(&crate::parse("3dMarvel").unwrap(), &mut rng).unwrap();
        assert_eq!(result.dice[0].face, DieFace::Numeric(1));
        assert!(!is_marvel_face(result.dice[0].face));
        assert_eq!(rank(result.dice[0].face), 1);
    }

    // --- Marvel Edge/Trouble exhaustive enumeration oracles ---

    /// Enumerate all `6^len` rng sequences for a Marvel expression and collect
    /// `(total, m_shown, auto_fail)` per trial.
    fn enumerate_marvel(expr_str: &str, len: usize) -> Vec<(i64, bool, bool)> {
        let expr = crate::parse(expr_str).unwrap();
        let mut results = Vec::new();
        let mut sequence = vec![1u32; len];
        enumerate_marvel_recursive(&expr, &mut sequence, len, 0, &mut results);
        results
    }

    fn enumerate_marvel_recursive(
        expr: &Expr,
        sequence: &mut Vec<u32>,
        len: usize,
        pos: usize,
        results: &mut Vec<(i64, bool, bool)>,
    ) {
        if pos == len {
            let mut rng = TestRng::new(sequence.clone());
            let result = evaluate_with_rng(expr, &mut rng).unwrap();
            match result.outcome {
                RollOutcome::Marvel(o) => results.push((o.total, o.m_shown, o.auto_fail)),
                other => panic!("expected Marvel outcome, got {other:?}"),
            }
            return;
        }
        for v in 1..=6u32 {
            sequence[pos] = v;
            enumerate_marvel_recursive(expr, sequence, len, pos + 1, results);
        }
    }

    /// Compare two Marvel expressions per rng sequence, asserting identical outcomes.
    fn assert_marvel_equivalent(a: &str, b: &str, len: usize) {
        let expr_a = crate::parse(a).unwrap();
        let expr_b = crate::parse(b).unwrap();
        let mut sequence = vec![1u32; len];
        assert_marvel_equivalent_recursive(&expr_a, &expr_b, &mut sequence, len, 0);
    }

    fn assert_marvel_equivalent_recursive(
        expr_a: &Expr,
        expr_b: &Expr,
        sequence: &mut Vec<u32>,
        len: usize,
        pos: usize,
    ) {
        if pos == len {
            let mut rng_a = TestRng::new(sequence.clone());
            let result_a = evaluate_with_rng(expr_a, &mut rng_a).unwrap();
            let mut rng_b = TestRng::new(sequence.clone());
            let result_b = evaluate_with_rng(expr_b, &mut rng_b).unwrap();
            match (result_a.outcome, result_b.outcome) {
                (RollOutcome::Marvel(a), RollOutcome::Marvel(b)) => {
                    assert_eq!(a.total, b.total, "total mismatch for sequence {sequence:?}");
                    assert_eq!(a.m_shown, b.m_shown, "m_shown for sequence {sequence:?}");
                    assert_eq!(
                        a.auto_fail, b.auto_fail,
                        "auto_fail for sequence {sequence:?}"
                    );
                }
                _ => panic!("expected Marvel outcomes for sequence {sequence:?}"),
            }
            return;
        }
        for v in 1..=6u32 {
            sequence[pos] = v;
            assert_marvel_equivalent_recursive(expr_a, expr_b, sequence, len, pos + 1);
        }
    }

    #[test]
    fn marvel_edge_n1_enumeration_oracle() {
        let outcomes = enumerate_marvel("3dMarvele1", 4);
        assert_eq!(outcomes.len(), 1296);
        let sum_total: i64 = outcomes.iter().map(|(t, _, _)| t).sum();
        let m_shown_count = outcomes.iter().filter(|(_, m, _)| *m).count();
        assert_eq!(sum_total, 16849, "E[total] * 1296 = 16849");
        assert_eq!(m_shown_count, 256, "P(M) = 256/1296 = 16/81");
    }

    #[test]
    fn marvel_base_enumeration_oracle() {
        let outcomes = enumerate_marvel("3dMarvel", 3);
        assert_eq!(outcomes.len(), 216);
        let sum_total: i64 = outcomes.iter().map(|(t, _, _)| t).sum();
        let m_shown_count = outcomes.iter().filter(|(_, m, _)| *m).count();
        let auto_fail_count = outcomes.iter().filter(|(_, _, a)| *a).count();
        assert_eq!(sum_total, 2443, "E[total] * 216 = 2443");
        assert_eq!(m_shown_count, 36, "P(M) = 36/216 = 1/6");
        assert_eq!(auto_fail_count, 1, "P(auto_fail) = 1/216");
    }

    #[test]
    fn marvel_cancellation_e2t1_matches_e1_per_sequence() {
        assert_marvel_equivalent("3dMarvele2t1", "3dMarvele1", 4);
    }

    #[test]
    fn marvel_cancellation_t1e2_matches_e1_per_sequence() {
        assert_marvel_equivalent("3dMarvelt1e2", "3dMarvele1", 4);
    }

    #[test]
    fn marvel_cancellation_e1t1_matches_base_per_sequence() {
        assert_marvel_equivalent("3dMarvele1t1", "3dMarvel", 3);
    }

    #[test]
    fn marvel_cancellation_t2e1_matches_t1_per_sequence() {
        assert_marvel_equivalent("3dMarvelt2e1", "3dMarvelt1", 4);
    }

    #[test]
    fn marvel_edge_n2_enumeration_regression() {
        let outcomes = enumerate_marvel("3dMarvele2", 5);
        assert_eq!(outcomes.len(), 7776);
        let sum_total: i64 = outcomes.iter().map(|(t, _, _)| t).sum();
        let m_shown_count = outcomes.iter().filter(|(_, m, _)| *m).count();
        assert_eq!(sum_total, 109863, "E[total] * 7776 for e2");
        assert_eq!(m_shown_count, 1816, "P(M) * 7776 for e2");
    }

    #[test]
    fn marvel_edge_n3_enumeration_regression() {
        let outcomes = enumerate_marvel("3dMarvele3", 6);
        assert_eq!(outcomes.len(), 46656);
        let sum_total: i64 = outcomes.iter().map(|(t, _, _)| t).sum();
        let m_shown_count = outcomes.iter().filter(|(_, m, _)| *m).count();
        assert_eq!(sum_total, 696193, "E[total] * 46656 for e3");
        assert_eq!(m_shown_count, 12476, "P(M) * 46656 for e3");
    }

    #[test]
    fn marvel_trouble_n1_enumeration_regression() {
        let outcomes = enumerate_marvel("3dMarvelt1", 4);
        assert_eq!(outcomes.len(), 1296);
        let sum_total: i64 = outcomes.iter().map(|(t, _, _)| t).sum();
        let m_shown_count = outcomes.iter().filter(|(_, m, _)| *m).count();
        assert_eq!(sum_total, 12657, "E[total] * 1296 for t1");
        assert_eq!(m_shown_count, 36, "P(M) * 1296 for t1");
        // Trouble lowers the total on average compared to the base roll.
        assert!(
            sum_total < 2443 * 6,
            "trouble E[total] must be below base E[total]"
        );
    }

    #[test]
    fn marvel_edge_n1_not_equivalent_to_top_3_of_4d6() {
        // 4d6kh3 rolls 4d6 and keeps the highest 3 by value. Enumerate 6^4
        // sequences and sum the totals. This must differ from 3dMarvele1's
        // sum (16849) — Marvel Edge is position-aware (M ranks 7), not ordinary
        // top-3-of-4 by value.
        let expr = crate::parse("4d6kh3").unwrap();
        let mut sum_total: i64 = 0;
        let mut sequence = vec![1u32; 4];
        sum_numeric_recursive(&expr, &mut sequence, 4, 0, &mut sum_total);
        assert_ne!(
            sum_total, 16849,
            "3dMarvele1 must not be equivalent to 4d6kh3"
        );
    }

    fn sum_numeric_recursive(
        expr: &Expr,
        sequence: &mut Vec<u32>,
        len: usize,
        pos: usize,
        sum: &mut i64,
    ) {
        if pos == len {
            let mut rng = TestRng::new(sequence.clone());
            let result = evaluate_with_rng(expr, &mut rng).unwrap();
            *sum += result.outcome.as_numeric().unwrap();
            return;
        }
        for v in 1..=6u32 {
            sequence[pos] = v;
            sum_numeric_recursive(expr, sequence, len, pos + 1, sum);
        }
    }

    #[test]
    fn marvel_edge_format_renders_modifiers() {
        // Pool [3,3,4] (ranks 3,3,4); e2 = 2 Edge steps.
        // Step1: lowest=index0 (tie rank3, lowest index), reroll 2 (rank2 < 3 → keep old 3).
        // Step2: lowest=index0 (still rank3), reroll 3 (rank3 not > 3 → keep old 3).
        // Dice stay [3,3,4]; m=3 not M → m_contrib=3; total=10; no suffix.
        let mut rng = TestRng::new(vec![3, 3, 4, 2, 3]);
        let result = evaluate_with_rng(&crate::parse("3dMarvele2").unwrap(), &mut rng).unwrap();
        assert_eq!(
            result.expression, "3dMarvele2[3, 3, 4] = 10",
            "got: {}",
            result.expression
        );
    }

    // --- ChaseFantastic behavioral tests ---

    /// Build a 3-die Marvel `Vec<DieResult>` from raw values, routing each
    /// through the per-index Marvel producer so a raw 1 on the middle die is
    /// the M face (matching how `roll_pool` builds a real Marvel pool).
    fn marvel_dice(raws: [i64; 3]) -> Vec<DieResult> {
        raws.iter()
            .enumerate()
            .map(|(i, &raw)| {
                let face = marvel_face(i, raw);
                DieResult {
                    face,
                    history: vec![face],
                    dropped: false,
                    is_crit_success: false,
                    is_crit_failure: false,
                }
            })
            .collect()
    }

    #[test]
    fn chase_fantastic_targets_marvel_die_when_not_m() {
        // dice=[2,5,4]; index1=5 (not M, rank 5). ChaseFantastic targets index 1.
        // RerollLowest would target index0 (rank2) — so this distinguishes the policy.
        // Reroll 1 → rank 7 > 5 ⇒ keep. dice[1].face becomes 1 (M); index0 unchanged.
        let mut rng = TestRng::new(vec![1]);
        let mut dice = marvel_dice([2, 5, 4]);
        let mut evaluator = Evaluator {
            rng: &mut rng,
            total_only: false,
        };
        evaluator
            .apply_edge(&mut dice, 1, EdgePolicy::ChaseFantastic)
            .unwrap();
        assert_eq!(dice[0].face.as_numeric(), Some(2));
        assert_eq!(
            dice[1].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]))
        );
        assert_eq!(dice[2].face.as_numeric(), Some(4));
    }

    #[test]
    fn chase_fantastic_falls_back_to_lowest_when_m_shown() {
        // dice=[5,1,4]; index1=1 (M, rank 7). ChaseFantastic falls back to
        // lowest_rank_index → index2 (rank 4). Reroll 6 → rank 6 > 4 ⇒ keep.
        // dice[1] (M) unchanged; dice[2] becomes 6.
        let mut rng = TestRng::new(vec![6]);
        let mut dice = marvel_dice([5, 1, 4]);
        let mut evaluator = Evaluator {
            rng: &mut rng,
            total_only: false,
        };
        evaluator
            .apply_edge(&mut dice, 1, EdgePolicy::ChaseFantastic)
            .unwrap();
        assert_eq!(dice[0].face.as_numeric(), Some(5));
        assert_eq!(
            dice[1].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]))
        );
        assert_eq!(dice[2].face.as_numeric(), Some(6));
    }

    #[test]
    fn chase_fantastic_re_evaluates_target_per_step() {
        // dice=[2,5,4], 2 steps, rerolls [1, 6].
        // Step1: index1=5 not M → target index1 → reroll 1 (rank7>5 keep) → [2,1,4].
        // Step2: index1=1 (M) → fall back to lowest_rank_index.
        //   ranks (0,2),(1,7),(2,4) → index0 rank2. reroll 6 (rank6>2 keep) → [6,1,4].
        let mut rng = TestRng::new(vec![1, 6]);
        let mut dice = marvel_dice([2, 5, 4]);
        let mut evaluator = Evaluator {
            rng: &mut rng,
            total_only: false,
        };
        evaluator
            .apply_edge(&mut dice, 2, EdgePolicy::ChaseFantastic)
            .unwrap();
        assert_eq!(dice[0].face.as_numeric(), Some(6));
        assert_eq!(
            dice[1].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]))
        );
        assert_eq!(dice[2].face.as_numeric(), Some(4));
    }

    // --- Narrative die pool rolling tests ---

    #[test]
    fn roll_pool_produces_narrative_symbol_faces() {
        // A 2-die Ability(d8) pool with rolls 4, 6: roll 4 = S+S, roll 6 = A.
        let mut rng = TestRng::new(vec![4, 6]);
        let mut evaluator = Evaluator {
            rng: &mut rng,
            total_only: false,
        };
        let pool = DicePool {
            count: 2,
            kind: DieKind::Narrative(NarrativeDie::Ability),
        };
        let dice = evaluator.roll_pool(&pool);
        assert_eq!(
            dice[0].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Success, Symbol::Success]))
        );
        assert_eq!(
            dice[1].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Advantage]))
        );
        assert_eq!(dice[0].history, vec![dice[0].face]);
        assert_eq!(dice[1].history, vec![dice[1].face]);
        assert!(!dice[0].dropped && !dice[1].dropped);
    }

    /// An `Rng` that asserts every `roll` call requests `expected_max`,
    /// catching a `roll_pool` narrative branch that requests the wrong
    /// die's side count.
    struct AssertingRng {
        expected_max: u32,
        value: u32,
    }

    impl Rng for AssertingRng {
        fn roll(&mut self, max: u32) -> u32 {
            assert_eq!(max, self.expected_max, "requested wrong die's side count");
            self.value
        }
    }

    #[test]
    fn roll_pool_requests_the_rolled_narrative_dies_own_count() {
        let mut rng = AssertingRng {
            expected_max: NarrativeDie::Proficiency.count(),
            value: 12,
        };
        let mut evaluator = Evaluator {
            rng: &mut rng,
            total_only: false,
        };
        let pool = DicePool {
            count: 1,
            kind: DieKind::Narrative(NarrativeDie::Proficiency),
        };
        let dice = evaluator.roll_pool(&pool);
        assert_eq!(
            dice[0].face,
            DieFace::Symbols(SymbolPool::of(&[Symbol::Triumph]))
        );
    }

    // --- MAX_REROLLS guard tests ---

    #[test]
    fn marvel_edge_count_over_limit_errors() {
        let mut rng = TestRng::new(vec![1, 1, 1]);
        let result = roll_marvel_with_rng(200, 0, 0, 0, EdgePolicy::RerollLowest, &mut rng);
        assert!(matches!(result, Err(Error::RerollLimit(_))));
    }

    #[test]
    fn marvel_trouble_count_over_limit_errors() {
        let mut rng = TestRng::new(vec![1, 1, 1]);
        let result = roll_marvel_with_rng(0, 200, 0, 0, EdgePolicy::RerollLowest, &mut rng);
        assert!(matches!(result, Err(Error::RerollLimit(_))));
    }

    // --- Typed Marvel API tests ---

    use crate::ast::{Fantastic, MarvelCheck};

    /// Enumerate all 6^3 ordered Marvel triples, calling `roll_marvel_with_rng`
    /// once per triple with a `TestRng` carrying exactly that triple, and
    /// collect `(total, m_shown, auto_fail, success, fantastic)` per trial.
    fn enumerate_marvel_check(
        edges: u32,
        troubles: u32,
        policy: EdgePolicy,
        target: i64,
        modifier: i64,
    ) -> Vec<MarvelCheck> {
        let mut results = Vec::new();
        for l in 1..=6u32 {
            for m in 1..=6u32 {
                for r in 1..=6u32 {
                    let mut rng = TestRng::new(vec![l, m, r]);
                    let check =
                        roll_marvel_with_rng(edges, troubles, target, modifier, policy, &mut rng)
                            .unwrap();
                    results.push(check);
                }
            }
        }
        results
    }

    /// Enumerate all 6^len rng sequences for the typed Marvel API (used for
    /// edges/troubles enumerations where each trial consumes len RNG draws).
    fn enumerate_marvel_check_long(
        edges: u32,
        troubles: u32,
        policy: EdgePolicy,
        target: i64,
        modifier: i64,
        len: usize,
    ) -> Vec<MarvelCheck> {
        let mut results = Vec::new();
        let mut sequence = vec![1u32; len];
        enumerate_marvel_check_long_recursive(
            edges,
            troubles,
            policy,
            target,
            modifier,
            len,
            0,
            &mut sequence,
            &mut results,
        );
        results
    }

    #[allow(clippy::too_many_arguments)]
    fn enumerate_marvel_check_long_recursive(
        edges: u32,
        troubles: u32,
        policy: EdgePolicy,
        target: i64,
        modifier: i64,
        len: usize,
        pos: usize,
        sequence: &mut Vec<u32>,
        results: &mut Vec<MarvelCheck>,
    ) {
        if pos == len {
            let mut rng = TestRng::new(sequence.clone());
            let check =
                roll_marvel_with_rng(edges, troubles, target, modifier, policy, &mut rng).unwrap();
            results.push(check);
            return;
        }
        for v in 1..=6u32 {
            sequence[pos] = v;
            enumerate_marvel_check_long_recursive(
                edges,
                troubles,
                policy,
                target,
                modifier,
                len,
                pos + 1,
                sequence,
                results,
            );
        }
    }

    #[test]
    fn marvel_check_base_216_enumeration_oracle() {
        let checks = enumerate_marvel_check(0, 0, EdgePolicy::RerollLowest, 0, 0);
        assert_eq!(checks.len(), 216);

        let mut histogram = [0usize; 19];
        let mut sum_total: i64 = 0;
        let mut m_shown_count = 0usize;
        let mut auto_fail_count = 0usize;
        for check in &checks {
            histogram[check.outcome.total as usize] += 1;
            sum_total += check.outcome.total;
            if check.outcome.m_shown {
                m_shown_count += 1;
            }
            if check.outcome.auto_fail {
                auto_fail_count += 1;
            }
        }

        let expected = [
            0, 0, 0, 1, 1, 3, 6, 10, 15, 22, 26, 28, 28, 26, 20, 14, 9, 5, 2,
        ];
        for total in 3..=18 {
            assert_eq!(
                histogram[total], expected[total],
                "total {total}: got {}, expected {}",
                histogram[total], expected[total]
            );
        }
        assert_eq!(sum_total, 2443, "E[total] * 216 = 2443");
        assert_eq!(m_shown_count, 36, "P(M) = 36/216 = 1/6");
        assert_eq!(auto_fail_count, 1, "P(auto_fail) = 1/216");
    }

    #[test]
    fn marvel_check_fantastic_conditioning_on_total_14() {
        // P(M | total=14) cannot be derived from the total histogram alone —
        // the histogram says how many triples sum to 14, but not how many of
        // those showed M on the middle die. Per-trial aggregation is the only
        // way to recover this conditioning, which is why `MarvelCheck`
        // carries `m_shown` alongside `total`.
        let checks = enumerate_marvel_check(0, 0, EdgePolicy::RerollLowest, 0, 0);
        let total_14: Vec<_> = checks.iter().filter(|c| c.outcome.total == 14).collect();
        let total_14_m_shown: Vec<_> = total_14.iter().filter(|c| c.outcome.m_shown).collect();
        assert_eq!(total_14.len(), 20, "20 triples sum to total=14");
        assert_eq!(total_14_m_shown.len(), 5, "5 of those show M");
        let p_m_given_14 = total_14_m_shown.len() as f64 / total_14.len() as f64;
        assert!((p_m_given_14 - 0.25).abs() < 1e-12);
    }

    #[test]
    fn marvel_check_edge_n1_enumeration_oracle() {
        let checks = enumerate_marvel_check_long(1, 0, EdgePolicy::RerollLowest, 0, 0, 4);
        assert_eq!(checks.len(), 1296);
        let sum_total: i64 = checks.iter().map(|c| c.outcome.total).sum();
        let m_shown_count = checks.iter().filter(|c| c.outcome.m_shown).count();
        assert_eq!(
            sum_total, 16849,
            "E[total] * 1296 = 16849 for e1 RerollLowest"
        );
        assert_eq!(m_shown_count, 256, "P(M) = 256/1296 for e1 RerollLowest");
    }

    #[test]
    fn marvel_check_trouble_n1_enumeration_oracle() {
        let checks = enumerate_marvel_check_long(0, 1, EdgePolicy::RerollLowest, 0, 0, 4);
        assert_eq!(checks.len(), 1296);
        let sum_total: i64 = checks.iter().map(|c| c.outcome.total).sum();
        let m_shown_count = checks.iter().filter(|c| c.outcome.m_shown).count();
        assert_eq!(sum_total, 12657, "E[total] * 1296 = 12657 for t1");
        assert_eq!(m_shown_count, 36, "P(M) = 36/1296 for t1");
    }

    #[test]
    fn marvel_check_edge_n1_chase_fantastic_enumeration_oracle() {
        let checks = enumerate_marvel_check_long(1, 0, EdgePolicy::ChaseFantastic, 0, 0, 4);
        assert_eq!(checks.len(), 1296);
        let sum_total: i64 = checks.iter().map(|c| c.outcome.total).sum();
        let m_shown_count = checks.iter().filter(|c| c.outcome.m_shown).count();

        // ChaseFantastic drives the Marvel die toward M, raising P(M) above
        // the RerollLowest baseline of 256/1296.
        assert_eq!(m_shown_count, 396, "ChaseFantastic P(M) = 396/1296 = 11/36");
        assert!(
            m_shown_count > 256,
            "ChaseFantastic m_shown_count ({m_shown_count}) must exceed RerollLowest (256)"
        );

        // Report the exact integer sum; the design doc stated ≈12.3866.
        // 16053 / 1296 ≈ 12.3866 — assert the exact sum we computed.
        assert_eq!(
            sum_total, 16053,
            "ChaseFantastic E[total] * 1296 = 16053 (≈12.3866)"
        );

        // ChaseFantastic and RerollLowest produce different total distributions.
        let reroll_lowest_checks =
            enumerate_marvel_check_long(1, 0, EdgePolicy::RerollLowest, 0, 0, 4);
        let mut rl_hist = [0usize; 19];
        let mut cf_hist = [0usize; 19];
        for c in &reroll_lowest_checks {
            rl_hist[c.outcome.total as usize] += 1;
        }
        for c in &checks {
            cf_hist[c.outcome.total as usize] += 1;
        }
        assert_ne!(
            rl_hist, cf_hist,
            "ChaseFantastic total histogram must differ from RerollLowest"
        );
    }

    #[test]
    fn marvel_check_auto_fail_overrides_success() {
        // [1,1,1]: total=3, auto_fail=true. With target=3, modifier=0, the raw
        // total meets the target (3 >= 3) but auto-fail OVERRIDES success.
        let mut rng = TestRng::new(vec![1, 1, 1]);
        let check = roll_marvel_with_rng(0, 0, 3, 0, EdgePolicy::RerollLowest, &mut rng).unwrap();
        assert_eq!(check.outcome.total, 3);
        assert!(check.outcome.auto_fail);
        assert!(!check.success, "auto-fail must override success");
        assert_eq!(check.fantastic, Some(Fantastic::Failure));
    }

    #[test]
    fn marvel_check_target_applied_fantastic_success_and_failure() {
        // [2,1,4]: m_shown, total=12, !auto_fail.
        // target=12 -> success=true, fantastic=Success.
        let mut rng = TestRng::new(vec![2, 1, 4]);
        let check = roll_marvel_with_rng(0, 0, 12, 0, EdgePolicy::RerollLowest, &mut rng).unwrap();
        assert!(check.success);
        assert_eq!(check.fantastic, Some(Fantastic::Success));

        // target=13 -> success=false, fantastic=Failure.
        let mut rng = TestRng::new(vec![2, 1, 4]);
        let check = roll_marvel_with_rng(0, 0, 13, 0, EdgePolicy::RerollLowest, &mut rng).unwrap();
        assert!(!check.success);
        assert_eq!(check.fantastic, Some(Fantastic::Failure));

        // target=12, modifier=1 -> 12+1=13>=12 success=true, fantastic=Success.
        let mut rng = TestRng::new(vec![2, 1, 4]);
        let check = roll_marvel_with_rng(0, 0, 12, 1, EdgePolicy::RerollLowest, &mut rng).unwrap();
        assert!(check.success);
        assert_eq!(check.fantastic, Some(Fantastic::Success));

        // target=20 -> success=false, fantastic=Failure.
        let mut rng = TestRng::new(vec![2, 1, 4]);
        let check = roll_marvel_with_rng(0, 0, 20, 0, EdgePolicy::RerollLowest, &mut rng).unwrap();
        assert!(!check.success);
        assert_eq!(check.fantastic, Some(Fantastic::Failure));

        // [3,3,4]: m_shown=false, total=10 -> fantastic=None regardless of target.
        let mut rng = TestRng::new(vec![3, 3, 4]);
        let check = roll_marvel_with_rng(0, 0, 5, 0, EdgePolicy::RerollLowest, &mut rng).unwrap();
        assert_eq!(check.fantastic, None);
    }

    #[test]
    fn marvel_check_expression_carries_formatted_roll() {
        // [3,3,4] with no modifiers: total=10, formatted as "3dMarvel[3, 3, 4] = 10".
        let mut rng = TestRng::new(vec![3, 3, 4]);
        let det_check =
            roll_marvel_with_rng(0, 0, 10, 0, EdgePolicy::RerollLowest, &mut rng).unwrap();
        assert_eq!(det_check.expression, "3dMarvel[3, 3, 4] = 10");
        // Sanity: the public `roll_marvel` returns a MarvelCheck with an expression.
        let check = roll_marvel(0, 0, 10, 0, EdgePolicy::RerollLowest).unwrap();
        assert!(!check.expression.is_empty());
    }

    #[test]
    fn marvel_check_troubles_only_expression_omits_zero_edge() {
        // [3,3,4] with t1: trouble rerolls the highest-ranked die (index 2, rank 4).
        // Reroll 1 (rank 1 < 4, keep worse) -> [3,3,1], total 7. The formatted
        // expression must carry only the `t1` modifier (no `e0`).
        let mut rng = TestRng::new(vec![3, 3, 4, 1]);
        let det_check =
            roll_marvel_with_rng(0, 1, 7, 0, EdgePolicy::RerollLowest, &mut rng).unwrap();
        assert_eq!(det_check.expression, "3dMarvelt1[3, 3, 1] = 7");
        assert_eq!(det_check.outcome.total, 7);
    }

    #[test]
    fn marvel_check_chase_fantastic_expression_uses_public_edge_notation() {
        // ChaseFantastic is selected by typed API/CLI policy, not by parser notation.
        // The formatted expression should not invent a policy suffix that the parser
        // does not accept as Marvel notation.
        let mut rng = TestRng::new(vec![2, 5, 4, 1]);
        let det_check =
            roll_marvel_with_rng(1, 0, 10, 0, EdgePolicy::ChaseFantastic, &mut rng).unwrap();
        assert_eq!(det_check.expression, "3dMarvele1[2, M, 4] = 12 (M shown)");
    }
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::{DieResult, FastRng, Rng, RngCheckpoint, RollResult};
    use crate::ast::{
        Annotation, DieFace, EdgePolicy, Fantastic, MarvelCheck, MarvelOutcome, RollOutcome,
        Symbol, SymbolPool,
    };

    #[test]
    fn roll_result_serializes_to_json() {
        let result = RollResult {
            outcome: RollOutcome::Numeric(7),
            dice: vec![
                DieResult {
                    face: DieFace::Numeric(6),
                    history: vec![DieFace::Numeric(6)],
                    dropped: false,
                    is_crit_success: true,
                    is_crit_failure: false,
                },
                DieResult {
                    face: DieFace::Numeric(1),
                    history: vec![DieFace::Numeric(1)],
                    dropped: true,
                    is_crit_success: false,
                    is_crit_failure: true,
                },
            ],
            expression: "2d6".to_string(),
            annotations: vec![],
        };

        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["outcome"]["Numeric"], 7);
        assert_eq!(json["expression"], "2d6");
        assert_eq!(json["dice"][0]["face"]["Numeric"], 6);
        assert_eq!(json["dice"][0]["history"][0]["Numeric"], 6);
        assert_eq!(json["dice"][0]["dropped"], false);
        assert_eq!(json["dice"][0]["is_crit_success"], true);
        assert_eq!(json["dice"][0]["is_crit_failure"], false);
        assert_eq!(json["dice"][1]["dropped"], true);
        assert_eq!(json["dice"][1]["is_crit_failure"], true);
        assert_eq!(json["annotations"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn marvel_m_face_serializes_as_symbols_map() {
        let m_face = DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]));
        let json = serde_json::to_value(m_face).unwrap();
        assert_eq!(json, serde_json::json!({ "Symbols": { "Marvel": 1 } }));

        let restored: DieFace = serde_json::from_value(json).unwrap();
        assert_eq!(restored, m_face);
    }

    #[test]
    fn marvel_outcome_round_trips_through_serde() {
        let outcome = RollOutcome::Marvel(MarvelOutcome {
            total: 13,
            auto_fail: false,
            m_shown: true,
        });
        let json = serde_json::to_string(&outcome).unwrap();
        let restored: RollOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, outcome);
        assert_eq!(
            json,
            r#"{"Marvel":{"total":13,"auto_fail":false,"m_shown":true}}"#
        );
    }

    #[test]
    fn roll_result_serializes_marvel_annotations() {
        let result = RollResult {
            outcome: RollOutcome::Marvel(MarvelOutcome {
                total: 3,
                auto_fail: true,
                m_shown: true,
            }),
            dice: vec![],
            expression: "3dMarvel[1, M, 1] = 3 (auto-fail)".to_string(),
            annotations: vec![Annotation::Fantastic, Annotation::AutoFail],
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["outcome"]["Marvel"]["total"], 3);
        assert_eq!(json["outcome"]["Marvel"]["auto_fail"], true);
        assert_eq!(json["outcome"]["Marvel"]["m_shown"], true);
        let anns = json["annotations"].as_array().unwrap();
        assert_eq!(anns.len(), 2);
        assert_eq!(anns[0], "Fantastic");
        assert_eq!(anns[1], "AutoFail");
    }

    #[test]
    fn rng_checkpoint_deserializes_for_restore() {
        let mut rng = FastRng::with_seed(7);

        let _before_checkpoint = rng.roll(10);
        let checkpoint = rng.checkpoint();
        let expected = rng.roll(10);

        let json = serde_json::to_string(&checkpoint).unwrap();
        let restored_checkpoint: RngCheckpoint = serde_json::from_str(&json).unwrap();

        rng.restore(restored_checkpoint);

        assert_eq!(rng.roll(10), expected);
    }

    #[test]
    fn edge_policy_round_trips_through_serde() {
        let reroll_lowest = EdgePolicy::RerollLowest;
        let json = serde_json::to_string(&reroll_lowest).unwrap();
        let restored: EdgePolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, reroll_lowest);
        assert_eq!(json, r#""RerollLowest""#);

        let chase = EdgePolicy::ChaseFantastic;
        let json = serde_json::to_string(&chase).unwrap();
        let restored: EdgePolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, chase);
        assert_eq!(json, r#""ChaseFantastic""#);
    }

    #[test]
    fn fantastic_round_trips_through_serde() {
        let success = Fantastic::Success;
        let json = serde_json::to_string(&success).unwrap();
        let restored: Fantastic = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, success);
        assert_eq!(json, r#""Success""#);

        let failure = Fantastic::Failure;
        let json = serde_json::to_string(&failure).unwrap();
        let restored: Fantastic = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, failure);
        assert_eq!(json, r#""Failure""#);
    }

    #[test]
    fn marvel_check_serializes_to_json() {
        let check = MarvelCheck {
            outcome: MarvelOutcome {
                total: 10,
                auto_fail: false,
                m_shown: false,
            },
            target: 12,
            modifier: 1,
            success: false,
            fantastic: None,
            expression: "3dMarvel[3, 3, 4] = 10".to_string(),
        };
        let json = serde_json::to_value(&check).unwrap();
        assert_eq!(json["outcome"]["total"], 10);
        assert_eq!(json["outcome"]["auto_fail"], false);
        assert_eq!(json["outcome"]["m_shown"], false);
        assert_eq!(json["target"], 12);
        assert_eq!(json["modifier"], 1);
        assert_eq!(json["success"], false);
        assert_eq!(json["fantastic"], serde_json::Value::Null);
        assert_eq!(json["expression"], "3dMarvel[3, 3, 4] = 10");
    }
}
