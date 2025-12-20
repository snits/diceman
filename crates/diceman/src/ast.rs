// ABOUTME: Abstract Syntax Tree types for dice notation expressions.
// ABOUTME: Represents parsed dice expressions like "4d6kh3+5".

use std::fmt;

/// A complete dice expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal number.
    Number(i64),
    /// A dice roll with optional modifiers.
    Roll(Roll),
    /// A binary operation (e.g., addition, subtraction).
    BinOp {
        op: Op,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// A parenthesized group.
    Group(Box<Expr>),
}

/// A dice roll expression (e.g., "4d6kh3").
#[derive(Debug, Clone, PartialEq)]
pub struct Roll {
    /// Number of dice to roll.
    pub count: u32,
    /// Type of dice (number of sides, percent, or fudge).
    pub sides: Sides,
    /// Modifiers applied to the roll.
    pub modifiers: Vec<Modifier>,
}

/// The type of dice to roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sides {
    /// A die with N sides (d6, d20, etc.).
    Number(u32),
    /// A percentile die (d% = d100).
    Percent,
    /// A fudge die (dF = {-1, 0, 1}).
    Fudge,
}

impl Sides {
    /// Returns the number of sides for this die type.
    pub fn count(&self) -> u32 {
        match self {
            Sides::Number(n) => *n,
            Sides::Percent => 100,
            Sides::Fudge => 3, // -1, 0, 1
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

/// A modifier applied to a dice roll.
#[derive(Debug, Clone, PartialEq)]
pub enum Modifier {
    /// Keep the highest N dice.
    KeepHighest(u32),
    /// Keep the lowest N dice.
    KeepLowest(u32),
    /// Drop the highest N dice.
    DropHighest(u32),
    /// Drop the lowest N dice.
    DropLowest(u32),
    /// Explode dice matching the condition.
    Explode {
        /// If true, only explode once per die.
        once: bool,
        /// The condition for explosion (defaults to max value).
        condition: Option<Condition>,
    },
    /// Reroll dice matching the condition.
    Reroll {
        /// If true, only reroll once per die.
        once: bool,
        /// The condition for reroll (defaults to 1).
        condition: Option<Condition>,
    },
    /// Count successes: count dice matching condition instead of summing.
    CountSuccesses(Condition),
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
