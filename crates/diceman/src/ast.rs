// ABOUTME: Abstract Syntax Tree types for dice notation expressions.
// ABOUTME: Represents parsed dice expressions like "4d6kh3+5".

use std::fmt;

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
    /// A digit dice roll (e.g., D66 = roll 2d6, read as two-digit number).
    DigitRoll {
        /// Number of sides on each die.
        sides: u32,
        /// Number of dice (digits) to roll.
        count: u32,
    },
    /// A parenthesized group.
    Group(Box<Expr>),
}

/// A dice pool: what to roll.
#[derive(Debug, Clone, PartialEq)]
pub struct DicePool {
    /// Number of dice to roll.
    pub count: u32,
    /// Kind of die (numeric N-sided, percentile, or fudge).
    pub kind: DieKind,
}

/// A dice roll plan: the normalized execution model produced by the parser.
#[derive(Debug, Clone, PartialEq)]
pub struct RollPlan {
    /// The dice pool to roll.
    pub pool: DicePool,
    /// Modifiers applied to the pool before scoring.
    pub modifiers: Vec<RollModifier>,
    /// How modified dice are converted to a final numeric result.
    pub scoring: ScoringMode,
    /// Annotations detecting interesting outcomes (descriptive only).
    pub annotation_rules: Vec<AnnotationRule>,
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
}

/// How modified dice become a final numeric outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum ScoringMode {
    /// Sum the values of non-dropped dice.
    Sum,
    /// Count dice matching the condition instead of summing.
    CountSuccesses(Condition),
}

/// A rule that detects an interesting outcome (descriptive only; no gameplay effect).
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationRule {
    CriticalSuccess(Condition),
    CriticalFailure(Condition),
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
}

impl DieKind {
    /// Returns the number of sides for this die type.
    pub fn count(&self) -> u32 {
        match self {
            DieKind::Number(n) => *n,
            DieKind::Percent => 100,
            DieKind::Fudge => 3, // -1, 0, 1
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
}
