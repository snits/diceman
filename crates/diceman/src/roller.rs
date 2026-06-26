// ABOUTME: Dice rolling and expression evaluation logic.
// ABOUTME: Evaluates parsed AST nodes to produce roll results.

use crate::ast::{
    Annotation, AnnotationRule, Compare, Condition, DicePool, DieFace, DieKind, Expr, Op,
    RollModifier, RollOutcome, RollPlan, ScoringMode,
};
use crate::error::{Error, Result};
use crate::format;

/// Maximum number of explosions/rerolls allowed to prevent infinite loops.
const MAX_EXPLOSIONS: u32 = 100;
const MAX_REROLLS: u32 = 100;

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
    Ok(evaluator.evaluate(expr)?.outcome.as_numeric())
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
                let left = left_result.outcome.as_numeric();
                let right = right_result.outcome.as_numeric();
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
        // Pipeline: roll_pool -> apply_modifiers -> score -> apply_annotations -> format
        let mut dice = self.roll_pool(&plan.pool);
        self.apply_modifiers(&mut dice, &plan.pool.kind, &plan.modifiers)?;
        let outcome = Self::score(&dice, &plan.scoring)?;

        let (expression, annotations) = if self.total_only {
            (String::new(), vec![])
        } else {
            let annotations =
                Self::apply_annotations(&mut dice, &plan.annotation_rules, &plan.scoring);
            (format::format_roll(plan, &dice, outcome), annotations)
        };

        Ok(RollResult {
            outcome,
            dice,
            expression,
            annotations,
        })
    }

    /// Roll a fresh pool of dice, one `DieResult` per die in the pool.
    fn roll_pool(&mut self, pool: &DicePool) -> Vec<DieResult> {
        (0..pool.count)
            .map(|_| {
                let value = self.roll_die(&pool.kind);
                DieResult {
                    face: DieFace::Numeric(value),
                    history: vec![DieFace::Numeric(value)],
                    dropped: false,
                    is_crit_success: false,
                    is_crit_failure: false,
                }
            })
            .collect()
    }

    /// Apply modifiers in order: reroll -> explode -> keep/drop.
    fn apply_modifiers(
        &mut self,
        dice: &mut Vec<DieResult>,
        kind: &DieKind,
        modifiers: &[RollModifier],
    ) -> Result<()> {
        for modifier in modifiers {
            match modifier {
                RollModifier::Reroll { once, condition } => {
                    self.apply_reroll(dice, kind, *once, condition.as_ref())?;
                }
                RollModifier::Explode {
                    compounding,
                    penetrating,
                    condition,
                } => {
                    self.apply_explode(dice, kind, *compounding, *penetrating, condition.as_ref())?;
                }
                RollModifier::KeepHighest(n) => self.apply_keep_highest(dice, *n),
                RollModifier::KeepLowest(n) => self.apply_keep_lowest(dice, *n),
                RollModifier::DropHighest(n) => self.apply_drop_highest(dice, *n),
                RollModifier::DropLowest(n) => self.apply_drop_lowest(dice, *n),
            }
        }
        Ok(())
    }

    /// Convert modified dice into a final outcome per the scoring mode.
    fn score(dice: &[DieResult], scoring: &ScoringMode) -> Result<RollOutcome> {
        let outcome = match scoring {
            ScoringMode::Sum => RollOutcome::Numeric(
                dice.iter()
                    .filter(|d| !d.dropped)
                    .map(|d| d.face.as_numeric())
                    .sum(),
            ),
            ScoringMode::CountSuccesses(condition) => RollOutcome::Successes(
                dice.iter()
                    .filter(|d| !d.dropped)
                    .filter(|d| {
                        condition
                            .compare
                            .check(d.face.as_numeric(), condition.value)
                    })
                    .count() as i64,
            ),
            ScoringMode::DigitConcatenate => RollOutcome::Numeric(
                dice.iter()
                    .filter(|d| !d.dropped)
                    .fold(0i64, |acc, d| acc * 10 + d.face.as_numeric()),
            ),
            ScoringMode::MarvelMultiverse => {
                // Contract: 3 dice, index 1 = Marvel die, all DieFace::Numeric.
                debug_assert!(dice.len() >= 3, "Marvel Multiverse scoring requires 3 dice");
                let l = dice[0].face.as_numeric();
                let m = dice[1].face.as_numeric();
                let r = dice[2].face.as_numeric();
                let (m_shown, auto_fail) = Self::marvel_facts(dice);
                let m_contrib = if m == 1 {
                    if auto_fail {
                        1
                    } else {
                        6
                    }
                } else {
                    m
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

    /// Derive the Marvel Multiverse facts from the dice.
    ///
    /// The Marvel die is the middle die (index 1); its 1 face is M.
    /// `m_shown` is true when the middle die showed M; `auto_fail` is true
    /// when the raw roll was `1 / M / 1`. The dice are the source of truth.
    fn marvel_facts(dice: &[DieResult]) -> (bool, bool) {
        debug_assert!(dice.len() >= 3, "Marvel Multiverse scoring requires 3 dice");
        let l = dice[0].face.as_numeric();
        let m = dice[1].face.as_numeric();
        let r = dice[2].face.as_numeric();
        (m == 1, l == 1 && m == 1 && r == 1)
    }

    /// Mark per-die crit annotations and return pool-level annotations.
    ///
    /// For Marvel rolls, pushes `Fantastic` when the middle die showed M
    /// and `AutoFail` when the roll was raw `1 / M / 1`. The Marvel facts are
    /// re-derived from the dice (the source of truth), not from the outcome.
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
                die.is_crit_success = c.compare.check(die.face.as_numeric(), c.value);
            }
            if let Some(c) = failure_cond {
                die.is_crit_failure = c.compare.check(die.face.as_numeric(), c.value);
            }
        }

        if let ScoringMode::MarvelMultiverse = scoring {
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
        } else {
            Vec::new()
        }
    }

    fn roll_die(&mut self, kind: &DieKind) -> i64 {
        match kind {
            DieKind::Number(n) => self.rng.roll(*n) as i64,
            DieKind::Percent => self.rng.roll(100) as i64,
            DieKind::Fudge => self.rng.roll(3) as i64 - 2, // -1, 0, 1
            DieKind::MarvelD6 => self.rng.roll(6) as i64,
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
                .check(die.face.as_numeric(), condition.value)
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
        condition: Option<&Condition>,
    ) -> Result<()> {
        let max_val = kind.count() as i64;
        let default_condition = Condition {
            compare: Compare::Equal,
            value: max_val,
        };
        let condition = condition.unwrap_or(&default_condition);

        let mut i = 0;
        while i < dice.len() {
            if dice[i].dropped {
                i += 1;
                continue;
            }

            let mut current_value = dice[i].face.as_numeric();
            let mut explode_count = 0;

            while condition.compare.check(current_value, condition.value) {
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
                    dice[i].face = DieFace::Numeric(dice[i].face.as_numeric() + added_value);
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

                // For non-compounding, break so the new die gets checked in the outer loop
                if !compounding {
                    break;
                }
            }
            i += 1;
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
        indices.sort_by_key(|&i| dice[i].face.as_numeric());

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
        indices.sort_by_key(|&i| std::cmp::Reverse(dice[i].face.as_numeric()));

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
        indices.sort_by_key(|&i| std::cmp::Reverse(dice[i].face.as_numeric()));

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
        indices.sort_by_key(|&i| dice[i].face.as_numeric());

        for &i in indices.iter().take(n) {
            dice[i].dropped = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::DicePool;

    /// Build the lowered `RollPlan` for a digit-dice expression (Dnn).
    fn digit_plan(count: u32, sides: u32) -> Expr {
        Expr::Roll(RollPlan {
            pool: DicePool {
                count,
                kind: DieKind::Number(sides),
            },
            modifiers: vec![],
            scoring: ScoringMode::DigitConcatenate,
            annotation_rules: vec![],
        })
    }

    /// A deterministic RNG for testing.
    struct TestRng {
        values: Vec<u32>,
        index: usize,
    }

    impl TestRng {
        fn new(values: Vec<u32>) -> Self {
            Self { values, index: 0 }
        }
    }

    impl Rng for TestRng {
        fn roll(&mut self, _max: u32) -> u32 {
            let value = self.values[self.index % self.values.len()];
            self.index += 1;
            value
        }
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
        let plan = RollPlan {
            pool: DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(7));
    }

    #[test]
    fn test_evaluate_keep_highest() {
        let plan = RollPlan {
            pool: DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::KeepHighest(3)],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Should keep 5, 3, 6 = 14
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(14));
    }

    #[test]
    fn test_evaluate_expression() {
        let expr = Expr::BinOp {
            op: Op::Add,
            left: Box::new(Expr::Roll(RollPlan {
                pool: DicePool {
                    count: 2,
                    kind: DieKind::Number(6),
                },
                modifiers: vec![],
                scoring: ScoringMode::Sum,
                annotation_rules: vec![],
            })),
            right: Box::new(Expr::Number(5)),
        };
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(12)); // 3 + 4 + 5
    }

    #[test]
    fn test_evaluate_fudge() {
        let plan = RollPlan {
            pool: DicePool {
                count: 4,
                kind: DieKind::Fudge,
            },
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 2, 3, 2]); // -1, 0, 1, 0 = 0
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(0));
    }

    #[test]
    fn test_evaluate_drop_lowest() {
        let plan = RollPlan {
            pool: DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::DropLowest(1)],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Drop 1, keep 5+3+6 = 14
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(14));
    }

    #[test]
    fn test_evaluate_drop_highest() {
        let plan = RollPlan {
            pool: DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::DropHighest(1)],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Drop 6, keep 1+5+3 = 9
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(9));
    }

    #[test]
    fn test_evaluate_count_successes() {
        let plan = RollPlan {
            pool: DicePool {
                count: 5,
                kind: DieKind::Number(10),
            },
            modifiers: vec![],
            scoring: ScoringMode::CountSuccesses(Condition {
                compare: Compare::GreaterOrEqual,
                value: 8,
            }),
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![10, 7, 8, 3, 9]); // 10, 8, 9 >= 8 = 3 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Successes(3));
    }

    #[test]
    fn test_evaluate_count_successes_zero() {
        let plan = RollPlan {
            pool: DicePool {
                count: 3,
                kind: DieKind::Number(6),
            },
            modifiers: vec![],
            scoring: ScoringMode::CountSuccesses(Condition {
                compare: Compare::Equal,
                value: 6,
            }),
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 2, 3]); // No 6s = 0 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Successes(0));
    }

    #[test]
    fn test_evaluate_count_successes_output_format() {
        let plan = RollPlan {
            pool: DicePool {
                count: 4,
                kind: DieKind::Number(10),
            },
            modifiers: vec![],
            scoring: ScoringMode::CountSuccesses(Condition {
                compare: Compare::GreaterOrEqual,
                value: 8,
            }),
            annotation_rules: vec![],
        };
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
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Explode {
                compounding: true,
                penetrating: true,
                condition: None,
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        // Rolls: 6 (explode), 6 (explode), 4 (stop)
        // Added: 6 + (6-1) + (4-1) = 6 + 5 + 3 = 14
        let mut rng = TestRng::new(vec![6, 6, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(14));
    }

    #[test]
    fn test_evaluate_penetrating_explode_no_explosion() {
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Explode {
                compounding: true,
                penetrating: true,
                condition: None,
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        // Roll: 4 (no explosion)
        // Total: 4 (no -1 because no explosion occurred)
        let mut rng = TestRng::new(vec![4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(4));
    }

    #[test]
    fn test_evaluate_standard_explode_creates_new_dice() {
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                condition: None,
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
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
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Explode {
                compounding: false,
                penetrating: false,
                condition: None,
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
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
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Explode {
                compounding: true,
                penetrating: false,
                condition: None,
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
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
        let plan = RollPlan {
            pool: DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            modifiers: vec![
                RollModifier::Explode {
                    compounding: false,
                    penetrating: false,
                    condition: None,
                },
                RollModifier::KeepHighest(2),
            ],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
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
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Explode {
                compounding: false,
                penetrating: true,
                condition: None,
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        // Rolls: 6 (explode, create new die with -1), 4 (stop)
        // Result: 2 dice with values 6 and 3 (4-1 penetrating)
        let mut rng = TestRng::new(vec![6, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(9)); // 6 + 3 (4-1)
        assert_eq!(result.dice.len(), 2); // Two separate dice
        assert_eq!(result.dice[0].face.as_numeric(), 6);
        assert_eq!(result.dice[1].face.as_numeric(), 3); // 4-1 penetrating
    }

    #[test]
    fn test_crit_success_marker_output() {
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(20),
            },
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![AnnotationRule::CriticalSuccess(Condition {
                compare: Compare::Equal,
                value: 20,
            })],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![20]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.expression.contains("20**"));
        assert!(result.dice[0].is_crit_success);
    }

    #[test]
    fn test_crit_failure_marker_output() {
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(20),
            },
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![AnnotationRule::CriticalFailure(Condition {
                compare: Compare::Equal,
                value: 1,
            })],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.expression.contains("1*"));
        assert!(result.dice[0].is_crit_failure);
    }

    #[test]
    fn test_crit_both_markers_output() {
        let plan = RollPlan {
            pool: DicePool {
                count: 3,
                kind: DieKind::Number(20),
            },
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 20,
                }),
                AnnotationRule::CriticalFailure(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            ],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![20, 10, 1]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.expression.contains("20**"));
        assert!(result.expression.contains("1*"));
        assert!(!result.expression.contains("10*"));
    }

    #[test]
    fn test_crit_no_effect_on_total() {
        let plan = RollPlan {
            pool: DicePool {
                count: 2,
                kind: DieKind::Number(20),
            },
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 20,
                }),
                AnnotationRule::CriticalFailure(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            ],
        };
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
        let plan = RollPlan {
            pool: DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::KeepHighest(3)],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 6,
                }),
                AnnotationRule::CriticalFailure(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            ],
        };
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
            .find(|d| d.face.as_numeric() == 6)
            .unwrap();
        assert!(crit_die.is_crit_success);
    }

    // --- Reroll evaluation tests ---

    #[test]
    fn test_reroll_basic() {
        // 2d6r: default reroll condition is =1
        // Rolls: 1 (rerolled), 4 (replacement), 5 (second die, no reroll)
        let plan = RollPlan {
            pool: DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Reroll {
                once: false,
                condition: None, // defaults to =1
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 5, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(9)); // 4 (rerolled from 1) + 5
    }

    #[test]
    fn test_reroll_once() {
        // 1d6ro: reroll once, even if replacement still matches
        // Rolls: 1 (matches =1, reroll), 1 (still matches but once=true, stop)
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Reroll {
                once: true,
                condition: None, // defaults to =1
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1, 1]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(1)); // Rerolled once to 1, kept because once=true
    }

    #[test]
    fn test_reroll_with_condition() {
        // 1d6r<3: reroll if value < 3
        // Rolls: 2 (matches <3, reroll), 4 (does not match, stop)
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Reroll {
                once: false,
                condition: Some(Condition {
                    compare: Compare::LessThan,
                    value: 3,
                }),
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![2, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(4));
    }

    #[test]
    fn test_reroll_no_match() {
        // 2d6r: default condition =1, but no dice roll 1
        // Rolls: 5, 3 — neither matches =1, no reroll
        let plan = RollPlan {
            pool: DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Reroll {
                once: false,
                condition: None,
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![5, 3]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(8));
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
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Reroll {
                once: false,
                condition: Some(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![1]); // Wraps around, always returns 1
        let result = evaluate_with_rng(&expr, &mut rng);
        assert!(matches!(result, Err(Error::RerollLimit(_))));
    }

    #[test]
    fn test_evaluate_keep_lowest() {
        let plan = RollPlan {
            pool: DicePool {
                count: 4,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::KeepLowest(1)],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![2, 5, 3, 6]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(2)); // Keep lowest: 2
        assert_eq!(result.dice.iter().filter(|d| d.dropped).count(), 3);
    }

    #[test]
    fn test_evaluate_keep_lowest_all() {
        let plan = RollPlan {
            pool: DicePool {
                count: 2,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::KeepLowest(5)], // Keep more than rolled
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(7)); // All kept
    }

    #[test]
    fn test_evaluate_percent_dice() {
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Percent,
            },
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![42]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.outcome, RollOutcome::Numeric(42));
    }

    #[test]
    fn test_evaluate_percent_dice_max() {
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Percent,
            },
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
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
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
                kind: DieKind::Number(6),
            },
            modifiers: vec![RollModifier::Explode {
                compounding: true,
                penetrating: false,
                condition: None, // defaults to =max (6)
            }],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![],
        };
        let expr = Expr::Roll(plan);
        let mut rng = TestRng::new(vec![6]); // Wraps around, always returns 6
        let result = evaluate_with_rng(&expr, &mut rng);
        assert!(matches!(result, Err(Error::ExplodeLimit(_))));
    }

    // --- Marvel Multiverse scoring tests ---

    /// Build a 3dMarvel `Expr` for testing.
    fn marvel_expr() -> Expr {
        Expr::Roll(RollPlan {
            pool: DicePool {
                count: 3,
                kind: DieKind::MarvelD6,
            },
            modifiers: vec![],
            scoring: ScoringMode::MarvelMultiverse,
            annotation_rules: vec![AnnotationRule::MarvelFantastic],
        })
    }

    /// Score a 3-die face triple directly via the `score` function.
    fn score_marvel(l: i64, m: i64, r: i64) -> RollOutcome {
        let dice = vec![
            DieResult {
                face: DieFace::Numeric(l),
                history: vec![DieFace::Numeric(l)],
                dropped: false,
                is_crit_success: false,
                is_crit_failure: false,
            },
            DieResult {
                face: DieFace::Numeric(m),
                history: vec![DieFace::Numeric(m)],
                dropped: false,
                is_crit_success: false,
                is_crit_failure: false,
            },
            DieResult {
                face: DieFace::Numeric(r),
                history: vec![DieFace::Numeric(r)],
                dropped: false,
                is_crit_success: false,
                is_crit_failure: false,
            },
        ];
        Evaluator::<TestRng>::score(&dice, &ScoringMode::MarvelMultiverse).unwrap()
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
        assert_eq!(o.as_numeric(), 7);
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
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::{DieResult, FastRng, Rng, RngCheckpoint, RollResult};
    use crate::ast::{Annotation, DieFace, MarvelOutcome, RollOutcome};

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
}
