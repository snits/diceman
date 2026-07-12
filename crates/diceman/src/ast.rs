// ABOUTME: Abstract Syntax Tree types for dice notation expressions.
// ABOUTME: Represents parsed dice expressions like "4d6kh3+5".

use std::fmt;

use crate::error::{Error, Result};

/// A complete dice expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal number.
    Number(i64),
    /// A dice roll plan with optional modifiers.
    Roll(RollPlan),
    /// A binary operation (e.g., addition, subtraction).
    BinOp {
        op: Op,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// A parenthesized group.
    Group(Box<Expr>),
}

/// A dice pool: what to roll.
#[derive(Debug, Clone, PartialEq)]
pub struct DicePool {
    /// Number of dice to roll.
    pub count: u32,
    /// Kind of die (numeric N-sided, percentile, fudge, or Marvel d6).
    pub kind: DieKind,
}

/// A dice roll plan: the normalized execution model produced by the parser.
#[derive(Debug, Clone, PartialEq)]
pub struct RollPlan {
    /// The dice pool to roll.
    pool: DicePool,
    /// Modifiers applied to the pool before scoring.
    modifiers: Vec<RollModifier>,
    /// How modified dice are converted to a final numeric result.
    scoring: ScoringMode,
    /// Annotations detecting interesting outcomes (descriptive only).
    annotation_rules: Vec<AnnotationRule>,
}

impl RollPlan {
    /// Build a `RollPlan`, validating the Marvel Multiverse invariant.
    ///
    /// A plan is Marvel exactly when its scoring is `MarvelMultiverse` and its
    /// pool kind is `MarvelD6`; those two must agree. A Marvel plan must roll
    /// exactly 3 dice, may only carry `Edge`/`Trouble` modifiers, and must
    /// carry `annotation_rules` equal to exactly `[AnnotationRule::MarvelFantastic]`.
    /// A non-Marvel plan may not carry `Edge`/`Trouble` modifiers or a
    /// `MarvelFantastic` annotation, but critical-success/failure annotations
    /// remain allowed.
    pub fn new(
        pool: DicePool,
        modifiers: Vec<RollModifier>,
        scoring: ScoringMode,
        annotation_rules: Vec<AnnotationRule>,
    ) -> Result<Self> {
        let marvel_scoring = matches!(scoring, ScoringMode::MarvelMultiverse);
        let marvel_kind = pool.kind == DieKind::MarvelD6;

        if marvel_scoring != marvel_kind {
            return Err(Error::InvalidMarvelRoll(format!(
                "MarvelMultiverse scoring and MarvelD6 pool kind must be used together \
                 (scoring is MarvelMultiverse: {marvel_scoring}, kind is MarvelD6: {marvel_kind})"
            )));
        }

        if marvel_scoring {
            if pool.count != 3 {
                return Err(Error::InvalidMarvelRoll(format!(
                    "expected 3 dice, found {}",
                    pool.count
                )));
            }
            for modifier in &modifiers {
                if !matches!(
                    modifier,
                    RollModifier::Edge { .. } | RollModifier::Trouble { .. }
                ) {
                    return Err(Error::InvalidMarvelRoll(format!(
                        "{modifier:?} is not supported on Marvel rolls; only Edge and Trouble"
                    )));
                }
            }
            if annotation_rules != [AnnotationRule::MarvelFantastic] {
                return Err(Error::InvalidMarvelRoll(
                    "Marvel rolls must carry exactly one MarvelFantastic annotation rule"
                        .to_string(),
                ));
            }
        } else {
            for modifier in &modifiers {
                if matches!(
                    modifier,
                    RollModifier::Edge { .. } | RollModifier::Trouble { .. }
                ) {
                    return Err(Error::InvalidMarvelRoll(format!(
                        "{modifier:?} is only supported on Marvel rolls"
                    )));
                }
            }
            if annotation_rules.contains(&AnnotationRule::MarvelFantastic) {
                return Err(Error::InvalidMarvelRoll(
                    "MarvelFantastic annotation is only supported on Marvel rolls".to_string(),
                ));
            }
        }

        Ok(Self {
            pool,
            modifiers,
            scoring,
            annotation_rules,
        })
    }

    /// Build a `RollPlan` without validating the Marvel Multiverse invariant.
    ///
    /// Bypasses the checks performed by `new`; callers must guarantee the
    /// plan is already valid (e.g. the parser and roller, which validate
    /// upstream before assembling the plan).
    pub(crate) fn new_unchecked(
        pool: DicePool,
        modifiers: Vec<RollModifier>,
        scoring: ScoringMode,
        annotation_rules: Vec<AnnotationRule>,
    ) -> Self {
        Self {
            pool,
            modifiers,
            scoring,
            annotation_rules,
        }
    }

    /// The dice pool to roll.
    pub fn pool(&self) -> &DicePool {
        &self.pool
    }

    /// Modifiers applied to the pool before scoring.
    pub fn modifiers(&self) -> &[RollModifier] {
        &self.modifiers
    }

    /// How modified dice are converted to a final numeric result.
    pub fn scoring(&self) -> &ScoringMode {
        &self.scoring
    }

    /// Annotations detecting interesting outcomes (descriptive only).
    pub fn annotation_rules(&self) -> &[AnnotationRule] {
        &self.annotation_rules
    }
}

/// A modifier that transforms the dice pool before scoring.
#[derive(Debug, Clone, PartialEq)]
pub enum RollModifier {
    /// Keep the highest N dice.
    KeepHighest(u32),
    /// Keep the lowest N dice.
    KeepLowest(u32),
    /// Drop the highest N dice.
    DropHighest(u32),
    /// Drop the lowest N dice.
    DropLowest(u32),
    /// Reroll dice matching the condition.
    Reroll {
        /// If true, only reroll once per die.
        once: bool,
        /// The condition for reroll (defaults to 1).
        condition: Option<Condition>,
    },
    /// Explode dice matching the condition.
    Explode {
        /// If true, add explosions to same die (compounding/Shadowrun).
        /// If false, create new dice for each explosion (standard/Roll20).
        compounding: bool,
        /// If true, subtract 1 from each explosion roll's added value.
        penetrating: bool,
        /// The condition for explosion (defaults to max value).
        condition: Option<Condition>,
    },
    /// Marvel Edge: reroll the lowest-ranked die, keeping the better result by rank.
    Edge {
        /// Number of reroll steps to apply.
        count: u32,
        /// Which die to target for each step.
        policy: EdgePolicy,
    },
    /// Marvel Trouble: reroll the highest-ranked die, keeping the worse result by rank.
    Trouble {
        /// Number of reroll steps to apply.
        count: u32,
    },
}

/// Which die a Marvel Edge step targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EdgePolicy {
    /// Reroll the lowest-ranked die (tie broken by lowest pool index).
    #[default]
    RerollLowest,
    /// Reroll the Marvel die (index 1) until it shows M, then fall back to RerollLowest.
    ChaseFantastic,
}

/// How modified dice become a final numeric outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum ScoringMode {
    /// Sum the values of non-dropped dice.
    Sum,
    /// Count dice matching the condition instead of summing.
    CountSuccesses(Condition),
    /// Concatenate non-dropped die values as digits (e.g., D66: 3, 5 -> 35).
    DigitConcatenate,
    /// Score a Marvel Multiverse 3dMarvel roll.
    MarvelMultiverse,
}

/// A rule that detects an interesting outcome (descriptive only; no gameplay effect).
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationRule {
    CriticalSuccess(Condition),
    CriticalFailure(Condition),
    MarvelFantastic,
}

/// The face value a die landed on.
///
/// Numeric dice produce `Numeric`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DieFace {
    /// A numeric face (e.g., a d6 showing 4).
    Numeric(i64),
}

impl DieFace {
    /// Return the numeric value of a `Numeric` face.
    pub fn as_numeric(self) -> i64 {
        match self {
            DieFace::Numeric(n) => n,
        }
    }
}

impl fmt::Display for DieFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DieFace::Numeric(n) => write!(f, "{n}"),
        }
    }
}

/// The final outcome of scoring a roll.
///
/// `Numeric` covers summed and digit-concatenated results;
/// `Successes` covers success-counting results;
/// `Marvel` covers a Marvel Multiverse 3dMarvel roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RollOutcome {
    Numeric(i64),
    Successes(i64),
    Marvel(MarvelOutcome),
}

/// The scored outcome of a Marvel Multiverse 3dMarvel roll.
///
/// The Marvel die is the middle die (index 1); its 1 face is M.
/// M counts as 6 for the total except on raw `1 / M / 1`, where M reverts
/// to 1 and the check auto-fails regardless of target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarvelOutcome {
    /// The 3dMarvel total, in 3..=18.
    pub total: i64,
    /// True when the raw roll was `1 / M / 1` (auto-fail regardless of target).
    pub auto_fail: bool,
    /// True when the middle die showed M (its face was 1).
    pub m_shown: bool,
}

impl RollOutcome {
    /// Return the numeric value of the outcome, if it has one.
    ///
    /// `Some` for variants with a numeric value: `Successes` yields the
    /// count, `Marvel` yields the total (lenient extraction consistent with
    /// arithmetic coercion). `None` is reserved for future non-numeric
    /// (e.g. symbolic) outcomes that have no arithmetic meaning.
    pub fn as_numeric(self) -> Option<i64> {
        match self {
            RollOutcome::Numeric(n) | RollOutcome::Successes(n) => Some(n),
            RollOutcome::Marvel(o) => Some(o.total),
        }
    }
}

/// The target-applied Marvel verdict when the Marvel die shows M.
///
/// `Success` means the check met its target despite M; `Failure` means it
/// missed. Only produced when the middle die showed M (otherwise the roll
/// is not Fantastic and no `Fantastic` value is attached).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Fantastic {
    /// The check succeeded (met or beat the target after modifier, no auto-fail).
    Success,
    /// The check failed (missed the target, or auto-fail overrode an otherwise-passing total).
    Failure,
}

/// The target-applied result of a single Marvel Multiverse 3dMarvel roll.
///
/// Carries the scored `MarvelOutcome` alongside a target/modifier pair and the
/// derived `success`/`fantastic` booleans, plus the formatted roll expression
/// so callers can render the roll without re-running the pipeline.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MarvelCheck {
    /// The scored Marvel outcome (total, auto_fail, m_shown).
    pub outcome: MarvelOutcome,
    /// The target value the check is compared against.
    pub target: i64,
    /// The modifier added to the total before comparison.
    pub modifier: i64,
    /// Whether the check succeeded: `(total + modifier >= target) && !auto_fail`.
    pub success: bool,
    /// The Fantastic verdict when the Marvel die showed M, else `None`.
    pub fantastic: Option<Fantastic>,
    /// The formatted roll expression (e.g., `3dMarvel[3, 3, 4] = 10`).
    pub expression: String,
}

/// A pool-level annotation describing an interesting outcome (descriptive only).
///
/// `Fantastic` and `AutoFail` are populated by Marvel Multiverse scoring
/// (middle die showed M, or raw `1 / M / 1`). `CriticalSuccess` and
/// `CriticalFailure` are reserved variants for pool-level crit surfacing;
/// per-die crit rules currently surface via `DieResult::is_crit_success`
/// and `DieResult::is_crit_failure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Annotation {
    /// A die matched the critical-success condition.
    CriticalSuccess,
    /// A die matched the critical-failure condition.
    CriticalFailure,
    /// The Marvel middle die showed M.
    Fantastic,
    /// The Marvel roll was raw `1 / M / 1` (auto-fail regardless of target).
    AutoFail,
}

/// The type of dice to roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DieKind {
    /// A die with N sides (d6, d20, etc.).
    Number(u32),
    /// A percentile die (d% = d100).
    Percent,
    /// A fudge die (dF = {-1, 0, 1}).
    Fudge,
    /// The six-sided Marvel Multiverse die used in a 3dMarvel pool.
    MarvelD6,
}

impl DieKind {
    /// Returns the number of sides for this die type.
    pub fn count(&self) -> u32 {
        match self {
            DieKind::Number(n) => *n,
            DieKind::Percent => 100,
            DieKind::Fudge => 3, // -1, 0, 1
            DieKind::MarvelD6 => 6,
        }
    }
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Add => write!(f, "+"),
            Op::Sub => write!(f, "-"),
            Op::Mul => write!(f, "*"),
            Op::Div => write!(f, "/"),
        }
    }
}

/// A comparison condition for explode/reroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Condition {
    pub compare: Compare,
    pub value: i64,
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    Equal,
    NotEqual,
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
}

impl Compare {
    /// Check if the given value satisfies this comparison.
    pub fn check(&self, roll: i64, target: i64) -> bool {
        match self {
            Compare::Equal => roll == target,
            Compare::NotEqual => roll != target,
            Compare::LessThan => roll < target,
            Compare::LessOrEqual => roll <= target,
            Compare::GreaterThan => roll > target,
            Compare::GreaterOrEqual => roll >= target,
        }
    }
}

impl fmt::Display for Compare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Compare::Equal => write!(f, "="),
            Compare::NotEqual => write!(f, "<>"),
            Compare::LessThan => write!(f, "<"),
            Compare::LessOrEqual => write!(f, "<="),
            Compare::GreaterThan => write!(f, ">"),
            Compare::GreaterOrEqual => write!(f, ">="),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roll_plan_has_annotation_rules() {
        let plan = RollPlan {
            pool: DicePool {
                count: 1,
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
        assert!(plan
            .annotation_rules
            .iter()
            .any(|r| matches!(r, AnnotationRule::CriticalSuccess(_))));
        assert!(plan
            .annotation_rules
            .iter()
            .any(|r| matches!(r, AnnotationRule::CriticalFailure(_))));
    }

    #[test]
    fn marvel_d6_has_six_faces() {
        assert_eq!(DieKind::MarvelD6.count(), 6);
    }

    fn marvel_pool() -> DicePool {
        DicePool {
            count: 3,
            kind: DieKind::MarvelD6,
        }
    }

    fn numeric_pool() -> DicePool {
        DicePool {
            count: 1,
            kind: DieKind::Number(20),
        }
    }

    #[test]
    fn new_rejects_marvel_kind_with_non_marvel_scoring() {
        let result = RollPlan::new(marvel_pool(), vec![], ScoringMode::Sum, vec![]);
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_scoring_with_non_marvel_kind() {
        let result = RollPlan::new(
            numeric_pool(),
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_wrong_pool_count() {
        let pool = DicePool {
            count: 5,
            kind: DieKind::MarvelD6,
        };
        let result = RollPlan::new(
            pool,
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_non_edge_trouble_modifier() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![RollModifier::KeepHighest(1)],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_empty_annotation_rules() {
        let result = RollPlan::new(marvel_pool(), vec![], ScoringMode::MarvelMultiverse, vec![]);
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_critical_annotation_only() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::CriticalSuccess(Condition {
                compare: Compare::Equal,
                value: 6,
            })],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_critical_annotation_alongside_fantastic() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![
                AnnotationRule::MarvelFantastic,
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 6,
                }),
            ],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_non_marvel_plan_with_edge_modifier() {
        let result = RollPlan::new(
            numeric_pool(),
            vec![RollModifier::Edge {
                count: 1,
                policy: EdgePolicy::RerollLowest,
            }],
            ScoringMode::Sum,
            vec![],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_non_marvel_plan_with_marvel_fantastic_annotation() {
        let result = RollPlan::new(
            numeric_pool(),
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_accepts_valid_marvel_plan_with_edge_modifier() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![RollModifier::Edge {
                count: 1,
                policy: EdgePolicy::RerollLowest,
            }],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_valid_marvel_plan_with_trouble_and_edge_modifiers() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![
                RollModifier::Edge {
                    count: 1,
                    policy: EdgePolicy::RerollLowest,
                },
                RollModifier::Trouble { count: 1 },
            ],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_valid_marvel_plan_with_no_modifiers() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_valid_non_marvel_plan() {
        let pool = DicePool {
            count: 6,
            kind: DieKind::Number(6),
        };
        let result = RollPlan::new(pool, vec![], ScoringMode::Sum, vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_non_marvel_plan_with_critical_success_annotation() {
        let pool = DicePool {
            count: 1,
            kind: DieKind::Number(20),
        };
        let result = RollPlan::new(
            pool,
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::CriticalSuccess(Condition {
                compare: Compare::Equal,
                value: 20,
            })],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_unchecked_builds_without_validation() {
        // Deliberately invalid: MarvelD6 kind paired with Sum scoring.
        let plan = RollPlan::new_unchecked(marvel_pool(), vec![], ScoringMode::Sum, vec![]);
        assert_eq!(plan.pool(), &marvel_pool());
        assert_eq!(plan.modifiers(), &[]);
        assert_eq!(plan.scoring(), &ScoringMode::Sum);
        assert_eq!(plan.annotation_rules(), &[]);
    }
}
